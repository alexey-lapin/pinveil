# Security policy

## Reporting a vulnerability

Please report security issues privately through GitHub's **Report a vulnerability**
(Security → Advisories) rather than a public issue. We aim to acknowledge within a few
days.

## Threat model (summary)

The server is assumed honest but curious: it may read everything it stores. It never
receives anything that helps decrypt a message except the PIN, which it sees by design
and stores only as a salted, peppered Argon2id hash.

- The passphrase lives in the link's fragment (`#...`), which browsers and the CLI never
  send. Message text, file names, and file types are encrypted on the sender's device.
- The PIN alone can't resist offline guessing. Its job is online gating: three wrong PINs
  delete the message. Someone with a copy of the store gets ciphertext, hashes, and
  metadata, never passphrases.
- A message can be opened at most once, and opening deletes it even if decryption then
  fails, for example because of an incomplete link.

**In scope:** confidentiality of message content against the server and the network,
at-most-once delivery, PIN brute force over the network, and the CLI and browser
handling hostile message content (file names, terminal escapes).

**Out of scope:** a compromised sender or recipient device, an attacker who gets both the
link and the PIN, traffic analysis (sizes and timing), and availability against a
determined flood. TLS is the operator's job.

The full threat model, with the accepted tradeoffs and the tests that check them, is in
[docs/threat-model.md](docs/threat-model.md). The specification, including every invariant,
is in [APP.md](APP.md).

## Distribution integrity

Releases publish `checksums.txt` next to the archives, and the Homebrew formula and Scoop
manifest pin the same SHA-256 hashes. Install from the GitHub releases, the tap, the
bucket, or `ghcr.io/alexey-lapin/pinveil`.
