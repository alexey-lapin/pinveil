//! Runs the real `pinveil` binary against a real server started in-process, behind a TCP
//! proxy that records every byte the CLI sends. Interactive runs use a pseudo-terminal
//! (expectrl), the same harness the Phase 0 probe proved on every target.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use expectrl::process::{Healthcheck, NonBlocking};
use expectrl::session::OsSession;
use expectrl::{Expect, Session};

pub const CLI: &str = env!("CARGO_BIN_EXE_pinveil");
pub const DEADLINE: Duration = Duration::from_mins(1);

/// A server on a loopback port, reached through a recording proxy.
pub struct Server {
    /// The proxy's URL, which every test talks to.
    pub url: String,
    recorded: Arc<Mutex<Vec<u8>>>,
}

impl Server {
    pub fn start() -> Self {
        Self::start_with(&[])
    }

    pub fn start_with(env: &[(&str, &str)]) -> Self {
        let upstream = start_upstream(env);
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", proxy.local_addr().unwrap());
        let sink = Arc::clone(&recorded);
        std::thread::spawn(move || {
            for client in proxy.incoming().flatten() {
                let Ok(server) = TcpStream::connect(upstream) else {
                    continue;
                };
                pipe(
                    client.try_clone().unwrap(),
                    server.try_clone().unwrap(),
                    Some(Arc::clone(&sink)),
                );
                pipe(server, client, None);
            }
        });
        Self { url, recorded }
    }

    /// Everything clients sent so far, as one byte string.
    pub fn sent(&self) -> Vec<u8> {
        self.recorded.lock().unwrap().clone()
    }
}

/// Copies `from` to `to` on a thread, appending what passes to `record`.
fn pipe(mut from: TcpStream, mut to: TcpStream, record: Option<Arc<Mutex<Vec<u8>>>>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match from.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Some(record) = &record {
                        record.lock().unwrap().extend_from_slice(&buf[..n]);
                    }
                    if to.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            }
        }
        let _ = to.shutdown(Shutdown::Write);
    });
}

/// Starts `pinveil_server::serve` on its own runtime thread. Returns its address.
fn start_upstream(env: &[(&str, &str)]) -> std::net::SocketAddr {
    let mut vars: Vec<(String, String)> = vec![
        ("PINVEIL_PIN__PEPPER".into(), "cli-test-pepper".into()),
        (
            "PINVEIL_RATE_LIMIT__CREATE_PER_WINDOW".into(),
            "10000".into(),
        ),
        (
            "PINVEIL_RATE_LIMIT__RETRIEVE_PER_WINDOW".into(),
            "10000".into(),
        ),
    ];
    vars.extend(env.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
    let sources = pinveil_server::config::Sources {
        args: vec!["--listen".into(), "127.0.0.1:0".into()],
        env: vars,
        read_file: &|_| Err(std::io::Error::other("no config file in tests")),
        cpus: 2,
    };
    let (config, _) = pinveil_server::config::load(&sources).expect("test server config");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            pinveil_server::serve(listener, config).await.unwrap();
        });
    });
    addr
}

/// A scratch directory with a private profile directory, so tests never touch the real one.
pub struct Scratch {
    pub dir: tempfile::TempDir,
}

impl Scratch {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn config_dir(&self) -> PathBuf {
        self.path("config")
    }

    pub fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

/// How an interactive run gets its stdin and where its stdout goes.
#[derive(Default)]
pub struct Io<'a> {
    pub stdin: Option<&'a Path>,
    pub stdout: Option<&'a Path>,
    pub stderr: Option<&'a Path>,
}

/// An interactive `pinveil` run on a pseudo-terminal.
pub struct Tty {
    session: OsSession,
    shown: Vec<u8>,
}

impl Tty {
    pub fn spawn(scratch: &Scratch, args: &[&str], io: &Io<'_>) -> Self {
        Self::spawn_with_env(scratch, args, io, &[])
    }

