# Coverage matrix

Maps every APP.md rule to the implementation-plan step that owns it and the tests that prove it.
Steps are written `P<phase>.<step>` and refer to
`docs/plans/2026-10-30-rust-rewrite-implementation-plan.md`.

Status values:

- `planned`: has an owning step, but no test exists yet.
- `local`: the tests exist and pass locally. CI hasn't confirmed them yet. Not done.
- `covered`: every listed test exists and passes in CI.
- `accepted`: a documented tradeoff, asserted by a test instead of fixed.

A step isn't done until each row it owns reads `covered` or `accepted`. Test ids are
`<area>.<case>`. The test name in code contains the id, so `grep` finds it.

## Harness probes (Phase 0)

| Id | What it proves | Test | Status |
|---|---|---|---|
| H-contract | The registered contract suite starts the real `pinveil-server` binary and reaches it | `crates/pinveil-server/tests/contract/main.rs` `harness_reaches_the_real_server_binary` | covered (CI run 41162610508 at 0ff07c4) |
| H-js | The JS runner loads `web/static` modules under Node Web Crypto | `tests/js/crypto.test.js` "INV-4 round trip ..." (replaced `harness.test.js` in Phase 1) | local (macOS; CI pending) |
| H-pty | A hidden prompt works in a PTY while stdin and stdout are redirected, without echoing the input | `crates/pty-probe/tests/terminal.rs` `hidden_prompt_works_with_stdin_from_a_file`, `hidden_prompt_works_with_stdin_from_a_pipe` | covered (CI run 41162610508 at 0ff07c4) |
| H-noterm | Without a terminal, the prompt fails clearly (exit 3) | `crates/pty-probe/tests/terminal.rs` `fails_clearly_without_a_terminal` | covered (CI run 41162610508 at 0ff07c4) |
| H-deps | Bundled SQLite and reqwest with rustls (aws-lc-rs) build and link | `pinveil-server` `bundled_sqlite_links`, `pinveil-cli` `rustls_client_builds` | covered (CI run 41162610508 at 0ff07c4) |

## Invariants

| Id | Rule | Owner | Tests (planned ids) | Status |
|---|---|---|---|---|
| INV-1 | Plaintext, names, and MIME types never leave a client unencrypted. The passphrase never reaches the server. The PIN does, over TLS, and only its verifier is stored. The only TLS exception is `--insecure-http` to loopback. | P1.5, P2.4, P3.4, P4.3, P4.9 | `INV-1.browser-request-capture`, `INV-1.cli-request-capture`, `INV-1.no-raw-pin-stored`, `INV-1.insecure-http-loopback-only` | planned |
| INV-2 | The fragment is never sent, logged, or put in errors | P3.4, P4.1, P4.3, P4.9 | `INV-2.browser-requests`, `INV-2.cli-requests`, `INV-2.cli-error-redaction`, `INV-2.parser-diagnostics-redacted` | planned |
| INV-3 | The server treats blobs as opaque bytes | P2.5 | `INV-3.arbitrary-bytes-roundtrip`, `INV-3.no-format-inspection` | planned |
| INV-4 | The key needs both the passphrase and the PIN | P1.5, P1.7 | `INV-4.wrong-pin-fails`, `INV-4.wrong-passphrase-fails`, `INV-4.vectors` | planned |
| INV-5 | The PIN is stored only as a salted, peppered Argon2id hash | P2.4, P5.1 | `INV-5.verifier-composition`, `INV-5.salt-16-bytes`, `INV-5.params-applied`, `INV-5.sqlite-row-has-no-pin` | planned |
| INV-6 | At-most-once: the consume commits before the response, and a failed commit sends no blob | P2.6, P5.4 | `INV-6.parallel-same-id`, `INV-6.commit-failure-no-bytes`, `INV-6.sqlite-restart-no-resurrection` | planned |
| INV-7 | Wrong attempts and the burn commit before the response | P2.6, P5.4 | `INV-7.burn-sequence`, `INV-7.concurrent-attempts`, `INV-7.sqlite-attempts-survive-restart` | planned |
| INV-8 | Expiry is logical, and sweeps remove later | P2.3, P5.2 | `INV-8.boundary-controlled-clock`, `INV-8.sweeper`, `INV-8.cap-reclaim`, `INV-8.sqlite-startup-sweep` | planned |
| INV-9 | One identical 404 for every gone state | P2.6 | `INV-9.unknown-expired-burned-consumed-equal` | planned |
| INV-10 | The store mode is disclosed, and restart behavior matches it | P2.2, P3.1, P3.2, P4.4, P4.5, P5.4 | `INV-10.config-store-mode`, `INV-10.browser-notice`, `INV-10.cli-notice`, `INV-10.memory-restart-loses`, `INV-10.sqlite-restart-keeps` | planned |
| INV-11 | Same-origin assets only, no third-party code | P2.2, P3.4 | `INV-11.request-origins`, `INV-11.csp-header` | planned |
| INV-12 | Passphrase generation fails closed on a bad wordlist | P1.1, P3.4 | `INV-12.embedded-invalid-refused`, `INV-12.js-http-404`, `INV-12.js-truncated`, `INV-12.js-duplicates` | planned |
| INV-13 | One JSON error shape on every rejection | P2.2, P2.5, P2.6 | `INV-13.unknown-route`, `INV-13.wrong-method`, `INV-13.media-type`, `INV-13.missing-header`, `INV-13.missing-part`, `INV-13.duplicate-part`, `INV-13.malformed-multipart`, `INV-13.oversize`, `INV-13.static-errors` | planned |
| INV-14 | Two-stage validation: the header before the KDF, the manifest before any output | P1.3, P1.4, P1.5, P1.7 | `INV-14.bad-header-before-kdf`, `INV-14.kdf-range`, `INV-14.bad-manifest-before-output`, `INV-14.tamper-before-render` | planned |

