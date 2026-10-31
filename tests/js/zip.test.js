// The store-only zip writer, checked by independent readers (Python zipfile and unzip -t).
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { crc32, zipParts } from "../../web/static/zip.js";

const encoder = new TextEncoder();

function zipBytes(entries, date) {
    const parts = zipParts(entries, date);
    const out = new Uint8Array(parts.reduce((sum, p) => sum + p.length, 0));
    let offset = 0;
    for (const part of parts) {
        out.set(part, offset);
        offset += part.length;
    }
    return out;
}

function withZipFile(bytes, fn) {
    const dir = mkdtempSync(join(tmpdir(), "pinveil-zip-"));
    try {
        const path = join(dir, "test.zip");
        writeFileSync(path, bytes);
        return fn(path);
    } finally {
        rmSync(dir, { recursive: true, force: true });
    }
}

// Lists [name, sha-free content as hex, compress_type, flag_bits] per entry, after testzip().
const PYTHON_READER = `
import sys, zipfile, json
with zipfile.ZipFile(sys.argv[1]) as z:
    bad = z.testzip()
    assert bad is None, bad
    print(json.dumps([[i.filename, z.read(i).hex(), i.compress_type, i.flag_bits, list(i.date_time)] for i in z.infolist()]))
`;

function readWithPython(path) {
    const result = spawnSync("python3", ["-I", "-c", PYTHON_READER, path], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
}

function testWithUnzip(path) {
    const result = spawnSync("unzip", ["-t", path], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    return result.stdout;
}

const hex = (bytes) => Buffer.from(bytes).toString("hex");

test("WEB-zip.crc32 matches the check value", () => {
    assert.equal(crc32(encoder.encode("123456789")), 0xcbf43926);
    assert.equal(crc32(new Uint8Array()), 0);
});

test("WEB-zip.independent-readers read names, contents, and the stored method", () => {
    const entries = [
        { name: "report.pdf", data: encoder.encode("%PDF fake") },
        { name: "empty.txt", data: new Uint8Array() },
        { name: "ünïcødé 🗝.bin", data: Uint8Array.from({ length: 70_000 }, (_, i) => i % 251) },
        { name: "notes (2).txt", data: encoder.encode("line\r\nline\n") },
    ];
    const date = new Date(2026, 9, 31, 13, 45, 30);
    withZipFile(zipBytes(entries, date), (path) => {
        const read = readWithPython(path);
        assert.deepEqual(
            read.map(([name, content]) => [name, content]),
            entries.map(({ name, data }) => [name, hex(data)]),
        );
        for (const [, , method, flags, dateTime] of read) {
            assert.equal(method, 0, "stored");
            assert.equal(flags & 0x0800, 0x0800, "UTF-8 names");
            assert.deepEqual(dateTime, [2026, 10, 31, 13, 45, 30]);
        }
        const report = testWithUnzip(path);
        assert.match(report, /No errors detected/);
    });
});

test("WEB-zip.empty archive is valid", () => {
    const bytes = zipBytes([]);
    assert.equal(bytes.length, 22);
    withZipFile(bytes, (path) => assert.deepEqual(readWithPython(path), []));
});

test("WEB-zip.data parts are the caller's views, not copies", () => {
    const data = encoder.encode("shared");
    const parts = zipParts([{ name: "a", data }]);
    assert.ok(parts.includes(data));
});

test("WEB-zip.zip32-bounds refuse instead of writing a broken archive", () => {
    // Fake views: only `length` is read before the bounds checks, so nothing big is allocated.
    const fake = (length) => ({ length });
    assert.throws(() => zipParts([{ name: "big", data: fake(2 ** 32) }]), RangeError);
    assert.throws(
        () => zipParts([{ name: "a", data: fake(2 ** 31) }, { name: "b", data: fake(2 ** 31) }]),
        RangeError,
        "offsets past 4 GiB",
    );
    assert.throws(() => zipParts(Array.from({ length: 65_536 }, () => ({ name: "x", data: new Uint8Array() }))), RangeError);
});
