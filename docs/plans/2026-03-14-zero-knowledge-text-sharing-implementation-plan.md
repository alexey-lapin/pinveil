# Zero-Knowledge Text Sharing Implementation Plan

Date: 2026-03-14

## Phase 1: Project Skeleton

1. Create a Gradle-based Micronaut application
2. Add JTE integration for server-rendered pages
3. Add HTMX and a minimal browser JavaScript bundle for crypto workflows
4. Set up environment-driven configuration for TTL presets, size limits, PIN attempt limits, and security headers

## Phase 2: Core Domain Model

1. Define message record model for in-memory storage
2. Define request/response DTOs for create and retrieve flows
3. Define validation rules for id format, PIN format, TTL preset, and payload size
4. Implement Diceware-based generators for:
   - `3` word message ids
   - `4` word passphrases
   - `6` digit suffixes and PIN validation helpers

## Phase 3: In-Memory Storage And Lifecycle

1. Implement in-memory message repository
2. Add TTL expiration support and background cleanup
3. Add atomic one-time retrieval deletion behavior
4. Add failed PIN attempt counting and delete-on-threshold logic
5. Add collision-safe create handling

## Phase 4: Client-Side Crypto

1. Implement browser-side payload handling for text and file inputs
2. Generate random content keys and IVs in the browser
3. Encrypt payloads with `AES-GCM`
4. Derive passphrase-based wrapping keys using the chosen KDF
5. Wrap and unwrap content keys
6. Implement browser-side decrypt and render/download flows

## Phase 5: Create Flow

1. Build JTE create page with form for text or file, PIN, and TTL preset
2. Add client-side validation for size, input mode, and PIN format
3. Intercept submission in browser JavaScript
4. Encrypt locally and submit encrypted package to `POST /api/messages`
5. Show generated URL and sender guidance after success

## Phase 6: Retrieve Flow

1. Build JTE retrieval shell page for `/message/{id}`
2. Read passphrase from URL fragment in browser JavaScript
3. Prompt for PIN and submit to `POST /api/messages/{id}/retrieve`
4. Decrypt returned payload locally
5. Render text or trigger file download
6. Show generic unavailable state for expired, deleted, or invalid retrieval outcomes

## Phase 7: Security Hardening

1. Add strict response headers
2. Add rate limiting on create and retrieve endpoints
3. Store only PIN verifiers, not raw PINs at rest
4. Normalize error messages and state exposure
5. Verify no plaintext or passphrase enters logs

## Phase 8: Testing

1. Add unit tests for generators, crypto metadata, validation, and PIN verifier logic
2. Add integration tests for full create/retrieve/delete flows
3. Add browser tests for text and file scenarios
4. Add failure-path tests for wrong PIN, expiry, collision, and decryption mismatch

## Phase 9: Deployment And Ops

1. Configure production HTTPS deployment
2. Document single-instance memory-only limitation
3. Document server restart behavior and operational caveats
4. Optionally evaluate Redis-backed ephemeral storage for a later phase

## Suggested First Milestone

Deliver text-only create and retrieve first, with:

- in-memory storage
- one-time retrieval deletion
- failed-PIN deletion
- browser-side encryption and decryption

Then add file support as the second milestone once the core lifecycle is stable.
