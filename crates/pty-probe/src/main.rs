//! Test-only probe for the terminal harness. Never shipped.
//!
//! It mimics the CLI's hardest terminal case: the content comes from redirected stdin, the PIN
//! comes from a hidden prompt on the controlling terminal, and the result goes to redirected
//! stdout. Without a terminal it exits with code 3 and a clear message.

#![forbid(unsafe_code)]

use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut content = Vec::new();
    if let Err(error) = io::stdin().read_to_end(&mut content) {
        eprintln!("pty-probe: cannot read stdin: {error}");
        return ExitCode::from(4);
    }
    let pin = match rpassword::prompt_password("PIN: ") {
        Ok(pin) => pin,
        Err(error) => {
            eprintln!("pty-probe: no terminal for the PIN prompt: {error}");
            return ExitCode::from(3);
        }
    };
    let mut stdout = io::stdout().lock();
    match writeln!(stdout, "content-bytes={} pin={}", content.len(), pin.trim())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(5),
    }
}
