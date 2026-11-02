# pinveil

Self-hosted, one-time secret sharing. You encrypt some text, files, or both in your browser or
terminal, and get back a link and a 6-digit PIN. The recipient needs both to open the message,
and the server deletes it the moment it hands it out.

The server never sees the plaintext, the file names, or the key. The key lives in the part of
the link after `#`, which browsers and the CLI never send. The server stores an encrypted blob
and a hash of the PIN, and it enforces expiry, one read per message, and a limit of three wrong
PINs (the operator can change that number).

## How a message travels

1. The sender's client generates a passphrase and a PIN, encrypts the message with a key
   derived from both, and uploads only the ciphertext.
2. The sender gets `https://<server>/message/<id>#<passphrase>` and the PIN. Send them through
   different channels, for example the link by email and the PIN by chat.
3. The recipient opens the link and enters the PIN. The server checks the PIN, deletes the
   message, and returns the ciphertext. The recipient's client decrypts it locally.

A message that nobody opens is deleted when it expires (15 minutes by default). Three wrong
PINs delete it too.

## Install

The server ships as a container image and as archives for Linux, macOS, and Windows. The CLI
ships as archives, a Homebrew formula, and a Scoop manifest. Every release lists SHA-256 hashes
in `checksums.txt`.

| What | Where |
|---|---|
| Server image | `ghcr.io/alexey-lapin/pinveil` (linux/amd64, linux/arm64) |
| Server binary | `pinveil-server_<version>_<os>_<arch>` on the [releases page](https://github.com/alexey-lapin/pinveil/releases) |
| CLI, macOS and Linux | `brew install alexey-lapin/tap/pinveil` |
| CLI, Windows | `scoop bucket add alexey-lapin https://github.com/alexey-lapin/scoop-bucket` then `scoop install alexey-lapin/pinveil` |
| CLI binary | `pinveil_<version>_<os>_<arch>` on the releases page |

Archives exist for `linux_amd64`, `linux_arm64` (static musl builds), `darwin_arm64`, and
`windows_amd64`.

## Run a server

```sh
docker run -d --name pinveil -p 127.0.0.1:8080:8080 \
  -e PINVEIL_PIN__PEPPER="$(openssl rand -base64 32)" \
  ghcr.io/alexey-lapin/pinveil:0.1.0
```

Open <http://localhost:8080>. The port is published on localhost only: the server speaks
plain HTTP, and users must reach it only through an HTTPS proxy. Keep the pepper stable across
restarts. By default messages live in memory, so a restart deletes
them. To keep them across restarts, use the SQLite store:

```sh
docker run -d --name pinveil -p 127.0.0.1:8080:8080 \
  -v pinveil-data:/data \
  -e PINVEIL_PIN__PEPPER="<a stable secret>" \
  -e PINVEIL_STORE__MODE=sqlite \
  -e PINVEIL_STORE__PATH=/data/pinveil.db \
  ghcr.io/alexey-lapin/pinveil:0.1.0
```

The [operator guide](docs/operator-guide.md) covers configuration, TLS, reverse proxies,
backups, and upgrades.

## Use the CLI

Save the server once, then send and receive:

```sh
pinveil profile set work --server https://secrets.example.com --default

echo "db password: hunter2" | pinveil send --ttl 1h
pinveil send report.pdf keys.txt

pinveil get
```

`send` prints the link on stdout and shows the PIN on the terminal only, never on stdout or
stderr, so a link piped into another program never carries the PIN with it. `get` asks for the
link and the PIN at hidden prompts, writes text to stdout, and saves files to the current
directory or `--out DIR`. If a file already exists, it asks whether to overwrite, save
elsewhere, or quit; `--force` overwrites without asking.

A link made in the browser opens with the CLI, and the other way round.

## Good to know

- Opening a message deletes it before it is decrypted. If the link was copied incompletely,
  the message is gone anyway. The page and the CLI say this before you enter the PIN.
- If the connection drops while the message downloads, it is lost. Ask the sender for a new
  one.
- The PIN protects against online guesses when someone has only the link: three wrong
  PINs delete the message. Someone with the link and a copy of the ciphertext can try all
  million PINs offline, and the attempt limit doesn't help there. Treat the whole link as a
  secret and send the PIN separately.
- Message ids appear in the URL path and in the server's access log. They are random and
  can't be guessed, but they are not secret.

## Documentation

- [Operator guide](docs/operator-guide.md): running and maintaining a server.
- [Threat model](docs/threat-model.md): what pinveil protects, against whom, and what it
  doesn't.
- [APP.md](APP.md): the full behavior spec, including the HTTP API, the cryptography, and the
  message format.
- [SECURITY.md](SECURITY.md): how to report a vulnerability.

## License

MIT. The passphrase wordlist is the [EFF large wordlist](https://www.eff.org/dice), CC BY 4.0;
see `NOTICE`.
