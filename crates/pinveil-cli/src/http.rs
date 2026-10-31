//! Talking to a pinveil server (APP.md, "HTTP"): rustls with the platform verifier, no
//! redirects, HTTPS only unless `--insecure-http` let a loopback IP through. Request URLs are
//! built from a [`ServerUrl`] and a message id, so they can never carry a fragment.

use std::time::Duration;

use pinveil_core::envelope::is_accepted_kdf_iterations;
use pinveil_core::payload::CreationLimits;
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response, multipart};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::error::{CliError, ExitClass};
use crate::links::ServerUrl;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Generous: a 25 MiB upload on a slow link.
const REQUEST_TIMEOUT: Duration = Duration::from_mins(10);

/// The client config a server advertises (APP.md, "Client config"). Unknown fields are
/// ignored, so a newer server stays usable.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub store_mode: String,
    pub max_payload_bytes: u64,
    pub max_files: usize,
    pub max_manifest_bytes: usize,
    pub pbkdf2_iterations: u32,
    pub min_ttl_seconds: u64,
    pub max_ttl_seconds: u64,
    pub default_ttl_seconds: u64,
}

impl ServerConfig {
    /// The values must make sense before anything is encrypted or uploaded against them.
    fn validate(&self) -> Result<(), String> {
        if !matches!(self.store_mode.as_str(), "memory" | "sqlite") {
            return Err(format!("unknown store mode {:?}", self.store_mode));
        }
        if self.max_payload_bytes == 0 {
            return Err("maxPayloadBytes is zero".into());
        }
        if CreationLimits::new(self.max_files, self.max_manifest_bytes).is_none() {
            return Err("maxFiles or maxManifestBytes is outside the protocol ceilings".into());
        }
        if !is_accepted_kdf_iterations(self.pbkdf2_iterations) {
            return Err("pbkdf2Iterations is outside the accepted range".into());
        }
        if !(1..=self.default_ttl_seconds).contains(&self.min_ttl_seconds)
            || self.default_ttl_seconds > self.max_ttl_seconds
        {
            return Err("the TTL range is inconsistent".into());
        }
        Ok(())
    }

