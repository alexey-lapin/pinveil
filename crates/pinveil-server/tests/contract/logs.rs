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
    // Poll until all three request lines have been read from the server's stderr.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while server.stderr().matches("request").count() < 3 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
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

#[tokio::test]
async fn acc_message_ids_appear_in_logs() {
    // Accepted tradeoff (APP.md gap 13): ids are public, so the path in each request line
    // carries the message id.
    let server = Server::start().await;
    let form = Form::new()
        .part("blob", Part::bytes(b"x".to_vec()).file_name("blob"))
        .text("pin", PIN)
        .text("ttl", "600");
    let created: Value = server
        .client
        .post(server.url("/api/messages"))
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    server
        .client
        .get(server.url(&format!("/api/messages/{id}/retrieve")))
        .header("x-message-pin", PIN)
        .send()
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !server.stderr().contains(id) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let logs = server.stderr();
    assert!(
        logs.contains(&format!("/api/messages/{id}/retrieve")),
        "{logs}"
    );
}
