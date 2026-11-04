//! Server URLs and share links (APP.md, "HTTP" and "Secrets and streams").
//!
//! A share link's fragment is split off as soon as the link is parsed; every request URL is
//! built from the server base without it. No error here quotes the link it was given.

use std::net::IpAddr;

use pinveil_core::id_template::is_valid_id;
use reqwest::Url;
use zeroize::Zeroizing;

use crate::error::CliError;

/// A validated server base URL: no credentials, query, or fragment; the path ends in `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerUrl(Url);

impl ServerUrl {
    /// Parses and checks a server URL. Plain HTTP needs `insecure_http` and a loopback IP
    /// literal host (`127.0.0.0/8` or `[::1]`); `localhost` and other names never qualify.
    pub fn parse(text: &str, insecure_http: bool) -> Result<Self, CliError> {
        let url = Url::parse(text.trim())
            .map_err(|_| CliError::usage("the server URL is not a valid absolute URL"))?;
        Self::from_url(url, insecure_http)
    }

    fn from_url(mut url: Url, insecure_http: bool) -> Result<Self, CliError> {
        if !url.username().is_empty() || url.password().is_some() {
            return Err(CliError::usage(
                "the server URL must not contain a user name or password",
            ));
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(CliError::usage(
                "the server URL must not contain a query string or a fragment",
            ));
        }
        match url.scheme() {
            "https" => {}
            "http" if insecure_http && is_loopback_ip(&url) => {}
            "http" if insecure_http => {
                return Err(CliError::usage(
                    "--insecure-http only allows a loopback IP address such as 127.0.0.1 or [::1], not a host name",
                ));
            }
            "http" => {
                return Err(CliError::usage(
                    "the server must use HTTPS (for local testing against 127.0.0.1 or [::1], add --insecure-http)",
                ));
            }
            _ => return Err(CliError::usage("the server URL must start with https://")),
        }
        if !url.path().ends_with('/') {
            let path = format!("{}/", url.path());
            url.set_path(&path);
        }
        Ok(Self(url))
    }

    /// The URL of an API path such as `api/config` (no leading slash).
    #[must_use]
    pub fn join(&self, path: &str) -> Url {
        self.0.join(path).expect("relative API paths always join")
    }

    /// The share link for a message: the server base, `message/<id>`, and the fragment.
    #[must_use]
    pub fn share_link(&self, id: &str, passphrase: &str) -> Zeroizing<String> {
        Zeroizing::new(format!("{}message/{id}#{passphrase}", self.0))
    }

    /// The base URL as text, for status lines. Never contains secrets.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

fn is_loopback_ip(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
        _ => false,
    }
}

/// A parsed share link: where the message lives, and its passphrase.
pub struct ShareLink {
    pub server: ServerUrl,
    pub id: String,
    pub passphrase: Zeroizing<String>,
}

impl ShareLink {
    /// Parses `https://host[/prefix]/message/<id>#<passphrase>`.
    pub fn parse(text: &str, insecure_http: bool) -> Result<Self, CliError> {
        let text = text.trim();
        let (base, fragment) = match text.split_once('#') {
            Some((base, fragment)) if !fragment.is_empty() => (base, fragment),
            _ => {
                return Err(CliError::usage(
                    "the link is incomplete: the part after # is missing. Copy the whole link",
                ));
            }
        };
        let passphrase = Zeroizing::new(fragment.to_owned());
        let mut url =
            Url::parse(base).map_err(|_| CliError::usage("the link is not a valid URL"))?;
        let not_a_link = || CliError::usage("that is not a pinveil message link");
        let (prefix, id) = {
            let path = url.path();
            let (prefix, id) = path.rsplit_once("/message/").ok_or_else(not_a_link)?;
            // Any id a server template can produce, not just the default shape, so links
            // keep working when an operator changes the template.
            if !is_valid_id(id) {
                return Err(not_a_link());
            }
            (format!("{prefix}/"), id.to_owned())
        };
        url.set_path(&prefix);
        Ok(Self {
            server: ServerUrl::from_url(url, insecure_http)?,
            id,
            passphrase,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ServerUrl, ShareLink};

    #[test]
    fn inv_1_insecure_http_loopback_only() {
        assert!(ServerUrl::parse("https://pinveil.example", false).is_ok());
        assert!(ServerUrl::parse("http://pinveil.example", false).is_err());
        assert!(ServerUrl::parse("http://pinveil.example", true).is_err());
        assert!(ServerUrl::parse("http://localhost:8080", true).is_err());
        assert!(ServerUrl::parse("http://127.0.0.1:8080", false).is_err());
        assert!(ServerUrl::parse("http://127.0.0.1:8080", true).is_ok());
        assert!(ServerUrl::parse("http://127.9.9.9", true).is_ok());
        assert!(ServerUrl::parse("http://[::1]:9", true).is_ok());
        assert!(ServerUrl::parse("http://10.0.0.1", true).is_err());
        assert!(ServerUrl::parse("ftp://127.0.0.1", true).is_err());
        // --insecure-http never weakens HTTPS.
        assert!(ServerUrl::parse("https://pinveil.example", true).is_ok());
    }

    #[test]
    fn cli_profile_server_urls_reject_credentials_query_and_fragment() {
        for bad in [
            "https://user:pw@pinveil.example",
            "https://user@pinveil.example",
            "https://pinveil.example/?a=1",
            "https://pinveil.example/#x",
            "pinveil.example",
        ] {
            assert!(ServerUrl::parse(bad, false).is_err(), "{bad}");
        }
        let server = ServerUrl::parse("https://pinveil.example/base", false).unwrap();
        assert_eq!(
            server.join("api/config").as_str(),
            "https://pinveil.example/base/api/config"
        );
    }

    #[test]
    fn inv_2_cli_share_links_split_the_fragment_and_errors_never_quote_it() {
        let link = ShareLink::parse(
            "https://pinveil.example/sub/message/a-b-c-123456#cloak-tidal-barcode-unbalance",
            false,
        )
        .unwrap();
        assert_eq!(link.id, "a-b-c-123456");
        assert_eq!(link.passphrase.as_str(), "cloak-tidal-barcode-unbalance");
        assert_eq!(link.server.as_str(), "https://pinveil.example/sub/");
        assert_eq!(
            link.server
                .join("api/messages/a-b-c-123456/retrieve")
                .as_str(),
            "https://pinveil.example/sub/api/messages/a-b-c-123456/retrieve"
        );
        let link = ShareLink::parse(
            "https://pinveil.example/message/Ticket_42.~Xy-z#cloak-tidal",
            false,
        )
        .unwrap();
        assert_eq!(link.id, "Ticket_42.~Xy-z", "case and . _ ~ are kept");
        for bad in [
            "https://pinveil.example/message/a-1",
            "https://pinveil.example/message/a-1#",
            "https://pinveil.example/other/a-1#cloak-tidal",
            "https://pinveil.example/message/a%2Fb#cloak-tidal",
            "https://pinveil.example/message/a b#cloak-tidal",
            "https://pinveil.example/message/a*b#cloak-tidal",
            "http://pinveil.example/message/a-1#cloak-tidal",
            "not a url#cloak-tidal",
        ] {
            let error = ShareLink::parse(bad, false).err().expect(bad);
            assert!(!error.message.contains("cloak"), "{bad}: {}", error.message);
        }
    }
}
