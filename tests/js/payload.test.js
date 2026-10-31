import assert from "node:assert/strict";
import { test } from "node:test";

import { CEILING_LIMITS, decodePayload, encodePayload } from "../../web/static/payload.js";
import { MAX_FILES_CEILING } from "../../web/static/protocol.js";

const enc = new TextEncoder();
const file = (name, data, mimeType = "text/plain") => ({ name, mimeType, data: enc.encode(data) });

function framed(manifest, body = new Uint8Array()) {
    const m = enc.encode(manifest);
    const bytes = new Uint8Array(4 + m.length + body.length);
    new DataView(bytes.buffer).setUint32(0, m.length, false);
    bytes.set(m, 4);
    bytes.set(body, 4 + m.length);
    return bytes;
}

function rejects(fn, reason) {
    assert.throws(fn, (error) => error.stage === "payload" && error.reason === reason, `expected payload/${reason}`);
}

function plain(payload) {
    return { text: payload.text, files: payload.files.map((f) => ({ ...f, data: [...f.data] })) };
}

test("FMT-inner round trips text, files, and both", () => {
    for (const payload of [
        { text: "hello", files: [] },
        { text: null, files: [file("a.txt", "abc"), file("a.txt", ""), file("ünïcødé 🗝.bin", "\u0000")] },
        { text: "note", files: [file("key.pem", "-----")] },
    ]) {
        assert.deepEqual(plain(decodePayload(encodePayload(payload, CEILING_LIMITS))), plain(payload));
    }
});

test("FMT-inner canonical bytes match core", () => {
    const bytes = encodePayload({ text: "hi", files: [file("a.txt", "abc")] }, CEILING_LIMITS);
    assert.deepEqual(
        bytes,
        framed('{"text":{"byteLength":2},"files":[{"name":"a.txt","mimeType":"text/plain","byteLength":3}]}', enc.encode("hiabc")),
    );
    const noText = encodePayload({ text: null, files: [file("a", "")] }, CEILING_LIMITS);
    assert.match(new TextDecoder().decode(noText), /\{"text":null,"files":\[/);
});

test("FMT-inner creation limits vs ceilings", () => {
    const three = { text: null, files: [file("a", ""), file("b", ""), file("c", "")] };
    rejects(() => encodePayload(three, { maxFiles: 2, maxManifestBytes: 1024 }), "too-many-files");
    assert.ok(decodePayload(encodePayload(three, CEILING_LIMITS)), "decoding only checks the ceilings");
    rejects(
        () => encodePayload({ text: null, files: [file("n".repeat(60), "")] }, { maxFiles: 10, maxManifestBytes: 40 }),
        "manifest-too-large",
    );
});

test("FMT-inner names and emptiness", () => {
    const maxName = "é".repeat(127) + "x";
    encodePayload({ text: null, files: [file(maxName, "")] }, CEILING_LIMITS);
    for (const [payload, reason] of [
        [{ text: null, files: [] }, "empty"],
        [{ text: "", files: [] }, "empty"],
        [{ text: null, files: [file("", "x")] }, "bad-name"],
        [{ text: null, files: [file("é".repeat(128), "x")] }, "bad-name"],
        [{ text: null, files: [file("a", "", "")] }, "bad-mime-type"],
    ]) {
        rejects(() => encodePayload(payload, CEILING_LIMITS), reason);
    }
});

test("INV-14.bad-manifest-before-output every malformed manifest is refused", () => {
    const good = '{"text":{"byteLength":2},"files":[{"name":"a.txt","mimeType":"text/plain","byteLength":3}]}';
    const hiabc = enc.encode("hiabc");
    const tooMany = `{"text":null,"files":[${Array(MAX_FILES_CEILING + 1).fill('{"name":"a","mimeType":"b","byteLength":0}').join(",")}]}`;
    const cases = [
        [new Uint8Array([0, 0]), "truncated"],
        [new Uint8Array([0, 0, 0, 50, 123]), "truncated"],
        [new Uint8Array([0x7f, 0xff, 0xff, 0xff]), "manifest-too-large"],
        [framed("nope"), "malformed"],
        [framed(good, enc.encode("hiab")), "length-mismatch"],
        [framed(good, enc.encode("hiabcd")), "length-mismatch"],
        [framed(good.replace('"byteLength":3', '"byteLength":-1'), hiabc), "malformed"],
        [framed(good.replace('"byteLength":3', '"byteLength":9007199254740992'), hiabc), "malformed"],
        [
            framed(good.replace('"byteLength":2', '"byteLength":9007199254740991').replace('"byteLength":3', '"byteLength":9007199254740991')),
            "length-mismatch",
        ],
        [framed(good.replace('"name":"a.txt"', '"name":""'), hiabc), "bad-name"],
        [framed(good.replace("text/plain", ""), hiabc), "bad-mime-type"],
        [framed('{"text":null,"files":[]}'), "empty"],
        [framed(good.replaceAll(",", ", "), hiabc), "not-canonical"],
        [framed('{"files":[{"name":"a.txt","mimeType":"text/plain","byteLength":3}]}', enc.encode("abc")), "malformed"],
        [framed(good.replace("}]}", ',"extra":1}]}'), hiabc), "malformed"],
        [framed(good, new Uint8Array([0xff, 0xfe, 97, 98, 99])), "invalid-text"],
        [framed(tooMany), "too-many-files"],
    ];
    for (const [bytes, reason] of cases) {
        rejects(() => decodePayload(bytes), reason);
    }
});

test("FMT-fuzz random bytes only ever raise payload errors", () => {
    for (let i = 0; i < 2000; i += 1) {
        const bytes = crypto.getRandomValues(new Uint8Array(Math.floor(Math.random() * 400)));
        if (i % 2 === 0 && bytes.length >= 4) {
            new DataView(bytes.buffer).setUint32(0, Math.floor(Math.random() * 500), false);
        }
        try {
            decodePayload(bytes);
        } catch (error) {
            assert.equal(error.stage, "payload", String(error));
        }
    }
});
