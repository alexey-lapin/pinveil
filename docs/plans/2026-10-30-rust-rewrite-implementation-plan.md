# Pinveil Rust rewrite: implementation plan

Date: 2026-10-30
Status: Reviewed (3 rounds with Codex, AGREE)
Spec: `APP.md`. Design: `docs/plans/2026-10-30-rust-rewrite-design.md`.

## Principles

- **Tests define done.** Inside each step, the tests for the APP.md rules it covers are written first, then made to pass. Red tests never land in a commit that's meant to be green. A step is done when CI is green on all four targets and its acceptance checks pass.
- **Learning order.** The author is new to Rust, so the sequence goes from synchronous, pure code (`pinveil-core`) to async networking (server) to OS-facing code (CLI terminal and file handling, SQLite).
- **Small green steps.** Each phase splits into numbered steps. Each step is a test-plus-code pair that lands green and is small enough to review in one sitting.
- **Spec first.** When implementation shows that APP.md is wrong or vague, update APP.md in the same change and say why in the commit.
- **Automated peer review.** At the end of each phase, Codex reviews the phase's diff against APP.md and this plan, until it agrees. The user asked for this as a development practice. It is not a release gate, and nothing here adds a manual verification step.

## Workspace layout

```
Cargo.toml                     virtual workspace, shared lints and profiles
Cargo.lock                     committed
rust-toolchain.toml            pinned stable toolchain + rustfmt, clippy, target list
crates/
  pinveil-core/                format, crypto, validation, generators, export names, wire types
  pinveil-server/              binary pinveil-server (axum)
    tests/contract/            black-box HTTP contract suite, a registered integration test
  pinveil-cli/                 binary pinveil
    tests/                     CLI integration and PTY tests
web/                           plain ES modules, CSS, HTML pages (embedded by the server)
assets/eff_large_wordlist.txt  plus its license note
tests/
  vectors/                     cross-client crypto vectors (JSON)
  js/                          Node runner for the JS vector tests (package.json + lockfile)
  browser/                     Playwright tests (dev-only Node tooling)
packaging/                     Dockerfile, Homebrew formula template, release scripts
docs/coverage.md               rule to step to test matrix
```

Integration tests inside a crate find their own binary through `CARGO_BIN_EXE_<name>`. Harnesses outside Cargo (Playwright, the JS runner, release smoke tests) get binary paths from the `PINVEIL_SERVER_BIN` and `PINVEIL_CLI_BIN` environment variables. CI sets them from the actual build output, including the target-triple directory and `.exe` on Windows. They never assume `target/debug`.

Node is a dev and test dependency only. The served JS is never built or bundled.

## Crates

| Crate | Purpose and version notes |
|---|---|
| `aes-gcm`, `aes-kw`, `pbkdf2`, `sha2` | Client crypto. |
| `argon2` | Server PIN verifier, constructed explicitly with the configured t, m, p. |
| `rand` 0.10 (`sys_rng`) or `getrandom` directly | CSPRNG through the fallible system source. Errors propagate and abort creation. Deterministic sources exist only in test code. |
| `base64`, `serde` (derive), `serde_json` | Envelope and wire types. |
| `thiserror` (libraries), `anyhow` (binaries) | Errors. |
| `zeroize` | Wipe key material, PINs, and passphrases after use. Prompt results are wrapped right away. |
| `tokio`, `axum` (`multipart`), `tower`, `tower-http` | Server runtime and middleware. |
| `rusqlite` (`bundled`) | SQLite store, server only. Needs a C compiler on each runner. |
| `toml`, `humantime-serde`, `ipnet` | Configuration. TTL preset lists use a wrapper type (`Vec<humantime_serde::Serde<Duration>>`). Comma-separated env values are split before typing. `expiresAt` uses an explicit RFC 3339 serializer. |
| `tracing`, `tracing-subscriber` | Logs. Never secrets. |
| `usage-rs` 7 | CLI parsing. `unknown_flags = "error"`, repeated single-value options rejected, a fallible and redacting path for the positional URL. |
| `reqwest` 0.13 (`default-features = false`, features `rustls`, `json`, `multipart`) | CLI HTTP. Verified on crates.io: the 0.13 `rustls` feature uses `aws-lc-rs` (which needs a C toolchain and CMake on some targets) and `rustls-platform-verifier`, so operators add a private CA through the OS trust store. Redirects are disabled explicitly. |
| `rpassword` 7 | Hidden prompts on the TTY or console. A separate console writer prints the generated PIN to the terminal, not to stderr. |
| `directories` | Platform config directory. |
| `tempfile` | Editor temp files. The `--force` temp file is created in the destination directory, because `persist` can't cross filesystems. It doesn't replace the platform file-safety adapter (no following symlinks or reparse points, owner-only permissions on Windows). |
| `expectrl` (dev) | PTY tests. Uses ConPTY on Windows, so start with a simple synchronous session. |
| `proptest` (dev) | Property tests for parsers. |

