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
`<area>.<case>`. JS test titles contain the id as written (`INV-14.kdf-range ...`). Rust test
functions use the id in snake case (`inv_14_kdf_range_edges`, `fmt_vectors_...`), so
`grep -ri 'inv.14'` finds both.

## Harness probes (Phase 0)

| Id | What it proves | Test | Status |
|---|---|---|---|
| H-contract | The registered contract suite starts the real `pinveil-server` binary and reaches it | `crates/pinveil-server/tests/contract/main.rs` `harness_reaches_the_real_server_binary` | covered (CI run 41162610508 at 0ff07c4) |
| H-js | The JS runner loads `web/static` modules under Node Web Crypto | `tests/js/crypto.test.js` "INV-4 round trip ..." (replaced `harness.test.js` in Phase 1) | covered (CI run 41165815026 at c0d0b65) |
| H-pty | A hidden prompt works in a PTY while stdin and stdout are redirected, without echoing the input | `crates/pty-probe/tests/terminal.rs` `hidden_prompt_works_with_stdin_from_a_file`, `hidden_prompt_works_with_stdin_from_a_pipe` | covered (CI run 41162610508 at 0ff07c4) |
| H-noterm | Without a terminal, the prompt fails clearly (exit 3) | `crates/pty-probe/tests/terminal.rs` `fails_clearly_without_a_terminal` | covered (CI run 41162610508 at 0ff07c4) |
| H-deps | Bundled SQLite and reqwest with rustls (aws-lc-rs) build and link | `pinveil-server` `bundled_sqlite_links`, `pinveil-cli` `rustls_client_builds` | covered (CI run 41162610508 at 0ff07c4) |

## Invariants

