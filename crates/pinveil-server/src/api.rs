//! The message API: `POST /api/messages` and `GET /api/messages/{id}/retrieve`.

use std::sync::Arc;

use axum::Router;

use crate::app::AppState;

/// API routes, merged into the application router.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
}
