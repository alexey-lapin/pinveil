//! The message API: `POST /api/messages` and `GET /api/messages/{id}/retrieve`.

use std::sync::Arc;

use axum::extract::multipart::MultipartError;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Instant;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, DefaultBodyLimit, FromRequest, Multipart, Path, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::app::AppState;
use crate::error::{ApiError, error_response};
use crate::ratelimit::{Bucket, client_address};

/// Room for the multipart framing and the `pin` and `ttl` parts on top of the blob itself.
pub const MULTIPART_OVERHEAD_BYTES: usize = 64 * 1024;

/// API routes, merged into the application router. `route_layer` puts the limiter on the
/// named method only: other methods on these paths get 405 without counting.
pub fn routes(state: &Arc<AppState>) -> Router<Arc<AppState>> {
    let body_limit = state.config.messages.max_payload_bytes + MULTIPART_OVERHEAD_BYTES;
    let limited = |bucket| middleware::from_fn_with_state((Arc::clone(state), bucket), rate_limit);
    Router::new()
        .route(
            "/api/messages",
            post(create)
                .layer(DefaultBodyLimit::max(body_limit))
                .route_layer(limited(Bucket::Create)),
        )
        .route(
            "/api/messages/{id}/retrieve",
            // axum would also run a GET handler for HEAD and drop the body, which would
            // consume the message without delivering it. HEAD gets an explicit 405 before
            // any lookup, verification, or attempt. It is added after the limiter, so it
            // doesn't count against the GET bucket.
            get(retrieve)
                .route_layer(limited(Bucket::Retrieve))
                .head(|| async {
                    error_response(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed")
                }),
        )
}

async fn rate_limit(
    State((state, bucket)): State<(Arc<AppState>, Bucket)>,
    request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |info| info.0.ip());
    let values: Option<Vec<&str>> = request
        .headers()
        .get_all("x-forwarded-for")
        .iter()
        .map(|v| v.to_str().ok())
        .collect();
    // A non-UTF-8 value counts as malformed, which falls back to the peer address.
    let client = match values {
        Some(values) => client_address(peer, &values, &state.config.trusted_proxies),
        None => peer,
    };
    if !state.limiter.allow(bucket, client, Instant::now()) {
        return ApiError::RateLimited.into_response();
    }
    next.run(request).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateResponse {
    id: String,
    expires_at: String,
}

async fn create(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Result<Response, ApiError> {
    // Bound how many creates are in flight before reading any body. The slot covers the
    // whole create, its store work included, even if the client disconnects (see below).
    let upload = Arc::clone(&state.uploads)
        .try_acquire_owned()
        .map_err(|_| {
            ApiError::ServiceUnavailable("Too many uploads in progress, try again shortly")
        })?;
    let is_multipart = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("multipart/form-data"));
    if !is_multipart {
        // Read the (size-capped) body before answering. Answering with body bytes still
        // unread makes the server close the connection, and some clients (Windows in
        // particular) then see a reset instead of the response.
        // The route's body limit still applies: over the cap is 413, not 415.
        return Err(match Bytes::from_request(request, &state).await {
            Ok(_) => ApiError::UnsupportedMediaType,
            Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                ApiError::PayloadTooLarge
            }
            Err(_) => ApiError::BadRequest("Could not read the request body"),
        });
    }
    let mut multipart = Multipart::from_request(request, &state)
        .await
        .map_err(|_| ApiError::BadRequest("Malformed multipart body"))?;

    // Read every part before deciding, for the same reason: a duplicate or unexpected part
    // is remembered and reported once the whole body has been consumed.
    let (mut blob, mut pin, mut ttl) = (None, None, None);
    let mut problem = None;
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        let name = field.name().unwrap_or_default().to_owned();
        let slot_taken = match name.as_str() {
            "blob" => blob
                .replace(field.bytes().await.map_err(multipart_error)?.to_vec())
                .is_some(),
            "pin" => pin
                .replace(field.text().await.map_err(multipart_error)?)
                .is_some(),
            "ttl" => ttl
                .replace(field.text().await.map_err(multipart_error)?)
                .is_some(),
            _ => {
                field.bytes().await.map_err(multipart_error)?;
                problem.get_or_insert("Unexpected part; expected blob, pin, and ttl");
                false
            }
        };
        if slot_taken {
            problem.get_or_insert("A part was sent twice");
        }
    }
    if let Some(problem) = problem {
        return Err(ApiError::BadRequest(problem));
    }
    let (Some(blob), Some(pin), Some(ttl)) = (blob, pin, ttl) else {
        return Err(ApiError::BadRequest(
            "Missing part; expected blob, pin, and ttl",
        ));
    };

    // An owned task holds the upload slot until the create finishes, so a disconnect that
    // drops this handler can't free the slot while hashing or the insert still runs: the
    // work in flight stays within `server.max_concurrent_uploads`.
    let service = Arc::clone(&state.service);
    let created = tokio::spawn(async move {
        let _upload = upload;
        service.create(blob, pin, &ttl).await
    })
    .await
    .map_err(|_| ApiError::Internal)??;
    let body = CreateResponse {
        id: created.id,
        expires_at: created.expires_at,
    };
    Ok((StatusCode::CREATED, Json(body)).into_response())
}

// By value because `map_err` passes the error by value.
#[allow(clippy::needless_pass_by_value)]
fn multipart_error(error: MultipartError) -> ApiError {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::PayloadTooLarge
    } else {
        ApiError::BadRequest("Malformed multipart body")
    }
}

