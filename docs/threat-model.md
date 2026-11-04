# Threat model

What pinveil protects, from whom, and where it stops. The mechanisms are specified in
[APP.md](../APP.md); this page explains what they buy you.

## What a message is made of

| Piece | Who sees it |
|---|---|
| Text, file names, file types, file contents | The sender's and recipient's devices only. Encrypted before upload. |
| Passphrase (4 words, about 51.7 bits) | Lives in the link after `#`. Browsers and the CLI never send it to the server. |
| PIN (6 digits, at most 19.9 bits) | The sender, the recipient, and the server, which sees it over TLS on create and retrieve and stores only a salted, peppered Argon2id hash. |
| Message id | Public. It is in the link's path and in access logs. |
| Ciphertext, its size, creation and expiry times | The server. |

The key that encrypts a message comes from the passphrase and the PIN together, through
PBKDF2-HMAC-SHA-256 with at least 600,000 iterations. Opening a message needs both.

## Who we defend against

### The server and its operator

The server is treated as honest but curious: it runs the code as written, but anyone with
access to it may read everything it stores and every request it receives.

What they get: ciphertext, PIN hashes, the pepper, ids, sizes, times, and client addresses.
They also see each PIN in transit, since the server checks it.

What they can't do: decrypt a message. They never receive the passphrase. Even with the PIN,
the ciphertext needs the passphrase, which takes on the order of 2^51 PBKDF2 derivations to
guess.

What they could do if they turned malicious, which is outside this model: serve a modified
browser page that sends the passphrase home. The browser client trusts the server that serves
it, as every web app does. The CLI doesn't have this problem: it carries its own code and
wordlist and only uploads and downloads ciphertext. If you don't trust the operator, use the
CLI, or run your own server.

### Someone who copies the server's data

A stolen database, backup, disk image, or memory dump holds ciphertext, Argon2id PIN hashes,
and metadata. Without the passphrases, the ciphertext stays closed. Without the pepper, the
PIN hashes can't be checked offline either.

