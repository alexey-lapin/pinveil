//! `pinveil-server`: Phase 0 skeleton. It binds, reports its address, and answers a placeholder page.

use std::net::SocketAddr;

use axum::{Router, routing::get};

const DEFAULT_LISTEN: &str = "0.0.0.0:8080";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listen = listen_arg(std::env::args().skip(1))?;
    let listener = tokio::net::TcpListener::bind(listen).await?;
    // Tests read this line to learn the port when started with `--listen 127.0.0.1:0`.
    println!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app()).await
}

fn app() -> Router {
    Router::new().route("/", get(|| async { "pinveil-server (phase 0 skeleton)" }))
}

fn listen_arg(mut args: impl Iterator<Item = String>) -> std::io::Result<SocketAddr> {
    let value = match (args.next().as_deref(), args.next()) {
        (None, _) => DEFAULT_LISTEN.to_owned(),
        (Some("--listen"), Some(addr)) => addr,
        _ => return Err(invalid("usage: pinveil-server [--listen ADDR]")),
    };
    value
        .parse()
        .map_err(|_| invalid("--listen expects an address such as 127.0.0.1:8080"))
}

fn invalid(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::listen_arg;

    #[test]
    fn listen_defaults_and_parses() {
        assert_eq!(listen_arg(std::iter::empty()).unwrap().port(), 8080);
        let args = ["--listen", "127.0.0.1:0"].map(String::from);
        assert_eq!(listen_arg(args.into_iter()).unwrap().port(), 0);
        assert!(listen_arg(["--bogus".to_owned()].into_iter()).is_err());
    }

    /// Dependency smoke test: the bundled `SQLite` compiles and links on every target.
    #[test]
    fn bundled_sqlite_links() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let one: i64 = conn.query_row("select 1", [], |row| row.get(0)).unwrap();
        assert_eq!(one, 1);
    }
}
