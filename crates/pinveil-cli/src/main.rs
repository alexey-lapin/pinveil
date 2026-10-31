//! `pinveil`: the command-line client (APP.md, "CLI").

#![forbid(unsafe_code)]

mod args;
mod display;
mod editor;
mod error;
mod get;
mod http;
mod links;
mod output;
mod profile;
mod send;
mod term;

use std::process::ExitCode;

use args::Command;

fn main() -> ExitCode {
    let command_line: Vec<String> = std::env::args().collect();
    let result = args::parse(&command_line).and_then(|command| match command {
        Command::Version => {
            println!(
                "pinveil {} (protocol {})",
                env!("CARGO_PKG_VERSION"),
                pinveil_core::PROTOCOL_VERSION
            );
            Ok(())
        }
        Command::Help(text) => {
            println!("{text}");
            Ok(())
        }
        Command::Send(args) => send::run(&args),
        Command::Get(args) => get::run(&args),
        Command::Profile(args) => profile::run(args),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pinveil: {error}");
            error.exit_code()
        }
    }
}

/// What the server's store mode means for users (APP.md invariant 10).
pub(crate) fn store_notice(config: &http::ServerConfig) -> &'static str {
    if config.store_mode == "sqlite" {
        "Note: this server stores encrypted messages on disk, so they survive a restart. Deleted messages may stay recoverable on that disk."
    } else {
        "Note: this server keeps messages in memory only. A server restart deletes every message."
    }
}