In `sqlite` mode, deleted messages can survive in the database file, the write-ahead log,
snapshots, and backups. They are still ciphertext, but they outlive the message, and anyone
who later gets the matching link and PIN could open them. The [operator
guide](operator-guide.md#store-modes) covers this.

### The network

TLS is the operator's job. With TLS, an observer sees the server name, sizes, and timing. Without
it, they also see the PIN and the ciphertext, but still not the passphrase, which never
leaves the client.

The CLI requires HTTPS, refuses redirects, verifies certificates against the system's trust
store, and has no switch to skip verification. Its only plain-HTTP mode, `--insecure-http`,
works only with loopback IP addresses.

### Someone who has the link but not the PIN

They can try PINs against the server. The server allows three wrong PINs per message (the
operator can change the number), then deletes it, and rate-limits retrievals per client
address. The chance of guessing a random PIN in three tries is 3 in a million. Every wrong
attempt and the deletion are committed before the response goes out, so racing requests
doesn't buy extra tries.

### Someone who has the link and a copy of the ciphertext

This is the weak spot. If an attacker has the passphrase (from the link) and the ciphertext
(from the server's data or a captured response), only the PIN stands between them and the
plaintext, and the attempt limit doesn't apply offline. Trying all million PINs at 600,000
PBKDF2 iterations each is about 6 x 10^11 HMAC-SHA-256 evaluations. Going by our
[benchmark](kdf-benchmark.md), one Apple M4 Pro core does a full scan in about 12 hours, and
finds a random PIN in about 6 on average. Running on many cores or GPUs cuts that further. The
KDF cost slows the attack down; it doesn't stop it.

So: the link is the secret. The PIN protects it against people who get only the link, such as
a mail server, a chat log, or an over-the-shoulder glance at a URL. Send the link and the PIN
through different channels, and don't post the link anywhere a copy of the server's data might
also end up.

### Someone who has neither

Ids are random, about 58.7 bits with the default template, so nobody can find messages by
guessing. An operator can choose a weaker template down to `message_id.min_bits` (32 by
default); near that floor, a guesser with many requests can find live ids. The server returns
the same 404 for a message that never existed, expired, was opened, or was deleted after wrong
PINs.

### A malicious sender, against the recipient

The recipient's client treats everything in a message as hostile until checked:

- Before decrypting, it checks the header: field names, sizes, algorithm labels, and a
  key-derivation cost within the accepted range, so a sender can't make the recipient's device
  run billions of iterations. See the [KDF benchmark](kdf-benchmark.md) for the range.
- After decrypting, it checks the manifest, every length, the file count, and the names before
  showing text or writing a file.
- The browser saves files as `application/octet-stream`, so a file can't make the browser
  render it as a page.
- File names pass through one export policy in both clients: no paths, no `..`, no device
  names or control characters. The CLI creates files exclusively with owner-only permissions,
  never follows a symlink, and asks before overwriting.
- The CLI neutralizes terminal control sequences and Unicode direction overrides when it
  prints text to a terminal, unless `--raw` is given.

### Other users of the same server

A flood of creates or retrievals can fill the store or tie up PIN hashing. The server bounds
every resource (stored count and bytes, uploads in flight, concurrent and queued hashes,
rate-limiter state) and answers 503 or 429 when one runs out, rather than slowing down without
limit. That keeps the server alive, but a determined flood can still deny service to honest
users. Availability against such a flood is out of scope.

## What pinveil promises

- The server never receives plaintext, file names, file types, or the passphrase.
- A message can be opened at most once. The server commits the deletion before it sends the
  ciphertext, and sends nothing if that commit fails.
- A message can't be opened after its expiry time, whether anyone tried to open it or not.
  The server tries to remove expired messages every minute; a failed sweep is retried later,
  and removal is not secure erasure.
- Each wrong PIN attempt is committed before the response, and the limit deletes the message.
  In `sqlite` mode the count survives restarts.
- The browser loads nothing from third parties: no CDN, no analytics, no fonts. Every script
  and style comes from the server itself, and the Content-Security-Policy enforces it.

## Accepted tradeoffs

These are deliberate. Each is checked by a test, so a change in behavior shows up as a
failure.

| Tradeoff | Why | Test |
|---|---|---|
| A correct PIN with a wrong or truncated passphrase still deletes the message. | The server can't check the passphrase without learning it. Both clients warn before the PIN is sent. | `ACC-wrong-passphrase-consumes` in `tests/browser/roundtrip.spec.js` |
| If the connection drops after the server answers, the message is lost. | Deleting before delivery is what makes "at most once" hold. A retry would need the server to keep the message after handing it out. | `WEB-retrieve.lost-in-transit` in `tests/browser/robustness.spec.js` |
| A wrong PIN on a live message returns 403, while an unknown id returns 404, so a 403 confirms the id exists. | Ids can't be guessed with the default template, and the recipient needs to know a PIN was wrong rather than the message gone. Weaker templates make this oracle more useful, which is why `message_id.min_bits` exists. | `acc_403_live_id` in `crates/pinveil-server/tests/contract/messages.rs` |
| Message ids appear in retrieval paths and access logs. | They are public by design. The log shows when someone tried to open a message, not what it holds. The create request's line has no id. | `acc_message_ids_appear_in_logs` in `crates/pinveil-server/tests/contract/logs.rs` |

## Out of scope

- A compromised sender or recipient device. Malware there sees the plaintext.
- Anyone who has the link and the PIN. That is what a recipient is.
- An operator who changes the server code to attack browser users (see above).
- Traffic analysis: message sizes and timing are visible to the server and the network.
- Availability against a determined flood.
- Secure erasure. Expired and deleted data leaves memory and disk on the operating system's
  schedule, and in `sqlite` mode may stay recoverable.
- Plaintext the CLI's editor mode leaves behind if the editor or the CLI crashes.

## Reporting

See [SECURITY.md](../SECURITY.md).