Each crate's manifest records the features it needs. The toolchain and minimum Rust version are pinned. Phase 0 builds smoke stubs that pull in `rusqlite` and `reqwest`, so native-toolchain problems on the musl targets show up on day one.

## Test seams

The service layer gets injectable adapters from the start. Hard cases can then be tested deterministically:

- **Clock.** Expiry boundaries and sweeps use a controlled clock. One real-time sweeper integration test remains.
- **Entropy and id source.** Reproducible vectors, forced id collisions (the 10-attempt limit), PIN boundary cases.
- **PIN verifier.** A barrier-backed fake shows that two different ids can be verifying at the same time when at least two permits are configured, while the same id stays serialized. Timing ratios are never the assertion. Timeouts only catch deadlocks.
- **Store fault adapter.** It fails a commit before or after consume, attempt, or burn, and simulates busy or full storage.

The fault adapter for the spawned server binary is compiled in only with the non-default Cargo feature `test-failpoints`. The contract suite builds a separate test binary with it. Release builds never enable it, and a release check confirms the shipped binary has no failpoint interface. It is never ordinary runtime configuration.

## Phases

Every step below lands green. Coverage is tracked in `docs/coverage.md`, which maps each APP.md invariant and rule to a step and named tests (format: `INV-6.parallel-same-id`, `HTTP-create.empty-blob-400`). It is created in Phase 0 and filled in as steps land.

### Phase 0: Baseline, removal, skeleton

1. Commit the agreed APP.md, design, and plan (checkpoint 3 of the planning work). Record that commit hash as the pre-removal baseline.
2. Move the retained assets with `git mv`, so history follows them:
   - `pinveil-app/src/main/resources/static/*` (JS, CSS) to `web/`,
   - `pinveil-app/src/main/jte/*.jte` to `web/` as source material. The page HTML gets rewritten later, because unresolved JTE directives must never be served,
   - `pinveil-core/src/main/resources/static/eff_large_wordlist.txt` to `assets/`, with a license and source note.
   Check the checksums of the moved files.
3. Remove, by explicit list:
   - `pinveil-core/`, `pinveil-app/`,
   - `build.gradle.kts`, `settings.gradle.kts`, `gradle/`, `gradlew`, `gradlew.bat`,
   - `Dockerfile.native`,
   - the Java CI workflow, replaced in step 5 of this phase.
   Don't use a blanket clean or reset, since there may be unrelated untracked files. Keep `docs/` and `.github/` config that is still valid (renovate, labeler, release notes).
4. Create the workspace with stub crates that build: the core library, the server binary answering a placeholder route, the CLI binary printing its version. Add `rust-toolchain.toml` and the lint config (`unsafe_code = "forbid"`). Add dependency smoke stubs for `rusqlite` and `reqwest`.
5. CI on native runners for the four targets:
   - `cargo fmt --all --check`,
   - `cargo clippy --workspace --all-targets --locked -- -D warnings`,
   - `cargo test --workspace --locked`.
   Linux jobs also build and run the musl targets, with the musl target and a C toolchain installed. Add a JS job (Node pinned, `npm ci` from the lockfile) and cargo caching.
6. Harness probes that prove the plumbing executes:
   - a contract-suite test that starts the stub server through `CARGO_BIN_EXE_pinveil-server` and gets a response,
   - a JS runner test,
   - a PTY probe on all four targets. It shows that a hidden console prompt works while stdin is piped and stdout is redirected separately, plus a no-console case.
   If `expectrl` can't model these on Windows, choose another automated harness here. A manual fallback isn't allowed.
