# Pinveil Readability & Architecture Refactor — Design

**Date:** 2026-06-27
**Status:** Approved (pending spec review)
**Scope:** Clean-architecture refactor with extension seams; no new infrastructure.

## 1. Goal

Pinveil is a zero-knowledge text/file sharing app (client-side encrypt → store
ciphertext keyed by a server-issued ID, protected by a PIN + URL-fragment
passphrase → TTL expiry). It works, but the code carries the structural debt of
a vibecoded first pass. This refactor makes the Java and JavaScript idiomatic and
readable, and introduces clean abstractions ("seams") so storage and rate
limiting could be backed by other implementations later — **without** adding
persistence, distributed infra, or a JS build step now.

### Objectives

- Idiomatic Java (Micronaut) and JavaScript following best practices.
- Real abstractions where they earn their place: a storage seam and a rate-limit
  seam.
- Remove rate limiting from controllers.
- Stop hand-crafting JSON.
- Untangle the `MessageService` god class.
- Split the monolithic `app.js`.
- Fix correctness bugs found along the way (explicitly called out).
- Expand the test suite alongside the new seams.

### Non-goals (explicitly deferred)

- Persistence / durable storage (stays in-memory).
- Distributed / shared rate limiting.
- Metrics / observability.
- JS build tooling (stays no-build for auditability).
- Changing the crypto envelope format or the HTTP wire contract.

## 2. Decisions (from brainstorming)

| Decision | Choice |
|---|---|
| Scope | Clean architecture with seams (in-memory default) |
| JS strategy | Stay no-build; restructure plain ES modules |
| Bug fixes | Refactor **and** fix correctness bugs found, each called out |
| Tests | Expand tests with the refactor |
| Rate-limit attachment | `HttpServerFilter` on `/api/messages/**` |
| Base package | `com.github.alexeylapin.pinveil` |
| Spec location | `docs/plans/` (matches existing repo convention) |

## 3. Target package structure

Rename `com.example` → `com.github.alexeylapin.pinveil`, grouped by component:

```
com.github.alexeylapin.pinveil
├─ Application
├─ web/                         ← HTTP edge only
│   ├─ MessageApiController      (thin: parse → call service → respond)
│   ├─ PageController
│   ├─ SecurityHeadersFilter
│   ├─ ApiExceptionHandler      (extracted from the controller)
│   └─ dto/
│       ├─ CreateMessageResponse
│       ├─ ErrorResponse
│       └─ ClientConfig          (@Serdeable, replaces hand-built JSON)
├─ message/                     ← domain core
│   ├─ MessageService           (policy/orchestration only)
│   ├─ StoredMessage            (domain entity; leaves the old model/ pkg)
│   ├─ MessageStore             (interface — the storage seam)
│   ├─ InMemoryMessageStore     (map + byte accounting + @Scheduled sweep)
│   ├─ MessageException
│   └─ MessageError
├─ ratelimit/                   ← the rate-limit seam
│   ├─ RateLimiter              (interface)
│   ├─ InMemoryRateLimiter      (windowed counter impl)
│   └─ RateLimitFilter          (@Filter on /api/messages/**)
├─ security/
│   └─ PinVerifier              (argon2; renamed from PinVerifierService)
├─ passphrase/
│   └─ DicewareService
└─ config/
    ├─ MessagePolicyConfig      (ttl, payload, capacity, pbkdf2 client iters)
    ├─ PinSecurityConfig        (pepper, argon params, max failed attempts)
    └─ RateLimitConfig          (window, per-bucket limits)
```

## 4. Component designs

### 4.1 Storage seam — `MessageStore`

Interface owning all persistence concerns so `MessageService` holds none:

```java
interface MessageStore {
    void save(StoredMessage message);
    Optional<StoredMessage> find(String id);
    boolean remove(String id);
    boolean contains(String id);
    int count();
    long storedBytes();
    int removeExpired(Instant now);   // returns number removed
}
```

- `InMemoryMessageStore` owns a `ConcurrentHashMap<String, StoredMessage>` and
  the running `storedBytes` total, updating the byte count on every save/remove
  so it can never drift from the map.
- The `@Scheduled(fixedDelay = "1m")` expiry sweep moves here (calls
  `removeExpired`).
- Capacity limits (`maxStoredMessages`, `maxStoredBytes`) are enforced by
  `MessageService` using `count()` / `storedBytes()`, keeping the store a dumb
  container and the policy in one place.

### 4.2 `MessageService` — pure policy/orchestration

After extraction it does only: validate the command → check capacity →
generate a unique ID (via `DicewareService`) → hash the PIN (via `PinVerifier`)
→ build and `save` the `StoredMessage`; and on retrieve: validate PIN format →
look up → check expiry → verify PIN (with failed-attempt burn-down) → remove and
return. No map, no byte arithmetic, no scheduling. `CreateCommand` /
`CreateResult` records remain as the service's API. Validation helpers
(`isValidPin`, TTL range) stay here as the single server-side source of truth.

### 4.3 Rate-limit seam — `RateLimiter` + `RateLimitFilter`