## HTTP interface

| Id | Rule | Owner | Status |
|---|---|---|---|
| HTTP-pages | `/` and `/message/{id}` render with the escaped client config, and the same way whether or not the id exists | P2.2 | planned |
| HTTP-config | `/api/config` returns exactly the allowlisted fields, with headers, and isn't rate limited | P2.2 | planned |
| HTTP-create | The 201, 400, 413, 429, and 503 cases, exactly at the limit and one byte over, chunked oversize, empty blob gets 400 | P2.5 | planned |
| HTTP-retrieve | Check order, a missing or malformed PIN doesn't count an attempt, 403 then burn then 404, 503 on storage error | P2.6 | planned |
| HTTP-headers | Security headers on every response, errors included | P2.2 | planned |
| HTTP-limits | Body limit = blob limit + bounded multipart overhead | P2.5 | planned |

## Server

| Id | Rule | Owner | Status |
|---|---|---|---|
| SRV-ids | Id format, CSPRNG, the 10-collision limit (id-source seam) | P1.2, P2.3 | planned |
| SRV-caps | Atomic count and byte caps under concurrent creates | P2.3, P5.3 | planned |
| SRV-serialize | Same id serialized, different ids concurrent (barrier verifier) | P2.6 | planned |
| SRV-hash-pool | Permit limit, queue limit (503), memory budget, 13-hash default | P2.1, P2.7 | planned |
| SRV-admission | In-flight bodies and pending uploads are bounded. A cancelled request never reports success before its commit. | P2.7 | planned |
| SRV-ratelimit | Fixed windows, key cap, peer address by default, trusted proxies walked right to left, spoofed left-most entry, malformed header falls back | P2.8 | planned |
| SRV-logs | No PIN, header value, or body content in logs | P2.9 | planned |
| SRV-failpoints | The fault interface exists only with `test-failpoints`. Release binaries lack it. | P2.6, P6.3 | planned |

## SQLite store

| Id | Rule | Owner | Status |
|---|---|---|---|
| SQL-schema | Schema version, `protocolVersion` stamp, incompatible schema refused | P5.1 | planned |
| SQL-pragmas | WAL and `synchronous=FULL` set and verified, WAL bounded | P5.2 | planned |
| SQL-lock | A second process is refused | P5.2 | planned |
| SQL-pepper | A pepper fingerprint mismatch refuses startup | P5.1 | planned |
| SQL-faults | Commit failures before and after consume, attempt, and burn. Busy and full storage map to 503. | P5.3, P5.4 | planned |
| SQL-races | The sweeper and attempt handling never recreate a row | P5.4 | planned |
| SQL-contract | The full contract suite passes in `sqlite` mode, nothing skipped | P5.4 | planned |

## Configuration

| Id | Rule | Owner | Status |
|---|---|---|---|
| CFG-precedence | Flags > env > TOML > defaults. `--config` before `PINVEIL_CONFIG`. Comma-separated lists. A repeated single-value option is an error. | P2.1 | planned |
| CFG-validate | TTLs, impossible limits, KDF range, store path, ZIP32 bounds, hash budget below one hash: startup refused | P2.1 | planned |
| CFG-pepper-warn | An empty pepper logs a warning | P2.1 | planned |

