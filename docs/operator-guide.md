# Operator guide

This guide is for people who run `pinveil-server`. [APP.md](../APP.md) is the full spec; this
page covers the decisions an operator has to make and the ways a deployment goes wrong.

## Checklist

Before you point real users at a server:

- It is reachable only over HTTPS.
- `pin.pepper` is set to a long random secret, and it will be the same on every start.
- The reverse proxy accepts request bodies of at least `max_payload_bytes` plus 64 KiB.
- `server.trusted_proxies` lists the proxy, so rate limits count clients, not the proxy.
- You chose a store mode on purpose, and in `sqlite` mode the database is on a local
  persistent volume.

## Running it

The image runs the static Linux binary as UID 65532, listens on 8080, and has an empty,
writable `/data` for the SQLite store. Nothing else is in it: no shell, no package manager.

```sh
docker run -d --name pinveil -p 127.0.0.1:8080:8080 \
  -e PINVEIL_PIN__PEPPER="<a stable secret>" \
  ghcr.io/alexey-lapin/pinveil:0.1.0
```

`-p 127.0.0.1:8080:8080` publishes the port on the host's loopback interface only. Plain
`-p 8080:8080` listens on every interface, so users could bypass the HTTPS proxy and send their
PIN in the clear. If the proxy runs in a container too, put both on a private network and don't
publish the server's port at all.

The archive binary works the same way:

```sh
PINVEIL_PIN__PEPPER="<a stable secret>" pinveil-server --listen 127.0.0.1:8080
```

`pinveil-server --version` prints the version. The server prints one line to stdout,
`listening on <address>`, and logs to stderr. It has two flags, `--listen ADDR` and
`--config PATH`; everything else is configuration.

Run one server per store. Multiple instances behind a load balancer are not supported in
either mode: each would have its own memory store, and in `sqlite` mode the second process is
refused at startup.

## Configuration

Settings come from four places. The first one that sets a key wins:

1. Command-line flags (`--listen` only).
2. Environment variables: `PINVEIL_` plus the key in upper case, with `.` written as `__`.
   `store.mode` becomes `PINVEIL_STORE__MODE`. Lists are comma-separated.
3. A TOML file, from `--config PATH` or the `PINVEIL_CONFIG` variable.
4. Defaults.

An unknown key is an error, from any source, so a typo stops the server instead of being
ignored. The same file as environment variables:

```toml
[server]
listen = "0.0.0.0:8080"
trusted_proxies = ["10.0.0.0/8"]

[messages]
max_ttl = "7d"
default_ttl = "1h"
ttl_presets = ["15m", "1h", "1d"]
max_payload_bytes = 52428800

[store]
mode = "sqlite"
path = "/data/pinveil.db"

[pin]
pepper = "<a stable secret>"
```

The pepper is a secret, so prefer the environment variable or a file only the server's user
can read. Durations are whole numbers with a unit: `90s`, `15m`, `2h`, `7d`.

