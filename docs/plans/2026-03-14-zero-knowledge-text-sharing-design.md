# Zero-Knowledge Text Sharing Design

Date: 2026-03-14

## Goal

Build an anonymous, zero-knowledge encrypted sharing app where the browser encrypts text or a file locally, the backend
stores only encrypted data in memory for a limited time, and the recipient can retrieve the ciphertext only after
providing the correct PIN. Plaintext must never cross the network.

## Product Shape

- Create page: `https://my-web-app.com`
- Retrieve page: `https://my-web-app.com/message/{id}#{passphrase}`
- Message id format: exactly `3` Diceware-style words plus `6` random digits
- Passphrase format: exactly `4` Diceware-style words from the EFF large wordlist (`7776` words, ~`12.9` bits/word)
- Passphrase entropy: ~`51.7` bits from the passphrase plus ~`19.9` bits from the PIN = ~`71.6` bits total KDF input
- PIN format: exactly `6` digits
- Payload types: plain text or a single file
- Initial payload limit: `25 MB`
- TTL presets: `15 minutes` (default), `30 minutes`, `1 hour`, plus user-custom between `1 minute` and `24 hours`

## User Flow

### Create

1. User opens the home page.
2. User enters text or selects a file.
3. User enters a `6` digit PIN.
4. User selects a TTL preset from the UI.
5. User clicks `Generate secure link`.
6. Browser generates a passphrase, salts, IV, and a random content key.
7. Browser derives a key-encryption key from the passphrase and PIN using `PBKDF2`.
8. Browser wraps the content key with the passphrase-derived key using `AES-KW`.
9. Browser encrypts the payload with `AES-GCM`, using the wrapped content key as additional authenticated data (AAD).
10. Browser packs the wrapped key, crypto metadata, and ciphertext into a single opaque blob.
11. Browser sends the raw PIN, TTL, and blob as form data to the backend.
12. Backend generates the message id, derives and stores a PIN verifier, stores the blob as opaque bytes in memory.
13. Backend returns the generated message id and expiry timestamp.
14. Browser builds and displays the final share URL using the server-returned id and the local passphrase.

### Share

- Sender communicates the generated URL and the PIN to the recipient.
- The URL fragment contains the passphrase and remains client-side in normal browser behavior.

### Retrieve

1. Recipient opens the generated URL.
2. Browser reads the message id from the path and the passphrase from the fragment.
3. Page prompts for the `6` digit PIN.
4. Browser sends the id and PIN to the backend.
5. Backend validates the message state and PIN attempt policy.
6. If valid, backend returns the opaque blob as raw bytes and deletes the message immediately.
7. Browser unpacks the blob to extract the wrapped key, crypto metadata, and ciphertext.
8. Browser derives the KEK from the passphrase and PIN, unwraps the CEK, decrypts the payload, and renders the
   text or triggers file download.

## Security Model

### Trust Model

- Threat model: honest-but-curious server
- Server may inspect anything it receives, but should not receive plaintext or the URL fragment passphrase
- HTTPS is required for all traffic

### Core Guarantees

- Plaintext never leaves the browser
- The server never receives the URL fragment passphrase
- The server stores only an opaque encrypted blob and verification data needed for policy enforcement
- Decryption requires both the passphrase (from the URL fragment) and the PIN, making it two-factor
- Successful retrieval deletes the message immediately
- `3` failed PIN attempts delete the message by default, with the threshold configurable on the server side
- Expired messages are removed automatically

### Known Trade-offs

- **Deletion before receipt**: the server deletes the message immediately after issuing the retrieval response. If
  the connection drops before the client receives the full response, the message is lost with no retry. This is an
  accepted trade-off of one-time retrieval semantics.
- **Message ID in server logs**: the message ID appears in URL paths and may be recorded in server access logs.
  Combined with IP addresses and timestamps, this could allow correlation between creators and retrievers. Accepted
  for v1; can be mitigated later with log filtering.

### PIN Handling

- The PIN is an **online access gate**, not a standalone cryptographic secret. Its `6` digits provide ~`19.9` bits
  of entropy, which is insufficient to resist offline brute-force on its own.
- The PIN is also used as an input to the client-side KDF alongside the passphrase, making decryption two-factor.
  Server memory compromise does not compromise message confidentiality because the server never receives the
  passphrase.
- The backend receives the raw PIN during create and retrieve requests over HTTPS
- The backend should not store the raw PIN at rest
- The backend stores a strong verifier, such as `Argon2id(pin + per-message salt + server pepper)`
- This allows server-side PIN verification without keeping the PIN itself in memory beyond request handling

## Crypto Design

### Recommended Scheme

- Generate a random `256-bit` content encryption key (CEK) in the browser for each message
- Derive a key-encryption key (KEK) from the passphrase and PIN using `PBKDF2` with a per-message salt
- Wrap the CEK with the KEK using `AES-KW` (RFC 3394, available in Web Crypto)
- Encrypt the payload with `AES-GCM` using the CEK, including the wrapped CEK bytes as additional authenticated
  data (AAD) to bind the ciphertext and wrapped key together cryptographically
