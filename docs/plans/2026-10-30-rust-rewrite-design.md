# Pinveil Rust rewrite: design decisions

Date: 2026-10-30
Status: Reviewed (3 rounds with Codex, AGREE)
Spec: `APP.md` (the agreed behavior spec; this document records how the rewrite changes and implements it)

## Goal

Rewrite pinveil in Rust, server and new CLI, keeping the existing web UI slice. The main motive is learning and owning a Rust codebase. The result still ships to outside self-hosters, released when it works, with no extra review gate. v1 is a clean cutover from the Java version: only the new server, web UI, and CLI must interoperate, and the wire format may change. There is no obligation to read Java-era blobs.

The author is new to Rust.

### v1 done means

1. Every APP.md invariant holds, as amended below.
2. Every actionable defect in APP.md's known gaps has an explicit fix and an automated check. Accepted tradeoffs (consume before the response arrives, a wrong passphrase still consuming the message, public ids in access logs, 403 for a wrong PIN on a live message) stay documented and are explained in the UI.
3. The CLI sends and retrieves interactively, and any link opens in either the browser or the CLI.
4. The convenience features below work.

## Decisions

| Area | Decision |
|---|---|
| Java | Removed from the branch at the start. Server and CLI grow together in one cargo workspace. |
| Crypto | Same design as today, 4-word passphrases. Fix the found flaws: fail-closed wordlist, validated envelope header, a numeric accepted KDF iteration range shared by both clients. |
| Web UI | Plain JS ES modules, no build step, served by the Rust server. Extended for generated PIN, multiple files, store-mode disclosure. |
| Storage | In-memory by default. Optional SQLite store so messages survive restarts (see [Persistence](#persistence)). Single process. |
| Generated PIN | Both clients pre-fill a random 6-digit PIN by default; the sender may replace it. |
| Multiple files | One message carries optional text plus multiple files, within resource bounds. Browser lists each file with a download button plus "Download all" as a zip. |
| Payload limit | 25 MiB default, configurable, measured on the final encrypted blob. |
| CLI | See [CLI contract](#cli-contract). |
| Binaries | Two: `pinveil` (CLI) and `pinveil-server`, sharing a core crate. |
| Distribution | GitHub release binaries (linux amd64/arm64, macOS arm64, windows amd64), multi-arch GHCR image for the server, Homebrew tap. |

## Secrets and what the server sees

Plaintext, file metadata, and the passphrase never enter server requests or routine logs. The PIN is sent over TLS on create and retrieve, and the server only ever stores its verifier. Showing the generated PIN and share link to the sender is part of the UI, not logging.

Clients generate the PIN with a CSPRNG, uniform over 000000 to 999999, zero-padded. A generated default raises typical PIN entropy to the full ~20 bits. It does not strengthen a PIN the sender overrides, and it does not stop offline PIN guessing once the link and ciphertext both leak.

## Message format (schemaVersion 2)

The outer envelope keeps the existing framing and AEAD design (`[u32 BE header length][JSON header][ciphertext]`), with `schemaVersion: 2`.

The inner plaintext uses a manifest followed by raw bodies:

```
[u32 BE manifest length][UTF-8 JSON manifest][text bytes, if any][file bytes, in array order]
```

```json
{"text":{"byteLength":12},"files":[{"name":"a.txt","mimeType":"text/plain","byteLength":3}]}
```

- `text: null` when there is no text, `files: []` when there are no files. At least one of them must be present. Zero-length files are allowed.
- Lengths are byte counts, never character counts. Parsers require non-negative integer lengths that fit inside the buffer and consume the body exactly. They reject overflow, unknown schema versions, unknown algorithm labels, malformed metadata, and missing required fields.
- Files are identified by array index. Names need not be unique.
- Resource bounds, to be fixed with test fixtures: max file count (start at 256), max manifest size (start at 256 KiB), max file name length.
- `max-payload-bytes` applies to the final encrypted outer blob, headers and tag included. Both clients compute the exact blob size before upload and refuse an oversized one. The server enforces the same number.
- Clients use views and slices to avoid extra whole-message copies.

## Persistence

The SQLite store is opt-in, set by an explicit store mode and path on a local persistent volume.

### Atomicity

- Each message has a linearization point. Retrieval and attempt handling for one id are serialized, together with expiry for that id. Different ids proceed in parallel. The per-id coordination is bounded: a fixed set of striped locks or conditional row revisions, never another uncapped map.
- Argon2 runs outside any global lock and outside any DB transaction. SQLite allows one writer at a time, so write transactions stay short.
- A consume, an attempt increment, or a burn commits before the response goes out. If the commit fails, the client gets a generic storage error and never the blob bytes.
- A row the sweeper deletes during verification stays deleted. An attempt update must not recreate it.
- Creation, id uniqueness, and count and byte cap admission are atomic.
- The memory store follows the same rules.

### Durability and operations

- SQLite runs in WAL mode with `synchronous=FULL`. The server verifies the pragmas at startup, and the docs name the filesystem assumptions (a local disk, not a network share).
- A second process opening the same database is refused (lock file or exclusive lock).
- Expiry deadlines and failed-attempt counts persist. The server sweeps expired rows at startup and then periodically. Count and byte caps still apply. Disk-full and busy errors map to the generic 503. WAL growth is bounded through checkpoints.
- The pepper must stay stable across restarts. If it changes while messages are stored, every correct PIN would count as wrong and burn the message. The server stores a fingerprint of the pepper and refuses to start on a mismatch unless the operator explicitly wipes the store.
- The database has a schema version, and each row records the protocol (envelope schema) version the server was built for when it accepted the message. The server records this itself and never reads it from the blob (APP.md invariant 3). Later Rust releases either read earlier envelope versions until those messages expire, or require draining the store before an incompatible upgrade.
- Restoring a database backup can bring old messages back. That is outside the at-most-once guarantee, and normal startup never does it.

### Disclosure

The client config tells the browser which store mode is active. Both clients and the docs show it. With the SQLite store, the database file, WAL, snapshots, and backups can retain ciphertext and PIN verifiers after deletion. The passphrase still protects the content.

## CLI contract

### Streams

- `send`: content comes from stdin, or from an interactive prompt or `$EDITOR` when stdin is a terminal. Files are path arguments. The PIN and the link are written to the controlling terminal, never read from stdin. The share URL goes to stdout. The generated PIN goes to the terminal, with a reminder to send it over a different channel.
- `get`: by default, the share URL is read at a hidden prompt on the terminal. A positional URL is allowed as an explicit opt-in, and its help text says it exposes the passphrase in shell history and the process list. The PIN is read at a hidden terminal prompt. Only text content goes to stdout. Status and written file paths go to stderr.
- If a terminal input is required and there is no terminal, the command fails with a clear error. No flags or environment variables for content, PIN, or passphrase.
- The client strips the fragment from the URL right away and builds request URLs without it. Error messages never include the full URL or the PIN.
- Exit codes are defined per failure class.
- Before printing decrypted text to a terminal, the CLI neutralizes terminal control sequences, unless the user explicitly asks for raw output.

### HTTP client

reqwest with the rustls backend, set explicitly. TLS is required except for an explicit local-development switch. Redirects are off. Operators with a private CA load it into the trust store. There is never a "skip verification" option.

### File output

- Validate the whole manifest and plan every destination before writing anything.
- Export names follow one deterministic policy, shared by the CLI and the browser zip. It rejects path separators, absolute paths, drive and UNC prefixes, `.` and `..`, NUL, control characters, and names that are invalid on the platform (e.g. Windows device names and alternate data streams). It also disambiguates duplicate names and names that collide only by case. The original names are displayed as plain text.
- Files are created with `create_new` (atomic, no overwrite) relative to the destination directory, with owner-only permissions.
- With `--force`, the CLI writes to a temp file and renames it over the destination. It never follows an existing symlink or reparse point and never truncates a link target.
- The server has already consumed the message, so a collision must not lose the data. The decrypted message stays in memory while the user picks another directory or confirms `--force`. Quitting at that point discards it, and the prompt says so.
- Partial writes and disk-full errors clean up whatever files the CLI created and report clearly.

### Profiles

A TOML file in the platform config directory. Only non-secret settings are allowed: server URL, default TTL, default output directory. The CLI rejects server URLs that carry credentials, query strings, or fragments. Command-line options override the profile, which overrides defaults. The CLI never stores PINs, fragments, plaintext, message history, or payload paths.

### Editor mode

The CLI writes to a unique owner-only temp file and deletes it on success and on error. The docs say that editor swap and backup files, and crashes, can leave plaintext behind.

### Parser

The parser is `usage-rs` (typed derives, verified on crates.io v7.1.4 and in mise main), with `unknown_flags = "error"` and an explicit policy for repeated options. Paths use `PathBuf`. Mutually incompatible input modes are validated after parsing. Content and PIN never pass through the parser. The one exception is the opt-in positional URL for `get`, which carries the passphrase in its fragment. For that argument, parser diagnostics (usage errors, help echoes, debug output) must redact the fragment just as HTTP errors do, and a test covers it.

## Server runtime

- tokio + axum. Static assets and the wordlist are embedded in the binary.
- Argon2 and SQLite are blocking work. They run on a bounded blocking pool. Concurrent Argon2 operations and their queue each have their own limit, so they can't exhaust threads or memory. No DB connection or global lock is held while hashing.
- Admission accounts for in-flight request bodies and hashing memory, not just stored blobs.
- A cancelled HTTP task never reports success before a commit. Blocking work that has already started runs to its commit or rollback.
- Body limits come from the blob limit plus bounded multipart overhead, set explicitly. axum's multipart extractor defaults to 2 MB, which is too small. Chunked uploads, duplicate or unexpected fields, multipart parse errors, unsupported methods and media types, and unknown routes all return the JSON error shape with the security headers.
- Forwarding headers are not trusted by default. An explicit trusted-proxy setting enables them.
- Rate-limit state is bounded (see APP.md gap 2).

## Configuration

Precedence, highest first: command-line options, environment variables, the TOML file, then defaults. At startup the server validates:

- TTL presets and the default against min and max,
- that the limits are possible,
- the PBKDF2 iteration count against the shared accepted range.

It refuses to start on anything invalid, instead of creating messages nobody can decrypt. The client config is injected with safe JSON and HTML escaping. Every asset stays same-origin under the existing CSP.

## Wordlist

The server checks its embedded EFF list at startup: 7776 entries, all distinct and valid. The browser checks the fetched list again before using it, because a proxy can still return an error page or a truncated file. The CLI uses its embedded copy, checks it once, and never fetches it. A CSPRNG failure aborts creation.

## Browser zip

A small plain-JS module writes store-only zip files (method 0, CRC-32, UTF-8 names, local headers, central directory, end record), following PKWARE APPNOTE. It runs only when "Download all" is clicked, from already decrypted files, using the shared export-name policy and fixed timestamps. There's no compression or encryption, and nothing goes to the server. "Download all" covers the files only. The text stays separately copyable.

ZIP32 limits on file count and offsets are enforced through the message bounds. A deployment that raises the payload limit past what ZIP32 supports must be rejected at startup, or ZIP64 implemented on purpose. Tests use independent zip readers.

## Release

- Exact target triples, minimum OS and libc versions, and archive names are fixed per binary: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`. Values still to be confirmed.
- Native CI runners for each target at first. musl makes the Linux binaries portable. No claim that every platform build is fully static.
- SQLite is bundled with rusqlite's `bundled` feature, which needs a C toolchain on each runner. The CLI crate doesn't depend on SQLite.
- The toolchain is pinned (`rust-toolchain.toml`) and `Cargo.lock` is committed.
- TLS verification is tested on real Linux, macOS, and Windows.
- Both Linux architectures are built and smoke-tested before the GHCR manifest is assembled. The docs give the container a persistent volume for SQLite mode.
- Homebrew uses an `alexey-lapin/homebrew-tap` repo with a formula that installs the prebuilt archives, with per-platform URLs and checksums. A release job updates it. No Intel macOS build.

## Spec changes to apply to APP.md

1. Invariant 1: "browser" becomes "client". Plaintext, metadata, and the passphrase never reach the server. The PIN does, by design (see [Secrets](#secrets-and-what-the-server-sees)).
2. Invariant 10: a restart wipes everything in memory mode only. Each mode is disclosed to users.
3. Non-goals: drop "more than one file per message" and "durable storage". Multiple instances remain a non-goal.
4. The glossary, create flow, retrieve flow, and UI notes change to the text-plus-files model and the generated PIN.
5. The envelope section becomes schemaVersion 2 as specified above. The "old blobs need not be readable" note becomes the persistence compatibility rule.
6. New sections: CLI contract, persistence, configuration precedence, release artifacts.
7. New config fields: store mode and path, trusted proxies, file count and manifest limits, KDF range.
8. The known gaps get a disposition column: fixed (with its check) or accepted tradeoff. Empty blob becomes 400. Starting without a pepper logs a warning. The wrong-PIN 403 stays.

## Verification

All automated. No human review gate.

- **Contract suite from APP.md** run against both stores: status codes, the error shape on every path (framework errors included), burn and expiry, rate limits and bounded limiter state, security headers, the trusted-proxy policy, body limits (normal 25 MiB messages pass, oversize, chunked, malformed multipart).
- **At-most-once, burn, and expiry races** for both stores. Parallel requests on different ids prove there's no global hashing lock. Resource-bound tests cover pending uploads, hash work, and the lock and counter registries.
- **Persistence:** restart with pending, read, burned, and expired records and with failed counts. Pepper and schema mismatch refusal. Fault injection before and after the consume or burn commit and before the response. A failed commit returns no blob, and an acknowledged commit never comes back to life. Verify the pragmas. A process kill doesn't simulate power loss, so the docs don't claim it does.
- **Crypto vectors** from JS and Rust, checked both ways. They include malformed lengths, extra and truncated bytes, unsupported versions and labels, bad tags and wrapped keys, and KDF bounds. Payloads cover text only, files only, both together, zero-length files, duplicate and Unicode names, count and manifest limits, and exact final-blob boundaries.
- **Browser UI** (headless): custom limits, store-mode disclosure, generated PIN and override, file selection and downloads, a missing fragment, a wrong passphrase, wordlist failures (status, truncation, duplicates).
- **CLI integration:** piped stdin with terminal prompts, errors when there's no terminal, stdout and stderr separation, credential redaction in errors, editor failure and cleanup, output collisions and symlinks, `--force` semantics, partial-write cleanup, redirect refusal.
- **Zip:** independent readers on all supported desktop platforms, with Unicode, duplicate names, and empty files.
- **Release:** run the produced artifacts, check that the static assets are embedded, exercise SQLite and TLS, and test installation (Homebrew, container) on the target platforms.

## Open questions

None blocking. The resource bounds (file count, manifest size, name length, KDF range, Argon2 concurrency) get their final values from fixtures and benchmarks during implementation.