```java
interface RateLimiter {
    boolean tryAcquire(String bucket, String key);   // false = limit exceeded
}
```

- `InMemoryRateLimiter` keeps the existing windowed-counter logic behind this
  interface, looking up the per-bucket limit from `RateLimitConfig`.
- `RateLimitFilter` (`@Filter("/api/messages/**")`) maps method → bucket
  (`POST` → `"create"`, `GET` → `"retrieve"`), resolves the client IP **once**
  via `HttpClientAddressResolver`, and calls `tryAcquire`. On rejection it
  returns `429 Too Many Requests` as an `ErrorResponse` without invoking the
  controller.
- Controllers lose `enforceRateLimit` entirely.

### 4.4 JSON config — `ClientConfig` record

Replace `PageController.clientConfigJson()` string concatenation with:

```java
@Serdeable
record ClientConfig(long maxPayloadBytes, int pbkdf2Iterations,
                    long minTtlSeconds, long maxTtlSeconds,
                    long defaultTtlSeconds, List<TtlPreset> ttlPresets) {
    @Serdeable record TtlPreset(long seconds, String label) {}
}
```

Assembled from config and serialized with the injected `JsonMapper`, then placed
in the `<script id="app-config" type="application/json">` tag in the JTE
template. Proper escaping, type-safe, unit-testable. The TTL-label formatting
helper moves into the assembler.

### 4.5 Configuration split

`MessageConfiguration` currently mixes client-facing crypto params, server
storage policy, and PIN security. Split into:

- `MessagePolicyConfig` (`app.messages`): ttl min/max/default, ttl presets,
  maxPayloadBytes, maxStoredMessages, maxStoredBytes, pbkdf2Iterations (client
  KDF cost, surfaced to the browser).
- `PinSecurityConfig` (`app.pin`): pinPepper, argon2 iterations/memory/
  parallelism (currently hard-coded constants in `PinVerifierService` — moved to
  config), maxFailedPinAttempts.
- `RateLimitConfig` (`app.rate-limit`): unchanged in spirit; per-bucket limits.

`application.properties` keys updated accordingly; defaults preserved.

### 4.6 JavaScript restructure (no-build)

Split `static/app.js` into ES modules the browser imports directly:

- `app.js` — tiny dispatcher: read `body.dataset.page`, delegate.
- `create-page.js` — create flow (form, TTL presets, mode switch, file drop).
- `retrieve-page.js` — retrieve flow.
- `config.js` — parse `app-config`; shared constants (PIN regex, max custom TTL).
- `wordlist.js` — cached EFF wordlist fetch (replaces the
  `window.__pinveilWordListPromise` global with a module-scoped promise).
- `ui.js` — `setFeedback`, `formatBytes`, `parseJson` helpers.
- `crypto.js` — unchanged.

Duplicated magic values (25 MB limit, 1440-minute cap, `^\d{6}$`) consolidated in
`config.js`.

## 5. Bugs to fix (each called out in implementation)

1. **Rate limiting keyed on raw socket IP.** `MessageApiController.enforceRateLimit`
   limits on `request.getRemoteAddress()` but only *logs* the proxy-resolved
   address — so behind a reverse proxy all clients share one bucket. Fixed by
   resolving the client IP via `HttpClientAddressResolver` in `RateLimitFilter`.
2. **Hand-built JSON is unescaped.** TTL labels are concatenated raw into the
   client config JSON. Fixed by serializing `ClientConfig` via `JsonMapper`.
3. `HttpStatus.valueOf(413)` → `HttpStatus.REQUEST_ENTITY_TOO_LARGE` (polish, in
   the extracted `ApiExceptionHandler`).

Any further bugs discovered during implementation are surfaced and listed, not
silently changed.

## 6. Testing plan

Expand alongside each seam; keep the existing suite green.

- `MessageServiceTest` — drive against a fake/in-memory `MessageStore`: validation
  (PIN format, TTL range, payload size), capacity rejection, PIN verify success/
  failure, failed-attempt burn-down deletion, expiry-on-retrieve.
- `InMemoryMessageStoreTest` — save/find/remove, byte accounting accuracy,
  `removeExpired` correctness, count.
- `InMemoryRateLimiterTest` — per-bucket independence, window rollover, limit
  enforcement.
- `ClientConfigTest` — serialization shape and escaping.
- `MessageApiControllerTest` / filter test — 429 path, IP resolution, error
  mapping.
- Existing `DicewareServiceTest` retained.

## 7. Sequencing

The refactor is one coherent effort, sequenced so tests land with each seam:

1. Package rename + move to `com.github.alexeylapin.pinveil` (mechanical, keep
   green).
2. Extract `MessageStore` / `InMemoryMessageStore`; slim `MessageService`; tests.
3. Extract rate-limit seam + `RateLimitFilter`; remove from controller; fix IP
   bug; tests.
4. `ClientConfig` + `JsonMapper`; remove hand-built JSON; tests.
5. Config split; extract `ApiExceptionHandler`; status-code polish.
6. JS module split.
7. Final pass: full test suite green, manual smoke test of create/retrieve.
