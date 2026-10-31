# Release checklist

## Cutting a release

1. Check the items below.
2. In a pull request, set `version` under `[workspace.package]` in `Cargo.toml` to the new
   version: `X.Y.Z`, or `X.Y.Z-rc.N` for a candidate. The binaries print this version, and
   the release tag must match it.
3. After merging, run **Release (dispatch)** on `main`. It tags `vX.Y.Z` and calls the
   release workflow. (Pushing the tag by hand works too.)
4. The workflow builds every target, tests the exact artifacts (version, embedded assets,
   no fault interface, CLI round trips over HTTP and over HTTPS with a private CA, the
   image with a SQLite volume, `brew install` from a temporary formula), then stages them
   (draft release, candidate image tag) and publishes in a fixed order: GitHub release,
   image tags, then the Homebrew formula and Scoop manifest. A candidate (`-rc.N`) stops
   after the prerelease and its version image tag.
5. If publishing fails partway, the job summary lists what is live. Re-run the publish job
   only; finished steps are no-ops.

Repository secrets the publish job needs (fine-grained PATs, Contents: read and write):

- `TAP_GITHUB_TOKEN` for `alexey-lapin/homebrew-tap`
- `SCOOP_GITHUB_TOKEN` for `alexey-lapin/scoop-bucket`

Pushes to `feature/**` that change `packaging/` or the release workflows run the same
build and tests as a dry run, with nothing staged or published.

## Store compatibility

- If the release changes the envelope format or adds a protocol generation, every client it
  ships (browser UI and CLI) must still decode every generation that can still be live in a
  `sqlite` store, until those messages expire. Otherwise the release notes must tell
  operators to drain the store before upgrading: stop accepting new messages, then wait
  until no live rows remain (or until the latest `expires_at` among stored rows has
  passed). Waiting the currently configured maximum TTL is not enough, because rows may
  have been accepted under an earlier, larger maximum. The server stays opaque either way: it never parses blobs
  (`inv_3_arbitrary_bytes_roundtrip` runs in both store modes).
- If the release changes the `SQLite` schema, it must migrate version `1` stores or refuse
  them with a clear message. It must never open an unknown version.
