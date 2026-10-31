//! Test-only helper: runs a command on a pseudo-terminal and answers its prompts. Never shipped.
//!
//! The browser tests drive the real `pinveil` CLI through this on every target. Node has no
//! pseudo-terminal of its own, and the CLI reads the link and PIN at hidden prompts.
//!
//! ```text
//! tty-run [--stdin FILE] [--stdout FILE] [--answer PROMPT REPLY]... -- CMD ARGS...
//! ```
//!
//! Each `--answer` waits for PROMPT (use text without spaces: Windows conpty may render a
//! space as a cursor move), then types REPLY and Enter, in order. Prints one JSON object:
//! `{"exit": N, "terminal": "...", "plain": "..."}`, where `plain` has escape sequences
//! removed.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::io::{ErrorKind, Read};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use expectrl::process::{Healthcheck, NonBlocking};
use expectrl::{Expect, Session};

const DEADLINE: Duration = Duration::from_mins(1);

struct Options {
    stdin: Option<String>,
    stdout: Option<String>,
    answers: Vec<(String, String)>,
    command: Vec<String>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        stdin: None,
        stdout: None,
        answers: Vec::new(),
        command: Vec::new(),
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--stdin" => {
                options.stdin = args.get(i + 1).cloned();
                i += 2;
            }
            "--stdout" => {
                options.stdout = args.get(i + 1).cloned();
                i += 2;
            }
            "--answer" => {
                let (Some(prompt), Some(reply)) = (args.get(i + 1), args.get(i + 2)) else {
                    return Err("--answer needs PROMPT and REPLY".into());
                };
                options.answers.push((prompt.clone(), reply.clone()));
                i += 3;
            }
            "--" => {
                options.command = args[i + 1..].to_vec();
                break;
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    if options.command.is_empty() {
        return Err("no command after --".into());
    }
    Ok(options)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("tty-run: {error}");
            return ExitCode::from(2);
        }
    };
    let mut command = shell(&options);
    // conpty (Windows) builds the child's environment only from variables set on the
    // command, so pass the inherited ones explicitly.
    command.envs(std::env::vars_os());
    let mut session = match Session::spawn(command) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("tty-run: cannot spawn: {error}");
            return ExitCode::from(2);
        }
    };
    session.set_expect_timeout(Some(DEADLINE));
    let mut shown = Vec::new();
    for (prompt, reply) in &options.answers {
        match session.expect(prompt.as_str()) {
            Ok(found) => {
                shown.extend_from_slice(found.before());
                shown.extend_from_slice(prompt.as_bytes());
            }
            Err(error) => {
                eprintln!("tty-run: {prompt:?} not shown: {error}");
                break;
            }
        }
        if session.send_line(reply).is_err() {
            break;
        }
    }
    shown.extend(drain_until_exit(&mut session));
    let terminal = String::from_utf8_lossy(&shown).into_owned();
    let plain = strip_escapes(&terminal);
    let exit = exit_marker(&plain).unwrap_or(-1);
    println!(
        "{{\"exit\":{exit},\"terminal\":{},\"plain\":{}}}",
        json_string(&terminal),
        json_string(&plain)
    );
    ExitCode::SUCCESS
}

/// The command through the platform shell, with redirections and an exit marker.
#[cfg(unix)]
fn shell(options: &Options) -> Command {
    let mut script = String::from(r#""$0" "$@""#);
    if options.stdin.is_some() {
        script.push_str(r#" < "$TTY_RUN_STDIN""#);
    }
    if options.stdout.is_some() {
        script.push_str(r#" > "$TTY_RUN_STDOUT""#);
    }
    script.push_str("; echo \"[exit $?]\"");
    let mut command = Command::new("sh");
    command.arg("-c").arg(script).args(&options.command);
    if let Some(path) = &options.stdin {
        command.env("TTY_RUN_STDIN", path);
    }
    if let Some(path) = &options.stdout {
        command.env("TTY_RUN_STDOUT", path);
    }
    command
}

/// Under conpty the shell can inherit this process's redirected handles instead of the
/// pseudo-console, so every stream not sent to a file goes to CON explicitly.
#[cfg(windows)]
fn shell(options: &Options) -> Command {
    use std::os::windows::process::CommandExt;
    let quote = |word: &str| format!("\"{word}\"");
    let mut line: Vec<String> = options.command.iter().map(|w| quote(w)).collect();
    let target = |path: &Option<String>| path.as_deref().map_or_else(|| "CON".to_owned(), quote);
    line.push(format!("< {}", target(&options.stdin)));
    line.push(format!("> {}", target(&options.stdout)));
    line.push("2> CON & echo [exit !ERRORLEVEL!] > CON".to_owned());
    let mut command = Command::new("cmd.exe");
    command
        .arg("/V:ON")
        .arg("/C")
        .raw_arg(format!("\"{}\"", line.join(" ")));
    command
}

fn drain_until_exit<P, S>(session: &mut Session<P, S>) -> Vec<u8>
where
    Session<P, S>: Read + NonBlocking + Healthcheck,
{
    let _ = session.set_blocking(false);
    let deadline = Instant::now() + DEADLINE;
    let mut shown = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut exited_at = None;
    while Instant::now() < deadline {
        match session.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => shown.extend_from_slice(&chunk[..n]),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if !session.is_alive().unwrap_or(false) {
                    let at = *exited_at.get_or_insert_with(Instant::now);
                    if at.elapsed() > Duration::from_millis(500) {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
    shown
}

/// The exit code from the `[exit N]` marker, ignoring whitespace conpty may have moved.
fn exit_marker(plain: &str) -> Option<i32> {
    let squashed: String = plain.chars().filter(|c| !c.is_whitespace()).collect();
    let at = squashed.rfind("[exit")?;
    squashed[at + 5..].split(']').next()?.parse().ok()
}

/// Removes CSI and OSC escape sequences.
fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
