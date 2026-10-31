// Cross-client vectors (coverage FMT-vectors). The browser modules must open every positive blob
// in both files, re-seal it byte for byte, and refuse every negative at its stage.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import { openPayload, sealPayload } from "../../web/static/crypto.js";
import { base64Decode } from "../../web/static/envelope.js";
import { CEILING_LIMITS, encodePayload } from "../../web/static/payload.js";
import { JS_VECTORS_URL, buildJsVectors, replayFill } from "./vectors-js.js";

const load = async (name) => JSON.parse(await readFile(new URL(`../vectors/${name}`, import.meta.url), "utf8"));
const jsMade = await load("js-made.json");
const rustMade = await load("rust-made.json");

const fromHex = (value) => Uint8Array.from(Buffer.from(value, "hex"));
const payloadOf = (spec) => ({
    text: spec.text,
    files: spec.files.map((f) => ({ name: f.name, mimeType: f.mimeType, data: base64Decode(f.data) })),
});
const plain = (payload) => ({ text: payload.text, files: payload.files.map((f) => ({ ...f, data: Buffer.from(f.data).toString("base64") })) });

async function checkPositive(cases) {
    assert.ok(cases.length > 0);
    for (const vector of cases) {
        const blob = base64Decode(vector.blob);
        const expected = payloadOf(vector.payload);
        const opened = await openPayload(blob, vector.passphrase, vector.pin);
        assert.deepEqual(plain(opened), plain(expected), `${vector.case}: payload`);
        const resealed = await sealPayload(
            encodePayload(expected, CEILING_LIMITS),
            vector.passphrase,
            vector.pin,
            vector.kdfIterations,
            replayFill(fromHex(vector.entropy)),
        );
        assert.ok(Buffer.from(resealed).equals(Buffer.from(blob)), `${vector.case}: re-sealed bytes differ`);
    }
}

test("FMT-vectors js-made.json reproduces exactly", async () => {
    assert.equal(await buildJsVectors(), await readFile(JS_VECTORS_URL, "utf8"), "js-made.json is stale; run node vectors-js.js --write");
});

test("FMT-vectors js-made positive blobs open and re-seal exactly", async () => {
    await checkPositive(jsMade.positive);
});

test("FMT-vectors rust-made positive blobs open and re-seal exactly", async () => {
    await checkPositive(rustMade.positive);
});

test("FMT-vectors negatives are refused at their stage", async () => {
    assert.ok(jsMade.negative.length >= 20);
    for (const vector of jsMade.negative) {
        const started = performance.now();
        await assert.rejects(
            openPayload(base64Decode(vector.blob), vector.passphrase, vector.pin),
            (error) => error.stage === vector.stage,
            `${vector.case}: expected stage ${vector.stage}`,
        );
        if (vector.stage === "header") {
            assert.ok(performance.now() - started < 100, `${vector.case}: header refusal must not derive keys`);
        }
    }
});
