//! The HTTP application: routes, the security-header and JSON-error layers, and the page,
//! static, and config handlers. The message API handlers live in `api.rs`.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Serialize;

use crate::assets::{self, INDEX_HTML, MESSAGE_HTML};
use crate::config::{Config, ttl_label};
use crate::error::{ApiError, error_response, message_for_status};
use crate::pin::Argon2Verifier;
use crate::ratelimit::RateLimiter;
use crate::service::{HashPool, Parts, Policy, Service, SystemClock, WordIds};
use crate::store::MemoryStore;
use pinveil_core::wordlist::Wordlist;
use tokio::sync::Semaphore;

/// Shared state for every handler.
pub struct AppState {
    pub config: Config,
    pub service: Arc<Service>,
    /// Bounds create requests whose bodies are in flight (`server.max_concurrent_uploads`).
    pub uploads: Semaphore,
    pub limiter: RateLimiter,
    /// The client config as script-safe JSON, built once at startup.
    client_config_json: String,
    /// `index.html` with the client config filled in.
    index_html: String,
}

impl AppState {
    /// The production state: memory store, Argon2 verifier, system clock, diceware ids.
    #[must_use]
    pub fn new(config: Config, wordlist: Wordlist<'static>) -> Self {
        let service = Service::new(Parts {
            store: Arc::new(MemoryStore::new(
                config.store.max_messages,
                config.store.max_bytes,
            )),
            verifier: Arc::new(Argon2Verifier::new(&config.pin)),
            pool: HashPool::new(
                config.pin.max_concurrent_hashes,
                config.pin.max_queued_hashes,
            ),
            clock: Arc::new(SystemClock),
            ids: Arc::new(WordIds::new(
                wordlist,
                config.message_id.words,
                config.message_id.digits,
            )),
            policy: Policy::from_config(&config),
        });
        Self::with_service(config, service)
    }

    /// State around a given service, for tests that swap in seams.
    #[must_use]
    pub fn with_service(config: Config, service: Arc<Service>) -> Self {
        let client_config = ClientConfig::from_config(&config);
        let json = serde_json::to_string(&client_config).expect("client config serializes");
        let client_config_json = assets::script_safe_json(&json);
        let index_html = INDEX_HTML.replace("{{client_config_json}}", &client_config_json);
        let uploads = Semaphore::new(config.max_concurrent_uploads);
        let limiter = RateLimiter::new(&config.rate_limit);
        Self {
            config,
            service,
            uploads,
            limiter,
            client_config_json,
            index_html,
        }
    }
}

/// The browser-facing configuration (APP.md, "Client config"). Built from an explicit
/// allowlist of fields; the server configuration itself is never serialized.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    store_mode: &'static str,
    max_payload_bytes: usize,
    max_files: usize,
    max_manifest_bytes: usize,
    pbkdf2_iterations: u32,
    min_ttl_seconds: u64,
    max_ttl_seconds: u64,
    default_ttl_seconds: u64,
    ttl_presets: Vec<TtlPreset>,
}

#[derive(Serialize)]
struct TtlPreset {
    seconds: u64,
    label: String,
}

impl ClientConfig {
    #[must_use]
    pub fn from_config(config: &Config) -> Self {
        let m = &config.messages;
        Self {
            store_mode: config.store.mode.as_str(),
            max_payload_bytes: m.max_payload_bytes,
            max_files: m.max_files,
            max_manifest_bytes: m.max_manifest_bytes,
            pbkdf2_iterations: m.pbkdf2_iterations,
            min_ttl_seconds: m.min_ttl.as_secs(),
            max_ttl_seconds: m.max_ttl.as_secs(),
            default_ttl_seconds: m.default_ttl.as_secs(),
            ttl_presets: m
                .ttl_presets
                .iter()
                .map(|d| TtlPreset {
                    seconds: d.as_secs(),
                    label: ttl_label(*d),
                })
                .collect(),
        }
    }
}

/// The full application router.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_page))
        .route("/message/{id}", get(message_page))
        .route("/static/{file}", get(static_file))
        .route("/api/config", get(api_config))
        .merge(crate::api::routes(&state))
        .fallback(|| async { ApiError::NotFound })
        .method_not_allowed_fallback(|| async {
            error_response(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed")
        })
        .layer(middleware::from_fn(json_errors))
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(request_log))
        .with_state(state)
}

async fn index_page(State(state): State<Arc<AppState>>) -> Response {
    html(state.index_html.clone())
}

/// Renders for any id without looking it up, so the page reveals nothing about existence.
async fn message_page(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    html(
        MESSAGE_HTML
            .replace("{{client_config_json}}", &state.client_config_json)
            .replace("{{message_id}}", &assets::html_escape(&id)),
    )
}

async fn static_file(Path(file): Path<String>) -> Response {
    match assets::find(&file) {
        Some(asset) => ([(header::CONTENT_TYPE, asset.content_type)], asset.bytes).into_response(),
        None => ApiError::NotFound.into_response(),
    }
}

async fn api_config(State(state): State<Arc<AppState>>) -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        state.client_config_json.clone(),
    )
        .into_response()
}

fn html(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response()
}

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";

/// One log line per request: method, path, status, and duration. Never headers, query
/// strings, or bodies, so PINs and content can't reach the logs (coverage `SRV-logs`).
/// The path holds the message id, which APP.md accepts as public.
async fn request_log(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    tracing::info!(
        %method,
        path,
        status = response.status().as_u16(),
        elapsed_ms = started.elapsed().as_millis(),
        "request"
    );
    response
}

/// Adds the APP.md security headers to every response, errors included.
async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in [
        (header::CACHE_CONTROL, "no-store, max-age=0"),
        (header::PRAGMA, "no-cache"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CONTENT_SECURITY_POLICY, CONTENT_SECURITY_POLICY),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

/// The safety net for invariant 13: any error response that isn't already JSON (a framework
/// rejection, a body-limit error) is replaced with the JSON shape, keeping its status.
async fn json_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"application/json"));
    if (status.is_client_error() || status.is_server_error()) && !is_json {
        return error_response(status, message_for_status(status));
    }
    response
}