7. Create `docs/coverage.md` listing every APP.md invariant, endpoint rule, CLI rule, store rule, and fixed gap, with an owning step.

Done when CI is green on all four targets, with all three probes running.

### Phase 1: `pinveil-core`

Each step pairs Rust tests with the matching JS changes in `web/`, run by the JS runner.

1. **Wordlist.** Embedded with `include_str!` and validated: 7776 entries, distinct, well formed. Tests feed invalid lists (short, duplicates, malformed) and expect refusal. JS validation of a fetched list: status, count, duplicates, truncation.
2. **Generators.** Message id, passphrase (unbiased sampling), PIN (uniform, zero-padded). Rejection-sampling boundary cases come through injected entropy. An entropy failure aborts. The JS generators get matching tests.
3. **Outer envelope v2.** Encode, decode, first-stage validation (length prefix, header fields, labels, KDF range, decoded field sizes), exact blob-size computation. Property tests check that random bytes never panic or read out of bounds.
4. **Inner payload.** Manifest types, encode, decode with every second-stage check: protocol ceilings, 255-byte UTF-8 name boundary, length overflow, exact consumption, text-or-files presence. Creation limits are tested separately from decoding ceilings.
5. **Crypto.** PBKDF2 to KEK, AES-KW, AES-GCM with AAD, zeroization. Tampering with the tag, wrapped key, IV, or header fails before any rendering or output.
6. **Export names.** A pure function. The Windows rules are tested on every platform.
7. **Cross-client vectors.**
   - The JS side (under Node, with test-only entropy) generates `tests/vectors/*.json`. Core decrypts them, re-encrypts the same inputs, and compares bytes.
   - Core generates its own vectors, and the JS runner decrypts them.
   - Negative vectors (bad lengths, bad tag, unknown labels, out-of-range KDF, over-ceiling manifests) are rejected by both.

Done when all vectors pass in both directions.

### Phase 2: Server, memory mode, with its contract suite

The contract suite lives in `crates/pinveil-server/tests/contract/` and grows step by step with the server. Each test takes the store mode as a parameter. This phase registers only `memory`. Phase 5 adds `sqlite`, and it must pass every case with nothing skipped.

1. **Config.** Precedence (flags, env, TOML file, defaults), `--config` before `PINVEIL_CONFIG`, comma-separated lists, repeated options rejected. Startup validation: TTLs, limits, KDF range, store mode and path, ZIP32 bounds, hash budget below one hash, ceilings. Pepper warning. The hash-concurrency formula (13 with defaults, explicit values capped).
2. **Skeleton and error layer.** Uniform JSON errors and security headers on every path: unknown route, wrong method, unsupported media type, static and page errors, `/api/config`. Pages with the escaped client config, rendered the same whether or not an id exists, with HTML-escaped ids. Embedded static assets with correct content types. `/api/config` returns exactly the allowlisted fields.
3. **Store and service, memory.** Atomic create with count and byte caps (including concurrent creates), id collision exhaustion through the id-source seam, expiry with the controlled clock, sweep and reclamation when a cap is reached.
4. **PIN verifier.** Explicit Argon2id construction. Tests check the `pin:salt:pepper` composition, the 16-byte salt, the configured t, m, p, correct and wrong PINs, and that the raw PIN is never stored.
5. **Create endpoint.** Multipart with explicit limits: missing, duplicate, or unexpected parts, malformed bodies, every PIN and TTL case, an empty blob (400), an exactly-at-limit blob (201), one byte more (413), chunked oversize. Arbitrary non-empty bytes are accepted without parsing and returned byte for byte later.
6. **Retrieve endpoint.**
   - A missing or malformed PIN gets 400 and doesn't count an attempt.
   - Burn sequence: 403, 403, 403, then 404.
   - Same 404 body for unknown, expired, burned, and consumed ids.
   - At-most-once under parallel retrievals of one id.
   - Different ids verify concurrently (barrier verifier).
   - Commit failure (fault adapter) sends no blob bytes.