async fn retrieve(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    // A missing header is treated like a malformed one: 400, no attempt counted.
    let pin = headers
        .get("x-message-pin")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::InvalidPin)?
        .to_owned();
    let blob = state.service.retrieve(id, pin).await?;
    Ok(([(header::CONTENT_TYPE, "application/octet-stream")], blob).into_response())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, SystemTime};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use pinveil_core::wordlist::Wordlist;
    use tower::ServiceExt;

    use crate::app::{AppState, router};
    use crate::config::{Sources, load};
    use crate::pin::PinVerifier;
    use crate::service::{HashPool, Parts, Policy, Service, SystemClock, WordIds};
    use crate::store::{
        Attempt, InsertError, MemoryStore, MessageMeta, NewMessage, Store, StoreError,
    };

    /// A memory store whose inserts wait until released, counting those that started.
    struct PausedStore {
        inner: MemoryStore,
        started: AtomicUsize,
        open: Mutex<bool>,
        gate: Condvar,
    }

    impl PausedStore {
        fn release(&self) {
            *self.open.lock().unwrap() = true;
            self.gate.notify_all();
        }
    }

    impl Store for PausedStore {
        fn insert(&self, message: NewMessage, now: SystemTime) -> Result<(), InsertError> {
            self.started.fetch_add(1, Ordering::SeqCst);
            let open = self.open.lock().unwrap();
            drop(self.gate.wait_while(open, |open| !*open).unwrap());
            self.inner.insert(message, now)
        }
        fn meta(&self, id: &str) -> Result<Option<MessageMeta>, StoreError> {
            self.inner.meta(id)
        }
        fn record_failure(
            &self,
            id: &str,
            g: u64,
            max: u32,
        ) -> Result<Option<Attempt>, StoreError> {
            self.inner.record_failure(id, g, max)
        }
        fn consume(&self, id: &str, g: u64) -> Result<Option<Vec<u8>>, StoreError> {
            self.inner.consume(id, g)
        }
        fn remove(&self, id: &str, g: u64) -> Result<bool, StoreError> {
            self.inner.remove(id, g)
        }
        fn sweep(&self, now: SystemTime) -> Result<usize, StoreError> {
            self.inner.sweep(now)
        }
    }

    struct PlainVerifier;

    impl PinVerifier for PlainVerifier {
        fn hash(&self, pin: &str, salt: &str) -> String {
            format!("{salt}:{pin}")
        }
        fn verify(&self, pin: &str, salt: &str, verifier: &str) -> bool {
            verifier == format!("{salt}:{pin}")
        }
    }

    fn create_request() -> Request<Body> {
        let boundary = "pinveil-test-boundary";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"blob\"; filename=\"blob\"\r\nContent-Type: application/octet-stream\r\n\r\nciphertext\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"pin\"\r\n\r\n123456\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"ttl\"\r\n\r\n600\r\n\
             --{boundary}--\r\n"
        );
        Request::post("/api/messages")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap()
    }

    /// A create whose client disconnects mid-insert keeps its upload slot until the insert
    /// finishes, so replacements can't pile up store work beyond the configured bound.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn srv_admission_cancelled_create_keeps_its_upload_slot() {
        let (config, _) = load(&Sources {
            args: vec![],
            env: vec![
                ("PINVEIL_SERVER__MAX_CONCURRENT_UPLOADS".into(), "1".into()),
                ("PINVEIL_PIN__PEPPER".into(), "p".into()),
            ],
            read_file: &|_| Err(std::io::Error::other("none")),
            cpus: 2,
        })
        .unwrap();
        let store = Arc::new(PausedStore {
            inner: MemoryStore::new(10, 1_000_000),
            started: AtomicUsize::new(0),
            open: Mutex::new(false),
            gate: Condvar::new(),
        });
        let service = Service::new(Parts {
            store: Arc::clone(&store) as Arc<dyn Store>,
            verifier: Arc::new(PlainVerifier),
            pool: HashPool::new(2, 8),
            clock: Arc::new(SystemClock),
            ids: Arc::new(WordIds::new(Wordlist::embedded().unwrap(), 3, 6)),
            policy: Policy::from_config(&config),
        });
        let app = router(Arc::new(AppState::with_service(config, service)));
        // Opens the gate however the test ends, so a failed assertion can't leave a blocking
        // thread waiting forever and hang the runtime's shutdown.
        struct Release(Arc<PausedStore>);
        impl Drop for Release {
            fn drop(&mut self) {
                self.0.release();
            }
        }
        let _release = Release(Arc::clone(&store));

        // The first create reaches its insert, then the client goes away.
        let first = tokio::spawn(app.clone().oneshot(create_request()));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while store.started.load(Ordering::SeqCst) == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "the insert never started"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        first.abort();
        let _ = first.await;

        // Its insert is still running, so the only upload slot is still taken.
        // (A request let through would block on the paused insert, hence the timeout.)
        for _ in 0..3 {
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.clone().oneshot(create_request()),
            )
            .await
            .expect("refused at once, not let through to the paused store")
            .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        }
        assert_eq!(store.started.load(Ordering::SeqCst), 1, "no work piled up");

        // Once the insert finishes, the slot is free again and the message was stored.
        store.release();
        loop {
            let response = app.clone().oneshot(create_request()).await.unwrap();
            if response.status() == StatusCode::CREATED {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the slot never came back"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            store.inner.usage().0,
            2,
            "the abandoned create still committed"
        );
    }
}
