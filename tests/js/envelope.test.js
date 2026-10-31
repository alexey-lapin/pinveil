import assert from "node:assert/strict";
import { test } from "node:test";

import { blobLength, decodeOuter, encodeOuter } from "../../web/static/envelope.js";
import { KDF_ITERATIONS_MAX, KDF_ITERATIONS_MIN, TAG_LEN } from "../../web/static/protocol.js";

const header = (kdfIterations) => ({
    wrappedContentKey: new Uint8Array(40).fill(7),
    kdfSalt: new Uint8Array(16).fill(1),
    iv: new Uint8Array(12).fill(2),
    kdfIterations,
});

const CANONICAL =
    '{"wrappedContentKey":"BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBw==","kdfSalt":"AQEBAQEBAQEBAQEBAQEBAQ==","iv":"AgICAgICAgICAgIC","kdfAlgorithm":"PBKDF2-SHA-256","kdfIterations":600000,"encryptionAlgorithm":"AES-256-GCM","wrappingAlgorithm":"AES-KW","schemaVersion":2}';

function framed(json, ciphertextLength = TAG_LEN) {
    const bytes = new TextEncoder().encode(json);
    const blob = new Uint8Array(4 + bytes.length + ciphertextLength);
    new DataView(blob.buffer).setUint32(0, bytes.length, false);
    blob.set(bytes, 4);
    return blob;
}

function rejects(blob, reason) {
    assert.throws(
        () => decodeOuter(blob),
        (error) => error.stage === "header" && error.reason === reason,
        `expected header/${reason}`,
    );
}

test("FMT-outer round trip and canonical bytes match core", () => {
    const ciphertext = new Uint8Array(40).fill(9);
    const blob = encodeOuter(header(600_000), ciphertext);
    assert.equal(new TextDecoder().decode(blob.subarray(4, blob.length - 40)), CANONICAL);
    const decoded = decodeOuter(blob);
    assert.deepEqual(decoded.header, header(600_000));
    assert.deepEqual(decoded.ciphertext, ciphertext);
});

test("FMT-outer blob length is exact", () => {
    for (const iterations of [KDF_ITERATIONS_MIN, 1_234_567, KDF_ITERATIONS_MAX]) {
        for (const payload of [0, 1, 1000]) {
            const blob = encodeOuter(header(iterations), new Uint8Array(payload + TAG_LEN));
            assert.equal(blob.length, blobLength(payload, iterations));
        }
    }
});

test("INV-14 truncated and oversized headers", () => {
    rejects(new Uint8Array([]), "truncated");
    rejects(new Uint8Array([0, 0, 1]), "truncated");
    rejects(new Uint8Array([0, 0, 0, 200, 123]), "truncated");
    rejects(new Uint8Array([255, 255, 255, 255]), "header-too-large");
    rejects(framed(CANONICAL, TAG_LEN - 1), "truncated");
});

test("INV-14.bad-header-before-kdf every malformed header is refused in stage one", () => {
    const cases = [
        ["not json", "malformed"],
        ["[]", "malformed"],
        [CANONICAL.replace('"schemaVersion":2', '"schemaVersion":1'), "unsupported-version"],
        [CANONICAL.replace('"schemaVersion":2', '"schemaVersion":"2"'), "unsupported-version"],
        [CANONICAL.replace("AES-256-GCM", "AES-128-GCM"), "unsupported-algorithm"],
        [CANONICAL.replace("PBKDF2-SHA-256", "Argon2id"), "unsupported-algorithm"],
        [CANONICAL.replace('"AES-KW"', '"AES-KWP"'), "unsupported-algorithm"],
        [CANONICAL.replace("600000", "1"), "kdf-iterations-out-of-range"],
        [CANONICAL.replace("600000", "1000000000"), "kdf-iterations-out-of-range"],
        [CANONICAL.replace("600000", "99999999999999"), "kdf-iterations-out-of-range"],
        [CANONICAL.replace("600000", "-600000"), "malformed"],
        [CANONICAL.replace("AgICAgICAgICAgIC", "AgICAgICAgICAgI="), "bad-field-size"],
        [CANONICAL.replace("AgICAgICAgICAgIC", "AgICAgICAgICAgICAgI="), "bad-field-size"],
        [CANONICAL.replace('"iv"', '"nonce"'), "malformed"],
        [CANONICAL.replace("}", ',"extra":1}'), "malformed"],
        [CANONICAL.replaceAll(",", ", "), "not-canonical"],
        // Core rejects 600000.0 while parsing; here it parses as 600000 and fails canonicity.
        [CANONICAL.replace("600000", "600000.0"), "not-canonical"],
        // Duplicate keys: core's parser refuses them; JSON.parse keeps the last, so canonicity catches it.
        [CANONICAL.replace('{"wrappedContentKey"', '{"iv":"AgICAgICAgICAgIC","wrappedContentKey"'), "not-canonical"],
    ];
    for (const [json, reason] of cases) {
        rejects(framed(json), reason);
    }
});

test("INV-14.kdf-range edges", () => {
    for (const [iterations, ok] of [
        [KDF_ITERATIONS_MIN - 1, false],
        [KDF_ITERATIONS_MIN, true],
        [KDF_ITERATIONS_MAX, true],
        [KDF_ITERATIONS_MAX + 1, false],
    ]) {
        const blob = framed(CANONICAL.replace("600000", String(iterations)));
        if (ok) {
            decodeOuter(blob);
        } else {
            rejects(blob, "kdf-iterations-out-of-range");
        }
    }
});

test("FMT-fuzz random bytes only ever raise header errors", () => {
    for (let i = 0; i < 2000; i += 1) {
        const bytes = crypto.getRandomValues(new Uint8Array(Math.floor(Math.random() * 400)));
        if (i % 2 === 0 && bytes.length >= 4) {
            new DataView(bytes.buffer).setUint32(0, Math.floor(Math.random() * 500), false);
        }
        try {
            decodeOuter(bytes);
        } catch (error) {
            assert.equal(error.stage, "header", String(error));
        }
    }
});
