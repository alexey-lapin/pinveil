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
- Passphrase format: exactly `4` Diceware-style words
- PIN format: exactly `6` digits
- Payload types: plain text or a single file
- Initial payload limit: `25 MB`

## User Flow

### Create

1. User opens the home page.
2. User enters text or selects a file.
3. User enters a `6` digit PIN.
4. User selects a TTL preset from the UI.
5. User clicks `Generate secure link`.
6. Browser generates the message id, passphrase, salts, IVs, and a random content key.
7. Browser encrypts the payload locally.
8. Browser sends only encrypted data, metadata, and PIN verification material to the backend.
9. Backend stores the record in memory until retrieval, expiry, or deletion policy removes it.
10. Browser displays the final share URL.

### Share

- Sender communicates the generated URL and the PIN to the recipient.
- The URL fragment contains the passphrase and remains client-side in normal browser behavior.

### Retrieve

1. Recipient opens the generated URL.
2. Browser reads the message id from the path and the passphrase from the fragment.
3. Page prompts for the `6` digit PIN.
4. Browser sends the id and PIN to the backend.
5. Backend validates the message state and PIN attempt policy.
6. If valid, backend returns the encrypted package and deletes the message immediately.
7. Browser decrypts locally with the fragment passphrase and renders the text or triggers file download.

## Security Model

### Trust Model

- Threat model: honest-but-curious server
- Server may inspect anything it receives, but should not receive plaintext or the URL fragment passphrase
- HTTPS is required for all traffic

### Core Guarantees

- Plaintext never leaves the browser
- The server never receives the URL fragment passphrase
- The server stores only ciphertext, crypto metadata, and verification data needed for policy enforcement
- Successful retrieval deletes the message immediately
- `3` failed PIN attempts delete the message by default, with the threshold configurable on the server side
- Expired messages are removed automatically

### PIN Handling

- The backend receives the raw PIN during create and retrieve requests over HTTPS
- The backend should not store the raw PIN at rest
- The backend stores a strong verifier, such as `Argon2id(pin + per-message salt + server pepper)`
- This allows server-side PIN verification without keeping the PIN itself in memory beyond request handling

## Crypto Design

### Recommended Scheme

- Generate a random `256-bit` content encryption key in the browser for each message
- Encrypt the payload with `AES-GCM`
- Derive a key-encryption key from the `4` word passphrase using a KDF
- Wrap the random content key with the passphrase-derived key
- Upload the ciphertext, wrapped content key, and crypto metadata to the backend

### KDF Choice

- Preferred: `Argon2id` in the browser via a small vetted WASM or JavaScript helper
- Fallback: Web Crypto friendly KDF such as `PBKDF2`, if implementation simplicity outweighs the benefits of a
  memory-hard KDF for v1
- The selected KDF and parameters must be stored in crypto metadata for versioned decryption

### Payload Handling

- Text payloads are encrypted as UTF-8 bytes
- File payloads are encrypted as raw bytes
- For file payloads, the backend may store unencrypted metadata limited to:
    - original filename
    - MIME type
    - size

## Backend Data Model

Each message record stored in memory contains:

- `id`
- `payloadType` (`text` or `file`)
- `ciphertext`
- `wrappedContentKey`
- `cryptoMetadata` (`salt`, `iv`, KDF parameters, algorithm version)
- `fileMetadata` {filename, mimeType, size} when applicable
- `pinVerifier`
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

Request includes:

- `id`
- `payloadType`
- `ciphertext`
- `wrappedContentKey`
- `cryptoMetadata`
- `pin` or material sufficient to derive the stored verifier
- `ttl`
- optional file metadata

Backend behavior:

- validate id format
- validate payload size and payload type
- validate TTL preset against server policy
- reject collisions and let the client retry with a new id
- return success plus canonical expiry timestamp

### `POST /api/messages/{id}/retrieve`

Request includes:

- `pin`

Backend behavior:

- validate existence and active state
- validate expiry
- validate failed-attempt threshold
- verify PIN
- increment failed attempts on failure
- delete the message if the failure threshold is reached
- return encrypted package on success and delete the message immediately

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
- Client cryptography: browser JavaScript using Web Crypto, with a small helper library if needed for `Argon2id`

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
    - allowed TTL presets
    - maximum payload size
    - maximum failed PIN attempts
    - KDF parameters
    - rate limits
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

## Open Implementation Decisions

- choose final in-browser KDF: `Argon2id` helper vs simpler Web Crypto native fallback
- choose backend runtime shape: single instance only for v1 or Redis-backed ephemeral storage soon after
- decide whether create and retrieve encrypted payload transfer should use JSON with base64 or multipart/binary payloads

## v1 Recommendation

Ship a narrow first version with:

- anonymous create and retrieve flows
- text and single-file payload support
- memory-only backend storage
- one-time retrieval deletion
- server-configured failed-PIN deletion
- a server-rendered Micronaut app using JTE, HTMX, and browser-side crypto JavaScript
