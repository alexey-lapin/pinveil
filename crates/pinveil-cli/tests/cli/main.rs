//! End-to-end tests of the `pinveil` binary against a real server.

mod harness;

use std::fs;

use harness::{Io, Scratch, Server, Tty, contains, exit_code, fake_server, run_detached, send};

/// Text with a CRLF, an escape sequence, a tab, and non-ASCII: it must come back byte-exact
/// through redirected stdout.
const TRICKY_TEXT: &[u8] = "line one\r\nansi \u{1b}[31mred\u{1b}[0m\ttab ünïcødé 🗝\n".as_bytes();

/// Opens `link` interactively, answering the link and PIN prompts. Returns the terminal.
fn get(
    scratch: &Scratch,
    link: &str,
    pin: &str,
    extra: &[&str],
    stdout: Option<&std::path::Path>,
) -> String {
    let mut args = vec!["get", "--insecure-http"];
    args.extend_from_slice(extra);
    let mut tty = Tty::spawn(
        scratch,
        &args,
        &Io {
            stdin: None,
            stdout,
            ..Io::default()
        },
    );
    tty.expect("Link:");
    tty.send_line(link);
    tty.expect("PIN:");
    tty.send_line(pin);
    tty.finish()
}

#[test]
fn cli_roundtrip_text_and_files_with_byte_exact_stdout() {
    let server = Server::start();
    let scratch = Scratch::new();
    let a = scratch.write("report ünï.txt", b"first file\n");
    let b = scratch.write("empty.bin", b"");
    let (link, pin) = send(&server, &scratch, Some(TRICKY_TEXT), &[&a, &b]);
    assert!(
        link.starts_with(&format!("{}/message/", server.url)),
        "{link}"
    );

    let out = scratch.path("out");
    let text = scratch.path("text.out");
    let shown = get(
        &scratch,
        &link,
        &pin,
        &["--out", out.to_str().unwrap()],
        Some(&text),
    );
    assert_eq!(exit_code(&shown), 0, "{shown}");
    assert_eq!(
        fs::read(&text).unwrap(),
        TRICKY_TEXT,
        "byte-exact through a redirect"
    );
    assert_eq!(
        fs::read(out.join("report ünï.txt")).unwrap(),
        b"first file\n"
    );
    assert_eq!(fs::read(out.join("empty.bin")).unwrap(), b"");
    assert!(!shown.contains(&pin), "the PIN prompt is hidden");
    assert!(
        !shown.contains(link.split('#').nth(1).unwrap()),
        "the link prompt is hidden"
    );
    assert!(shown.contains("memory only"), "store notice: {shown}");
}

#[test]
fn cli_output_terminal_neutralizes_control_sequences_unless_raw() {
    let server = Server::start();
    let scratch = Scratch::new();
    let (link, pin) = send(&server, &scratch, Some(TRICKY_TEXT), &[]);
    let shown = get(&scratch, &link, &pin, &[], None);
    assert_eq!(exit_code(&shown), 0, "{shown}");
    assert!(shown.contains("ansi ^[[31mred^[[0m"), "{shown}");
    assert!(
        !shown.contains("\u{1b}[31m"),
        "raw escape reached the terminal"
    );

    let (link, pin) = send(&server, &scratch, Some(TRICKY_TEXT), &[]);
    let shown = get(&scratch, &link, &pin, &["--raw"], None);
    assert!(shown.contains("\u{1b}[31m"), "--raw passes escapes through");
}

#[test]
fn inv_1_cli_request_capture_has_no_plaintext_names_or_fragment() {
    let server = Server::start();
    let scratch = Scratch::new();
    let secret = scratch.write("pinveil-name-sentinel.csv", b"pinveil-file-sentinel");
    let (link, pin) = send(
        &server,
        &scratch,
        Some(b"pinveil-text-sentinel"),
        &[&secret],
    );
    let shown = get(
        &scratch,
        &link,
        &pin,
        &["--out", scratch.path("o").to_str().unwrap()],
        Some(&scratch.path("t")),
    );
    assert_eq!(exit_code(&shown), 0, "{shown}");

    let sent = server.sent();
    assert!(
        contains(&sent, b"POST /api/messages"),
        "the upload was recorded"
    );
    let passphrase = link.split('#').nth(1).unwrap();
    for needle in [
        "pinveil-text-sentinel",
        "pinveil-file-sentinel",
        "pinveil-name-sentinel",
        "text/csv",
        passphrase,
    ] {
        assert!(
            !contains(&sent, needle.as_bytes()),
            "a request carried {needle:?}"
        );
    }
    // INV-2.cli-requests: the fragment is never part of a request line.
    assert!(contains(&sent, b"filename=\"blob\""));
}

