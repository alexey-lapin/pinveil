//! The controlling terminal (APP.md, "Secrets and streams"). Secrets are typed at hidden
//! prompts and the generated PIN is shown on the terminal itself, never on stdout or stderr,
//! so redirecting either stream can't capture them. Without a terminal these fail with a
//! usage error that says what was needed.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use zeroize::Zeroizing;

use crate::error::CliError;

#[cfg(unix)]
const TTY_IN: &str = "/dev/tty";
#[cfg(unix)]
const TTY_OUT: &str = "/dev/tty";
#[cfg(windows)]
const TTY_IN: &str = "CONIN$";
#[cfg(windows)]
const TTY_OUT: &str = "CONOUT$";

fn no_terminal(what: &str) -> CliError {
    CliError::usage(format!(
        "pinveil needs a terminal {what}, and there is none. Run it from an interactive terminal"
    ))
}

/// Asks for a secret without echoing it.
pub fn prompt_hidden(prompt: &str, what: &str) -> Result<Zeroizing<String>, CliError> {
    rpassword::prompt_password(prompt)
        .map(Zeroizing::new)
        .map_err(|_| no_terminal(what))
}

/// Writes to the terminal directly.
pub fn console(what: &str) -> Result<File, CliError> {
    OpenOptions::new()
        .write(true)
        .open(TTY_OUT)
        .map_err(|_| no_terminal(what))
}

/// Asks a visible question on the terminal and reads one line of answer.
pub fn ask(question: &str, what: &str) -> Result<String, CliError> {
    let mut out = console(what)?;
    write!(out, "{question}")
        .and_then(|()| out.flush())
        .map_err(|_| no_terminal(what))?;
    let input = File::open(TTY_IN).map_err(|_| no_terminal(what))?;
    let mut line = String::new();
    BufReader::new(input)
        .read_line(&mut line)
        .map_err(|_| no_terminal(what))?;
    Ok(line.trim().to_owned())
}
