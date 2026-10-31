//! Log redaction (coverage `SRV-logs`): no PIN, header value, or body content in logs.

use std::time::Duration;

use reqwest::multipart::{Form, Part};
use serde_json::Value;

use crate::harness::Server;

const PIN: &str = "246813";
const BODY_SENTINEL: &str = "BODY-SENTINEL-7c1e";
const HEADER_SENTINEL: &str = "203.0.113.77";

#[tokio::test]
async fn srv_logs_never_contain_pins_headers_or_bodies() {
    let server = Server::start_with(&[("PINVEIL_SERVER__TRUSTED_PROXIES", "127.0.0.1")]).await;
    let form = Form::new()
        .part(
            "blob",
            Part::bytes(BODY_SENTINEL.as_bytes().to_vec()).file_name("blob.bin"),
        )
        .text("pin", PIN)
        .text("ttl", "600");
    let created: Value = server
        .client
        .post(server.url("/api/messages"))
        .header("x-forwarded-for", HEADER_SENTINEL)
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    for pin in ["000000", PIN] {
        server
            .client
            .get(server.url(&format!("/api/messages/{id}/retrieve?pin={PIN}")))
            .header("x-message-pin", pin)
            .header("x-forwarded-for", HEADER_SENTINEL)
            .send()
            .await
            .unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let logs = server.stderr();
    assert!(
        logs.matches("request").count() >= 3,
        "requests are logged: {logs}"
    );
    assert!(logs.contains("/api/messages"), "{logs}");
    for secret in [PIN, BODY_SENTINEL, HEADER_SENTINEL, "x-message-pin", "pin="] {
        assert!(
            !logs.to_lowercase().contains(&secret.to_lowercase()),
            "{secret:?} leaked into the logs:\n{logs}"
        );
    }
}