| Id | Rule | Owner | Tests (planned ids) | Status |
|---|---|---|---|---|
| INV-1 | Plaintext, names, and MIME types never leave a client unencrypted. The passphrase never reaches the server. The PIN does, over TLS, and only its verifier is stored. The only TLS exception is `--insecure-http` to loopback. | P1.5, P2.4, P3.4, P4.3, P4.9 | `INV-1.browser-request-capture`, `INV-1.cli-request-capture`, `INV-1.no-raw-pin-stored`, `INV-1.insecure-http-loopback-only` | P3.4 part (`tests/browser` `WEB-roundtrip.text-only ...` and `.files-only ...`: a recording proxy checks every raw request body, header, and path for plaintext, names, MIME types, and the fragment; putting a file name in the multipart filename fails it) covered (CI run 41217908068 at 5c1e0f1); P4 parts (`crates/pinveil-cli/tests/cli` `inv_1_cli_request_capture_has_no_plaintext_names_or_fragment`: a recording TCP proxy sees no text, file data, file name, MIME type, or passphrase; `links.rs` `inv_1_insecure_http_loopback_only`) covered (CI run 41225442086 at d84d2f2) |
| INV-2 | The fragment is never sent, logged, or put in errors | P3.4, P4.1, P4.3, P4.9 | `INV-2.browser-requests`, `INV-2.cli-requests`, `INV-2.cli-error-redaction`, `INV-2.parser-diagnostics-redacted` | P3.4 part (same recording-proxy checks for the fragment) covered (CI run 41217908068 at 5c1e0f1); P4 parts (`inv_1_cli_request_capture_...` checks the passphrase too; `links.rs` `inv_2_cli_share_links_split_the_fragment_and_errors_never_quote_it`; `args.rs` `inv_2_parser_diagnostics_redacted`; `cli_exit_codes_per_failure_class` asserts no passphrase in the output) covered (CI run 41225442086 at d84d2f2) |
| INV-3 | The server treats blobs as opaque bytes | P2.5 | contract `inv_3_arbitrary_bytes_roundtrip` (5000 non-envelope bytes stored and returned byte for byte) | covered (CI run 41212423953 at cc6dce0) |
| INV-4 | The key needs both the passphrase and the PIN | P1.5, P1.7 | core `inv_4_round_trip_needs_both_inputs`; JS `INV-4 round trip ...`; vectors `decrypt: wrong PIN`, `decrypt: wrong passphrase` | covered (CI run 41165815026 at c0d0b65) |
| INV-5 | The PIN is stored only as a salted, peppered Argon2id hash | P2.4, P5.1 | `pin.rs` `inv_5_verifier_composition_and_params`, `inv_5_params_applied_and_old_hashes_still_verify`; `INV-5.sqlite-row-has-no-pin` (P5.1) | P2.4 parts covered (CI run 41212423953 at cc6dce0); P5.1 part (`sqlite::tests` `inv_5_sqlite_row_has_no_pin`: the table has no PIN column, only the verifier) local (macOS; CI pending) |
| INV-6 | At-most-once: the consume commits before the response, and a failed commit sends no blob | P2.6, P5.4 | service `inv_6_consume_once_*`, `inv_6_parallel_same_id_releases_once`, `inv_6_commit_failure_no_bytes`; store `inv_6_consume_is_at_most_once`; contract `inv_6_parallel_same_id`; `INV-6.sqlite-restart-no-resurrection` (P5.4); service `inv_6_stale_correct_pin_never_consumes_a_replacement` (expiry, sweep or cap reclamation, id reuse; ignoring the generation makes it fail); store `inv_6_stale_generation_never_touches_a_reused_id` | P2.6 parts covered (CI run 41212423953 at cc6dce0); P5.4 parts (SQL-faults, SQL-races, and the contract suite on SQLite) local (macOS; CI pending) |
| INV-7 | Wrong attempts and the burn commit before the response | P2.6, P5.4 | store `inv_7_failures_count_then_burn_and_never_recreate`; service `inv_7_burn_sequence_then_gone`; contract `http_retrieve_pin_rules_and_burn_sequence`; `INV-7.sqlite-attempts-survive-restart` (P5.4); service `inv_7_stale_wrong_pin_never_charges_a_replacement` | P2.6 parts covered (CI run 41212423953 at cc6dce0); P5.4 parts (SQL-faults, SQL-races, and the contract suite on SQLite) local (macOS; CI pending) |
| INV-8 | Expiry is logical, and sweeps remove later | P2.3, P5.2 | service `inv_8_boundary_controlled_clock`, `inv_8_sweeper_removes_expired`; store `inv_8_sweep_removes_only_expired_at_the_boundary`, `srv_caps_count_bytes_and_reclaim_expired`; contract `inv_9_*` (expired case); `INV-8.sqlite-startup-sweep` (P5.2) | P2.3 parts covered (CI run 41212423953 at cc6dce0); P5.2 part (startup and periodic sweep in `lib::serve`; contract `store::inv_10_sqlite_restart_keeps` checks an expired message is gone after restart) local (macOS; CI pending) |
| INV-9 | One identical 404 for every gone state | P2.6 | contract `inv_9_unknown_expired_burned_consumed_equal` | covered (CI run 41212423953 at cc6dce0) |
| INV-10 | The store mode is disclosed, and restart behavior matches it | P2.2, P3.1, P3.2, P4.4, P4.5, P5.4 | `INV-10.config-store-mode`, `INV-10.browser-notice`, `INV-10.cli-notice`, `INV-10.memory-restart-loses`, `INV-10.sqlite-restart-keeps` | P2.2 part (`storeMode` in config, contract `http_pages_and_config_match_and_are_allowlisted`) covered (CI run 41212423953 at cc6dce0); P3.1, P3.2 parts (JS `INV-10.browser-notice ...`; `tests/browser` `WEB-create.copy-controls ...`, `WEB-retrieve.warning-before-opening ...`) covered (CI run 41217908068 at 5c1e0f1); P4.4, P4.5 parts (`crates/pinveil-cli/tests/cli` `cli_roundtrip_text_and_files_with_byte_exact_stdout` asserts the notice on `get`; `send` prints the same notice) covered (CI run 41225442086 at d84d2f2); P5 part (contract `store::inv_10_memory_restart_loses`, `store::inv_10_sqlite_restart_keeps` with pending, failed-once, read, burned, and expired messages across a kill and restart; `sqlite::tests` `inv_10_sqlite_rows_survive_reopening`) local (macOS; CI pending) |
| INV-11 | Same-origin assets only, no third-party code | P2.2, P3.4 | `INV-11.request-origins`, `INV-11.csp-header` | P2.2 part (CSP header, contract `inv_11_pages_load_only_same_origin_assets`) covered (CI run 41212423953 at cc6dce0); P3.4 request capture (`tests/browser` `expectNoLeaks`: every browser request stays on the server origin) covered (CI run 41217908068 at 5c1e0f1) |
| INV-12 | Passphrase generation fails closed on a bad wordlist | P1.1, P3.4 | `INV-12.embedded-invalid-refused`, `INV-12.js-http-404`, `INV-12.js-truncated`, `INV-12.js-duplicates` | P1.1 parts covered (CI run 41165815026 at c0d0b65); P3.4 parts (`tests/browser/failures.spec.js` `INV-12.js-http-404`, `.js-truncated`, `.js-duplicates`: the real page shows the error and uploads nothing) covered (CI run 41217908068 at 5c1e0f1) |
| INV-13 | One JSON error shape on every rejection | P2.2, P2.5, P2.6 | contract `inv_13_framework_errors_use_the_json_shape` (unknown route, wrong method), `http_create_every_rejection` (missing, duplicate, unexpected part, malformed multipart, 415), `http_limits_*` (oversize, chunked), `http_retrieve_pin_rules_and_burn_sequence` (missing header), `http_static_assets_and_wordlist` (static 404); `harness::assert_error` checks shape and headers on each | covered (CI run 41212423953 at cc6dce0) |
| INV-14 | Two-stage validation: the header before the KDF, the manifest before any output | P1.3, P1.4, P1.5, P1.7 | core `inv_14_*` (envelope, payload, crypto); JS `INV-14.*`; core `inv_14_header_negatives_derive_no_keys` and the JS vector test count key derivations: 0 for every header refusal, 1 otherwise | covered (CI run 41165815026 at c0d0b65) |