    #[must_use]
    pub fn limits(&self) -> CreationLimits {
        CreationLimits::new(self.max_files, self.max_manifest_bytes).expect("validated")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub id: String,
    pub expires_at: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

/// What a retrieve attempt came back with.
pub enum Retrieved {
    Blob(Zeroizing<Vec<u8>>),
    WrongPin,
}

pub struct Api {
    client: Client,
    server: ServerUrl,
}

impl Api {
    pub fn new(server: ServerUrl) -> Result<Self, CliError> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("pinveil/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| CliError::new(ExitClass::Internal, format!("cannot start HTTP: {e}")))?;
        Ok(Self { client, server })
    }

    #[must_use]
    pub fn server(&self) -> &ServerUrl {
        &self.server
    }

    /// Fetches and validates `/api/config`. Nothing is uploaded unless this succeeds.
    pub fn config(&self) -> Result<ServerConfig, CliError> {
        let response = self.send(self.client.get(self.server.join("api/config")))?;
        if response.status() != StatusCode::OK {
            return Err(self.bad_server(&format!("/api/config answered {}", response.status())));
        }
        let config: ServerConfig = response
            .json()
            .map_err(|_| self.bad_server("/api/config is not a pinveil client config"))?;
        config.validate().map_err(|why| self.bad_server(&why))?;
        Ok(config)
    }

    pub fn create(&self, blob: Vec<u8>, pin: &str, ttl_seconds: u64) -> Result<Created, CliError> {
        // A fixed part filename: the original names exist only inside the ciphertext.
        let blob = multipart::Part::bytes(blob)
            .file_name("blob")
            .mime_str("application/octet-stream")
            .expect("static MIME type");
        let form = multipart::Form::new()
            .part("blob", blob)
            .text("pin", pin.to_owned())
            .text("ttl", ttl_seconds.to_string());
        let response = self.send(
            self.client
                .post(self.server.join("api/messages"))
                .multipart(form),
        )?;
        match response.status() {
            StatusCode::CREATED => response
                .json()
                .map_err(|_| self.bad_server("the create response is not valid")),
            status => Err(rejected(status, response)),
        }
    }

    pub fn retrieve(&self, id: &str, pin: &str) -> Result<Retrieved, CliError> {
        let url = self.server.join(&format!("api/messages/{id}/retrieve"));
        let response = self.send(self.client.get(url).header("X-Message-Pin", pin))?;
        match response.status() {
            StatusCode::OK => {
                // From here the server has deleted the message.
                let bytes = response.bytes().map_err(|_| {
                    CliError::new(
                        ExitClass::Network,
                        "the connection dropped while the message was downloading. The server has already deleted it",
                    )
                })?;
                Ok(Retrieved::Blob(Zeroizing::new(bytes.to_vec())))
            }
            StatusCode::FORBIDDEN => Ok(Retrieved::WrongPin),
            StatusCode::NOT_FOUND => Err(CliError::new(
                ExitClass::Unavailable,
                "this message doesn't exist. It may have been opened already, expired, or been deleted after too many wrong PINs",
            )),
            status => Err(rejected(status, response)),
        }
    }

    fn send(&self, request: reqwest::blocking::RequestBuilder) -> Result<Response, CliError> {
        let response = request.send().map_err(|e| {
            CliError::new(
                ExitClass::Network,
                format!("cannot reach {}: {}", self.server.as_str(), describe(&e)),
            )
        })?;
        if response.status().is_redirection() {
            return Err(CliError::new(
                ExitClass::Network,
                format!(
                    "{} answered with a redirect, which pinveil refuses. Use the server's final URL",
                    self.server.as_str()
                ),
            ));
        }
        Ok(response)
    }

    fn bad_server(&self, why: &str) -> CliError {
        CliError::new(
            ExitClass::Network,
            format!(
                "{} doesn't look like a compatible pinveil server: {why}",
                self.server.as_str()
            ),
        )
    }
}

/// What went wrong with a request, from its cause chain. The request URL itself is left out:
/// it only ever holds the server base and a message id, and the base is already quoted.
fn describe(error: &reqwest::Error) -> String {
    let mut text = if error.is_timeout() {
        "the request timed out".to_owned()
    } else if error.is_connect() {
        "the connection failed".to_owned()
    } else {
        "the request failed".to_owned()
    };
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

fn rejected(status: StatusCode, response: Response) -> CliError {
    let server_says = response
        .json::<ErrorBody>()
        .map(|body| body.error)
        .unwrap_or_default();
    let message = match status {
        StatusCode::TOO_MANY_REQUESTS => {
            "too many requests from this address. Wait a minute and try again".to_owned()
        }
        StatusCode::PAYLOAD_TOO_LARGE => "the server refused the message as too large".to_owned(),
        StatusCode::SERVICE_UNAVAILABLE => {
            format!("the server is busy or full ({server_says}). Try again shortly")
        }
        _ => format!("the server refused the request ({status}): {server_says}"),
    };
    CliError::new(ExitClass::Rejected, message)
}

#[cfg(test)]
mod tests {
    use super::ServerConfig;

    fn config() -> ServerConfig {
        ServerConfig {
            store_mode: "memory".into(),
            max_payload_bytes: 26_214_400,
            max_files: 256,
            max_manifest_bytes: 262_144,
            pbkdf2_iterations: 600_000,
            min_ttl_seconds: 60,
            max_ttl_seconds: 86_400,
            default_ttl_seconds: 900,
        }
    }

    #[test]
    fn cli_http_config_validation() {
        assert!(config().validate().is_ok());
        let broken = [
            ServerConfig {
                store_mode: "disk".into(),
                ..config()
            },
            ServerConfig {
                max_payload_bytes: 0,
                ..config()
            },
            ServerConfig {
                max_files: 257,
                ..config()
            },
            ServerConfig {
                max_manifest_bytes: 0,
                ..config()
            },
            ServerConfig {
                pbkdf2_iterations: 1000,
                ..config()
            },
            ServerConfig {
                min_ttl_seconds: 0,
                ..config()
            },
            ServerConfig {
                default_ttl_seconds: 100_000,
                ..config()
            },
            ServerConfig {
                min_ttl_seconds: 1000,
                ..config()
            },
        ];
        for config in broken {
            assert!(config.validate().is_err(), "{config:?}");
        }
    }
}