- Pack the wrapped CEK, crypto metadata, and ciphertext into a single opaque blob and upload it to the backend
- The server never parses the blob; it stores and returns it as raw bytes

### Blob Format

The client uses a two-layer envelope structure. Both layers use the same binary format:

```
[4 bytes: big-endian uint32 header length][JSON header bytes][raw body bytes]
```

**Outer layer (the blob stored by the server):**

- JSON header contains the wrapped content key and all crypto parameters needed for decryption:
  `wrappedContentKey`, `kdfSalt`, `iv`, `kdfAlgorithm`, `kdfIterations`, `encryptionAlgorithm`,
  `wrappingAlgorithm`, `schemaVersion`
- Body is the AES-GCM ciphertext

The header is not encrypted. It contains only standard crypto parameters (salt, IV, algorithm identifiers) and
the wrapped key, which is already encrypted under the KEK. None of these are secrets — the salt and IV provide
uniqueness, not secrecy, and algorithm identifiers are public by Kerckhoffs's principle.

**Inner layer (revealed after decryption):**

- JSON header contains the payload type and optional file metadata: `type`, `name`, `mimeType`
- Body is the raw content (UTF-8 text or file bytes)

The server sees only the outer layer as an opaque blob. The inner layer is only accessible after decryption.

### KDF Choice

- v1: `PBKDF2` via Web Crypto with a high iteration count (minimum `600,000` iterations for `PBKDF2-SHA-256`)
- Future: upgrade to `Argon2id` via a vetted WASM helper for memory-hard brute-force resistance
- KDF input: `passphrase || PIN` concatenated, with a random per-message salt
- The selected KDF, parameters, and algorithm versions must be stored in crypto metadata for versioned decryption

### Payload Handling

- Text payloads are encrypted as UTF-8 bytes
- File payloads are encrypted as raw bytes with file metadata (filename, MIME type) bundled inside the inner
  envelope so the server never sees them
- The server does not know the payload type; it can only infer approximate size from the blob length
- The client discovers the payload type after decryption by reading the inner envelope header

## Backend Data Model

Each message record stored in memory contains:

- `id`
- `blob` (opaque bytes — the outer envelope containing wrapped key, crypto metadata, and ciphertext)
- `pinVerifier`
- `pinSalt` (per-message salt for Argon2id PIN verifier)
- `expiresAt`
- `failedPinAttempts`
- `createdAt`

## Backend Lifecycle Rules

- Storage is memory-only for v1
- Message is deleted immediately after a successful retrieval response is issued
- Message is deleted after the configured maximum failed PIN attempts is reached
- Message is deleted once TTL expires
- Background cleanup removes expired records
- Retrieval also lazily enforces expiry on access
- A backend restart clears all stored messages

## API Shape

### `POST /api/messages`

Request format: `multipart/form-data`

Fields:

- `pin` (raw `6` digit string; server derives and stores the verifier)
- `ttl` (integer, seconds)
- `blob` (binary part — opaque encrypted package, see Blob Format)

Backend behavior:

- generate a unique message id
- validate blob size against server maximum
- validate TTL against server policy (must be between `1 minute` and `24 hours`)
- derive and store PIN verifier using `Argon2id(pin + per-message salt + server pepper)`
- store the blob as opaque bytes
- return the generated id and canonical expiry timestamp

Success response (`201`):

```json
{ "id": "amber-borrow-cliff-123456", "expiresAt": "2026-03-14T12:15:00Z" }
```

Error responses:

- `400` — invalid request (TTL out of range)
- `413` — blob too large
- `503` — server storage capacity reached

All error responses use a consistent shape:

```json
{ "error": "description" }
```

### `POST /api/messages/{id}/retrieve`

Request format: `application/x-www-form-urlencoded` or `application/json`

Fields:

- `pin` (raw `6` digit string)

Backend behavior:

- validate existence and active state
- validate expiry
- validate failed-attempt threshold
- verify PIN against stored verifier
- increment failed attempts on failure
- delete the message if the failure threshold is reached
- return the blob on success and delete the message immediately

Success response (`200`):

- Content-Type: `application/octet-stream`
- Body: raw blob bytes

Error responses (JSON):

- `403` — PIN verification failed (generic; does not reveal remaining attempts)
- `404` — message not found, expired, already retrieved, or deleted due to failed attempts (generic; same
  response for all terminal states to avoid information leakage)
- `429` — rate limited

### Omitted From v1

- No message preview endpoint
- No account system
- No multi-file bundles
- No resumable uploads

## Failure Handling

