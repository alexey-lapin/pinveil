//! `pinveil`: Phase 0 skeleton. Only `--version` exists so far; argument parsing with
//! `usage-rs` arrives in Phase 4.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        println!(
            "pinveil {} (protocol {})",
            env!("CARGO_PKG_VERSION"),
            pinveil_core::PROTOCOL_VERSION
        );
        return ExitCode::SUCCESS;
    }
    eprintln!("usage: pinveil --version");
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    /// Dependency smoke test: reqwest with rustls (aws-lc-rs) builds and links on every target,
    /// configured the way the CLI will use it: HTTPS only, no redirects.
    #[test]
    fn rustls_client_builds() {
        reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("build rustls client");
    }
}
