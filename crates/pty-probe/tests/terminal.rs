//! Proves the terminal harness works on every CI target (implementation plan, Phase 0 step 6):
//! a hidden prompt on a pseudo-terminal while stdin and stdout are redirected, and a clear
//! failure when there is no terminal at all.

#![deny(unsafe_code)]

use std::io::{ErrorKind, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use expectrl::process::{Healthcheck, NonBlocking};
use expectrl::{Expect, Session};

const PROBE: &str = env!("CARGO_BIN_EXE_pty-probe");
const CONTENT: &[u8] = b"hello probe";
const PIN: &str = "482913";
const DEADLINE: Duration = Duration::from_secs(30);

/// How the probe's stdin is supplied: redirected from a file, or through an anonymous pipe.
#[derive(Clone, Copy)]
enum Stdin {
    File,
    Pipe,
}

#[test]
fn hidden_prompt_works_with_stdin_from_a_file() {
    run_prompt_case(Stdin::File);
}

#[test]
fn hidden_prompt_works_with_stdin_from_a_pipe() {
    run_prompt_case(Stdin::Pipe);
}

fn run_prompt_case(stdin: Stdin) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.txt");
    let output = dir.path().join("out.txt");
    std::fs::write(&input, CONTENT).unwrap();

    let mut session = Session::spawn(shell(stdin, &input, &output)).expect("spawn in a pty");
    session.set_expect_timeout(Some(DEADLINE));
    session
        .expect("PIN: ")
        .expect("prompt shown on the terminal");
    session.send_line(PIN).unwrap();

    // Read everything the terminal shows until the shell and probe have exited, so a late
    // echo can't slip past the assertion below.
    let shown = drain_until_exit(&mut session);
    assert!(
        !String::from_utf8_lossy(&shown).contains(PIN),
        "PIN was echoed: {shown:?}"
    );

    let written = std::fs::read_to_string(&output).expect("probe wrote stdout");
    assert_eq!(
        written.trim(),
        format!("content-bytes={} pin={PIN}", CONTENT.len())
    );
}

/// Collects terminal output until the process exits and the stream is quiet or closed.
/// `WouldBlock` means "nothing yet", not "done".
fn drain_until_exit<P, S>(session: &mut Session<P, S>) -> Vec<u8>
where
    Session<P, S>: Read + NonBlocking + Healthcheck,
{
    session.set_blocking(false).unwrap();
    let deadline = Instant::now() + DEADLINE;
    let mut shown = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut exited_at = None;
    loop {
        assert!(Instant::now() < deadline, "probe did not finish in time");
        match session.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => shown.extend_from_slice(&chunk[..n]),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                // After exit, give the terminal a grace period to flush the last output.
                if !session.is_alive().unwrap_or(false) {
                    let at = *exited_at.get_or_insert_with(Instant::now);
                    if at.elapsed() > Duration::from_millis(500) {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            // Unix PTYs report EIO once the child side has closed: that is end of output.
            Err(_) if !session.is_alive().unwrap_or(false) => break,
            Err(error) => panic!("reading the terminal failed: {error}"),
        }
    }
    shown
}

#[test]
fn fails_clearly_without_a_terminal() {
    let output = detached(Command::new(PROBE))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("no terminal"));
    assert!(output.stdout.is_empty());
}

/// Runs the probe through the platform shell inside the pseudo-terminal, with stdout
/// redirected to `output` and stdin coming from `input` as a file or through a pipe.
#[cfg(unix)]
fn shell(stdin: Stdin, input: &Path, output: &Path) -> Command {
    let script = match stdin {
        Stdin::File => r#""$0" < "$1" > "$2""#,
        Stdin::Pipe => r#"cat "$1" | "$0" > "$2""#,
    };
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(script)
        .arg(PROBE)
        .arg(input)
        .arg(output);
    command
}

#[cfg(windows)]
fn shell(stdin: Stdin, input: &Path, output: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let (input, output) = (input.display(), output.display());
    let line = match stdin {
        Stdin::File => format!(r#""{PROBE}" < "{input}" > "{output}""#),
        Stdin::Pipe => format!(r#"type "{input}" | "{PROBE}" > "{output}""#),
    };
    let mut command = Command::new("cmd.exe");
    command.arg("/C").raw_arg(format!(r#""{line}""#));
    command
}

/// Starts the command with no controlling terminal (Unix) or no console (Windows).
#[cfg(unix)]
#[allow(unsafe_code)]
fn detached(mut command: Command) -> Command {
    use std::os::unix::process::CommandExt;
    // SAFETY: setsid is async-signal-safe and only detaches the child from our terminal.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}

#[cfg(windows)]
fn detached(mut command: Command) -> Command {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    command.creation_flags(DETACHED_PROCESS);
    command
}
