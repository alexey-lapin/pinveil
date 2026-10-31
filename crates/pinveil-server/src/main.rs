//! `pinveil-server` binary: loads configuration, then serves.

use std::process::ExitCode;

use pinveil_server::config::{self, Sources};

#[tokio::main]
async fn main() -> ExitCode {
    let (config, warnings) = match config::load(&Sources::from_process()) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("pinveil-server: {error}");
            return ExitCode::from(2);
        }
    };
    for warning in warnings {
        eprintln!("pinveil-server: warning: {}", warning.0);
    }
    let listener = match tokio::net::TcpListener::bind(config.listen).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!(
                "pinveil-server: cannot listen on {}: {error}",
                config.listen
            );
            return ExitCode::from(1);
        }
    };
    // Tests read this line to learn the port when started with `--listen 127.0.0.1:0`.
    match listener.local_addr() {
        Ok(addr) => println!("listening on {addr}"),
        Err(error) => {
            eprintln!("pinveil-server: {error}");
            return ExitCode::from(1);
        }
    }
    if let Err(error) = pinveil_server::serve(listener, config).await {
        eprintln!("pinveil-server: {error}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
