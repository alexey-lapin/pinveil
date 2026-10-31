//! Rate limiting and client-address resolution (APP.md, "Rate limiting"; Java-era gap 2).

use std::time::Duration;

use reqwest::Response;

use crate::harness::{Server, assert_error};

/// A create request that fails validation cheaply: it still counts against the bucket.
async fn create(server: &Server, forwarded_for: Option<&str>) -> Response {
    let mut request = server
        .client
        .post(server.url("/api/messages"))
        .header("content-type", "text/plain")
        .body("x");
    if let Some(value) = forwarded_for {
        request = request.header("x-forwarded-for", value);
    }
    request.send().await.unwrap()
}

/// Statuses for one create per forwarded value.
async fn statuses(server: &Server, values: &[Option<&str>]) -> Vec<u16> {
    let mut out = Vec::new();
    for value in values {
        out.push(create(server, *value).await.status().as_u16());
    }
    out
}

fn limited(per_window: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        ("PINVEIL_RATE_LIMIT__CREATE_PER_WINDOW", per_window),
        ("PINVEIL_RATE_LIMIT__RETRIEVE_PER_WINDOW", "100"),
    ]
}

#[tokio::test]
async fn srv_ratelimit_create_bucket_then_429() {
    let server = Server::start_with(&limited("3")).await;
    assert_eq!(
        statuses(&server, &[None, None, None]).await,
        [415, 415, 415]
    );
    assert_eq!(
        assert_error(create(&server, None).await, 429).await,
        "Rate limit exceeded"
    );
    let retrieve = server
        .client
        .get(server.url("/api/messages/x/retrieve"))
        .header("x-message-pin", "123456")
        .send()
        .await
        .unwrap();
    assert_eq!(retrieve.status(), 404, "the retrieve bucket is separate");
}

#[tokio::test]
async fn srv_ratelimit_spoofed_forwarding_ignored_without_trusted_proxies() {
    // The Java version let 14 creates through against a limit of 10 this way.
    let server = Server::start_with(&limited("3")).await;
    let spoofed: Vec<String> = (1..=6).map(|i| format!("10.0.0.{i}")).collect();
    let values: Vec<Option<&str>> = spoofed.iter().map(|s| Some(s.as_str())).collect();
    assert_eq!(
        statuses(&server, &values).await,
        [415, 415, 415, 429, 429, 429]
    );
}

#[tokio::test]
async fn srv_ratelimit_trusted_proxy_walks_right_to_left() {
    let mut env = limited("2");
    env.push(("PINVEIL_SERVER__TRUSTED_PROXIES", "127.0.0.1,10.0.0.0/8"));
    let server = Server::start_with(&env).await;
    // Behind a trusted proxy each real client gets its own bucket.
    assert_eq!(
        statuses(
            &server,
            &[Some("1.1.1.1"), Some("1.1.1.1"), Some("1.1.1.1")]
        )
        .await,
        [415, 415, 429]
    );
    assert_eq!(statuses(&server, &[Some("2.2.2.2")]).await, [415]);
    // A spoofed left-most entry can't pick the bucket: the proxy appended the real 3.3.3.3.
    let spoofed = [
        Some("6.6.6.1, 3.3.3.3"),
        Some("6.6.6.2, 3.3.3.3"),
        Some("6.6.6.3, 3.3.3.3"),
    ];
    assert_eq!(statuses(&server, &spoofed).await, [415, 415, 429]);
    // Trusted hops are skipped: 4.4.4.4 came through an internal proxy.
    assert_eq!(
        statuses(
            &server,
            &[
                Some("4.4.4.4, 10.0.0.5"),
                Some("4.4.4.4"),
                Some("4.4.4.4, 10.1.1.1")
            ]
        )
        .await,
        [415, 415, 429]
    );
}

#[tokio::test]
async fn srv_ratelimit_malformed_forwarding_falls_back_to_the_peer() {
    let mut env = limited("2");
    env.push(("PINVEIL_SERVER__TRUSTED_PROXIES", "127.0.0.1"));
    let server = Server::start_with(&env).await;
    // Padding goes inside the value: HTTP strips leading and trailing whitespace.
    let over_long = format!("1.1.1.1,{}2.2.2.2", " ".repeat(1100));
    let too_many = vec!["1.1.1.1"; 40].join(",");
    // All of these count against the peer, 127.0.0.1, so the third is already limited.
    let values = [
        Some("not-an-ip"),
        Some(over_long.as_str()),
        Some(too_many.as_str()),
        None,
    ];
    assert_eq!(statuses(&server, &values).await, [415, 415, 429, 429]);
}

#[tokio::test]
async fn srv_ratelimit_key_cap_refuses_new_addresses() {
    let mut env = limited("5");
    env.push(("PINVEIL_SERVER__TRUSTED_PROXIES", "127.0.0.1"));
    env.push(("PINVEIL_RATE_LIMIT__MAX_TRACKED_KEYS", "2"));
    let server = Server::start_with(&env).await;
    assert_eq!(
        statuses(
            &server,
            &[
                Some("1.1.1.1"),
                Some("2.2.2.2"),
                Some("3.3.3.3"),
                Some("1.1.1.1")
            ]
        )
        .await,
        [415, 415, 429, 415]
    );
}

#[tokio::test]
async fn srv_ratelimit_window_resets() {
    let mut env = limited("1");
    env.push(("PINVEIL_RATE_LIMIT__WINDOW", "1s"));
    let server = Server::start_with(&env).await;
    assert_eq!(statuses(&server, &[None, None]).await, [415, 429]);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(statuses(&server, &[None]).await, [415]);
}