7. **Hashing pool and admission.** Permit limit, queue limit (503 when full), memory budget, bounded pending uploads. A cancelled request never reports success before its commit.
8. **Rate limiter.**
   - Fixed windows per bucket.
   - The key cap.
   - Peer address only by default.
   - A trusted proxy with a right-to-left walk, including the spoofed left-most entry.
   - Malformed and over-long headers fall back to the peer address.
9. **Log redaction.** Captured logs contain no PINs, `X-Message-Pin` values, or body content.

Done when the contract suite passes in `memory` mode and the coverage matrix lists every server rule with its test.

### Phase 3: Web UI

1. **Create page.**
   - Text plus files.
   - A generated PIN that can be edited, plus copy controls for the link and PIN, with their warnings.
   - Limits from the config only.
   - Manifest and file-count checks and the exact blob-size check before upload.
   - Store-mode notice.
2. **Retrieve page.**
   - Warning before opening.
   - Multi-file list with per-file downloads.
   - Export names.
   - Store-mode notice.
3. **Zip writer.** `zip.js`, store-only, PKWARE APPNOTE, ZIP32 bounds.
4. **Playwright tests** against a real server:
   - Content: text only, files only, both together. Zero-length, duplicate, and Unicode names.
   - PIN: the generated one, and an override.
   - Failures: wordlist failures (404, truncated, duplicates), missing fragment, wrong passphrase, refusing an oversize message before upload, custom limits.
   - Network capture: no request carries plaintext, original file names or MIME types (the multipart filename too), or the fragment. Every request goes to the same origin.
   - Opening blobs that Rust core generated. Real CLI round trips come in Phase 4.
5. **Zip checks** with independent readers (Python `zipfile`, `unzip -t`), installed explicitly on each runner.

Done when the browser tests pass on the CI browsers.

### Phase 4: CLI

1. **Parser.** The `usage-rs` command spec: `send`, `get`, `profile`, with every APP.md flag. Unknown and repeated options are rejected. Parser diagnostics redact the fragment of a positional URL.
2. **Terminal I/O.**
   - Hidden prompts and the console writer, on the controlling terminal.
   - A clear error when a needed terminal is missing.
   - stdout for the URL and text, stderr for status.
   - Byte-exact text through redirected stdout. Control sequences neutralized on a TTY unless `--raw`.
3. **HTTP.**
   - Fetch `/api/config` and validate it. Stop before upload if that fails.
   - rustls with the platform verifier.
   - Redirects refused.
   - `--insecure-http` only for loopback IP literals.
   - The fragment is stripped before any request, and the URL and PIN are redacted from every error.
4. **send.** Text from stdin, the prompt, or `--edit`. `--edit` with piped stdin is an error. Files as arguments. A generated PIN, or `--choose-pin`. Store-mode notice.
5. **get.** A hidden URL prompt, or the opt-in positional URL. A hidden PIN prompt. Store-mode notice.
6. **File output.**
   - A plan before any write.
   - `create_new` with owner-only permissions, through the platform file-safety adapter.
   - `--force` through a temp file in the destination directory and a rename that never follows links.
   - A collision prompt that keeps the message in memory.
   - Cleanup on partial failure.
7. **Profiles and editor mode.**
   - Option and profile precedence.
   - Server URLs with credentials, a query string, or a fragment are rejected.
   - No secrets or history ever persisted (checked by inspecting the files).
   - Editor temp files are cleaned up on success and on error.
8. **Exit codes** per failure class.
9. **Tests.**
   - PTY tests on all four targets, through the harness from the Phase 0 probe.
   - Network capture: no plaintext, file names, MIME types, or fragment in any request.
   - Round trips with real binaries in both directions, browser to CLI and CLI to browser.

Done when the CLI tests and real round trips pass on all four targets.

### Phase 5: SQLite store

1. **Schema.** A version table. Rows as APP.md describes, with `protocolVersion` set by the server. A pepper fingerprint. Malformed or incompatible schemas are refused.
2. **Startup.**
   - WAL and `synchronous=FULL`, set and verified.
   - An exclusive lock, so a second process is refused.
   - A startup sweep.
   - Checkpointing that bounds WAL growth.
