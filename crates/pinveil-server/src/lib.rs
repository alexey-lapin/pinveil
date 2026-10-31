//! `pinveil-server`: stores opaque encrypted blobs for a limited time and serves the web UI.
//! The binary in `main.rs` only loads configuration and runs [`serve`]; everything else lives
//! here so tests can drive the real router in-process.

pub mod api;
pub mod app;
pub mod assets;
pub mod config;
pub mod error;
pub mod pin;
pub mod ratelimit;
pub mod service;
pub mod store;

use std::net::SocketAddr;
use std::sync::Arc;

use pinveil_core::wordlist::Wordlist;

use crate::app::{AppState, router};
use crate::config::Config;

/// How often expired messages are swept (APP.md, "Store modes").
const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_mins(1);

/// Validates startup invariants, then serves on `listener` until shutdown.
pub async fn serve(listener: tokio::net::TcpListener, config: Config) -> std::io::Result<()> {
    // Invariant 12: never serve or use a wordlist that fails validation.
    let wordlist = Wordlist::embedded()
        .map_err(|e| std::io::Error::other(format!("embedded wordlist: {e}")))?;
    let state = Arc::new(AppState::new(config, wordlist));
    let service = Arc::clone(&state.service);
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            ticks.tick().await;
            if service.sweep().is_err() {
                eprintln!("pinveil-server: expiry sweep failed");
            }
        }
    });
    let app = router(state);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

#[cfg(test)]
mod tests {
    /// Dependency smoke test: the bundled `SQLite` compiles and links on every target.
    #[test]
    fn bundled_sqlite_links() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let one: i64 = conn.query_row("select 1", [], |row| row.get(0)).unwrap();
        assert_eq!(one, 1);
    }
}
