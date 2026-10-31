//! The message API: `POST /api/messages` and `GET /api/messages/{id}/retrieve`.

use std::sync::Arc;

use axum::extract::multipart::MultipartError;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Instant;

use axum::extract::{ConnectInfo, DefaultBodyLimit, FromRequest, Multipart, Path, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::app::AppState;
use crate::error::ApiError;
use crate::ratelimit::{Bucket, client_address};

/// Room for the multipart framing and the `pin` and `ttl` parts on top of the blob itself.
pub const MULTIPART_OVERHEAD_BYTES: usize = 64 * 1024;

/// API routes, merged into the application router. Each route counts against its rate-limit
/// bucket before anything else runs.
pub fn routes(state: &Arc<AppState>) -> Router<Arc<AppState>> {
    let body_limit = state.config.messages.max_payload_bytes + MULTIPART_OVERHEAD_BYTES;
    let limited = |bucket| middleware::from_fn_with_state((Arc::clone(state), bucket), rate_limit);
    Router::new()
        .route(
            "/api/messages",
            post(create)
                .layer(DefaultBodyLimit::max(body_limit))
                .layer(limited(Bucket::Create)),
        )
        .route(
            "/api/messages/{id}/retrieve",
            get(retrieve).layer(limited(Bucket::Retrieve)),
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
    // Bound how many request bodies can be in flight before reading any of them.
    let _upload = state.uploads.try_acquire().map_err(|_| {
        ApiError::ServiceUnavailable("Too many uploads in progress, try again shortly")
    })?;
    let is_multipart = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("multipart/form-data"));
    if !is_multipart {
        return Err(ApiError::UnsupportedMediaType);
    }
    let mut multipart = Multipart::from_request(request, &state)
        .await
        .map_err(|_| ApiError::BadRequest("Malformed multipart body"))?;

    let (mut blob, mut pin, mut ttl) = (None, None, None);
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
                return Err(ApiError::BadRequest(
                    "Unexpected part; expected blob, pin, and ttl",
                ));
            }
        };
        if slot_taken {
            return Err(ApiError::BadRequest("A part was sent twice"));
        }
    }
    let (Some(blob), Some(pin), Some(ttl)) = (blob, pin, ttl) else {
        return Err(ApiError::BadRequest(
            "Missing part; expected blob, pin, and ttl",
        ));
    };

    let created = state.service.create(blob, pin, &ttl).await?;
    let body = CreateResponse {
        id: created.id,
        expires_at: humantime::format_rfc3339_seconds(created.expires_at).to_string(),
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