- Wrong PIN: return a generic failure response, increment attempts, delete on threshold
- Wrong passphrase: backend succeeds if PIN is correct; browser fails decryption locally
- Expired message: return unavailable/gone behavior
- Already opened message: return the same generic unavailable response as expired/deleted
- Oversized upload: block in browser and validate again on server
- Partial upload failure: do not create a record unless the full encrypted package is stored successfully

## UX Notes

- The create page should clearly state that plaintext never leaves the browser
- The result page should tell the sender that the link works only once and may expire earlier on server restart
- The retrieval page should ask for the PIN before requesting ciphertext
- Text should render in a minimal isolated view with explicit copy support
- File retrieval should decrypt in browser and trigger a download using the stored filename
- The UI should explain that link and PIN should ideally be shared separately

## Recommended Tech Stack

- Backend: Java + Gradle + Micronaut
- HTML rendering: JTE
- Browser interaction: HTMX for form and page enhancement
- Client cryptography: browser JavaScript using Web Crypto (`PBKDF2`, `AES-GCM`, `AES-KW`)

## Stack-Specific Architecture

- Micronaut serves the main pages and JSON endpoints
- JTE renders the create page and retrieval shell page
- HTMX handles progressive enhancement for forms and partial updates where appropriate
- Dedicated client-side JavaScript performs encryption, decryption, payload preparation, and file handling
- Use `fetch` where tighter control is needed for encrypted request/response payloads

## Operational Constraints

- Single backend instance is the simplest v1 deployment
- If multiple instances are introduced, the system needs sticky routing or a shared ephemeral store such as Redis
- Server config should define:
    - allowed TTL presets and custom TTL bounds (`1 minute` to `24 hours`)
    - maximum payload size
    - maximum total stored messages and/or total stored bytes (to prevent memory exhaustion)
    - maximum failed PIN attempts
    - KDF parameters (PBKDF2 iteration count for client guidance; Argon2id parameters for PIN verification)
    - rate limits (per IP and global, for both create and retrieve)
    - optional server pepper for PIN verification

## Security Hardening

- Enforce HTTPS
- Set `Cache-Control: no-store`
- Set `Referrer-Policy: no-referrer`
- Use a strict CSP
- Avoid third-party scripts and analytics on message pages
- Rate-limit create and retrieve endpoints
- Normalize error messages so state is not overexposed
- Ensure decrypted content is never placed in URLs, logs, or telemetry
- Be aware that message IDs in URL paths may appear in access logs (see Known Trade-offs)

## Testing Strategy

### Unit Tests

- id and passphrase format generation
- encryption/decryption round-trips for text and files
- PIN verifier creation and verification
- TTL behavior
- failed-attempt deletion behavior

### Integration Tests

- create -> retrieve -> decrypt -> confirm deletion
- wrong PIN until deletion threshold
- expiry path
- duplicate id retry path
- file upload/download round-trip

### Browser Tests

- fragment stays client-side
- text rendering flow
- file download flow
- mobile viewport sanity checks

### Security Verification

- inspect network payloads to confirm no plaintext transmission
- inspect logs to confirm no fragment or plaintext leakage
- verify cache and referrer headers

## Resolved Implementation Decisions

- **In-browser KDF**: `PBKDF2` via Web Crypto for v1; upgrade to `Argon2id` via WASM in a future version
- **Key wrapping**: `AES-KW` via Web Crypto
- **Ciphertext integrity**: wrapped content key included as AES-GCM AAD
- **PIN in KDF**: PIN is concatenated with passphrase as KDF input for two-factor decryption
- **Payload transfer**: `multipart/form-data` for create; raw `application/octet-stream` for retrieve response
- **File metadata**: encrypted inside the inner envelope; server knows nothing about payload type or content
- **Opaque blob**: the server stores a single opaque blob per message containing wrapped key, crypto metadata,
  and ciphertext. The server never parses the blob. Create request sends only `pin`, `ttl`, and `blob`.
- **Server-generated ID**: the server generates the message id using the EFF wordlist and returns it in the
  create response. The client never generates or sends an id. No collision retry logic needed on the client.
- **PIN on create**: raw PIN sent over HTTPS; server derives Argon2id verifier
- **TTL presets**: `15m` (default) / `30m` / `1h` / custom (`1 min` to `24 hours`)
- **Diceware wordlist**: EFF large wordlist (`7776` words)

## Open Implementation Decisions

- Choose backend runtime shape: single instance only for v1 or Redis-backed ephemeral storage soon after
- Define specific PBKDF2 iteration count (minimum `600,000` for SHA-256; benchmark on target browsers)
- Define Argon2id parameters for server-side PIN verification
- Define concrete rate limit values for create and retrieve endpoints
- Define maximum total stored messages and bytes for memory cap

## v1 Recommendation

Ship a narrow first version with:

- anonymous create and retrieve flows
- text and single-file payload support
- memory-only backend storage
- one-time retrieval deletion
- server-configured failed-PIN deletion
- a server-rendered Micronaut app using JTE, HTMX, and browser-side crypto JavaScript
