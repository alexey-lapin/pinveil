/**
 * Builds tests/vectors/js-made.json: blobs sealed by the browser modules with fixed entropy, and
 * negative cases both clients must refuse at a given stage. `node vectors-js.js --write` rewrites
 * the file; vectors.test.js checks the committed file still reproduces exactly.
 */

import { writeFile } from "node:fs/promises";

import { sealPayload } from "../../web/static/crypto.js";
import { base64Decode, base64Encode, decodeOuter } from "../../web/static/envelope.js";
import { CEILING_LIMITS, encodePayload } from "../../web/static/payload.js";

export const JS_VECTORS_URL = new URL("../vectors/js-made.json", import.meta.url);

const enc = new TextEncoder();

/** 60 deterministic entropy bytes (salt, IV, content key) for case `seed`. */
function entropyFor(seed) {
    return Uint8Array.from({ length: 60 }, (_, i) => (seed * 37 + i * 11) & 0xff);
}

export function replayFill(bytes) {
    let offset = 0;
    return (target) => {
        if (offset + target.length > bytes.length) {
            throw new Error("vector entropy exhausted");
        }
        target.set(bytes.subarray(offset, offset + target.length));
        offset += target.length;
    };
}

const hex = (bytes) => Buffer.from(bytes).toString("hex");
const file = (name, mimeType, data) => ({ name, mimeType, data });

const POSITIVE = [
    { case: "text only", passphrase: "abacus-abdomen-zoom-t-shirt", pin: "000000", kdfIterations: 600_000, payload: { text: "hello from the browser", files: [] } },
    {
        case: "unicode and control characters in text",
        passphrase: "yo-yo-felt-tip-drop-down-zoom",
        pin: "999999",
        kdfIterations: 600_000,
        payload: { text: "Grüße 🗝\u0000\t\r\n\u001b[31mred end", files: [] },
    },
    {
        case: "one binary file",
        passphrase: "alpha-bravo-charlie-delta",
        pin: "123456",
        kdfIterations: 600_000,
        payload: { text: null, files: [file("key.bin", "application/octet-stream", Uint8Array.from({ length: 256 }, (_, i) => i))] },
    },
    {
        case: "text plus files: empty, duplicate, and unicode names",
        passphrase: "alpha-bravo-charlie-delta",
        pin: "482913",
        kdfIterations: 600_000,
        payload: {
            text: "see attached",
            files: [
                file("a.txt", "text/plain", enc.encode("abc")),
                file("a.txt", "text/plain", new Uint8Array()),
                file("ünïcødé 🗝.bin", "application/octet-stream", Uint8Array.of(0, 255, 128)),
            ],
        },
    },
    { case: "maximum KDF iterations", passphrase: "zoom-zoom-zoom-zoom", pin: "314159", kdfIterations: 2_000_000, payload: { text: "slow", files: [] } },
];

function specOf(payload) {
    return {
        text: payload.text,
        files: payload.files.map((f) => ({ name: f.name, mimeType: f.mimeType, data: base64Encode(f.data) })),
    };
}

async function seal(spec, seed) {
    const entropy = entropyFor(seed);
    const bytes = encodePayload(spec.payload, CEILING_LIMITS);
    const blob = await sealPayload(bytes, spec.passphrase, spec.pin, spec.kdfIterations, replayFill(entropy));
    return { entropy, blob };
}

/** Sealed raw plaintext, for payload-stage negatives. */
async function sealRaw(plaintext, seed) {
    return sealPayload(plaintext, BASE.passphrase, BASE.pin, 600_000, replayFill(entropyFor(seed)));
}

function framed(json, body = new Uint8Array()) {
    const bytes = enc.encode(json);
    const out = new Uint8Array(4 + bytes.length + body.length);
    new DataView(out.buffer).setUint32(0, bytes.length, false);
    out.set(bytes, 4);
    out.set(body, 4 + bytes.length);
    return out;
}

const BASE = { passphrase: "alpha-bravo-charlie-delta", pin: "123456" };