#[test]
fn cli_exit_codes_per_failure_class() {
    let server = Server::start();
    let scratch = Scratch::new();

    // Usage: unknown flag; --edit with redirected stdin.
    let out = run_detached(&scratch, &["send", "--pin", "123456"], b"");
    assert_eq!(out.status.code(), Some(2));
    let out = run_detached(
        &scratch,
        &["send", "--edit", "--server", &server.url, "--insecure-http"],
        b"text",
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--edit"));

    // Network: nothing listens there.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let tty = Tty::spawn(
        &scratch,
        &["send", "--server", &dead, "--insecure-http"],
        &Io {
            stdin: Some(&scratch.write("x.txt", b"x")),
            stdout: None,
            ..Io::default()
        },
    );
    let shown = tty.finish();
    assert_eq!(exit_code(&shown), 3, "{shown}");

    // Unavailable: a well-formed link to a message that doesn't exist.
    let missing = format!(
        "{}/message/no-such-message-123456#cloak-tidal-barcode-unbalance",
        server.url
    );
    let shown = get(&scratch, &missing, "123456", &[], None);
    assert_eq!(exit_code(&shown), 5, "{shown}");
    assert!(!shown.contains("cloak-tidal"));

    // Decryption: right PIN, wrong passphrase; the message is gone afterwards.
    let (link, pin) = send(&server, &scratch, Some(b"x"), &[]);
    let wrong = format!("{}#wrong-words-in-here", link.split('#').next().unwrap());
    let shown = get(&scratch, &wrong, &pin, &[], None);
    assert_eq!(exit_code(&shown), 6, "{shown}");
    assert!(shown.contains("already deleted"));
    let shown = get(&scratch, &link, &pin, &[], None);
    assert_eq!(exit_code(&shown), 5, "{shown}");
}

#[test]
fn cli_get_wrong_pin_asks_again() {
    let server = Server::start();
    let scratch = Scratch::new();
    let (link, pin) = send(&server, &scratch, Some(b"retry"), &[]);
    let wrong = if pin == "000000" { "111111" } else { "000000" };
    let mut tty = Tty::spawn(
        &scratch,
        &["get", "--insecure-http"],
        &Io {
            stdin: None,
            stdout: None,
            ..Io::default()
        },
    );
    tty.expect("Link:");
    tty.send_line(&link);
    tty.expect("PIN:");
    tty.send_line(wrong);
    tty.expect("Wrong PIN");
    tty.expect("PIN:");
    tty.send_line(&pin);
    let shown = tty.finish();
    assert_eq!(exit_code(&shown), 0, "{shown}");
    assert!(shown.contains("retry"));
}

#[test]
fn cli_no_terminal_fails_clearly_before_upload() {
    let server = Server::start();
    let scratch = Scratch::new();
    let out = run_detached(
        &scratch,
        &["send", "--server", &server.url, "--insecure-http"],
        b"text",
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("needs a terminal"));
    assert!(out.stdout.is_empty());
    assert!(
        !contains(&server.sent(), b"POST /api/messages"),
        "nothing was uploaded"
    );

    let out = run_detached(&scratch, &["get", "--insecure-http"], b"");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("needs a terminal"));
}

#[test]
fn cli_output_collision_prompt_keeps_existing_files_on_quit() {
    let server = Server::start();
    let scratch = Scratch::new();
    let file = scratch.write("a.txt", b"new");
    let (link, pin) = send(&server, &scratch, None, &[&file]);
    let out = scratch.path("dest");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("a.txt"), b"old").unwrap();

    let mut tty = Tty::spawn(
        &scratch,
        &["get", "--insecure-http", "--out", out.to_str().unwrap()],
        &Io {
            stdin: None,
            stdout: None,
            ..Io::default()
        },
    );
    tty.expect("Link:");
    tty.send_line(&link);
    tty.expect("PIN:");
    tty.send_line(&pin);
    tty.expect("[q]uit");
    tty.send_line("q");
    let shown = tty.finish();
    assert_eq!(exit_code(&shown), 7, "{shown}");
    assert_eq!(fs::read(out.join("a.txt")).unwrap(), b"old");
}

