# Pinveil

Pinveil is a self-hosted service for sending a secret once. The sender's client, a browser or the `pinveil` CLI, encrypts some text, one or more files, or both. The server holds the ciphertext for a limited time. The recipient needs two things to read it, a link and a 6-digit PIN, and the server deletes the message the moment it hands it out.

The server never sees plaintext, file names, or the key material in the link. It stores an opaque blob and a hash of the PIN, and it enforces expiry, one-time reads, and a limit on wrong PIN attempts.

This document is the behavior spec for pinveil v1 in Rust: `pinveil-server`, the browser UI it serves, and the `pinveil` CLI. The design decisions and their reasoning are in `docs/plans/2026-10-30-rust-rewrite-design.md`. Sections marked **Invariant** must hold in every implementation. The last section records the gaps found in the earlier Java version and how v1 handles each one.

## Glossary

| Term | Meaning |
|---|---|
| Client | The browser UI or the `pinveil` CLI. Both implement the same crypto and message format. |
| Message | One stored secret: optional text plus zero or more files (at least one of the two), with its server-side metadata. |
| Blob | The opaque encrypted package a client uploads. The server never parses it. |
| Message id | Server-generated public identifier, e.g. `barcode-unbalance-cloak-102856`. Appears in the URL path. |
| Passphrase | Client-generated secret, 4 diceware words, e.g. `tidal-abacus-cloak-ember`. Lives only in the URL fragment. |
| PIN | 6 decimal digits. Clients generate one by default and the sender may replace it. Gates retrieval on the server and is also an input to the client KDF. |
| TTL | How long the message lives before the server deletes it unread. |
| Burn | Deleting a message after too many wrong PINs. |
| Store mode | `memory` (default) or `sqlite`. Decides whether messages survive a server restart. |

## Invariants