    pub fn spawn_with_env(
        scratch: &Scratch,
        args: &[&str],
        io: &Io<'_>,
        env: &[(&str, &str)],
    ) -> Self {
        let mut command = shell(args, io);
        // conpty (Windows) builds the child's environment only from variables set on the
        // command, so pass the inherited ones explicitly; without SystemRoot, Winsock fails.
        command.envs(std::env::vars_os());
        command.envs(env.iter().copied());
        command.env("PINVEIL_CONFIG_DIR", scratch.config_dir());
        command.current_dir(scratch.dir.path());
        let mut session = Session::spawn(command).expect("spawn in a pty");
        session.set_expect_timeout(Some(DEADLINE));
        Self {
            session,
            shown: Vec::new(),
        }
    }

    /// Waits for `text` on the terminal. Use text without spaces: Windows `ConPTY` can render
    /// a space as a cursor move, and it never sends trailing blanks.
    pub fn expect(&mut self, text: &str) {
        match self.session.expect(text) {
            Ok(found) => self.shown.extend_from_slice(found.before()),
            Err(error) => panic!(
                "{text:?} not shown ({error}); terminal so far: {}",
                self.shown()
            ),
        }
    }

    pub fn send_line(&mut self, line: &str) {
        self.session.send_line(line).unwrap();
    }

    /// Reads the terminal until the process exits. Returns everything it showed.
    pub fn finish(mut self) -> String {
        let rest = drain_until_exit(&mut self.session);
        self.shown.extend_from_slice(&rest);
        self.shown()
    }

    fn shown(&self) -> String {
        String::from_utf8_lossy(&self.shown).into_owned()
    }
}

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
        assert!(Instant::now() < deadline, "pinveil did not finish in time");
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
            Err(_) if !session.is_alive().unwrap_or(false) => break,
            Err(error) => panic!("reading the terminal failed: {error}"),
        }
    }
    shown
}

