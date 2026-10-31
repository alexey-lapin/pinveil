//! Pages, static assets, `/api/config`, headers, and framework errors.

use serde_json::Value;

use crate::harness::{Server, assert_error, assert_security_headers, refuses_to_start};

const CLIENT_CONFIG_KEYS: &[&str] = &[
    "defaultTtlSeconds",
    "maxFiles",
    "maxManifestBytes",
    "maxPayloadBytes",
    "maxTtlSeconds",
    "minTtlSeconds",
    "pbkdf2Iterations",
    "storeMode",
    "ttlPresets",
];

fn embedded_config(html: &str) -> Value {
    const OPEN: &str = r#"<script id="app-config" type="application/json">"#;
    let start = html.find(OPEN).expect("config script") + OPEN.len();
    let end = start + html[start..].find("</script>").unwrap();
    serde_json::from_str(&html[start..end]).expect("embedded config is JSON")
}

#[tokio::test]
async fn http_pages_and_config_match_and_are_allowlisted() {
    let server = Server::start_with(&[("PINVEIL_PIN__PEPPER", "pepper-sentinel-7f3a")]).await;
    let page = server.get("/").await;
    assert_eq!(page.status(), 200);
    assert_security_headers(&page);
    assert!(
        page.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    let html = page.text().await.unwrap();
    let from_page = embedded_config(&html);

    let api = server.get("/api/config").await;
    assert_eq!(api.status(), 200);
    assert_security_headers(&api);
    let text = api.text().await.unwrap();
    let from_api: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(from_page, from_api);
    let mut keys: Vec<_> = from_api.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, CLIENT_CONFIG_KEYS, "HTTP-config allowlist");
    assert_eq!(from_api["storeMode"], "memory");
    assert_eq!(from_api["maxPayloadBytes"], 26_214_400);
    assert_eq!(from_api["ttlPresets"][0]["label"], "15 minutes");
    assert!(!text.contains("pepper-sentinel-7f3a") && !html.contains("pepper-sentinel-7f3a"));
}

#[tokio::test]
async fn http_pages_render_for_any_id_with_the_id_escaped() {
    let server = Server::start().await;
    let one = server
        .get("/message/never-existed-123456")
        .await
        .text()
        .await
        .unwrap();
    let two = server
        .get("/message/a%22%3E%3Cscript%3Ex")
        .await
        .text()
        .await
        .unwrap();
    assert!(one.contains(r#"data-message-id="never-existed-123456""#));
    assert!(
        two.contains(r#"data-message-id="a&quot;&gt;&lt;script&gt;x""#),
        "{two}"
    );
    assert!(!two.contains(r#"a"><script>x"#));
    assert_eq!(
        one.replace("never-existed-123456", "ID"),
        two.replace("a&quot;&gt;&lt;script&gt;x", "ID"),
        "pages differ only by the escaped id"
    );
}

#[tokio::test]
async fn inv_11_pages_load_only_same_origin_assets() {
    let server = Server::start().await;
    for path in ["/", "/message/x"] {
        let html = server.get(path).await.text().await.unwrap();
        for attr in [r#"src=""#, r#"href=""#] {
            for (i, _) in html.match_indices(attr) {
                let value = &html[i + attr.len()..];
                assert!(
                    value.starts_with('/') && !value.starts_with("//"),
                    "{path}: {}",
                    &value[..40.min(value.len())]
                );
            }
        }
    }
}

#[tokio::test]
async fn http_static_assets_and_wordlist() {
    let server = Server::start().await;
    let js = server.get("/static/crypto.js").await;
    assert_eq!(js.status(), 200);
    assert_security_headers(&js);
    assert_eq!(
        js.headers()["content-type"],
        "text/javascript; charset=utf-8"
    );
    let words = server
        .get("/static/eff_large_wordlist.txt")
        .await
        .bytes()
        .await
        .unwrap();
    let on_disk = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/eff_large_wordlist.txt"
    ))
    .unwrap();
    assert_eq!(words.as_ref(), on_disk.as_slice());
    assert_error(server.get("/static/missing.js").await, 404).await;
    assert_error(server.get("/static/..%2FCargo.toml").await, 404).await;
}

#[tokio::test]
async fn inv_13_framework_errors_use_the_json_shape() {
    let server = Server::start().await;
    assert_eq!(
        assert_error(server.get("/nope").await, 404).await,
        "Not found"
    );
    assert_eq!(
        assert_error(server.get("/api/nope").await, 404).await,
        "Not found"
    );
    let post_page = server.client.post(server.url("/")).send().await.unwrap();
    assert_eq!(assert_error(post_page, 405).await, "Method not allowed");
    let put_config = server
        .client
        .put(server.url("/api/config"))
        .send()
        .await
        .unwrap();
    assert_eq!(assert_error(put_config, 405).await, "Method not allowed");
}

#[tokio::test]
async fn cfg_validate_refuses_to_start_and_warns() {
    let stderr = refuses_to_start(&[("PINVEIL_MESSAGES__PBKDF2_ITERATIONS", "1000")]).await;
    assert!(stderr.contains("pbkdf2_iterations"), "{stderr}");
    let stderr = refuses_to_start(&[("PINVEIL_NOT__A_SETTING", "x")]).await;
    assert!(stderr.contains("not.a_setting"), "{stderr}");

    let server = Server::start_with(&[("PINVEIL_PIN__PEPPER", "")]).await;
    server.get("/").await;
    assert!(
        server.stderr().contains("pin.pepper is empty"),
        "CFG-pepper-warn: {}",
        server.stderr()
    );
}
