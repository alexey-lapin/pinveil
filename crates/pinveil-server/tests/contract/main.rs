//! Black-box HTTP contract suite (APP.md "HTTP interface").
//!
//! Phase 0 holds only the harness probe: it proves this registered test target starts the real
//! `pinveil-server` binary and reaches it over HTTP. Phase 2 grows the suite from here.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// A running `pinveil-server` on a random loopback port, killed on drop.
struct Server {
    base_url: String,
    _child: Child,
}

impl Server {
    async fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_pinveil-server"))
            .args(["--listen", "127.0.0.1:0"])
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("start pinveil-server");
        let stdout = child.stdout.take().expect("server stdout");
        let mut first_line = String::new();
        tokio::time::timeout(
            Duration::from_secs(30),
            BufReader::new(stdout).read_line(&mut first_line),
        )
        .await
        .expect("server announced its address in time")
        .expect("read server stdout");
        let addr = first_line
            .trim()
            .strip_prefix("listening on ")
            .unwrap_or_else(|| panic!("unexpected first line: {first_line:?}"));
        Self {
            base_url: format!("http://{addr}"),
            _child: child,
        }
    }
}

#[tokio::test]
async fn harness_reaches_the_real_server_binary() {
    let server = Server::start().await;
    let response = reqwest::get(format!("{}/", server.base_url))
        .await
        .expect("GET /");
    assert_eq!(response.status(), 200);
    assert!(response.text().await.unwrap().contains("pinveil-server"));
}