#[test]
fn cli_profiles_supply_the_server_and_never_store_secrets() {
    let server = Server::start();
    let scratch = Scratch::new();
    let out = run_detached(
        &scratch,
        &[
            "profile",
            "set",
            "local",
            "--server",
            &server.url,
            "--ttl",
            "5m",
            "--default",
        ],
        b"",
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let input = scratch.write("in.txt", b"profile-text-sentinel");
    let link_file = scratch.path("link.txt");
    let tty = Tty::spawn(
        &scratch,
        &["send", "--insecure-http"],
        &Io {
            stdin: Some(&input),
            stdout: Some(&link_file),
            ..Io::default()
        },
    );
    let shown = tty.finish();
    let pin = harness::pin_shown(&shown);
    assert_eq!(exit_code(&shown), 0, "{shown}");
    let link = fs::read_to_string(&link_file).unwrap();
    assert!(
        contains(&server.sent(), b"name=\"ttl\"\r\n\r\n300\r\n"),
        "the profile TTL was used"
    );
    // An option beats the profile.
    let tty = Tty::spawn(
        &scratch,
        &["send", "--insecure-http", "--ttl", "10m"],
        &Io {
            stdin: Some(&input),
            stdout: Some(&scratch.path("link2.txt")),
            ..Io::default()
        },
    );
    let shown = tty.finish();
    assert_eq!(exit_code(&shown), 0, "{shown}");
    assert!(
        contains(&server.sent(), b"name=\"ttl\"\r\n\r\n600\r\n"),
        "--ttl overrides the profile"
    );

    // The profile file holds only the server and TTL.
    let saved = fs::read_to_string(scratch.config_dir().join("profiles.toml")).unwrap();
    for secret in [
        pin.as_str(),
        link.trim(),
        link.split('#').nth(1).unwrap().trim(),
        "profile-text-sentinel",
    ] {
        assert!(!saved.contains(secret), "profiles.toml holds {secret:?}");
    }
    let listed = run_detached(&scratch, &["profile", "list"], b"");
    assert_eq!(String::from_utf8_lossy(&listed.stdout), "local (default)\n");
    let removed = run_detached(&scratch, &["profile", "remove", "local"], b"");
    assert_eq!(removed.status.code(), Some(0));
}

#[test]
fn cli_send_refuses_an_oversized_message_before_upload() {
    let server = Server::start_with(&[("PINVEIL_MESSAGES__MAX_PAYLOAD_BYTES", "4000")]);
    let scratch = Scratch::new();
    let big = scratch.write("big.bin", &[0u8; 5000]);
    let tty = Tty::spawn(
        &scratch,
        &[
            "send",
            "--server",
            &server.url,
            "--insecure-http",
            big.to_str().unwrap(),
        ],
        &Io {
            stdin: Some(&scratch.write("e", b"")),
            stdout: None,
            ..Io::default()
        },
    );
    let shown = tty.finish();
    assert_eq!(exit_code(&shown), 2, "{shown}");
    assert!(shown.contains("once encrypted"), "{shown}");
    assert!(!contains(&server.sent(), b"POST /api/messages"));
}

#[test]
fn cli_send_choose_pin_and_get_with_the_positional_link() {
    let server = Server::start();
    let scratch = Scratch::new();
    let input = scratch.write("in.txt", b"chosen");
    let link_file = scratch.path("link.txt");
    let mut tty = Tty::spawn(
        &scratch,
        &[
            "send",
            "--server",
            &server.url,
            "--insecure-http",
            "--choose-pin",
        ],
        &Io {
            stdin: Some(&input),
            stdout: Some(&link_file),
            ..Io::default()
        },
    );
    tty.expect("PIN (6 digits):");
    tty.send_line("12345");
    tty.expect("six digits");
    tty.expect("PIN (6 digits):");
    tty.send_line("424242");
    let shown = tty.finish();
    assert_eq!(exit_code(&shown), 0, "{shown}");
    assert!(!shown.contains("424242"), "a chosen PIN is never shown");
    let link = fs::read_to_string(&link_file).unwrap();

    // The opt-in positional link skips the link prompt.
    let mut tty = Tty::spawn(
        &scratch,
        &["get", "--insecure-http", link.trim()],
        &Io::default(),
    );
    tty.expect("PIN:");
    tty.send_line("424242");
    let shown = tty.finish();
    assert_eq!(exit_code(&shown), 0, "{shown}");
    assert!(shown.contains("chosen"));
}

#[test]
fn cli_editor_temp_file_is_removed_on_success_and_on_error() {
    let server = Server::start();
    let scratch = Scratch::new();
    let side = scratch.path("editor-saw.txt");
    for (fails, expected_exit) in [(false, 0), (true, 2)] {
        let editor = editor_script(&scratch, &side, fails);
        let command = ["send", "--server", &server.url, "--insecure-http", "--edit"];
        let tty = Tty::spawn_with_env(
            &scratch,
            &command,
            &Io::default(),
            &[("EDITOR", &editor), ("VISUAL", "")],
        );
        let shown = tty.finish();
        assert_eq!(exit_code(&shown), expected_exit, "{shown}");
        let temp = fs::read_to_string(&side).unwrap();
        let temp = temp.trim();
        assert!(!temp.is_empty(), "the editor ran");
        assert!(
            !std::path::Path::new(temp).exists(),
            "temp file {temp} left behind"
        );
        if !fails {
            assert!(contains(&server.sent(), b"POST /api/messages"));
        }
    }
}

/// An editor command that records the file it was given, then writes text into it or fails.
#[cfg(unix)]
fn editor_script(scratch: &Scratch, side: &std::path::Path, fails: bool) -> String {
    let body = if fails {
        format!("echo \"$1\" > '{}'\nexit 1\n", side.display())
    } else {
        format!(
            "echo \"$1\" > '{}'\nprintf 'edited-text' > \"$1\"\n",
            side.display()
        )
    };
    let script = scratch.write(if fails { "fail.sh" } else { "ok.sh" }, body.as_bytes());
    format!("sh {}", script.display())
}

#[cfg(windows)]
fn editor_script(scratch: &Scratch, side: &std::path::Path, fails: bool) -> String {
    let body = if fails {
        format!("@echo %~1> \"{}\"\r\n@exit /b 1\r\n", side.display())
    } else {
        format!(
            "@echo %~1> \"{}\"\r\n@echo edited-text> \"%~1\"\r\n",
            side.display()
        )
    };
    let script = scratch.write(if fails { "fail.cmd" } else { "ok.cmd" }, body.as_bytes());
    format!("cmd /C {}", script.display())
}

#[test]
fn cli_http_redirects_are_refused() {
    let (url, recorded) = fake_server(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let scratch = Scratch::new();
    let tty = Tty::spawn(
        &scratch,
        &["send", "--server", &url, "--insecure-http"],
        &Io {
            stdin: Some(&scratch.write("in.txt", b"x")),
            ..Io::default()
        },
    );
    let shown = tty.finish();
    assert_eq!(exit_code(&shown), 3, "{shown}");
    assert!(shown.contains("redirect"), "{shown}");
    assert!(!contains(&recorded.lock().unwrap(), b"POST"));
}

#[test]
fn cli_http_invalid_config_stops_before_upload() {
    let (url, recorded) = fake_server(concat!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
        r#"{"storeMode":"memory","maxPayloadBytes":1000,"maxFiles":256,"maxManifestBytes":262144,"pbkdf2Iterations":1000,"minTtlSeconds":60,"maxTtlSeconds":86400,"defaultTtlSeconds":900}"#,
    ));
    let scratch = Scratch::new();
    let tty = Tty::spawn(
        &scratch,
        &["send", "--server", &url, "--insecure-http"],
        &Io {
            stdin: Some(&scratch.write("in.txt", b"x")),
            ..Io::default()
        },
    );
    let shown = tty.finish();
    assert_eq!(exit_code(&shown), 3, "{shown}");
    assert!(shown.contains("compatible pinveil server"), "{shown}");
    let seen = recorded.lock().unwrap();
    assert!(contains(&seen, b"GET /api/config") && !contains(&seen, b"POST"));
}
