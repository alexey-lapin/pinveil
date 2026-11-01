// WEB-roundtrip: the browser opens blobs that pinveil-core sealed (tests/vectors/rust-made.json).
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { expect, openMessage, test } from "./harness.js";

const VECTORS = fileURLToPath(new URL("../../tests/vectors/rust-made.json", import.meta.url));
const { positive } = JSON.parse(await readFile(VECTORS, "utf8"));

for (const vector of positive) {
    test(`WEB-roundtrip.rust-made ${vector.case}`, async ({ page, server }) => {
        const form = new FormData();
        form.append("blob", new Blob([Buffer.from(vector.blob, "base64")]), "blob");
        form.append("pin", vector.pin);
        form.append("ttl", "900");
        const response = await fetch(`${server.url}/api/messages`, { method: "POST", body: form });
        expect(response.status).toBe(201);
        const { id } = await response.json();

        await openMessage(page, `${server.url}/message/${id}#${vector.passphrase}`, vector.pin);
        const { text, files } = vector.payload;
        if (text !== null) {
            await expect(page.locator("#message-output")).toHaveText(text);
        } else {
            await expect(page.locator("#text-output")).toBeHidden();
        }
        await expect(page.locator("#download-list li")).toHaveCount(files.length);
        if (files.length > 0) {
            await expect(page.locator("#download-list .file-name")).toHaveText(files.map((f) => f.name));
        }
    });
}
