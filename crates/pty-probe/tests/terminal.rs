//! Proves the terminal harness works on every CI target (implementation plan, Phase 0 step 6):
//! a hidden prompt on a pseudo-terminal while stdin and stdout are redirected, and a clear
//! failure when there is no terminal at all.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use expectrl::process::NonBlocking;
use expectrl::{Expect, Session};

const PROBE: &str = env!("CARGO_BIN_EXE_pty-probe");
const CONTENT: &[u8] = b"hello probe";
const PIN: &str = "482913";

#[test]
fn hidden_prompt_works_with_redirected_stdin_and_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.txt");
    let output = dir.path().join("out.txt");
    std::fs::write(&input, CONTENT).unwrap();

    let mut session = Session::spawn(shell_redirect(&input, &output)).expect("spawn in a pty");
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    session
        .expect("PIN: ")
        .expect("prompt shown on the terminal");
    session.send_line(PIN).unwrap();

    let written = wait_for_content(&output);
    assert_eq!(
        written.trim(),
        format!("content-bytes={} pin={PIN}", CONTENT.len())
    );

    // Whatever the terminal showed after the prompt must not include the PIN.
    std::thread::sleep(Duration::from_millis(300));
    let mut echoed = Vec::new();
    session.set_blocking(false).unwrap();
    let mut chunk = [0u8; 4096];
    while let Ok(n) = session.read(&mut chunk) {
        if n == 0 {
            break;
        }
        echoed.extend_from_slice(&chunk[..n]);
    }
    assert!(
        !String::from_utf8_lossy(&echoed).contains(PIN),
        "PIN was echoed: {echoed:?}"
    );
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

fn wait_for_content(path: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && text.ends_with('\n')
        {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "probe never wrote {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `PROBE < input > output`, run by the platform shell inside the pseudo-terminal.
#[cfg(unix)]
fn shell_redirect(input: &Path, output: &Path) -> Command {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(r#""$0" < "$1" > "$2""#)
        .arg(PROBE)
        .arg(input)
        .arg(output);
    command
}

#[cfg(windows)]
fn shell_redirect(input: &Path, output: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("cmd.exe");
    command.arg("/C").raw_arg(format!(
        r#"""{PROBE}" < "{}" > "{}"""#,
        input.display(),
        output.display()
    ));
    command
}

/// Starts the command with no controlling terminal (Unix) or no console (Windows).
#[cfg(unix)]
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