3. **Store implementation.** The same trait as the memory store. Short transactions, with no hashing inside them. Busy and full errors map to 503. Cap accounting.
4. **Tests.**
   - The full contract suite with `sqlite` registered.
   - Restart with pending, read, burned, and expired records and failed counts. Memory mode loses everything on restart, SQLite keeps it.
   - Pepper mismatch refusal.
   - Fault injection before and after every commit and before the response, with no bytes leaking when a commit fails.
   - Races between the sweeper and attempt handling never recreate a row.
   - A compatibility test confirms the server never parses blobs to decide anything. The "all clients can still read it, or drain the store" upgrade rule goes into the release checklist.

Done when both store modes pass everything.

### Phase 6: Release

The release runs in four stages: build, then stage, then test, then publish.

1. **Build.** One release build per target, for both binaries, with `test-failpoints` off. Archives with checksums and a manifest listing every artifact.
2. **Stage.**
   - Upload the artifacts to a draft or prerelease GitHub release.
   - Push the container under immutable candidate tags (by digest), for both Linux architectures.
   - Generate a temporary Homebrew formula that points at the staged archives.
3. **Test.**
   - Run each staged binary on its own target. Check `--version`, run a create and retrieve round trip between the staged server and CLI with the PTY harness, and confirm the assets are embedded and no failpoint interface exists.
   - TLS against a private-CA test server on Linux, macOS, and Windows.
   - Both container architectures, including a SQLite volume.
   - `brew install` from the temporary formula.
4. **Publish.** Only after every check passes:
   - publish the GitHub release,
   - add the stable and `latest` image tags to the tested digests,
   - push the formula to `alexey-lapin/homebrew-tap`.
   Pushing to the tap uses a dedicated, narrowly scoped token (a GitHub App or fine-grained PAT), stored as a secret that only the publish job can read. The workflow's own `GITHUB_TOKEN` can't write to another repository. Workflow permissions are least-privilege per job.

The tag pattern `v*.*.*` also matches release candidates such as `v1.0.0-rc.1`. The workflow detects a prerelease suffix. Candidates get prerelease artifacts and candidate image tags only. They never touch `latest`, the stable aliases, or the tap. A failed check in the test stage blocks publication entirely.

The publish steps themselves (GitHub release, image tags, tap commit) go to three separate services, so they can't be atomic. Instead:

- **Idempotent steps.** Each step is keyed to the tested artifact manifest and image digests. Re-running a step is a no-op once it has succeeded: the release is already published, the tag already points at that digest, or the formula already has those checksums.
- **Tracking.** The workflow runs the steps in a fixed order (GitHub release, then image tags, then tap) and records each step's result in the job summary.
- **Reporting.** If a step fails, the run fails, and the summary names exactly what is published and what isn't. A partly published release is never reported as complete.
- **Retries.** A retry re-runs only the publish job against the same manifest and digests. It never rebuilds.

Done when a release candidate goes through the whole workflow without publishing anything stable, and a dry run of the final publish path passes.

### Phase 7: Docs and v1.0.0

- **README:** what pinveil is, how to install it, and a quick start for both clients.
- **Operator guide:**
  - configuration,
  - store modes and how each is disclosed,
  - keeping the pepper stable,
  - trusted proxies,
  - TLS and private CAs,
  - backups and their at-most-once caveat,
  - upgrades and draining the store.
- **Threat model.**
- **Final check:** every row of `docs/coverage.md` is green. Accepted tradeoffs (wrong-passphrase consumption, dropped-response loss, 403 on a live id, public ids in logs) are asserted by tests and documented. Then tag `v1.0.0`.

## Risks

| Risk | Mitigation |
|---|---|
| Async Rust learning curve in Phase 2 | Phase 1 builds the basics first. Handlers stay thin, and the logic lives in sync, testable service and store code behind the seams. |
| Native toolchains on musl and Windows (SQLite, aws-lc-rs) | Dependency smoke stubs in Phase 0. Native runners per target. |
| JS and Rust format drift | Vectors in both directions from Phase 1, on every CI run. |
| Windows terminal behavior | A PTY probe in Phase 0. If `expectrl` falls short, switch to another automated harness. No manual fallback. |
| Flaky concurrency tests | Barriers and controlled clocks. No assertions on timing ratios. |
| Scope creep | v1 scope is fixed by APP.md. New ideas go on a post-v1 list. |