## HTTP interface

| Id | Rule | Owner | Tests | Status |
|---|---|---|---|---|
| HTTP-pages | `/` and `/message/{id}` render with the escaped client config, and the same way whether or not the id exists | P2.2 | contract `http_pages_and_config_match_and_are_allowlisted`, `http_pages_render_for_any_id_with_the_id_escaped`; `assets::tests::escaping` | covered (CI run 41212423953 at cc6dce0) |
| HTTP-config | `/api/config` returns exactly the allowlisted fields, with headers, and isn't rate limited | P2.2 | contract `http_pages_and_config_match_and_are_allowlisted` (exact keys, no pepper) | covered (CI run 41212423953 at cc6dce0) |
| HTTP-create | The 201, 400, 413, 415, 429, and 503 cases, exactly at the limit and one byte over, chunked oversize, empty blob gets 400 | P2.5 | service `http_create_validation`; contract `http_create_and_retrieve_round_trip`, `http_create_every_rejection`, `http_limits_*`, `srv_caps_store_full_is_503`, `srv_ratelimit_create_bucket_then_429`; contract `http_limits_non_multipart_oversize_is_413`, `http_create_longest_allowed_ttl_has_a_valid_expiry` | covered (CI run 41212423953 at cc6dce0) |
| HTTP-retrieve | Check order, a missing or malformed PIN doesn't count an attempt, 403 then burn then 404, 503 on storage error or full admission, HEAD refused without effect | P2.6 | contract `http_retrieve_pin_rules_and_burn_sequence`, `acc_403_live_id`; service `inv_6_commit_failure_no_bytes`; contract `http_retrieve_head_never_consumes_or_counts` (HEAD is 405; removing it lets HEAD consume) | covered (CI run 41212423953 at cc6dce0) |
| HTTP-headers | Security headers on every response, errors included | P2.2 | `harness::assert_security_headers` on pages, static, config, 201, 200, and every error | covered (CI run 41212423953 at cc6dce0) |
| HTTP-limits | Body limit = blob limit + bounded multipart overhead | P2.5 | contract `http_limits_exact_blob_boundary_and_oversize_bodies`, `http_limits_chunked_oversize_body`; contract `http_limits_non_multipart_oversize_is_413` (Content-Length and chunked) | covered (CI run 41212423953 at cc6dce0) |
| HTTP-static | Embedded static files with correct types; every `web/static` file is embedded; the served wordlist is the validated one | P2.2 | `assets::tests::every_web_static_file_is_embedded`; contract `http_static_assets_and_wordlist` | covered (CI run 41212423953 at cc6dce0) |

