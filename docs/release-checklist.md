# Release checklist

Items each release must check before tagging. Phase 6 adds the build, signing, and
publishing steps.

## Store compatibility

- If the release changes the envelope format or adds a protocol generation, every client it
  ships (browser UI and CLI) must still decode every generation that can still be live in a
  `sqlite` store, until those messages expire. Otherwise the release notes must tell
  operators to drain the store (stop accepting new messages and wait for the maximum TTL)
  before upgrading. The server stays opaque either way: it never parses blobs
  (`inv_3_arbitrary_bytes_roundtrip` runs in both store modes).
- If the release changes the `SQLite` schema, it must migrate version `1` stores or refuse
  them with a clear message. It must never open an unknown version.