The full key list with defaults is in [APP.md, Configuration](../APP.md#configuration). The
ones you are most likely to change:

| Key | Default | Notes |
|---|---|---|
| `messages.min_ttl`, `max_ttl` | `1m`, `24h` | The range senders can pick from. `max_ttl` can be at most 3650 days. |
| `messages.default_ttl`, `ttl_presets` | `15m`; `15m,30m,1h` | What the browser offers. All must be inside the range. |
| `messages.max_payload_bytes` | 25 MiB | The largest encrypted message. Your proxy's body limit must be higher. |
| `store.max_messages`, `max_bytes` | 1000, 256 MiB | Caps on what the store holds. Over a cap, creates get 503. |
| `pin.max_failed_attempts` | 3 | Wrong PINs before the message is deleted. |
| `rate_limit.create_per_window`, `retrieve_per_window` | 10, 30 per minute | Per client address. |
| `messages.pbkdf2_iterations` | 600,000 | Client-side key derivation cost for new messages, from 600,000 to 2,000,000. See [the KDF benchmark](kdf-benchmark.md). |

### Message ids

`message_id.template` sets the shape of message ids. The default, `{word:3}-{digits:6}`,
gives ids such as `barcode-unbalance-cloak-102856`.

| Placeholder | Produces | Bits |
|---|---|---|
| `{word}`, `{word:N}` | N EFF words joined with `-` | 12.9 per word |
| `{digits:N}` | N digits | 3.3 per digit |
| `{alnum:N}` | N characters from `a-z0-9` | 5.2 per character |
| `{Alnum:N}` | N characters from `A-Za-z0-9` | 6.0 per character |

```toml
[message_id]
template = "msg_{alnum:10}"   # msg_k7q2x8f2m4, 51.7 bits
min_bits = 32
```

N is 1 to 64. Text between placeholders may use only `A-Z a-z 0-9 - . _ ~`. The template can
be at most 256 bytes, and its longest possible id at most 128 characters. The server logs the
template's bits at startup and refuses to start when they fall below `min_bits`. It also
warns when the template allows fewer than 1000 ids per `store.max_messages`.

A template close to the 32-bit floor can be guessed at scale: with 1,000 live messages, about
4.3 million guesses find one of them, and the 403 for a wrong PIN tells the guesser the id is
live. Rate limits slow this down but don't stop a patient attacker. Keep the default unless
you need something else, and stay well above the floor.

Changing the template doesn't affect stored messages: their links keep working. Ids with
uppercase letters or `. _ ~` need a `pinveil` CLI newer than 0.1.0; 0.1.0 rejects those links.

The server checks the configuration at startup and refuses to start if something can't
work: presets outside the TTL range, a missing `store.path` in `sqlite` mode, an Argon2
memory budget smaller than one hash, or limits the browser's zip writer can't handle.

### Memory and CPU

PIN checks use Argon2id with 19 MiB per hash by default. `pin.max_hash_memory_bytes`
(256 MiB) caps the memory all hashes in flight may use. With the defaults that allows up to
13 at once, fewer on a machine with fewer CPUs; requests beyond that queue, up to
`pin.max_queued_hashes` (64), and then get 503. Stored messages count against
`store.max_bytes`, and uploads in flight against `server.max_concurrent_uploads` (16). As a
baseline for the container's memory limit, add the hash budget, the store cap, and about
`max_concurrent_uploads` times `max_payload_bytes` for uploads, then leave headroom for the
runtime and temporary copies.

## Store modes

| | `memory` (default) | `sqlite` |
|---|---|---|
| A restart | deletes every message | keeps every message |
| Deleted messages | gone from memory after the next sweep | may stay recoverable on disk |
| Needs | nothing | a local persistent volume |

Users are told which mode you run: the browser's "How it works" dialog says it, and both
CLI commands print it. Pick the mode you can describe honestly.

`memory` suits a server that restarts rarely and where losing unread messages on a deploy is
fine. The server doesn't write messages to disk in this mode. The operating system still may,
through swap or a crash dump, and request lines go to the logs; disable swap and core dumps
for the server if that matters to you.

`sqlite` survives restarts and deploys. The cost is that deleted data can stay on disk: in
free pages of the database file, in the write-ahead log, in filesystem snapshots, and in
backups. It is still ciphertext, but it outlives the message. Encrypt the volume if that
matters to you.

In `sqlite` mode:

- Put the database on a local volume. Network filesystems break SQLite's locking.
- The server holds an exclusive lock on `<path>.lock` while it runs. A second server on the
  same database refuses to start, and so does one reached through a symlink or a hard link.
- The server sweeps expired messages at startup and every minute, and tries to truncate the
  write-ahead log at each sweep. A program holding the database open, such as a backup tool,
  can delay that; the server logs it and tries again at the next sweep.
- A full disk or a lock held too long by another program returns 503 to clients; nothing is
  half-written.
- The server refuses to open a database with an unexpected schema.

## The pepper

The pepper is a server secret mixed into every PIN hash. Someone who copies the store but not
the pepper can't test PINs against the hashes.

Set it once and keep it. If the pepper changes, every stored PIN hash stops matching, so a
correct PIN would count as a wrong attempt and three tries would delete the message. In
`sqlite` mode the server stores a fingerprint of the pepper and refuses to start when the
configured one doesn't match. Its error never prints the pepper.

To change the pepper in `sqlite` mode, stop the server, delete the database file and its
`-wal`, `-shm`, and `.lock` files, and start with the new pepper. Every unread message is
lost. In `memory` mode a restart loses them anyway, so you can change it at any restart.

An empty pepper works, and the server logs a warning at startup.

## HTTPS and reverse proxies

The server speaks plain HTTP. Put it behind a proxy that terminates TLS (nginx, Caddy,
Traefik, a cloud load balancer). Without TLS, anyone on the network sees the PIN and the
ciphertext; with both, the passphrase is the only protection left.

The proxy must:

- Allow request bodies of at least `max_payload_bytes` plus 64 KiB. With the defaults that is
  about 25.1 MiB. nginx defaults to 1 MiB (`client_max_body_size`).
- Not cache anything. The server sends `Cache-Control: no-store` on every response; don't
  override it.
- Pass the `X-Message-Pin` request header through unchanged, and never log it.
- Not rewrite the response headers, in particular the Content-Security-Policy.

Then set `server.trusted_proxies` to the proxy's address or CIDR. Only for a trusted peer does
the server read `X-Forwarded-For`, and it takes the right-most address that isn't itself a
trusted proxy. If you don't set it, every request appears to come from the proxy, and the
rate limits apply to all your users together. If you set it too wide, clients can choose
their own rate-limit bucket.

An nginx example:

```nginx
server {
    listen 443 ssl;
    server_name secrets.example.com;
    # ssl_certificate ... ;

    client_max_body_size 26m;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header Host $host;
    }
}
```

### Private certificate authorities

The CLI verifies certificates with the operating system's trust store and has no option to
skip verification. For a server with a certificate from a private CA, add the CA to the trust
store of each machine that runs the CLI:

- macOS: add it to the System keychain and mark it trusted, for example
  `sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ca.pem`.
- Linux, Debian and Ubuntu: `sudo cp ca.pem /usr/local/share/ca-certificates/pinveil-ca.crt`
  then `sudo update-ca-certificates`. The file name must end in `.crt`, even for a PEM file.
  Fedora and Arch: `sudo trust anchor ca.pem`.
- Windows: import it into "Trusted Root Certification Authorities", for example
  `certutil -addstore -f Root ca.pem` from an elevated prompt.

Browsers use the same store on macOS and Windows; Firefox and some Linux browsers keep their
own.

The CLI's `--insecure-http` flag allows plain HTTP only to a loopback IP address
(`127.0.0.1`, `[::1]`). It is for local testing and does nothing for remote servers.

## Logs

The server logs one line per request to stderr: method, path, status, and duration. It never
logs headers, query strings, or bodies, so PINs and message content stay out of the logs.

The path of a retrieval contains the message id. Ids are random, and with the default
template (about 58.7 bits) they can't be guessed; how hard a custom template is to guess
depends on its bits (see [Message ids](#message-ids)). The log shows when someone tried to
open each message. The create request's line has no id. Keep access logs, including your proxy's, as private as the server.

`PINVEIL_LOG` sets the level, with `tracing` filter syntax: `warn`, `info` (default), or
`pinveil_server=debug`.

## Backups

In `memory` mode there is nothing to back up.

In `sqlite` mode you can copy the database, but think about what a restore does. A backup
holds every message that was unread when it was taken. Restoring it brings those messages
back, including ones that were opened or deleted since, which breaks the promise that a
message can be read once. The server can't detect this.

If you back up the database:

- Use SQLite's online backup (`sqlite3 pinveil.db ".backup /backups/pinveil.db"`) or stop the
  server first. Copying the file while the server writes to it can give you a corrupt copy.
- Treat backups as holding message ciphertext, PIN hashes, and expiry times, and keep them as
  private as the server. Don't keep them longer than `max_ttl`; after that, every message in
  them has expired anyway.
- Restore only to recover from losing the disk, and accept that messages read since the backup
  can be read again until they expire.

Many operators will decide not to back up at all: messages live for minutes or hours, and the
sender can send a new one.

## Upgrades

Upgrading the server keeps the stored messages in `sqlite` mode. The release notes say when a
release changes the message format or the database schema.

If a release changes the message format, the new browser page has to be able to read the
messages already stored, or those messages become unreadable. A release that drops an older
format says so, and you then have two options:

- Drain the store. Block `POST /api/messages` at the reverse proxy, keeping the pages and
  retrievals working. From the last accepted create, wait at least the longest TTL a stored
  message could have, then upgrade. That is `max_ttl`, or its earlier value if you lowered
  it recently, since a lower limit doesn't shorten messages already stored.
- Accept that messages created before the upgrade can't be opened.

The CLI is a separate install. A link opens in any client that understands its format, so
update CLIs along with the server when a release says the format changed.

### From 0.0.x

0.1.0 is a rewrite and keeps nothing from the Java server:

- Settings moved from `APP_*` variables to `PINVEIL_*` (see above), and the old names are
  ignored. The pepper is `PINVEIL_PIN__PEPPER`. The
  [v0.1.0 release notes](release-notes/v0.1.0.md) map every old name to its new one.
- `X-Forwarded-For` is trusted only from `server.trusted_proxies`. Set it if a proxy sits in
  front, or every user shares the proxy's rate limit.
- The image is built `FROM scratch` and runs as UID 65532. A volume mounted at `/data` must be
  writable by that user.
- Unread messages in the old in-memory store are lost, and links made by 0.0.x can't be
  opened by 0.1.0.