## Server

| Id | Rule | Owner | Tests | Status |
|---|---|---|---|---|
| SRV-ids | Id format, CSPRNG, the 10-collision limit (id-source seam) | P1.2, P2.3 | core `srv_ids_message_id_format`; service `srv_ids_collision_limit`; contract `http_create_and_retrieve_round_trip` (format) | covered (CI run 41212423953 at cc6dce0) |
| SRV-caps | Atomic count and byte caps under concurrent creates, sweep before refusing, expired ids released at once | P2.3, P5.3 | store `srv_caps_count_bytes_and_reclaim_expired`; contract `srv_caps_store_full_is_503`; SQLite (P5.3); service `srv_caps_concurrent_creates_respect_the_count_cap`, `srv_caps_concurrent_creates_respect_the_byte_cap`, `srv_caps_reclamation_under_contention` | P2.3 parts covered (CI run 41212423953 at cc6dce0); P5.3 part (`sqlite::tests` `sql_caps_count_bytes_sweep_before_refusing_and_reclaim_expired_ids`; contract `srv_caps_store_full_is_503` on SQLite) local (macOS; CI pending) |
| SRV-serialize | Same id serialized, different ids concurrent | P2.6 | service `srv_serialize_different_ids_verify_concurrently` (rendezvous verifier; one lock stripe makes it fail), `inv_6_parallel_same_id_releases_once` | covered (CI run 41212423953 at cc6dce0) |
| SRV-hash-pool | Permit limit, queue limit (503), memory budget, 13-hash default | P2.1, P2.7 | config `cfg_hash_concurrency_formula`; service `srv_hash_pool_queue_full_is_503` (gate verifier) | covered (CI run 41212423953 at cc6dce0) |
| SRV-admission | In-flight bodies, pending uploads, and admitted retrievals (hashing plus waiting for a lock or permit) are bounded. A cancelled request keeps its slot until its commit finishes. | P2.7 | contract `srv_admission_pending_uploads_are_bounded`; service `srv_admission_cancelled_request_still_commits_its_attempt` (no spawned retrieval makes it fail); service `srv_admission_same_id_waiters_are_bounded`, `srv_admission_unrelated_ids_sharing_a_stripe_are_bounded`, `srv_admission_cancelled_waiter_keeps_its_slot_until_done` (waiting instead of refusing makes them fail) | covered (CI run 41212423953 at cc6dce0) |
| SRV-ratelimit | Fixed windows, key cap, peer address by default, trusted proxies walked right to left, spoofed left-most entry, malformed header falls back, only POST create and GET retrieve count (HEAD and other methods don't) | P2.8 | `ratelimit` unit tests; contract `srv_ratelimit_*` (removing the trusted-peer check reproduces the Java bypass and fails; putting HEAD under the limiter fails `srv_ratelimit_only_post_create_and_get_retrieve_count`) | covered (CI run 41212423953 at cc6dce0) |
| SRV-logs | No PIN, header value, query string, or body content in logs | P2.9 | contract `srv_logs_never_contain_pins_headers_or_bodies` (logging headers makes it fail) | covered (CI run 41212423953 at cc6dce0) |
| SRV-failpoints | The fault interface exists only in test builds. Release binaries lack it. | P5, P6.3 | SQLite faults are in-process: `SqliteStore::fail_at` and the checks it drives are `#[cfg(test)]`, so no non-test build contains them, and the store and service tests drive every commit point directly. No `test-failpoints` Cargo feature exists. P6.3 adds a release check that the shipped binary has no fault interface | P5 part local (macOS; CI pending); P6.3 planned |

## SQLite store

| Id | Rule | Owner | Tests | Status |
|---|---|---|---|---|
| SQL-schema | Schema version, `protocolVersion` stamp, incompatible schema refused | P5.1 | `sqlite::tests` `sql_schema_version_and_protocol_stamp_are_written`, `sql_schema_incompatible_databases_are_refused` (other tables, missing columns, other version, not a database) | local (macOS; CI pending) |
| SQL-pragmas | WAL and `synchronous=FULL` set and verified, WAL bounded | P5.2 | `sqlite::tests` `sql_pragmas_wal_and_full_sync_are_verified` (WAL, `synchronous=2`, `journal_size_limit`, checkpoint) | local (macOS; CI pending) |
| SQL-lock | A second process is refused | P5.2 | `sqlite::tests` `sql_lock_refuses_a_second_store_on_the_same_file`; contract `store::sql_lock_second_server_on_the_same_store_is_refused` | local (macOS; CI pending) |
| SQL-pepper | A pepper fingerprint mismatch refuses startup | P5.1 | `sqlite::tests` `sql_pepper_mismatch_refuses_to_open`; contract `store::sql_pepper_changed_pepper_refuses_to_start` (the pepper is never printed) | local (macOS; CI pending) |
| SQL-faults | Commit failures before and after consume, attempt, and burn. Busy and full storage map to 503. | P5.3, P5.4 | `sqlite::tests` `sql_faults_before_commit_roll_back_and_after_commit_never_release_bytes` (insert, attempt, burn, consume, before and after commit); service `sql_faults_service_answers_503_and_never_sends_bytes` (503, no bytes, consumed at most once). Busy and full map to `StoreError`, hence 503. | local (macOS; CI pending) |
| SQL-races | The sweeper and attempt handling never recreate a row | P5.4 | `sqlite::tests` `sql_races_sweeper_and_attempts_never_recreate_a_row` (threads racing the sweeper and attempts); `sql_store_contract_generations_attempts_burn_consume` | local (macOS; CI pending) |
| SQL-contract | The full contract suite passes in `sqlite` mode, nothing skipped | P5.4 | CI step `Contract suite on SQLite` (`PINVEIL_CONTRACT_STORE=sqlite`) runs all contract tests with the SQLite store on every target; `web.rs` asserts the reported store mode, so the suite fails if SQLite isn't in use | local (macOS; CI pending) |

## Configuration

| Id | Rule | Owner | Tests | Status |
|---|---|---|---|---|
| CFG-precedence | Flags > env > TOML > defaults. `--config` before `PINVEIL_CONFIG`. Comma-separated lists. A repeated single-value option or unknown key is an error. | P2.1 | config `cfg_precedence_*` | covered (CI run 41212423953 at cc6dce0) |
| CFG-validate | TTLs (whole seconds, within min/max, max at most 3650 days), impossible limits, KDF range, store path, ZIP32 bounds, hash budget below one hash: startup refused | P2.1 | config `cfg_validate_refuses_impossible_settings`, `cfg_defaults_match_app_md`; contract `cfg_validate_refuses_to_start_and_warns`; config cases for fractional TTLs and `max_ttl` over 3650 days; contract `cfg_validate_refuses_to_start_and_warns` | covered (CI run 41212423953 at cc6dce0) |
| CFG-pepper-warn | An empty pepper logs a warning; the pepper never appears in Debug output | P2.1 | config `cfg_defaults_match_app_md`, `pepper_never_appears_in_debug_output`; contract `cfg_validate_refuses_to_start_and_warns` | covered (CI run 41212423953 at cc6dce0) |

## Crypto and format

| Id | Rule | Owner | Tests | Status |
|---|---|---|---|---|
| FMT-outer | Envelope v2: framing, header fields, exact size computation | P1.3 | core `fmt_outer_*`; JS `FMT-outer ...` | covered (CI run 41165815026 at c0d0b65) |
| FMT-inner | Manifest: text and files, ceilings vs creation limits, 255-byte names, exact consumption | P1.4 | core `fmt_inner_*`; JS `FMT-inner ...` | covered (CI run 41165815026 at c0d0b65) |
| FMT-fuzz | Random bytes never panic or read out of bounds | P1.3, P1.4 | core proptest `fmt_fuzz_*`; JS `FMT-fuzz ...` | covered (CI run 41165815026 at c0d0b65) |
| FMT-vectors | JS and Rust vectors in both directions, negative vectors rejected by both | P1.7 | core `tests/vectors.rs` `fmt_vectors_*`; JS `tests/js/vectors.test.js` `FMT-vectors ...` | covered (CI run 41165815026 at c0d0b65) |
| GEN-pin | PIN uniform and zero-padded, rejection-sampling boundaries, entropy failure aborts | P1.2 | core `gen_pin_*`, `gen_rejection_boundary_for_7776`, `gen_entropy_failure_aborts`; JS `GEN-pin ...` | covered (CI run 41165815026 at c0d0b65) |
| GEN-passphrase | Unbiased sampling from the validated list | P1.2 | core `gen_passphrase_uses_the_validated_list`; JS `GEN-passphrase ...` | covered (CI run 41165815026 at c0d0b65) |
| EXP-names | Export-name policy: separators, controls, Windows-invalid characters, device names (including superscripts), trailing dots and spaces, case-insensitive duplicates, 255-byte limit with long extensions | P1.6 | core `cli_output_export_names_match_shared_vectors`, `export_names_are_unique_short_and_safe`; JS `export names ...`; both read `tests/vectors/export-names.json` | covered (CI run 41165815026 at c0d0b65) |
| ZERO-keys | KEK, content key, KDF input, and plaintext buffers are zeroized; AES, AES-KW, and GHASH state is wiped on drop | P1.5 | compile-time `wiped_on_drop::<Aes256Gcm>()` and `::<KwAes256>()` in `crypto.rs`; `zeroize` features on aes-gcm, aes-kw, hmac, sha2. JS can't wipe Web Crypto state; it clears its own byte arrays. | covered (CI run 41165815026 at c0d0b65) |
| FMT-canonical | Canonical encoding (APP.md): fixed key order, no whitespace or BOM, minimal escaping, well-formed strings | P1.3, P1.4, P1.7 | vectors `escaping in manifest strings ...`, `byte-order mark before the manifest`, `lone high surrogate ...`, `lone low surrogate ...`, BOM text positives | covered (CI run 41165815026 at c0d0b65) |
| KDF-range | The accepted KDF range (provisionally 600,000 to 2,000,000) is benchmarked on representative browsers and CLI targets before v1 freezes it | P7 (decision) | benchmark report with timings at both ends | planned |

## Web UI

| Id | Rule | Owner | Tests | Status |
|---|---|---|---|---|
| WEB-create | Text plus files, generated editable PIN, copy controls and warnings, limits only from config, exact size check before upload | P3.1 | JS `compose.test.js` `WEB-create.*` (exact size equals the real blob size, one byte over refused, limits, TTL range), `payload.test.js` `WEB-create.measure-before-read ...`; `tests/browser` `WEB-create.generated-pin`, `.pin-override`, `.copy-controls`, `.exact-size` (refused before upload), `.limits-from-config`, `.ttl`; `tests/browser/robustness.spec.js` `WEB-create.ttl-default *` (non-preset default, no presets with minute and sub-minute ranges: selected, posted, created), `WEB-create.check-before-read` (an oversized selection reads no file; a file that grew after picking is refused before upload; both fail when the read moves first or the recheck is removed) | covered (CI run 41217908068 at 5c1e0f1) |
| WEB-retrieve | Warning before opening, file list, per-file downloads, export names | P3.2 | JS `WEB-retrieve.open-errors ...`; `tests/browser` `WEB-retrieve.warning-before-opening ...`, `.missing-fragment ...`, `WEB-roundtrip.files-only ...` (original names shown, export names downloaded); `robustness.spec.js` `WEB-retrieve.lost-in-transit` (a proxy cuts the body after 200: no retry, one retrieve request), `WEB-retrieve.copy-fallback` (denied clipboard selects the decrypted text) | covered (CI run 41217908068 at 5c1e0f1) |
| WEB-zip | Store-only zip read by independent readers, ZIP32 bounds | P3.3, P3.5 | JS `zip.test.js` `WEB-zip.*` (Python `zipfile` and `unzip -t` read names, contents, stored method, UTF-8 flag; ZIP32 bounds refused); `tests/browser` `WEB-roundtrip.files-only ...` reads the downloaded "Download all" zip with Python | covered (CI run 41217908068 at 5c1e0f1) |
| WEB-roundtrip | Opens Rust-made blobs (P3), and real CLI round trips both ways (P4) | P3.4, P4.9 | `tests/browser/rust-blobs.spec.js` `WEB-roundtrip.rust-made *` (every `rust-made.json` vector); `roundtrip.spec.js` browser to browser | P3.4 part covered (CI run 41217908068 at 5c1e0f1); P4.9 part (`tests/browser/cli.spec.js` browser to CLI and CLI to browser through the `tty-run` helper in `crates/pty-probe`, in the CI browser matrix on all four targets with the musl CLI on Linux; CLI to CLI in `crates/pinveil-cli/tests/cli`) covered (CI run 41225442086 at d84d2f2) |

## CLI

| Id | Rule | Owner | Tests | Status |
|---|---|---|---|---|
| CLI-parser | `usage-rs`, unknown and repeated options rejected, positional URL redacted in diagnostics | P4.1 | `args.rs` `cli_parser_*` (help only as an option, not a value), `inv_2_parser_diagnostics_redacted` (malformed links and stray `#` words too) | covered (CI run 41225442086 at d84d2f2) |
| CLI-streams | Prompts on the terminal, error without a terminal, stdout and stderr split, byte-exact redirected text, control sequences neutralized on a TTY unless `--raw` | P4.2 | `display.rs` `cli_output_terminal_control_sequences_neutralized` (every Bidi_Control character); `crates/pinveil-cli/tests/cli` `cli_roundtrip_text_and_files_with_byte_exact_stdout`, `cli_output_terminal_neutralizes_control_sequences_unless_raw`, `cli_no_terminal_fails_clearly_before_upload` (both PIN modes, no request at all); the harness `send` asserts the PIN never reaches stdout or stderr (printing it to stderr fails it) | covered (CI run 41225442086 at d84d2f2) |
| CLI-http | Config fetched and validated first, rustls with the platform verifier, no redirects, `--insecure-http` loopback IP literals only, fragment stripped | P4.3 | `http.rs` `cli_http_config_validation`; `links.rs` `inv_1_insecure_http_loopback_only`, `inv_2_cli_share_links_split_the_fragment_and_errors_never_quote_it`; `crates/pinveil-cli/tests/cli` `cli_http_redirects_are_refused`, `cli_http_invalid_config_stops_before_upload` | covered (CI run 41225442086 at d84d2f2) |
| CLI-send | stdin, prompt, or `--edit` (`--edit` with piped stdin is an error), files, generated PIN or `--choose-pin`, store-mode notice | P4.4 | core `cli_send_measure_matches_encoded_length_without_file_data`; `crates/pinveil-cli/tests/cli` `cli_roundtrip_*`, `cli_send_choose_pin_and_get_with_the_positional_link`, `cli_send_refuses_an_oversized_message_before_upload`, `cli_exit_codes_per_failure_class` (`--edit` with piped stdin) | covered (CI run 41225442086 at d84d2f2) |
| CLI-get | Hidden URL prompt or opt-in positional URL, hidden PIN prompt, store-mode notice | P4.5 | `crates/pinveil-cli/tests/cli` `cli_roundtrip_text_and_files_with_byte_exact_stdout` (prompts not echoed, notice shown), `cli_send_choose_pin_and_get_with_the_positional_link`, `cli_get_wrong_pin_asks_again` | covered (CI run 41225442086 at d84d2f2) |
| CLI-output | Plan first, `create_new`, owner-only, `--force` via temp file and rename that never follows links, collision keeps the message, cleanup on partial failure | P4.6 | `output.rs` `cli_output_*` (export names, quit/elsewhere/overwrite, links never followed, partial failure cleanup, `cli_output_directory_swapped_during_the_prompt_is_not_followed`); `crates/pinveil-fs` tests (exclusive create, 0600/0700 on Unix, links never followed, fd-relative writes survive a directory swap on Unix, an open directory and its parent can't be renamed on Windows, owner-only ACL under a parent granting Everyone on Windows, a redirected junction can't redirect operations on Windows, volumes without ACL support refused on Windows, read after replacement by rename); `crates/pinveil-cli/tests/cli` `cli_output_collision_prompt_keeps_existing_files_on_quit` | covered (CI run 41225442086 at d84d2f2) |
| CLI-profiles | Precedence, URL validation, no secrets or history persisted | P4.7 | `profile.rs` `cli_profile_*` (incl. `cli_profile_file_errors_never_quote_the_file`, `cli_profile_invalid_loaded_profiles_are_never_saved_again`); `links.rs` `cli_profile_server_urls_reject_credentials_query_and_fragment`; `crates/pinveil-cli/tests/cli` `cli_profiles_supply_the_server_and_never_store_secrets` (profile TTL used, `--ttl` overrides, file inspected for PIN, link, passphrase, text) | covered (CI run 41225442086 at d84d2f2) |
| CLI-editor | Owner-only temp file removed on success and on error | P4.7 | `crates/pinveil-cli/tests/cli` `cli_editor_temp_file_is_removed_on_success_and_on_error` (in-place save, save by replacement, failing editor; spaced, quoted editor path; the private directory is gone each time); `editor.rs` `cli_editor_command_parsing`; private directory ACL in `crates/pinveil-fs` | covered (CI run 41225442086 at d84d2f2) |
| CLI-exit | Exit code per failure class | P4.8 | `crates/pinveil-cli/tests/cli` `cli_exit_codes_per_failure_class` (2, 3, 5, 6), `cli_output_collision_prompt_keeps_existing_files_on_quit` (7), `cli_http_*` (3) | covered (CI run 41225442086 at d84d2f2) |

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
| 13 consume before delivery, wrong passphrase consumes, ids in logs | accepted | `ACC-wrong-passphrase-consumes` (P3.4, `tests/browser/roundtrip.spec.js`), UI notice tests (P3.2, `WEB-retrieve.warning-before-opening ...`) |
| 13a restart disclosure | fixed | INV-10 |
| 14 leftovers | moot | n/a |