/// `pinveil args...` through the platform shell, with optional redirections, and the exit
/// code written after it so tests can read it from the terminal.
#[cfg(unix)]
fn shell(args: &[&str], io: &Io<'_>) -> Command {
    let mut script = String::from(r#""$0" "$@""#);
    if let Some(path) = io.stdin {
        let _ = write!(script, " < '{}'", path.display());
    }
    if let Some(path) = io.stdout {
        let _ = write!(script, " > '{}'", path.display());
    }
    if let Some(path) = io.stderr {
        let _ = write!(script, " 2> '{}'", path.display());
    }
    script.push_str("; echo \"[exit $?]\"");
    let mut command = Command::new("sh");
    command.arg("-c").arg(script).arg(CLI).args(args);
    command
}

#[cfg(windows)]
fn shell(args: &[&str], io: &Io<'_>) -> Command {
    use std::os::windows::process::CommandExt;
    let mut line = format!(r#""{CLI}""#);
    for arg in args {
        let _ = write!(line, r#" "{arg}""#);
    }
    // Under conpty the shell can inherit the test process's redirected handles instead of
    // the pseudo-console, so every stream not sent to a file goes to CON explicitly.
    let target = |path: Option<&Path>| {
        path.map_or_else(|| "CON".to_owned(), |p| format!(r#""{}""#, p.display()))
    };
    let _ = write!(
        line,
        " < {} > {} 2> {}",
        target(io.stdin),
        target(io.stdout),
        target(io.stderr)
    );
    line.push_str(" & echo [exit !ERRORLEVEL!] > CON");
    let mut command = Command::new("cmd.exe");
    command
        .arg("/V:ON")
        .arg("/C")
        .raw_arg(format!(r#""{line}""#));
    command
}

/// Runs `pinveil args...` with no terminal at all, stdin from `stdin`.
pub fn run_detached(scratch: &Scratch, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = detached(Command::new(CLI))
        .args(args)
        .env("PINVEIL_CONFIG_DIR", scratch.config_dir())
        .current_dir(scratch.dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The CLI may exit before reading stdin (a usage error); a broken pipe is fine then.
    let _ = child.stdin.take().unwrap().write_all(stdin);
    child.wait_with_output().unwrap()
}

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

/// The `[exit N]` marker the shell wrapper prints.
pub fn exit_code(shown: &str) -> i32 {
    let plain = squash(shown);
    let at = plain
        .rfind("[exit")
        .unwrap_or_else(|| panic!("no exit marker shown: {plain:?}"));
    plain[at + 5..].split(']').next().unwrap().parse().unwrap()
}

/// Terminal text with escape sequences and all whitespace removed. Windows conpty re-renders
/// the screen and can replace spaces with cursor moves, so terminal assertions compare
/// squashed text on both sides.
pub fn squash(text: &str) -> String {
    strip_escapes(text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// Whether the terminal showed `text`, ignoring escapes and whitespace.
pub fn shows(shown: &str, text: &str) -> bool {
    squash(shown).contains(&squash(text))
}

/// Whether `haystack` contains `needle`.
pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Sends a message interactively: `stdin` is the text (or none), `files` are arguments.
/// Returns the link and the generated PIN.
pub fn send(
    server: &Server,
    scratch: &Scratch,
    text: Option<&[u8]>,
    files: &[&Path],
) -> (String, String) {
    let link_file = scratch.path(&format!("link-{}.txt", rand_suffix()));
    let input = text.map(|text| scratch.write(&format!("in-{}.txt", rand_suffix()), text));
    let file_args: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    let mut args = vec!["send", "--server", &server.url, "--insecure-http"];
    args.extend(file_args.iter().map(String::as_str));
    let empty = scratch.write(&format!("empty-{}.txt", rand_suffix()), b"");
    let errors = scratch.path(&format!("stderr-{}.txt", rand_suffix()));
    let tty = Tty::spawn(
        scratch,
        &args,
        &Io {
            stdin: Some(input.as_deref().unwrap_or(&empty)),
            stdout: Some(&link_file),
            stderr: Some(&errors),
        },
    );
    let shown = tty.finish();
    let pin = pin_shown(&shown);
    let errors = std::fs::read_to_string(&errors).unwrap();
    assert_eq!(exit_code(&shown), 0, "send failed: {shown}{errors}");
    let link = std::fs::read_to_string(&link_file)
        .unwrap()
        .trim()
        .to_owned();
    // The generated PIN goes to the terminal only, never to a redirectable stream.
    assert!(
        !link.contains(&pin) && !errors.contains(&pin),
        "PIN leaked to stdout or stderr"
    );
    assert!(
        errors.contains("Expires at"),
        "status goes to stderr: {errors}"
    );
    (link, pin)
}

fn rand_suffix() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed).to_string()
}

/// A fake server that answers every request with `response` and records what it received.
pub fn fake_server(response: &'static str) -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&recorded);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 16 * 1024];
            if let Ok(n) = stream.read(&mut buf) {
                sink.lock().unwrap().extend_from_slice(&buf[..n]);
            }
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.shutdown(Shutdown::Both);
        }
    });
    (url, recorded)
}

/// The generated PIN the terminal showed. Terminal escape sequences are removed first:
/// Windows conpty re-renders output and can put cursor sequences between the words.
pub fn pin_shown(shown: &str) -> String {
    let plain = squash(shown);
    let at = plain
        .find("PIN:")
        .unwrap_or_else(|| panic!("no PIN shown: {plain:?}"));
    let digits: String = plain[at + 4..].chars().take(6).collect();
    assert!(
        digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_digit()),
        "no PIN shown: {plain:?}"
    );
    digits
}

/// Removes CSI (`ESC [ ... final`) and OSC (`ESC ] ... BEL` or `ESC \\`) sequences.
pub fn strip_escapes(text: &str) -> String {
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
