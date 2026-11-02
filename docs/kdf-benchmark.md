# KDF range benchmark

Recommendation: keep the accepted range at **600,000 to 2,000,000 iterations**.
The local results support that range; they do not establish a phone latency bound.
No protocol constants or tests were changed.

## Method and machine

Measured on 2026-11-01 local time, starting 2026-11-02 03:22 UTC, on an Apple M4 Pro
with 14 logical CPUs, arm64 macOS 27.2, build 26B123. Source revision:
`3cf76329daa96bd4f275b831abbccbb114020e6a`. Playwright was 1.64.1 and rustc was 1.97.1.

Each endpoint has one untimed warm-up followed by seven serial measured runs; the
table shows medians in milliseconds. The operation is the production KEK
derivation: UTF-8 `passphrase + pin`, PBKDF2-HMAC-SHA-256, a 16-byte salt, and a
256-bit output, represented as a non-extractable AES-KW key in Web Crypto. Input
preparation and the production input wipe are included. Compilation, browser
startup, AES wrapping, and output checks are excluded.

Each warm-up and sample uses a different deterministic salt. Reusing one salt
initially produced sub-millisecond Firefox cache hits, which were discarded.
[Mozilla's NSS implementation caches PBKDF2 results by password, hash, iteration
count, key length, and salt](https://searchfox.org/firefox-main/source/security/nss/lib/softoken/lowpbe.c#899).

The harness extracts the unchanged private helpers from `web/static/crypto.js`
and `crates/pinveil-core/src/crypto.rs`. The Rust helper is compiled with
`rustc -C opt-level=3` against the workspace's locked release dependency rlibs.
This adds no Cargo workspace member, production example, or discovered test.
The last derived KEK at each endpoint is checked against Node's independent
raw-byte derivation; browser KEKs are checked by wrapping a fixed content key.

## Results

| Environment | Version | 600,000 | 2,000,000 |
| --- | --- | ---: | ---: |
| Chromium, headless | 156.0.8078.4 | 38.1 ms | 126.8 ms |
| WebKit, headless | 27.2 | 80.0 ms | 263.0 ms |
| System Firefox, headless | 158.0.1 | 55.1 ms | 186.9 ms |
| Rust CLI derivation, release | rustc 1.97.1 | 42.9 ms | 142.0 ms |
| Node Web Crypto | 24.22.2 | 42.4 ms | 142.2 ms |
| Chromium, requested CDP 4x CPU throttle | 156.0.8078.4 | 41.6 ms | 136.9 ms |

Bundled Playwright Firefox could not find its profile folder. System Firefox was
driven through [WebDriver BiDi](https://developer.mozilla.org/en-US/docs/Web/WebDriver/How_to/Create_BiDi_connection).
Its numbers include the local command/response round trip and salt construction,
timed from Node. The separate no-op round-trip median was 0.36 ms; it was not
subtracted. Chromium and WebKit used in-page `performance.now()`.

The requested 4x throttle slowed the JS calibration loop from 9.2 to 38.6 ms,
about 4.2x, but slowed the upper-end derivation only about 1.08x. It therefore
does **not** measure crypto on a device four times slower. Multiplying the slowest
desktop median by four gives roughly 1.1 seconds at the upper end, but that is a
projection, not an observed phone result. No physical phone or Linux/Windows CLI
target was measured.

## Range decision

Keep the lower endpoint at 600,000. That matches the current
[OWASP PBKDF2-HMAC-SHA-256 guidance](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#pbkdf2),
used here as a work-factor reference. There is no measured performance reason to
lower it.

Keep the upper endpoint at 2,000,000. It limits sender-selected work to about
3.33 times the minimum. All measured upper-end medians were below 0.27 seconds;
the slowest individual sample was 265 ms in WebKit. Raising the ceiling would
allow more recipient CPU use without evidence that the extra delay is acceptable.
The ceiling bounds iterations, not elapsed time: slow hardware, contention, and
multiple messages can cost more than these measurements. The throttle result
must not be described as a validated mobile budget.

Reproduce with `node bench/kdf/run.mjs --output bench/kdf/results.json` after
installing the existing browser-test dependencies and browsers. See the
[runner instructions](../bench/kdf/README.md) and
[raw samples, versions, and helper hashes](../bench/kdf/results.json).
