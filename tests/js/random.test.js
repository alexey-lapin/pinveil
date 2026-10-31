import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import { generatePassphrase, generatePin, isValidPin, uniformBelow } from "../../web/static/random.js";
import { parseWordList } from "../../web/static/wordlist.js";

const WORDS = parseWordList(await readFile(new URL("../../assets/eff_large_wordlist.txt", import.meta.url), "utf8"));

/** Replays big-endian u32 draws, then throws like a failed CSPRNG. */
function replay(...values) {
    const queue = [...values];
    return (bytes) => {
        if (queue.length === 0) {
            throw new Error("entropy exhausted");
        }
        new DataView(bytes.buffer).setUint32(0, queue.shift(), false);
    };
}

test("GEN rejection boundary for 7776 matches core", () => {
    const limit = 4_294_964_736;
    assert.equal(uniformBelow(7776, replay(limit - 1)), 7775);
    assert.equal(uniformBelow(7776, replay(limit, 2 ** 32 - 1, 7)), 7);
});

test("GEN-pin boundaries match core", () => {
    assert.equal(generatePin(replay(0)), "000000");
    assert.equal(generatePin(replay(4_293_999_999)), "999999");
    assert.equal(generatePin(replay(4_294_000_000, 42)), "000042");
});

test("GEN-pin entropy failure aborts", () => {
    assert.throws(() => generatePin(replay()), /entropy exhausted/);
    assert.throws(() => uniformBelow(7776, replay(2 ** 32 - 1)), /entropy exhausted/);
});

test("GEN-pin system PINs are well formed", () => {
    for (let i = 0; i < 1000; i += 1) {
        assert.ok(isValidPin(generatePin()));
    }
    for (const bad of ["", "12345", "1234567", "12345a", "١٢٣٤٥٦", " 12345"]) {
        assert.equal(isValidPin(bad), false, bad);
    }
});

test("GEN-passphrase uses the validated list, same picks as core", () => {
    assert.equal(generatePassphrase(WORDS, replay(0, 1, 7775, 7776)), "abacus-abdomen-zoom-abacus");
    assert.throws(() => generatePassphrase(WORDS, replay(0, 1)), /entropy exhausted/);
});
