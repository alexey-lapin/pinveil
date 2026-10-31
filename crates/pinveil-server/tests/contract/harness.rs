//! Starts `pinveil-server` on a random loopback port and captures its stderr.

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::{Client, Response};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

pub const BIN: &str = env!("CARGO_BIN_EXE_pinveil-server");

/// A running server, killed on drop.
pub struct Server {
    pub base_url: String,
    pub client: Client,
    stderr: Arc<Mutex<String>>,
    _child: Child,
}

/// Settings every test server gets unless overridden: a pepper (so no warning noise) and
/// generous rate limits (so only rate-limit tests hit them).
const BASE_ENV: &[(&str, &str)] = &[
    ("PINVEIL_PIN__PEPPER", "contract-test-pepper"),
    ("PINVEIL_RATE_LIMIT__CREATE_PER_WINDOW", "1000"),
    ("PINVEIL_RATE_LIMIT__RETRIEVE_PER_WINDOW", "1000"),
];

impl Server {
    pub async fn start() -> Self {
        Self::start_with(&[]).await
    }

    /// Starts with `BASE_ENV` plus `env` (later entries win).
    pub async fn start_with(env: &[(&str, &str)]) -> Self {
        let mut command = Command::new(BIN);
        command.args(["--listen", "127.0.0.1:0"]);
        for (key, value) in BASE_ENV.iter().chain(env) {
            command.env(key, value);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("start pinveil-server");

        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr);
        let mut err_lines = BufReader::new(child.stderr.take().unwrap()).lines();
        tokio::spawn(async move {
            while let Ok(Some(line)) = err_lines.next_line().await {
                let mut out = sink.lock().unwrap();
                out.push_str(&line);
                out.push('\n');
            }
        });

        let mut first_line = String::new();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        tokio::time::timeout(Duration::from_secs(30), stdout.read_line(&mut first_line))
            .await
            .expect("server announced its address in time")
            .expect("read server stdout");
        let addr = first_line
            .trim()
            .strip_prefix("listening on ")
            .unwrap_or_else(|| {
                panic!(
                    "unexpected first line {first_line:?}; stderr: {}",
                    stderr.lock().unwrap()
                )
            });
        Self {
            base_url: format!("http://{addr}"),
            client: Client::new(),
            stderr,
            _child: child,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    pub async fn get(&self, path: &str) -> Response {
        self.client.get(self.url(path)).send().await.expect("GET")
    }

    pub fn stderr(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }
}

/// Runs the binary with `env` and expects it to refuse to start. Returns its stderr.
pub async fn refuses_to_start(env: &[(&str, &str)]) -> String {
    let output = Command::new(BIN)
        .args(["--listen", "127.0.0.1:0"])
        .envs(env.iter().copied())
        .output()
        .await
        .expect("run pinveil-server");
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected a configuration refusal"
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub const SECURITY_HEADERS: &[(&str, &str)] = &[
    ("cache-control", "no-store, max-age=0"),
    ("pragma", "no-cache"),
    ("referrer-policy", "no-referrer"),
    (
        "content-security-policy",
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
    ),
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
];

pub fn assert_security_headers(response: &Response) {
    for (name, value) in SECURITY_HEADERS {
        assert_eq!(
            response.headers().get(*name).and_then(|v| v.to_str().ok()),
            Some(*value),
            "{name} on {} {}",
            response.status(),
            response.url()
        );
    }
}

/// Asserts the status, the security headers, and the single `{"error": "..."}` shape.
/// Returns the message.
pub async fn assert_error(response: Response, status: u16) -> String {
    assert_eq!(response.status().as_u16(), status, "{}", response.url());
    assert_security_headers(&response);
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        content_type.starts_with("application/json"),
        "{content_type}"
    );
    let body: Value = response.json().await.expect("JSON error body");
    let object = body.as_object().expect("error body is an object");
    assert_eq!(object.len(), 1, "{body}");
    object["error"]
        .as_str()
        .expect("error is a string")
        .to_owned()
}
