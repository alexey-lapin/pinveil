import assert from "node:assert/strict";
import { test } from "node:test";

import { openBlob, openPayload, sealPayload } from "../../web/static/crypto.js";
import { decodeOuter } from "../../web/static/envelope.js";
import { CEILING_LIMITS, encodePayload } from "../../web/static/payload.js";
import { KDF_ITERATIONS_MIN } from "../../web/static/protocol.js";

const PASSPHRASE = "alpha-bravo-charlie-delta";
const PIN = "123456";
const sample = () => ({ text: "hello", files: [{ name: "a.txt", mimeType: "text/plain", data: new TextEncoder().encode("abc") }] });

/** Replays bytes 0, 1, 2, ... like a deterministic CSPRNG. */
function counter() {
    let next = 0;
    return (bytes) => {
        for (let i = 0; i < bytes.length; i += 1) {
            bytes[i] = next++ & 0xff;
        }
    };
}

async function sealed() {
    return sealPayload(encodePayload(sample(), CEILING_LIMITS), PASSPHRASE, PIN, KDF_ITERATIONS_MIN);
}

function plain(payload) {
    return { text: payload.text, files: payload.files.map((f) => ({ ...f, data: [...f.data] })) };
}

function stage(expected) {
    return (error) => error.stage === expected;
}

test("INV-4 round trip needs both the passphrase and the PIN", async () => {
    const blob = await sealed();
    assert.deepEqual(plain(await openPayload(blob, PASSPHRASE, PIN)), plain(sample()));
    await assert.rejects(openPayload(blob, PASSPHRASE, "123457"), stage("decrypt"));
    await assert.rejects(openPayload(blob, "alpha-bravo-charlie-echo", PIN), stage("decrypt"));
});

test("INV-14.tamper-before-render a tampered blob never decrypts", async () => {
    const blob = await sealed();
    const headerLength = new DataView(blob.buffer).getUint32(0, false);
    const tag = blob.slice();
    tag[tag.length - 1] ^= 1;
    const body = blob.slice();
    body[4 + headerLength] ^= 1;
    const other = await sealed();
    const mixed = new Uint8Array(blob.length);
    mixed.set(other.subarray(0, 4 + headerLength));
    mixed.set(blob.subarray(4 + headerLength), 4 + headerLength);
    for (const tampered of [tag, body, mixed]) {
        await assert.rejects(openBlob(tampered, PASSPHRASE, PIN), stage("decrypt"));
    }
});

test("seal draws salt, IV, then key, and is deterministic under fixed entropy", async () => {
    const bytes = encodePayload(sample(), CEILING_LIMITS);
    const first = await sealPayload(bytes, PASSPHRASE, PIN, KDF_ITERATIONS_MIN, counter());
    const second = await sealPayload(bytes, PASSPHRASE, PIN, KDF_ITERATIONS_MIN, counter());
    assert.deepEqual(first, second);
    const { header } = decodeOuter(first);
    assert.deepEqual([...header.kdfSalt], [...Array(16).keys()]);
    assert.deepEqual([...header.iv], [...Array(12).keys()].map((i) => 16 + i));
});

test("seal refuses bad inputs and failed entropy", async () => {
    await assert.rejects(sealPayload(new Uint8Array([1]), PASSPHRASE, "12345", KDF_ITERATIONS_MIN), /six digits/);
    await assert.rejects(sealPayload(new Uint8Array([1]), PASSPHRASE, PIN, 1000), /kdf-iterations-out-of-range/);
    await assert.rejects(
        sealPayload(new Uint8Array([1]), PASSPHRASE, PIN, KDF_ITERATIONS_MIN, () => {
            throw new Error("entropy exhausted");
        }),
        /entropy exhausted/,
    );
});

test("open refuses a bad PIN without deriving", async () => {
    await assert.rejects(openBlob(await sealed(), PASSPHRASE, "12a456"), stage("input"));
});
