import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import { exportNames } from "../../web/static/export-names.js";

const vectors = JSON.parse(await readFile(new URL("../vectors/export-names.json", import.meta.url), "utf8"));

test("export names match the shared vectors", () => {
    assert.ok(vectors.cases.length >= 10);
    for (const { case: name, input, output } of vectors.cases) {
        assert.deepEqual(exportNames(input), output, name);
    }
});

test("export names are unique, short, and safe", () => {
    const out = exportNames(["a", "A", "a", "a (2)", "CON", "con", "..", "", "x/y", "x_y"]);
    assert.equal(new Set(out.map((n) => n.toLowerCase())).size, out.length, JSON.stringify(out));
    for (const name of out) {
        assert.ok(name.length > 0 && new TextEncoder().encode(name).length <= 255);
        assert.doesNotMatch(name, /[/\\:]/u);
        assert.doesNotMatch(name, /[. ]$/u);
    }
});
