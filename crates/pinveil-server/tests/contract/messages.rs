//! `POST /api/messages` and `GET /api/messages/{id}/retrieve`.

use std::time::{Duration, SystemTime};

use reqwest::Response;
use reqwest::multipart::{Form, Part};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::harness::{Server, assert_error, assert_security_headers};

const PIN: &str = "123456";

fn form(blob: &[u8], pin: &str, ttl: &str) -> Form {
    Form::new()
        .part("blob", Part::bytes(blob.to_vec()).file_name("blob.bin"))
        .text("pin", pin.to_owned())
        .text("ttl", ttl.to_owned())
}

async fn post(server: &Server, form: Form) -> Response {
    server
        .client
        .post(server.url("/api/messages"))
        .multipart(form)
        .send()
        .await
        .expect("POST")
}

async fn retrieve(server: &Server, id: &str, pin: Option<&str>) -> Response {
    let mut request = server
        .client
        .get(server.url(&format!("/api/messages/{id}/retrieve")));
    if let Some(pin) = pin {
        request = request.header("X-Message-Pin", pin);
    }
    request.send().await.expect("GET retrieve")
}

/// Creates a message and returns its id.
async fn create(server: &Server, blob: &[u8], ttl: &str) -> String {
    let response = post(server, form(blob, PIN, ttl)).await;
    assert_eq!(response.status(), 201, "{:?}", response.text().await);
    let body: Value = response.json().await.unwrap();
    body["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn http_create_and_retrieve_round_trip() {
    let server = Server::start().await;
    let response = post(&server, form(b"opaque blob", PIN, "600")).await;
    assert_eq!(response.status(), 201);
    assert_security_headers(&response);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body.as_object().unwrap().len(), 2, "{body}");
    let id = body["id"].as_str().unwrap();
    let parts: Vec<_> = id.split('-').collect();
    assert!(parts.len() >= 4 && parts.last().unwrap().len() == 6, "{id}");
    let expires =
        humantime::parse_rfc3339(body["expiresAt"].as_str().unwrap()).expect("RFC 3339 UTC");
    let from_now = expires.duration_since(SystemTime::now()).unwrap();
    assert!(
        (Duration::from_secs(590)..=Duration::from_mins(10)).contains(&from_now),
        "{from_now:?}"
    );

    let got = retrieve(&server, id, Some(PIN)).await;
    assert_eq!(got.status(), 200);
    assert_security_headers(&got);
    assert_eq!(got.headers()["content-type"], "application/octet-stream");
    assert_eq!(got.bytes().await.unwrap().as_ref(), b"opaque blob");
    assert_eq!(
        assert_error(retrieve(&server, id, Some(PIN)).await, 404).await,
        "Message unavailable"
    );
}

#[tokio::test]
async fn inv_3_arbitrary_bytes_roundtrip() {
    let server = Server::start().await;
    // Not a valid envelope: the server must neither parse nor reject it.
    let blob: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    let id = create(&server, &blob, "600").await;
    assert_eq!(
        retrieve(&server, &id, Some(PIN))
            .await
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        blob.as_slice()
    );
}

#[tokio::test]
async fn http_create_every_rejection() {
    let server = Server::start().await;
    let cases: Vec<(Form, u16, &str)> = vec![
        (
            Form::new().text("pin", PIN).text("ttl", "600"),
            400,
            "Missing part; expected blob, pin, and ttl",
        ),
        (
            Form::new()
                .part("blob", Part::bytes(b"x".to_vec()))
                .text("ttl", "600"),
            400,
            "Missing part; expected blob, pin, and ttl",
        ),
        (
            Form::new()
                .part("blob", Part::bytes(b"x".to_vec()))
                .text("pin", PIN),
            400,
            "Missing part; expected blob, pin, and ttl",
        ),
        (
            form(b"x", PIN, "600").text("pin", PIN),
            400,
            "A part was sent twice",
        ),
        (
            form(b"x", PIN, "600").text("note", "hi"),
            400,
            "Unexpected part; expected blob, pin, and ttl",
        ),
        (form(b"x", "12345", "600"), 400, "Invalid PIN format"),
        (form(b"x", "12a456", "600"), 400, "Invalid PIN format"),
        (
            form(b"x", PIN, "ten"),
            400,
            "TTL must be a whole number of seconds",
        ),
        (
            form(b"x", PIN, "59"),
            400,
            "TTL is outside the allowed range",
        ),
        (
            form(b"x", PIN, "86401"),
            400,
            "TTL is outside the allowed range",
        ),
        (form(b"", PIN, "600"), 400, "The blob is empty"),
    ];
    for (form, status, message) in cases {
        assert_eq!(
            assert_error(post(&server, form).await, status).await,
            message
        );
    }
    let json = server
        .client
        .post(server.url("/api/messages"))
        .json(&serde_json::json!({"pin": PIN}))
        .send()
        .await
        .unwrap();
    assert_eq!(assert_error(json, 415).await, "Unsupported media type");
    let garbage = server
        .client
        .post(server.url("/api/messages"))
        .header("content-type", "multipart/form-data; boundary=XYZ")
        .body("this is not multipart")
        .send()
        .await
        .unwrap();
    assert_eq!(assert_error(garbage, 400).await, "Malformed multipart body");
}

