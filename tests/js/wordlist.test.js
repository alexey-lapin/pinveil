import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import { WORDLIST_LEN, loadWordList, parseWordList } from "../../web/static/wordlist.js";

const EMBEDDED = await readFile(new URL("../../assets/eff_large_wordlist.txt", import.meta.url), "utf8");
const lines = EMBEDDED.trimEnd().split("\n");

function replaceLine(lineNumber, replacement) {
    return lines.map((line, i) => (i + 1 === lineNumber ? replacement : line)).join("\n");
}

function fakeFetch(status, body) {
    return async () => new Response(body, { status });
}

test("INV-12 the real wordlist parses", () => {
    const words = parseWordList(EMBEDDED);
    assert.equal(words.length, WORDLIST_LEN);
    assert.equal(words[0], "abacus");
    assert.equal(words.at(-1), "zoom");
    for (const hyphenated of ["drop-down", "felt-tip", "t-shirt", "yo-yo"]) {
        assert.ok(words.includes(hyphenated));
    }
});

test("INV-12.js-http-404 an error response is refused", async () => {
    await assert.rejects(loadWordList(fakeFetch(404, "Not Found")), /HTTP 404/);
    await assert.rejects(loadWordList(fakeFetch(200, "Not Found")), /malformed/);
});

test("INV-12.js-truncated a short list is refused", async () => {
    await assert.rejects(loadWordList(fakeFetch(200, "11111\tabacus\n")), /expected 7776/);
    assert.throws(() => parseWordList(lines.slice(0, -1).join("\n")), /7775 entries/);
    assert.throws(() => parseWordList(""), /malformed/);
});

test("INV-12.js-duplicates repeated words or codes are refused", () => {
    assert.throws(() => parseWordList(replaceLine(2, "11112\tabacus")), /line 2 repeats/);
    assert.throws(() => parseWordList(replaceLine(2, "11111\tabdomen")), /line 2 repeats/);
});

test("INV-12 malformed lines are refused", () => {
    for (const bad of [
        "11112 abdomen",
        "11117\tabdomen",
        "1111\tabdomen",
        "11112\tAbdomen",
        "11112\tab-",
        "11112\t",
        "11112\tabdomen\r",
    ]) {
        assert.throws(() => parseWordList(replaceLine(2, bad)), /line 2 is malformed/, JSON.stringify(bad));
    }
    assert.throws(() => parseWordList(`${EMBEDDED}\n`), /malformed/);
});

test("INV-12 a failed load is not cached", async () => {
    await assert.rejects(loadWordList(fakeFetch(500, "")));
    const words = await loadWordList(fakeFetch(200, EMBEDDED));
    assert.equal(words.length, WORDLIST_LEN);
});
