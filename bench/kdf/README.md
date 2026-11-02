# KDF benchmark

From the repository root, with Rust, Node 24+, and the dependencies/browsers from
`tests/browser` already installed:

```sh
node bench/kdf/run.mjs --output bench/kdf/results.json
```

The default is seven measured runs and one warm-up per endpoint. `--runs` accepts an
odd number from five through 1001. `--firefox /path/to/firefox` chooses a stock Firefox for
the WebDriver BiDi fallback if the bundled Playwright Firefox cannot launch.

The runner uses a temporary loopback server on an allocated port. It does not use
or restart the application server, change dependencies, or invoke a test suite.
Chromium, WebKit, and bundled Firefox are loaded from the existing Playwright
installation. Only one environment is measured at a time.

Both private derivation functions are extracted unchanged from the production
sources, with their hashes recorded in the results. The Rust runner uses `cargo
build --release --locked --offline -p pinveil-core` to locate the exact dependency
rlibs, then compiles `derive.rs` with `rustc -C opt-level=3`. No Cargo member,
production example, exported helper, or automatically discovered test is added.
Generated Rust code and the executable go in ignored `.build/`.

Timings include input preparation, PBKDF2 key import on JS, and one complete
256-bit PBKDF2 derivation, including the production input-wipe path. AES wrapping,
output checks, browser startup, and compilation are outside the timer. The fixed
benchmark inputs are public and must never be used as message secrets. The runner
checks Rust's raw KEK against Node and checks the browsers' non-extractable KEKs by
wrapping a fixed content key and comparing it with Node outside the timer.

Each warm-up and sample uses a distinct deterministic 16-byte salt. Repeating one
salt can benchmark NSS's PBKDF2 result cache instead of a derivation. Salt generation
is outside the in-page/Rust timers. The system Firefox fallback times individual
awaited derivations from Node; those samples include the BiDi command/response
overhead and salt construction. A separate no-op round-trip measurement is reported
without subtracting it from the KDF samples.

The CDP 4x row also measures a JS arithmetic loop. Compare its slowdown with the
KDF slowdown: native Web Crypto work may not be throttled, so that row alone cannot
establish a low-end phone latency bound. See the report in `docs/kdf-benchmark.md`.
