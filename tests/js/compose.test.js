import assert from "node:assert/strict";
import { test } from "node:test";

import {
    DEFAULT_MIME_TYPE,
    checkMessage,
    customTtlMinutes,
    mimeTypeOf,
    openErrorText,
    storeModeNotice,
    ttlFromMinutes,
} from "../../web/static/compose.js";
import { blobLength } from "../../web/static/envelope.js";
import { encodePayload } from "../../web/static/payload.js";
import { PinveilError } from "../../web/static/protocol.js";

const config = {
    storeMode: "memory",
    maxPayloadBytes: 26_214_400,
    maxFiles: 256,
    maxManifestBytes: 262_144,
    pbkdf2Iterations: 600_000,
    minTtlSeconds: 60,
    maxTtlSeconds: 86_400,
    defaultTtlSeconds: 900,
    ttlPresets: [],
};

// Stand-ins for browser File objects: checkMessage reads only name, type, and size.
const meta = (name, size, type = "text/plain") => ({ name, size, type });

test("WEB-create.exact-size the pre-upload size equals the real blob size", () => {
    const files = [meta("a.txt", 3), meta("b.bin", 0, "")];
    const size = checkMessage({ text: "hi", files }, config);
    const payload = encodePayload(
        {
            text: "hi",
            files: [
                { name: "a.txt", mimeType: "text/plain", data: new Uint8Array(3) },
                { name: "b.bin", mimeType: DEFAULT_MIME_TYPE, data: new Uint8Array(0) },
            ],
        },
        config,
    );
    assert.equal(size, blobLength(payload.length, config.pbkdf2Iterations));
});

test("WEB-create.exact-size refuses one byte over the limit and allows exactly the limit", () => {
    // 100 and 101 have the same digit count, so the manifest length is the same.
    const atLimit = { ...config, maxPayloadBytes: checkMessage({ text: "", files: [meta("f", 100)] }, config) };
    assert.equal(checkMessage({ text: "", files: [meta("f", 100)] }, atLimit), atLimit.maxPayloadBytes);
    assert.throws(() => checkMessage({ text: "", files: [meta("f", 101)] }, atLimit), /once encrypted/);
});

test("WEB-create.limits-from-config file count, manifest, and empty messages", () => {
    const two = { ...config, maxFiles: 2 };
    assert.throws(() => checkMessage({ text: "", files: [meta("a", 1), meta("b", 1), meta("c", 1)] }, two), /up to 2 files/);
    const small = { ...config, maxManifestBytes: 60 };
    assert.throws(() => checkMessage({ text: "", files: [meta("a-rather-long-name.txt", 1)] }, small), /names take up/);
    assert.throws(() => checkMessage({ text: "", files: [] }, config), /Add some text/);
    assert.throws(() => checkMessage({ text: "", files: [meta("x".repeat(256), 1)] }, config), /longer than 255/);
});

test("WEB-create.text empty text with files sends no text part", () => {
    const withEmpty = checkMessage({ text: "", files: [meta("a", 1)] }, config);
    const withText = checkMessage({ text: "x", files: [meta("a", 1)] }, config);
    assert.ok(withText > withEmpty);
});

test("WEB-create.mime unusable browser types become octet-stream", () => {
    assert.equal(mimeTypeOf({ type: "" }), DEFAULT_MIME_TYPE);
    assert.equal(mimeTypeOf({ type: "a".repeat(256) }), DEFAULT_MIME_TYPE);
    assert.equal(mimeTypeOf({ type: "image/png" }), "image/png");
});

test("WEB-create.ttl custom minutes stay inside the server range", () => {
    assert.deepEqual(customTtlMinutes(config), { min: 1, max: 1440 });
    assert.equal(ttlFromMinutes("15", config), 900);
    for (const bad of ["0", "1441", "1.5", "", "abc", "-1"]) {
        assert.throws(() => ttlFromMinutes(bad, config), /from 1 to 1440/, bad);
    }
    const odd = { ...config, minTtlSeconds: 90, maxTtlSeconds: 299 };
    assert.deepEqual(customTtlMinutes(odd), { min: 2, max: 4 });
    const none = { ...config, minTtlSeconds: 61, maxTtlSeconds: 119 };
    assert.equal(customTtlMinutes(none), null);
    assert.throws(() => ttlFromMinutes("1", none), /doesn't accept/);
});

test("INV-10.browser-notice wording names the store mode's restart behavior", () => {
    assert.match(storeModeNotice("memory"), /restart deletes every message/);
    assert.match(storeModeNotice("sqlite"), /survive a restart.*recoverable/);
});

test("WEB-retrieve.open-errors stay generic and say the message is gone", () => {
    const decrypt = openErrorText(new PinveilError("decrypt", "failed"));
    assert.match(decrypt, /^Cannot open this message/);
    assert.match(decrypt, /already deleted/);
    for (const error of [new PinveilError("header", "malformed"), new PinveilError("payload", "truncated"), new Error("x")]) {
        assert.equal(openErrorText(error), "Cannot open this message. The server has already deleted it.");
    }
});