async function negatives() {
    const base = (await seal({ ...BASE, kdfIterations: 600_000, payload: { text: "negative base", files: [] } }, 100)).blob;
    const other = (await seal({ ...BASE, kdfIterations: 600_000, payload: { text: "negative base", files: [] } }, 101)).blob;
    const headerLength = new DataView(base.buffer).getUint32(0, false);
    const headerText = new TextDecoder().decode(base.subarray(4, 4 + headerLength));
    const ciphertext = base.subarray(4 + headerLength);
    const withHeader = (text, body = ciphertext) => framed(text, body);
    const flip = (bytes, index) => {
        const copy = bytes.slice();
        copy[index < 0 ? copy.length + index : index] ^= 1;
        return copy;
    };
    const otherWrapped = base64Encode(decodeOuter(other).header.wrappedContentKey);
    const baseWrapped = base64Encode(decodeOuter(base).header.wrappedContentKey);
    const manifest = '{"text":{"byteLength":2},"files":[]}';

    const cases = [
        ["header: shorter than the length prefix", Uint8Array.of(0, 0, 1), "header"],
        ["header: length prefix over the 1 KiB cap", Uint8Array.of(0, 0, 4, 1), "header"],
        ["header: schema version 1", withHeader(headerText.replace('"schemaVersion":2', '"schemaVersion":1')), "header"],
        ["header: unknown algorithm label", withHeader(headerText.replace("AES-256-GCM", "ChaCha20-Poly1305")), "header"],
        ["header: KDF iterations far above the range (must fail without deriving)", withHeader(headerText.replace('"kdfIterations":600000', '"kdfIterations":1000000000')), "header"],
        ["header: KDF iterations below the range", withHeader(headerText.replace('"kdfIterations":600000', '"kdfIterations":1000')), "header"],
        ["header: IV of the wrong size", withHeader(headerText.replace(/"iv":"[^"]+"/u, '"iv":"AAAA"')), "header"],
        ["header: whitespace makes it non-canonical", withHeader(headerText.replace(",", ", ")), "header"],
        ["header: unknown extra field", withHeader(headerText.replace("}", ',"note":"x"}')), "header"],
        ["header: ciphertext shorter than the tag", withHeader(headerText, ciphertext.subarray(0, 15)), "header"],
        ["decrypt: flipped tag byte", flip(base, -1), "decrypt"],
        ["decrypt: flipped ciphertext byte", flip(base, 4 + headerLength), "decrypt"],
        ["decrypt: wrapped key from another blob", withHeader(headerText.replace(baseWrapped, otherWrapped)), "decrypt"],
        ["decrypt: wrong PIN", base, "decrypt", { pin: "654321" }],
        ["decrypt: wrong passphrase", base, "decrypt", { passphrase: "alpha-bravo-charlie-echo" }],
        ["payload: plaintext shorter than the prefix", await sealRaw(Uint8Array.of(0, 0), 102), "payload"],
        ["payload: body shorter than declared", await sealRaw(framed(manifest, enc.encode("h")), 103), "payload"],
        ["payload: trailing bytes after the body", await sealRaw(framed(manifest, enc.encode("hi!")), 104), "payload"],
        ["payload: non-canonical manifest", await sealRaw(framed('{"text": {"byteLength":2},"files":[]}', enc.encode("hi")), 105), "payload"],
        ["payload: no text and no files", await sealRaw(framed('{"text":null,"files":[]}'), 106), "payload"],
        ["payload: invalid UTF-8 text", await sealRaw(framed(manifest, Uint8Array.of(0xff, 0xfe)), 107), "payload"],
        ["payload: empty file name", await sealRaw(framed('{"text":null,"files":[{"name":"","mimeType":"a/b","byteLength":0}]}'), 108), "payload"],
        [
            "payload: 257 files, over the ceiling",
            await sealRaw(framed(`{"text":null,"files":[${Array(257).fill('{"name":"a","mimeType":"a/b","byteLength":0}').join(",")}]}`), 109),
            "payload",
        ],
        ["payload: declared length beyond 2^53-1", await sealRaw(framed('{"text":{"byteLength":9007199254740992},"files":[]}'), 110), "payload"],
    ];
    return cases.map(([name, blob, stage, override = {}]) => ({
        case: name,
        passphrase: override.passphrase ?? BASE.passphrase,
        pin: override.pin ?? BASE.pin,
        blob: base64Encode(blob),
        stage,
    }));
}

export async function buildJsVectors() {
    const positive = [];
    for (const [index, spec] of POSITIVE.entries()) {
        const { entropy, blob } = await seal(spec, index + 1);
        positive.push({
            case: spec.case,
            passphrase: spec.passphrase,
            pin: spec.pin,
            kdfIterations: spec.kdfIterations,
            entropy: hex(entropy),
            payload: specOf(spec.payload),
            blob: base64Encode(blob),
        });
    }
    const document = {
        description:
            "Made by web/static (tests/js/vectors-js.js). `entropy` is the salt, IV, and content key, in draw order. Both clients must open each positive blob to `payload`, re-seal it to the same bytes, and refuse each negative at `stage`.",
        positive,
        negative: await negatives(),
    };
    return `${JSON.stringify(document, null, 2)}\n`;
}

export { base64Decode };

if (process.argv.includes("--write")) {
    await writeFile(JS_VECTORS_URL, await buildJsVectors());
    console.log(`wrote ${JS_VECTORS_URL.pathname}`);
}
