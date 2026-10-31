// Harness probe (implementation plan, Phase 0 step 6): proves this runner loads the real browser
// modules from web/static and runs them under Node's Web Crypto. Phase 1 replaces it with the
// cross-client vector tests.
import assert from "node:assert/strict";
import { test } from "node:test";

import { decryptPayload, encodeEnvelope, encryptPayload } from "../../web/static/crypto.js";

test("web/static/crypto.js round-trips under Node", async () => {
    const inner = encodeEnvelope({ type: "text" }, new TextEncoder().encode("harness probe"));
    const blob = await encryptPayload(inner, "alpha-bravo-charlie-delta", "123456", 1000);
    const { metadata, payload } = await decryptPayload(blob, "alpha-bravo-charlie-delta", "123456");
    assert.deepEqual(metadata, { type: "text" });
    assert.equal(new TextDecoder().decode(payload), "harness probe");
});
