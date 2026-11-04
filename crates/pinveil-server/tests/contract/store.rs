//! Store modes across restarts, with real server processes (APP.md, "Store modes";
//! invariant 10).

use std::path::Path;
use std::time::Duration;

use reqwest::multipart::{Form, Part};
use serde_json::Value;

use crate::harness::{Server, refuses_to_start};

const PIN: &str = "123456";
const WRONG: &str = "000000";

fn sqlite_env(path: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("PINVEIL_STORE__MODE", "sqlite".into()),
        ("PINVEIL_STORE__PATH", path.display().to_string()),
        ("PINVEIL_MESSAGES__MIN_TTL", "1s".into()),
    ]
}

async fn start(env: &[(&'static str, String)]) -> Server {
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Server::start_with(&env).await
}

async fn create(server: &Server, blob: &[u8], ttl: &str) -> String {
    let form = Form::new()
        .part("blob", Part::bytes(blob.to_vec()).file_name("blob"))
        .text("pin", PIN)
        .text("ttl", ttl.to_owned());
    let response = server
        .client
        .post(server.url("/api/messages"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let body: Value = response.json().await.unwrap();
    body["id"].as_str().unwrap().to_owned()
}

async fn retrieve(server: &Server, id: &str, pin: &str) -> (u16, Vec<u8>) {
    let response = server
        .client
        .get(server.url(&format!("/api/messages/{id}/retrieve")))
        .header("X-Message-Pin", pin)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.bytes().await.unwrap().to_vec())
}

#[tokio::test]
async fn srv_ids_old_ids_survive_a_template_change() {
    let dir = tempfile::tempdir().unwrap();
    let env = sqlite_env(&dir.path().join("store.db"));
    let server = start(&env).await;
    let old = create(&server, b"made with the default template", "600").await;
    server.stop().await;

    let mut changed = env.clone();
    changed.push(("PINVEIL_MESSAGE_ID__TEMPLATE", "T_{Alnum:10}".into()));
    let server = start(&changed).await;
    let new = create(&server, b"made with the new template", "600").await;
    assert!(new.starts_with("T_") && new.len() == 12, "{new}");
    assert_eq!(
        retrieve(&server, &old, PIN).await,
        (200, b"made with the default template".to_vec())
    );
    assert_eq!(
        retrieve(&server, &new, PIN).await,
        (200, b"made with the new template".to_vec())
    );
}

#[tokio::test]
async fn inv_10_memory_restart_loses() {
    let env = vec![("PINVEIL_STORE__MODE", "memory".to_owned())];
    let server = start(&env).await;
    let id = create(&server, b"gone after restart", "600").await;
    server.stop().await;
    let server = start(&env).await;
    assert_eq!(retrieve(&server, &id, PIN).await.0, 404);
}

#[tokio::test]
async fn inv_10_sqlite_restart_keeps() {
    let dir = tempfile::tempdir().unwrap();
    let env = sqlite_env(&dir.path().join("store.db"));
    let server = start(&env).await;
    let pending = create(&server, b"pending", "600").await;
    let failed_once = create(&server, b"failed once", "600").await;
    let read = create(&server, b"read", "600").await;
    let burned = create(&server, b"burned", "600").await;
    let expiring = create(&server, b"expiring", "1").await;
    assert_eq!(retrieve(&server, &failed_once, WRONG).await.0, 403);
    assert_eq!(retrieve(&server, &read, PIN).await.0, 200);
    for _ in 0..3 {
        retrieve(&server, &burned, WRONG).await;
    }
    // Killed, not shut down: every commit must already be durable.
    server.stop().await;
    tokio::time::sleep(Duration::from_millis(1200)).await;

    let server = start(&env).await;
    // The startup sweep removes the expired row by itself, before any request could do it
    // (a retrieve would also remove an expired row, so it can't prove the sweep ran).
    let db = rusqlite::Connection::open_with_flags(
        dir.path().join("store.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let left: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE id = ?1",
                [&expiring],
                |r| r.get(0),
            )
            .unwrap();
        if left == 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the startup sweep never removed the expired row"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    drop(db);
    assert_eq!(
        retrieve(&server, &pending, PIN).await,
        (200, b"pending".to_vec())
    );
    // The earlier failure was kept: two more wrong PINs reach the limit of three.
    assert_eq!(retrieve(&server, &failed_once, WRONG).await.0, 403);
    assert_eq!(retrieve(&server, &failed_once, WRONG).await.0, 403);
    assert_eq!(retrieve(&server, &failed_once, PIN).await.0, 404, "burned");
    for id in [&read, &burned, &expiring] {
        assert_eq!(retrieve(&server, id, PIN).await.0, 404, "{id}");
    }
}

#[tokio::test]
async fn sql_lock_second_server_on_the_same_store_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let env = sqlite_env(&path);
    let _first = start(&env).await;
    let path_text = path.display().to_string();
    let stderr = refuses_to_start(&[
        ("PINVEIL_STORE__MODE", "sqlite"),
        ("PINVEIL_STORE__PATH", &path_text),
        ("PINVEIL_PIN__PEPPER", "contract-test-pepper"),
    ])
    .await;
    assert!(stderr.contains("already using"), "{stderr}");
}

#[tokio::test]
async fn sql_pepper_changed_pepper_refuses_to_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let server = start(&sqlite_env(&path)).await;
    create(&server, b"x", "600").await;
    server.stop().await;
    let path_text = path.display().to_string();
    let stderr = refuses_to_start(&[
        ("PINVEIL_STORE__MODE", "sqlite"),
        ("PINVEIL_STORE__PATH", &path_text),
        ("PINVEIL_PIN__PEPPER", "a-different-pepper"),
    ])
    .await;
    assert!(stderr.contains("different pin.pepper"), "{stderr}");
    assert!(
        !stderr.contains("a-different-pepper"),
        "the pepper is never printed"
    );
}