1. Plaintext, file names, and MIME types never leave the sender's client unencrypted, and only the recipient's client ever reconstructs them. The passphrase never reaches the server. The PIN does reach the server by design, over TLS, on create and retrieve. The server stores only its verifier. The one exception to TLS is the CLI's explicit `--insecure-http` development switch, which works only with loopback addresses.
2. The passphrase travels only in the URL fragment (`#...`). No client sends the fragment to the server, and no client writes it to logs or error messages.
3. The server stores the blob as opaque bytes. It does not know whether the payload is text or files, their names, or their types. Size is the only thing it can infer.
4. The decryption key derives from both the passphrase and the PIN. The PIN is weak on its own (at most 19.9 bits). If the link and ciphertext both leak, an attacker can try all million PINs offline, and the server's attempt limit does nothing to stop that. The passphrase carries the real strength.
5. The server never stores the raw PIN. It keeps a salted, peppered, slow hash.
6. A message can be read at most once. The consume step is committed before the server sends the blob. If the commit fails, the server sends no blob.
7. A message dies after `max_failed_attempts` wrong PINs (default 3). Each wrong attempt, and the burn, is committed before the response.
8. A message dies when its TTL runs out, whether or not anyone tried to read it. A retrieval checked after `expiresAt` fails. Physical removal can happen later, during a sweep, so expiry does not mean immediate secure erasure.
9. All error responses for "gone" states are identical. Expired, already read, burned, and never existed all return the same 404 and body.
10. The store mode is disclosed to users. In `memory` mode a restart loses every message. In `sqlite` mode messages survive restarts, and deleted data may remain recoverable on disk. Both clients and the docs say which mode a server runs.
11. No accounts, no analytics, no third-party scripts. Every browser asset comes from the same origin.
12. Passphrase generation fails closed. A client that can't get a complete, valid wordlist (7776 distinct entries) refuses to create the message instead of drawing words from whatever it got.
13. Every error response uses the single JSON shape, including request parsing errors, missing headers or multipart parts, oversized or malformed bodies, unsupported methods or media types, and unknown routes.
14. Clients validate in two stages. Before deriving keys or decrypting, they check the outer framing, the header fields, and that the KDF cost is in the accepted range. After authenticated decryption, and before rendering text, writing files, or building a zip, they check the manifest, its lengths, the file count, the names, and the totals. Details are in [Validation](#validation).

## User flows

### Create in the browser

1. The sender opens `/`.
2. They type optional text and add zero or more files, by drag-and-drop or a file picker. At least one of the two is required.
3. The PIN field holds a freshly generated 6-digit PIN. The sender may replace it. They choose an expiry from the configured presets (15 min, 30 min, 1 hour by default) or Custom, entered in minutes within the server's min and max.
4. On "Generate secure link" the browser:
   1. Validates the PIN, the file count, the manifest size, and the exact size of the final blob against `maxPayloadBytes`.
   2. Fetches and validates the EFF wordlist (invariant 12), then generates a 4-word passphrase.
   3. Builds the inner payload and encrypts it (see [Cryptography](#cryptography)).
   4. POSTs the blob, PIN, and TTL to `/api/messages`.
5. The server returns the message id and expiry time.
6. The browser shows `https://<origin>/message/<id>#<passphrase>` and the PIN, each with a copy button, plus the local expiry time. It tells the sender to send the link and the PIN over different channels, and states the store mode.

### Retrieve in the browser

1. The recipient opens `/message/<id>#<passphrase>`. The server renders a shell page for any id without checking whether the message exists, so the page itself reveals nothing.
2. If the fragment is missing, the page shows an error right away.
3. The page explains, before the PIN is submitted, that opening the message deletes it from the server even if decryption then fails.
4. The recipient enters the PIN. The browser calls `GET /api/messages/<id>/retrieve` with the PIN in the `X-Message-Pin` header.
5. On 200 the browser validates the blob and decrypts locally. Text shows in a `<pre>` with a copy button. Each file shows its name and size as plain text, with its own download button. With more than one file, "Download all" saves them as one zip.
6. On an error the page shows the server's generic message, or a local decryption error if the passphrase was wrong.

A wrong passphrase with a correct PIN still consumes the message. The server can't tell the passphrase is wrong. This is accepted behavior, and the page says so (step 3).

### CLI

`pinveil send` and `pinveil get` do the same as the two browser flows, from a terminal. The [CLI](#cli) section specifies the details. A link made by either client opens in the other.

## HTTP interface

Every error body has one shape (invariant 13):

```json
{ "error": "human-readable message" }
```

### `GET /`

Create page (HTML). Embeds the client config.

### `GET /message/{id}`

Retrieve page (HTML). Puts `id` in `data-message-id`, HTML-escaped. Embeds the client config.

### `GET /static/**`

CSS, JS modules, and `eff_large_wordlist.txt`. All embedded in the server binary.

### `POST /api/messages`

`multipart/form-data` with exactly these parts:

| Part | Type | Rule |
|---|---|---|
| `blob` | file | 1 to `max_payload_bytes` bytes |
| `pin` | text | matches `^\d{6}$` |
| `ttl` | text | integer seconds, `min_ttl` to `max_ttl` inclusive |

| Status | When | Body |
|---|---|---|
| 201 | created | `{"id": "...", "expiresAt": "2026-10-30T23:02:38Z"}` (ISO-8601 UTC) |
| 400 | missing, duplicate, or unexpected part; bad PIN; TTL not an integer or out of range; empty blob; malformed multipart | `{"error": ...}` |
| 413 | blob or request body over the limit | `{"error": "Payload too large"}` |
| 429 | create rate limit hit | `{"error": "Rate limit exceeded"}` |
| 503 | store full, storage error, or 10 id collisions in a row | `{"error": ...}` |

The request body limit is `max_payload_bytes` plus a bounded multipart overhead, enforced for chunked uploads too.

### `GET /api/messages/{id}/retrieve`

The PIN goes in the `X-Message-Pin` request header. Because the endpoint needs a custom header, link previewers and prefetchers can't trigger it by accident.

The server checks in this order:

1. PIN header present and well formed. If not, return 400. Don't look up the message and don't count an attempt.
2. Look up the message. If missing, return 404.
3. Check expiry. If expired, delete it and return 404.
4. Verify the PIN. If wrong, increment the failed count, delete the message when the count reaches `max_failed_attempts`, commit, and return 403.
5. Commit the consume, then return the blob.

Steps 2 to 5 are serialized per message id, so two concurrent retrievals of the same id can't both succeed. Different ids run in parallel.

| Status | Body |
|---|---|
| 200 | raw blob, `Content-Type: application/octet-stream` |
| 400 | `{"error": "Invalid PIN format"}` |
| 403 | `{"error": "Unable to retrieve message"}` (never reveals remaining attempts) |
| 404 | `{"error": "Message unavailable"}` (same for every gone state) |
| 429 | `{"error": "Rate limit exceeded"}` |
| 503 | `{"error": ...}` storage error. No blob is sent. |

With the default limit of 3, wrong PINs return 403, 403, 403. The third one burns the message, and every later request returns 404. A wrong PIN on a live message returns 403 while an unknown id returns 404. That is an accepted part of the contract: ids carry about 58 bits, so nobody can enumerate them.

### Response headers on every response

```
Cache-Control: no-store, max-age=0
Pragma: no-cache
Referrer-Policy: no-referrer
Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
```

Error responses carry them too. `no-store` keeps blobs out of caches. `no-referrer` keeps the URL out of `Referer` headers. The CSP rules out inline scripts and styles, so the client config goes in a `<script type="application/json">` element that the page reads as data.

### Client config

The server embeds this JSON, with safe JSON and HTML escaping, in `<script id="app-config" type="application/json">` on both pages:

```json
{
  "storeMode": "memory",
  "maxPayloadBytes": 26214400,
  "maxFiles": 256,
  "maxManifestBytes": 262144,
  "pbkdf2Iterations": 600000,
  "minTtlSeconds": 60,
  "maxTtlSeconds": 86400,
  "defaultTtlSeconds": 900,
  "ttlPresets": [
    { "seconds": 900, "label": "15 minutes" },
    { "seconds": 1800, "label": "30 minutes" },
    { "seconds": 3600, "label": "1 hour" }
  ]
}
```

The server builds labels from durations. Whole hours become "N hour(s)", whole minutes become "N minute(s)", and anything else becomes "N seconds". The UI shows only limits taken from this config, never hard-coded ones, so it can't offer something the server would reject.

The server builds this object from an allowlist of fields and never serializes its whole configuration, which holds the pepper and local paths. `maxFiles` and `maxManifestBytes` are creation limits. A server can set them below the protocol's decoding ceilings (see [Validation](#validation)) but never above them. Clients decode against the ceilings, so lowering a creation limit never makes stored messages unreadable.

### `GET /api/config`

Returns the client config above as `200 application/json`, with the usual security headers. It's not rate limited. Before uploading, the CLI fetches it from the same server it will post to, under the same TLS and redirect rules, and validates the values. If it can't get a valid config, it stops before uploading anything.

## Cryptography

Both clients run this locally: the browser through Web Crypto, the CLI through the RustCrypto crates. The server is not involved.

### Keys

| Item | Value |
|---|---|
| Passphrase | 4 words from the EFF large wordlist (7776 words), joined with `-`. About 51.7 bits. |
| PIN | 6 digits, at most 19.9 bits. Generated with a CSPRNG, uniform over 000000 to 999999, zero-padded. The sender may override it. |
| KDF | PBKDF2-HMAC-SHA-256. Input is the UTF-8 bytes of `passphrase + pin` with no separator. 16-byte random salt. Iterations from config, 600,000 by default, within the accepted range. |
| KEK | 256-bit AES-KW key derived by the KDF. |
| CEK | Fresh random 256-bit AES-GCM key per message. |
| Key wrap | AES-KW (RFC 3394) of the raw CEK under the KEK. |
| Encryption | AES-256-GCM with a 12-byte random IV and a 128-bit tag. AAD is the wrapped CEK bytes, which ties the ciphertext to its wrapped key. |

A CSPRNG failure aborts creation. The accepted KDF iteration range is a protocol constant shared by both clients (see [Validation](#validation)), and the server refuses to start with `pbkdf2_iterations` outside it. Clients wipe key material after use, including the internal state of the cipher implementations where the platform allows it.

When decrypting, the client reads the iteration count, salt, and IV from the blob header, not from the current config, so old blobs still decrypt after the operator changes `pbkdf2_iterations`.

### Outer envelope (schemaVersion 2)

The blob the server stores:

```
[u32 big-endian: header length N][N bytes: UTF-8 JSON header][AES-GCM ciphertext with tag]
```

```json
{
  "wrappedContentKey": "<base64>",
  "kdfSalt": "<base64>",
  "iv": "<base64>",
  "kdfAlgorithm": "PBKDF2-SHA-256",
  "kdfIterations": 600000,
  "encryptionAlgorithm": "AES-256-GCM",
  "wrappingAlgorithm": "AES-KW",
  "schemaVersion": 2
}
```

Base64 is the standard alphabet with padding. Nothing in this header is secret. The wrapped key is encrypted under the KEK.

### Inner payload

The GCM plaintext:

```
[u32 big-endian: manifest length M][M bytes: UTF-8 JSON manifest][text bytes, if any][file bytes, in array order]
```

```json
{
  "text": { "byteLength": 12 },
  "files": [
    { "name": "a.txt", "mimeType": "text/plain", "byteLength": 3 }
  ]
}
```

- `text` is `null` when there is no text. `files` is `[]` when there are no files. At least one must be present. Zero-length files are allowed.
- Text is UTF-8. `mimeType` defaults to `application/octet-stream`.
- Lengths are byte counts, never character counts. Files are identified by array index, and names need not be unique.

### Canonical encoding

The outer header and the inner manifest are each accepted only in one exact byte form, so the two clients accept exactly the same inputs. A header or manifest whose bytes differ from the canonical form is refused, even if it holds the same data. Canonical form is:

- keys in exactly the order shown in the examples above, no other keys, and the `text` key always present (`null` when there is no text),
- no whitespace between tokens, and no byte-order mark,
- integers written in plain decimal, with no sign, fraction, or exponent,
- strings escaped minimally: `"` and `\` are backslash-escaped, U+0008, U+0009, U+000A, U+000C, and U+000D use `\b`, `\t`, `\n`, `\f`, and `\r`, other characters below U+0020 use `\u00XX` with lowercase hex, and everything else, including non-ASCII characters, is written as raw UTF-8,
- strings well-formed Unicode: no unpaired surrogates, escaped or otherwise,
- base64 in the standard alphabet with padding, exactly as produced by encoding the decoded bytes.

This is the output of JavaScript's `JSON.stringify` and of Rust's `serde_json::to_vec` for these structures, which both clients use.

### Validation

Before any crypto work, a client checks the outer envelope (stage one):

- there are at least 4 bytes, the length prefix fits the buffer, and the header is at most 1024 bytes,
- `schemaVersion` is supported,
- the header has exactly the expected fields with the expected types,
- the algorithm labels are the ones above,
- `kdfIterations` is in the accepted range,
- the base64 fields decode to the right sizes (40-byte wrapped key, 16-byte salt, 12-byte IV),
- the header is in canonical form,
- the ciphertext is at least as long as the 16-byte tag.

After authenticated decryption, it checks the payload (stage two):

- the length prefix fits the buffer, and the manifest is at most the manifest ceiling,
- the manifest is in canonical form, with exactly the expected fields and types,
- every length is an integer from 0 to 2^53 - 1 (JavaScript's largest exact integer), and the sum doesn't overflow,
- the bodies consume the plaintext exactly,
- the text is valid UTF-8, and a leading U+FEFF is kept as part of the text,
- the file count is at most the file-count ceiling,
- every name is 1 to 255 UTF-8 bytes, and every MIME type is 1 to 255 bytes,
- there is text or at least one file.

The decoding ceilings are protocol constants that both clients share, and operators can't change them:

| Constant | Value |
|---|---|
| Accepted KDF iterations | 600,000 to 2,000,000. Provisional: the final range is set by the benchmark tracked in `docs/coverage.md` (`KDF-range`) before v1. |
| Outer header | 1024 bytes |
| Manifest | 256 KiB |
| Files per message | 256 |
| File name | 255 UTF-8 bytes |
| MIME type | 255 bytes |
| Declared length | 2^53 - 1 |

Any failure shows a generic "cannot open this message" error.

### Size accounting

`max_payload_bytes` applies to the final encrypted blob, headers and tag included. Both clients compute the exact blob size before encrypting and refuse an oversized message, and the server enforces the same number. Clients use views and slices to avoid extra copies of the whole message.

### Threat model

The server is honest but curious. It may look at everything it receives, so it must never receive anything that helps it decrypt beyond the PIN, which it sees by design. The PIN is too weak to resist offline brute force on its own. Its job is online gating plus an extra input to the KDF. Someone who dumps server memory or the SQLite files gets blobs, Argon2 hashes, and metadata, but never the passphrases, so the blobs stay unreadable. HTTPS is required in production and is the operator's job (see [Deployment](#deployment)).

## Server

### Message ids

- The server generates ids, never the client. The format is `word-word-word-NNNNNN`: 3 random EFF words, then a 6-digit number padded with zeros, all lowercase and joined with `-`. Both counts can be configured.
- The server uses a CSPRNG. On a collision it retries up to 10 times, then returns 503.
- Ids are public. They show up in paths and access logs. That's accepted.

### PIN verifier

- The server generates a 16-byte random salt per message, stored as base64.
- verifier = Argon2id(`pin + ":" + salt + ":" + pepper`), with t=3, m=19456 KiB, p=1 by default. That beats OWASP's minimum (19 MiB, t=2, p=1), and all three values can be configured.
- The pepper is a server secret from config. It may be empty, and the server logs a warning at startup when it is.

### Stored record

| Field | Notes |
|---|---|
| `id` | |
| `blob` | opaque bytes |
| `pinVerifier` | Argon2id encoded hash |
| `pinSalt` | base64 |
| `expiresAt` | absolute time, now + ttl at create time |
| `failedPinAttempts` | starts at 0 |
| `protocolVersion` | the protocol generation of the server that accepted the message. The server records it and never reads it from the blob, so it says nothing about what the uploaded bytes actually contain. Clients still validate every header. |

A message counts as expired when `expiresAt <= now`.

### Concurrency and resources

- Per-id serialization uses a bounded scheme: fixed striped locks or conditional row revisions, never an uncapped map.
- Argon2 and SQLite run on a bounded blocking pool. Concurrent Argon2 operations and their queue each have their own limit. No database connection or global lock is held while hashing.
- Admission accounts for in-flight request bodies and hashing memory as well as stored bytes.
- A cancelled request never reports success before its commit. Blocking work that has already started runs to its commit or rollback.

### Store modes

| | `memory` (default) | `sqlite` |
|---|---|---|
| Survives restart | No | Yes |
| Caps | count 1000, bytes 256 MiB | same caps, also enforced |
| Expiry sweep | every minute, and before rejecting a save over a cap | every minute, before rejecting a save over a cap, and at startup |
| Durability | n/a | WAL, `synchronous=FULL`, pragmas verified at startup, bounded WAL via checkpoints |
| Second process | n/a | refused through an exclusive lock |
| Deleted data | gone from memory after the next sweep, no secure erasure | may stay recoverable in the DB file, WAL, snapshots, and backups |

Rules for `sqlite` mode:

- The database lives on a local persistent volume, not a network share.
- Disk-full and busy errors map to 503.
- The database has a schema version.
- The server stores a fingerprint of the pepper. If the configured pepper doesn't match it at startup, the server refuses to start, because a changed pepper would turn every correct PIN into a wrong attempt and burn the stored messages. The operator has to wipe the store explicitly to change the pepper.
- A later release that changes the envelope format has two options. It can ship clients (browser UI and CLI) that still decode every protocol generation still live in the store, so those messages stay readable until they expire. Otherwise the store must be drained before the upgrade. The server stays opaque either way.
- Restoring a database backup can bring old messages back. That is outside the at-most-once guarantee, and normal startup never does it.

Multiple server instances remain out of scope in both modes.

### Rate limiting

- Fixed window, counted per client address and per bucket. It applies to `/api/messages/**` only. Pages, static files, and `/api/config` are not limited.
- POST requests use the `create` bucket (10 per minute), GET requests use the `retrieve` bucket (30 per minute).
- The client address is the socket peer address, unless that peer is in `trusted_proxies` (empty by default). For a trusted peer, the server reads only `X-Forwarded-For` (`Forwarded` is ignored). It walks the list from the right, skipping addresses that are themselves trusted proxies, and takes the first untrusted address. That way a fake left-most entry such as `spoofed, real-client` can't pick the bucket. A missing, malformed, or over-long header (the parser is bounded) falls back to the peer address.
- Limiter state is bounded: a cap on tracked keys plus eviction, with cleanup work bounded per request.

## Configuration

Settings come from, highest precedence first: command-line options, environment variables, the TOML file, then defaults. The TOML file comes from `--config`, or else from `PINVEIL_CONFIG`.

- An environment variable is `PINVEIL_` followed by the key in upper case, with `.` written as `__`. For example, `pin.pepper` becomes `PINVEIL_PIN__PEPPER`. Only names containing `__` are settings, so other `PINVEIL_` variables such as `PINVEIL_CONFIG` are never mistaken for one.
- An unknown key, from any source, is an error.
- Lists are comma-separated in environment variables, e.g. `PINVEIL_MESSAGES__TTL_PRESETS=15m,30m,1h`.
- Giving a single-value option twice on the command line is an error.

| Key | Default | Purpose |
|---|---|---|
| `server.listen` | `0.0.0.0:8080` | Listen address |
| `server.trusted_proxies` | empty | Peer addresses or CIDRs whose forwarding headers are trusted |
| `server.max_concurrent_uploads` | `16` | Create requests whose bodies may be in flight at once. Beyond this, the server returns 503. |
| `messages.min_ttl` | `1m` | Shortest TTL the server accepts |
| `messages.max_ttl` | `24h` | Longest TTL the server accepts |
| `messages.default_ttl` | `15m` | Preset the UI selects first |
| `messages.ttl_presets` | `15m,30m,1h` | Presets the UI offers |
| `messages.max_payload_bytes` | `26214400` (25 MiB) | Largest blob the server accepts |
| `messages.max_files` | `256` | Creation limit on file count, at most the protocol ceiling |
| `messages.max_manifest_bytes` | `262144` (256 KiB) | Creation limit on manifest size, at most the protocol ceiling |
| `messages.pbkdf2_iterations` | `600000` | Client KDF cost for new messages |
| `message_id.words` | `3` | Words in a message id |
| `message_id.digits` | `6` | Digits in a message id |
| `store.mode` | `memory` | `memory` or `sqlite` |
| `store.path` | none | SQLite file path. Required in `sqlite` mode. |
| `store.max_messages` | `1000` | Count cap |
| `store.max_bytes` | `268435456` (256 MiB) | Byte cap |
| `pin.pepper` | empty | Server secret mixed into PIN hashes |
| `pin.max_failed_attempts` | `3` | Wrong PINs before a burn |
| `pin.argon2_iterations` | `3` | Argon2id t |
| `pin.argon2_memory_kib` | `19456` | Argon2id m |
| `pin.argon2_parallelism` | `1` | Argon2id p |
| `pin.max_concurrent_hashes` | min(CPU count, floor(`max_hash_memory_bytes` / (`argon2_memory_kib` * 1024))) | Concurrent Argon2 operations. An explicit value is still capped by the memory budget. With the defaults, the budget allows 13. |
| `pin.max_hash_memory_bytes` | `268435456` (256 MiB) | Memory budget for in-flight Argon2 |
| `pin.max_queued_hashes` | `64` | Hash requests that may wait. Beyond this, the server returns 503. |
| `rate_limit.window` | `1m` | Rate-limit window |
| `rate_limit.create_per_window` | `10` | Create bucket size |
| `rate_limit.retrieve_per_window` | `30` | Retrieve bucket size |
| `rate_limit.max_tracked_keys` | `100000` | Limiter state cap |

The server refuses to start if:

- the TTL presets or the default fall outside min and max,
- a limit is impossible,
- `pbkdf2_iterations` is outside the accepted range,
- `store.path` is missing in `sqlite` mode,
- `max_hash_memory_bytes` is smaller than one Argon2 hash (`argon2_memory_kib` * 1024),
- the limits exceed what the browser zip (ZIP32) supports.

The final default values of these limits come from fixtures and benchmarks during implementation. The protocol ceilings and the KDF range are constants, not configuration (see [Validation](#validation)).

## Web UI

- Plain ES modules served as-is, with no build step and no third-party code.
- The create page takes text and files together. Its PIN field holds a generated PIN that the sender can edit.
- The wordlist is validated after every fetch: the HTTP status, 7776 entries, all distinct and well formed.
- "Download all" builds a store-only zip in the browser (method 0, CRC-32, UTF-8 names), following PKWARE APPNOTE, from the already decrypted files. Nothing goes to the server. The text stays separately copyable and isn't part of the zip.
- File names follow the [export name policy](#export-names), both in the zip and as single downloads. The original names are displayed as plain text.
- Both pages show the store mode. The retrieve page explains that opening deletes the message (see [Retrieve in the browser](#retrieve-in-the-browser)), and that a dropped connection after opening loses it.

## CLI

`pinveil` is a separate binary. It shares its crypto, format, and validation code with the server through the `pinveil-core` crate, and parses arguments with `usage-rs`, with unknown flags treated as errors.

### `pinveil send [FILE...]`

- Text comes from stdin when stdin isn't a terminal. When it is a terminal, the text comes from an interactive prompt or `$EDITOR` (`--edit`). Combining `--edit` with piped stdin is an error. File paths are arguments. At least one of text and files is required.
- The CLI generates the passphrase from its embedded wordlist, which it validates once and never fetches.
- The CLI generates the PIN and writes it to the controlling terminal, with a reminder to send it over a different channel. `--choose-pin` asks for a PIN at a hidden terminal prompt instead.
- The share URL goes to stdout.
- Options: `--ttl`, `--server`, `--profile`.

### `pinveil get [URL]`

- By default the CLI asks for the share URL at a hidden terminal prompt. A positional URL is an explicit opt-in, and its help text says it exposes the passphrase in shell history and the process list.
- The CLI asks for the PIN at a hidden terminal prompt.
- Text goes to stdout. When stdout is a pipe or a file, the bytes are written unchanged. When stdout is a terminal, control sequences are neutralized unless `--raw` is given. Status messages and written file paths go to stderr.
- Files go to the current directory, or to `--out DIR`.

### Secrets and streams

- Content and PIN are never accepted as flags or environment variables. Content and PIN never pass through the argument parser. The one exception is the opt-in positional URL, and parser diagnostics redact its fragment.
- When a required terminal input has no terminal, the command fails with a clear error.
- The CLI strips the fragment from the URL right away and builds request URLs without it. No error message contains the full URL or the PIN.
- Exit codes are fixed per failure class: usage, network, server rejection, message unavailable, decryption, output.

### HTTP

- reqwest with the rustls backend.
- TLS is required. `--insecure-http` allows plain HTTP for local development, only when the server URL's host is a loopback IP literal (`127.0.0.0/8` or `[::1]`). It never applies to DNS names, `localhost` included, and it never disables certificate verification for HTTPS.
- Redirects are refused.
- A private CA can be added to the trust store. There is no "skip verification" option.

### Export names

One policy, shared by the CLI and the browser zip:

- rejects path separators, absolute paths, drive and UNC prefixes, `.` and `..`, NUL and control characters, and names that are invalid on the platform (Windows device names, alternate data streams, trailing dots and spaces),
- replaces a rejected name with a safe derived name,
- disambiguates duplicate names and names that differ only by case.

### File output

1. The CLI validates the whole manifest and plans every destination before writing anything.
2. It creates each file with `create_new`, relative to the destination directory, with owner-only permissions.
3. With `--force`, it writes to a temp file and renames it over the destination. It never follows an existing symlink or reparse point.
4. On a collision without `--force`, the decrypted message stays in memory while the CLI asks for another directory or for confirmation. Quitting discards it, and the prompt says so.
5. On a partial write or a full disk, it removes the files it created and reports the error.

### Profiles

A TOML file in the platform config directory holds only non-secret settings: server URL, default TTL, default output directory.

- Command-line options override the profile, which overrides defaults.
- Server URLs with credentials, a query string, or a fragment are rejected.
- The CLI never stores PINs, fragments, plaintext, message history, or payload paths.

### Editor mode

The CLI uses a unique owner-only temp file and deletes it on success and on error. The docs say that editor swap and backup files, and crashes, can leave plaintext behind.

## Deployment

People self-host pinveil however they like. Each release, cut from a `v*.*.*` tag, ships:

- `pinveil-server` and `pinveil` binaries for `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `aarch64-apple-darwin`, and `x86_64-pc-windows-msvc`,
- a multi-arch server image at `ghcr.io/alexey-lapin/pinveil`,
- a Homebrew formula in `alexey-lapin/homebrew-tap` that installs the prebuilt archives.

Operators are responsible for:

- TLS,
- a stable pepper,
- a reverse-proxy body limit at least `max_payload_bytes` plus multipart overhead,
- `server.trusted_proxies` when a proxy sits in front,
- a persistent local volume in `sqlite` mode.

## Non-goals

Out of scope by design: accounts, message previews, resumable uploads, multiple server instances, metrics, a JS build step, and reading Java-era blobs.

## Gaps found in the Java version, and their v1 disposition

The Java/Micronaut implementation was mapped and reviewed on 2026-10-30 before its removal. Items marked "verified" were reproduced against a running build or the unmodified JS modules. Every "fixed" item has an automated check in the v1 test suites.

| # | Gap in the Java version | v1 disposition |
|---|---|---|
| 1 | Verified. `wordlist.js` used any HTTP response as the wordlist. A 404 page produced `Found-Found-Found-Found`, a truncated 200 produced `abacus-abacus-abacus-abacus`, and creation went ahead. | Fixed: invariant 12, browser and CLI validation. |
| 2 | Verified. Rate limiting trusted `X-Forwarded-For` from anyone: 14 creates against a limit of 10, 35 retrieves against 30. Each spoofed address added an entry to an uncapped map that every request scanned under a lock. | Fixed: `trusted_proxies` (empty by default), bounded limiter state. |
| 3 | Verified. Framework errors (missing header or part, unknown route, multipart size) used Micronaut's own body. | Fixed: invariant 13. |
| 4 | Verified. CI copied artifacts from the pre-split root `build/` paths and failed. | Fixed: new CI for the cargo workspace. |
| 5 | One global lock covered Argon2, so every create and retrieve ran one at a time. | Fixed: per-id serialization, bounded hashing pool. |
| 6 | The client ignored `schemaVersion` and the algorithm labels, and passed `kdfIterations` from the blob straight to PBKDF2, so a malicious sender could pin the recipient's CPU. | Fixed: invariant 14. |
| 7 | The UI hard-coded 25 MB and 1440 minutes. | Fixed: the UI uses only the client config. |
| 8 | The browser checked plaintext size while the server checked the blob, so a near-limit message failed after encryption. | Fixed: exact final-blob accounting in both clients. |
| 9 | An empty blob returned 413. | Fixed: 400. |
| 10 | No warning when the pepper is empty. | Fixed: startup warning. |
| 11 | A 403 confirms a live id. | Accepted: part of the contract, ids aren't enumerable. |
| 12 | HTMX was planned but never adopted. | Accepted: plain ES modules are the design. |
| 13 | Consume-before-delivery loses a message on a dropped connection, a wrong passphrase consumes it, and ids appear in access logs. | Accepted, and explained in the UI (the first two). |
| 13a | The UI never disclosed restart loss. | Fixed: invariant 10, store mode shown in both clients. |
| 14 | Small leftovers: dead `storedMessageCount()`, a Micronaut config warning, duplicate JNA versions, a negligible `uint32 % 7776` bias. | Moot, since the Java code is removed. The Rust clients sample words without bias. |