## Crypto and format

| Id | Rule | Owner | Status |
|---|---|---|---|
| FMT-outer | Envelope v2: framing, header fields, exact size computation | P1.3 | planned |
| FMT-inner | Manifest: text and files, ceilings vs creation limits, 255-byte names, exact consumption | P1.4 | planned |
| FMT-fuzz | Random bytes never panic or read out of bounds | P1.3, P1.4 | planned |
| FMT-vectors | JS and Rust vectors in both directions, negative vectors rejected by both | P1.7 | planned |
| GEN-pin | PIN uniform and zero-padded, rejection-sampling boundaries, entropy failure aborts | P1.2 | planned |
| GEN-passphrase | Unbiased sampling from the validated list | P1.2 | planned |

## Web UI

| Id | Rule | Owner | Status |
|---|---|---|---|
| WEB-create | Text plus files, generated editable PIN, copy controls and warnings, limits only from config, exact size check before upload | P3.1 | planned |
| WEB-retrieve | Warning before opening, file list, per-file downloads, export names | P3.2 | planned |
| WEB-zip | Store-only zip read by independent readers, ZIP32 bounds | P3.3, P3.5 | planned |
| WEB-roundtrip | Opens Rust-made blobs (P3), and real CLI round trips both ways (P4) | P3.4, P4.9 | planned |

## CLI

| Id | Rule | Owner | Status |
|---|---|---|---|
| CLI-parser | `usage-rs`, unknown and repeated options rejected, positional URL redacted in diagnostics | P4.1 | planned |
| CLI-streams | Prompts on the terminal, error without a terminal, stdout and stderr split, byte-exact redirected text, control sequences neutralized on a TTY unless `--raw` | P4.2 | planned |
| CLI-http | Config fetched and validated first, rustls with the platform verifier, no redirects, `--insecure-http` loopback IP literals only, fragment stripped | P4.3 | planned |
| CLI-send | stdin, prompt, or `--edit` (`--edit` with piped stdin is an error), files, generated PIN or `--choose-pin`, store-mode notice | P4.4 | planned |
| CLI-get | Hidden URL prompt or opt-in positional URL, hidden PIN prompt, store-mode notice | P4.5 | planned |
| CLI-output | Plan first, `create_new`, owner-only, `--force` via temp file and rename that never follows links, collision keeps the message, cleanup on partial failure | P4.6 | planned |
| CLI-profiles | Precedence, URL validation, no secrets or history persisted | P4.7 | planned |
| CLI-editor | Owner-only temp file removed on success and on error | P4.7 | planned |
| CLI-exit | Exit code per failure class | P4.8 | planned |

## Release

| Id | Rule | Owner | Status |
|---|---|---|---|
| REL-stage | Build, then stage, then test, then publish. Candidates never touch stable, `latest`, or the tap. | P6.1 to P6.4 | planned |
| REL-smoke | The staged binaries run, round trip, have embedded assets, and have no failpoints. TLS works with a private CA. Container works with a volume. `brew install` works. | P6.3 | planned |
| REL-publish | Publish steps are idempotent, partial state is reported, retries reuse the same manifest | P6.4 | planned |

## Java-era gaps (APP.md disposition table)

| Gap | Disposition | Proven by |
|---|---|---|
| 1 wordlist | fixed | INV-12 |
| 2 proxy spoofing, unbounded limiter | fixed | SRV-ratelimit |
| 3 framework error shape | fixed | INV-13 |
| 4 CI paths | fixed | the Phase 0 CI workflow (this matrix's harness rows) |
| 5 global hashing lock | fixed | SRV-serialize, SRV-hash-pool |
| 6 untrusted envelope header | fixed | INV-14 |
| 7 hard-coded UI limits | fixed | WEB-create |
| 8 plaintext vs blob size | fixed | FMT-outer, WEB-create |
| 9 empty blob 413 | fixed | HTTP-create |
| 10 empty pepper silent | fixed | CFG-pepper-warn |
| 11 403 on a live id | accepted | `ACC-403-live-id` (P2.6) |
| 12 HTMX | accepted | n/a (design choice) |
| 13 consume before delivery, wrong passphrase consumes, ids in logs | accepted | `ACC-wrong-passphrase-consumes` (P3.4), UI notice tests (P3.2) |
| 13a restart disclosure | fixed | INV-10 |
| 14 leftovers | moot | n/a |