#[tokio::test]
async fn http_limits_exact_blob_boundary_and_oversize_bodies() {
    let server = Server::start_with(&[("PINVEIL_MESSAGES__MAX_PAYLOAD_BYTES", "1000")]).await;
    create(&server, &[1; 1000], "600").await;
    assert_eq!(
        assert_error(post(&server, form(&[1; 1001], PIN, "600")).await, 413).await,
        "Payload too large"
    );
    let huge = vec![1u8; 1000 + 70_000];
    assert_eq!(
        assert_error(post(&server, form(&huge, PIN, "600")).await, 413).await,
        "Payload too large"
    );
}

#[tokio::test]
async fn http_limits_chunked_oversize_body() {
    let server = Server::start_with(&[("PINVEIL_MESSAGES__MAX_PAYLOAD_BYTES", "1000")]).await;
    let addr = server.base_url.trim_start_matches("http://").to_owned();
    let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let (mut reader, mut writer) = stream.into_split();
    // The server answers 413 and closes while we are still sending, which can reset the
    // connection. Read on a separate task, keeping whatever arrived before any reset.
    let response = tokio::spawn(async move {
        let mut received = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf).await {
            if n == 0 {
                break;
            }
            received.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&received).into_owned()
    });
    let head = format!(
        "POST /api/messages HTTP/1.1\r\nHost: {addr}\r\nContent-Type: multipart/form-data; boundary=B\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    );
    let part_head = "--B\r\nContent-Disposition: form-data; name=\"blob\"; filename=\"b\"\r\n\r\n";
    let mut body = format!("{:x}\r\n{part_head}\r\n", part_head.len()).into_bytes();
    let chunk = vec![b'x'; 16 * 1024];
    for _ in 0..10 {
        body.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        body.extend_from_slice(&chunk);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"0\r\n\r\n");
    if writer.write_all(head.as_bytes()).await.is_ok() {
        let _ = writer.write_all(&body).await;
    }
    let response = tokio::time::timeout(Duration::from_secs(10), response)
        .await
        .unwrap()
        .unwrap();
    assert!(
        response.starts_with("HTTP/1.1 413"),
        "{:?}",
        &response[..response.len().min(200)]
    );
    assert!(
        response.contains(r#"{"error":"Payload too large"}"#),
        "{response}"
    );
}

#[tokio::test]
async fn http_retrieve_pin_rules_and_burn_sequence() {
    let server = Server::start().await;
    let id = create(&server, b"secret", "600").await;
    assert_eq!(
        assert_error(retrieve(&server, &id, None).await, 400).await,
        "Invalid PIN format"
    );
    assert_eq!(
        assert_error(retrieve(&server, &id, Some("12a456")).await, 400).await,
        "Invalid PIN format"
    );
    // Malformed PINs counted nothing: three wrong PINs still give 403, 403, 403.
    for _ in 0..3 {
        assert_eq!(
            assert_error(retrieve(&server, &id, Some("000000")).await, 403).await,
            "Unable to retrieve message"
        );
    }
    assert_eq!(
        assert_error(retrieve(&server, &id, Some(PIN)).await, 404).await,
        "Message unavailable",
        "burned"
    );
}

#[tokio::test]
async fn acc_403_live_id() {
    // Accepted tradeoff (APP.md gap 11): a wrong PIN on a live message is 403, not 404.
    let server = Server::start().await;
    let id = create(&server, b"x", "600").await;
    assert_eq!(retrieve(&server, &id, Some("000000")).await.status(), 403);
    assert_eq!(
        retrieve(&server, "no-such-id-000000", Some("000000"))
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn inv_9_unknown_expired_burned_consumed_equal() {
    let server = Server::start_with(&[
        ("PINVEIL_MESSAGES__MIN_TTL", "1s"),
        ("PINVEIL_PIN__MAX_FAILED_ATTEMPTS", "1"),
    ])
    .await;
    let consumed = create(&server, b"a", "600").await;
    retrieve(&server, &consumed, Some(PIN)).await;
    let burned = create(&server, b"b", "600").await;
    retrieve(&server, &burned, Some("000000")).await;
    let expired = create(&server, b"c", "1").await;
    tokio::time::sleep(Duration::from_millis(1100)).await;

    let mut bodies = Vec::new();
    for id in [
        "never-existed-000000",
        consumed.as_str(),
        burned.as_str(),
        expired.as_str(),
    ] {
        let response = retrieve(&server, id, Some(PIN)).await;
        assert_eq!(response.status(), 404, "{id}");
        bodies.push(response.bytes().await.unwrap());
    }
    assert!(bodies.windows(2).all(|w| w[0] == w[1]), "{bodies:?}");
}

#[tokio::test]
async fn inv_6_parallel_same_id() {
    let server = Server::start().await;
    let id = create(&server, b"only once", "600").await;
    let attempts = (0..10).map(|_| retrieve(&server, &id, Some(PIN)));
    let statuses: Vec<u16> = futures_statuses(attempts).await;
    assert_eq!(
        statuses.iter().filter(|s| **s == 200).count(),
        1,
        "{statuses:?}"
    );
    assert!(
        statuses.iter().all(|s| *s == 200 || *s == 404),
        "{statuses:?}"
    );
}

async fn futures_statuses(
    requests: impl Iterator<Item = impl std::future::Future<Output = Response>>,
) -> Vec<u16> {
    let mut set = Vec::new();
    for request in requests {
        set.push(request);
    }
    let responses = futures_join(set).await;
    responses.iter().map(|r| r.status().as_u16()).collect()
}

async fn futures_join<F: std::future::Future>(futures: Vec<F>) -> Vec<F::Output> {
    // Polls all futures concurrently on this task.
    let mut pinned: Vec<_> = futures.into_iter().map(Box::pin).collect();
    let mut outputs: Vec<Option<F::Output>> = pinned.iter().map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut pending = false;
        for (future, output) in pinned.iter_mut().zip(outputs.iter_mut()) {
            if output.is_none() {
                match future.as_mut().poll(cx) {
                    std::task::Poll::Ready(value) => *output = Some(value),
                    std::task::Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    })
    .await;
    outputs.into_iter().map(Option::unwrap).collect()
}

#[tokio::test]
async fn srv_caps_store_full_is_503() {
    let server = Server::start_with(&[("PINVEIL_STORE__MAX_MESSAGES", "2")]).await;
    create(&server, b"a", "600").await;
    create(&server, b"b", "600").await;
    assert_eq!(
        assert_error(post(&server, form(b"c", PIN, "600")).await, 503).await,
        "Server storage is full"
    );
}

#[tokio::test]
async fn srv_admission_pending_uploads_are_bounded() {
    let server = Server::start_with(&[("PINVEIL_SERVER__MAX_CONCURRENT_UPLOADS", "1")]).await;
    let addr = server.base_url.trim_start_matches("http://").to_owned();
    let head = format!(
        "POST /api/messages HTTP/1.1\r\nHost: {addr}\r\nContent-Type: multipart/form-data; boundary=B\r\nContent-Length: 100000\r\n\r\n--B\r\n"
    );
    // Hold the only upload slot with a request whose body never finishes. A poll can reach
    // the slot first, in which case the slow request is answered 503 and must start over.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut slow: Option<tokio::net::TcpStream> = None;
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the slow upload never held the slot"
        );
        if slow.is_none() {
            let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
            stream.write_all(head.as_bytes()).await.unwrap();
            slow = Some(stream);
        }
        let stream = slow.as_mut().expect("just connected");
        // Anything on the slow socket means it was answered: it lost the race. Collect the
        // status line across partial reads before checking what it says.
        let mut answer = Vec::new();
        let mut buf = [0u8; 64];
        if let Ok(first) =
            tokio::time::timeout(Duration::from_millis(20), stream.read(&mut buf)).await
        {
            if let Ok(n) = first {
                answer.extend_from_slice(&buf[..n]);
                while n > 0 && answer.len() < 12 {
                    match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await
                    {
                        Ok(Ok(m)) if m > 0 => answer.extend_from_slice(&buf[..m]),
                        _ => break,
                    }
                }
            }
            assert!(
                answer.is_empty() || answer.starts_with(b"HTTP/1.1 503"),
                "unexpected answer to the slow upload: {:?}",
                String::from_utf8_lossy(&answer)
            );
            slow = None; // it was answered (or closed); try again
            continue;
        }
        let response = post(&server, form(b"x", PIN, "600")).await;
        if response.status() == 503 {
            assert_eq!(
                assert_error(response, 503).await,
                "Too many uploads in progress, try again shortly"
            );
            break;
        }
    }
    drop(slow);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while post(&server, form(b"x", PIN, "600")).await.status() != 201 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the slot was not released"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn http_retrieve_head_never_consumes_or_counts() {
    let server = Server::start().await;
    let id = create(&server, b"kept", "600").await;
    let head = |pin: &'static str| {
        server
            .client
            .head(server.url(&format!("/api/messages/{id}/retrieve")))
            .header("X-Message-Pin", pin)
            .send()
    };
    for pin in [PIN, "000000", "000000", "000000"] {
        let response = head(pin).await.unwrap();
        assert_eq!(response.status(), 405, "HEAD with {pin}");
        assert_security_headers(&response);
    }
    // Three wrong-PIN HEADs counted nothing and the correct one consumed nothing.
    let got = retrieve(&server, &id, Some(PIN)).await;
    assert_eq!(got.status(), 200);
    assert_eq!(got.bytes().await.unwrap().as_ref(), b"kept");
}

#[tokio::test]
async fn http_limits_non_multipart_oversize_is_413() {
    let server = Server::start_with(&[("PINVEIL_MESSAGES__MAX_PAYLOAD_BYTES", "1000")]).await;
    let small = server
        .client
        .post(server.url("/api/messages"))
        .header("content-type", "text/plain")
        .body("x")
        .send()
        .await
        .unwrap();
    assert_eq!(assert_error(small, 415).await, "Unsupported media type");
    let big = server
        .client
        .post(server.url("/api/messages"))
        .header("content-type", "text/plain")
        .body(vec![b'x'; 70_000])
        .send()
        .await
        .unwrap();
    assert_eq!(assert_error(big, 413).await, "Payload too large");
    // Chunked, with no Content-Length to reject up front.
    let addr = server.base_url.trim_start_matches("http://").to_owned();
    let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let (mut reader, mut writer) = stream.into_split();
    let response = tokio::spawn(async move {
        let mut received = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf).await {
            if n == 0 {
                break;
            }
            received.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&received).into_owned()
    });
    let mut body = format!(
        "POST /api/messages HTTP/1.1\r\nHost: {addr}\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    )
    .into_bytes();
    for _ in 0..10 {
        body.extend_from_slice(b"4000\r\n");
        body.extend_from_slice(&[b'x'; 0x4000]);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"0\r\n\r\n");
    let _ = writer.write_all(&body).await;
    let response = tokio::time::timeout(Duration::from_secs(10), response)
        .await
        .unwrap()
        .unwrap();
    assert!(
        response.starts_with("HTTP/1.1 413"),
        "{:?}",
        &response[..response.len().min(200)]
    );
}

#[tokio::test]
async fn http_create_longest_allowed_ttl_has_a_valid_expiry() {
    let server = Server::start_with(&[("PINVEIL_MESSAGES__MAX_TTL", "3650d")]).await;
    let response = post(&server, form(b"x", PIN, "315360000")).await;
    assert_eq!(response.status(), 201);
    let body: Value = response.json().await.unwrap();
    let expires = body["expiresAt"].as_str().unwrap();
    assert!(humantime::parse_rfc3339(expires).is_ok(), "{expires}");
}
