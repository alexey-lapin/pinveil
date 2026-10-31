// Edge cases from the Phase 3 review: a body lost after 200, TTL defaults outside the presets,
// the clipboard fallback, and the order of size checks and file reads.
import { apiRequests, createMessage, expect, file, openMessage, recorder, startServer, test } from "./harness.js";

test.describe("WEB-retrieve.lost-in-transit", () => {
    let cut;
    test.beforeAll(async () => {
        cut = await startServer({}, { truncateRetrieve: true });
    });
    test.afterAll(() => cut.stop());

    test("a body cut short after 200 offers no retry and says the message is gone", async ({ page, context }) => {
        const sent = recorder(cut);
        const { link, pin } = await createMessage(page, cut, { text: "lost on the way" });
        const reader = await context.newPage();
        await openMessage(reader, link, pin);
        await expect(reader.locator("#retrieve-status")).toContainText("connection dropped");
        await expect(reader.locator("#retrieve-status")).toContainText("already deleted");
        await expect(reader.locator("#retrieve-form")).toBeHidden();
        await expect(reader.locator("#output-card")).toBeHidden();
        expect(apiRequests(sent()).filter((r) => r.path.endsWith("/retrieve"))).toHaveLength(1);
    });
});

const TTL_CASES = [
    {
        name: "a default that isn't a preset",
        env: { PINVEIL_MESSAGES__TTL_PRESETS: "5m,15m", PINVEIL_MESSAGES__DEFAULT_TTL: "10m" },
        options: [["300", "5 minutes"], ["600", "10 minutes"], ["900", "15 minutes"], ["custom", "Custom"]],
        posted: "600",
    },
    {
        name: "no presets and a whole-minute range",
        env: { PINVEIL_MESSAGES__TTL_PRESETS: "", PINVEIL_MESSAGES__DEFAULT_TTL: "30m", PINVEIL_MESSAGES__MAX_TTL: "1h" },
        options: [["1800", "30 minutes"], ["custom", "Custom"]],
        posted: "1800",
    },
    {
        name: "no presets and a sub-minute range",
        env: {
            PINVEIL_MESSAGES__TTL_PRESETS: "",
            PINVEIL_MESSAGES__MIN_TTL: "1s",
            PINVEIL_MESSAGES__MAX_TTL: "59s",
            PINVEIL_MESSAGES__DEFAULT_TTL: "30s",
        },
        options: [["30", "30 seconds"]],
        posted: "30",
    },
];

for (const ttl of TTL_CASES) {
    test(`WEB-create.ttl-default ${ttl.name}: selected first, posted, and created`, async ({ page }) => {
        const server = await startServer(ttl.env);
        try {
            const sent = recorder(server);
            await page.goto(`${server.url}/`);
            const options = await page.locator("#ttl-preset option").evaluateAll((all) => all.map((o) => [o.value, o.textContent]));
            expect(options).toEqual(ttl.options);
            await expect(page.locator("#ttl-preset")).toHaveValue(ttl.posted);
            await expect(page.locator("#custom-ttl-field")).toBeHidden();
            await createMessage(page, server, { text: "ttl" });
            const [create] = apiRequests(sent());
            expect(create.body.toString("latin1")).toContain(`name="ttl"\r\n\r\n${ttl.posted}\r\n`);
        } finally {
            server.stop();
        }
    });
}

test("WEB-retrieve.copy-fallback selects the decrypted text when the clipboard is denied", async ({ page, context, server }) => {
    const { link, pin } = await createMessage(page, server, { text: "copy by hand" });
    const reader = await context.newPage();
    await reader.addInitScript(() => {
        Object.defineProperty(navigator, "clipboard", {
            value: { writeText: async () => Promise.reject(new DOMException("Denied", "NotAllowedError")) },
            configurable: true,
        });
    });
    await openMessage(reader, link, pin);
    await expect(reader.locator("#message-output")).toHaveText("copy by hand");
    await reader.locator("#copy-text-button").click();
    await expect(reader.locator("#copy-text-button")).toHaveText(/^Selected/);
    expect(await reader.evaluate(() => window.getSelection().toString())).toBe("copy by hand");
});

/** Counts `File.arrayBuffer` calls in `window.fileReads`; `extra` bytes are appended to each read. */
async function instrumentFileReads(page, extra = 0) {
    await page.addInitScript((extraBytes) => {
        window.fileReads = 0;
        const read = File.prototype.arrayBuffer;
        File.prototype.arrayBuffer = async function arrayBuffer() {
            window.fileReads += 1;
            const bytes = new Uint8Array(await read.call(this));
            const grown = new Uint8Array(bytes.length + extraBytes);
            grown.set(bytes);
            return grown.buffer;
        };
    }, extra);
}

test.describe("WEB-create.check-before-read", () => {
    let small;
    test.beforeAll(async () => {
        small = await startServer({ PINVEIL_MESSAGES__MAX_PAYLOAD_BYTES: "4000" });
    });
    test.afterAll(() => small.stop());

    test("an oversized selection is refused without reading any file", async ({ page }) => {
        const sent = recorder(small);
        await instrumentFileReads(page);
        await page.goto(`${small.url}/`);
        await page.locator("#message-files").setInputFiles([file("big.bin", Buffer.alloc(5000))]);
        await page.locator("#submit-button").click();
        await expect(page.locator("#create-status")).toContainText("once encrypted");
        expect(await page.evaluate(() => window.fileReads)).toBe(0);
        expect(apiRequests(sent())).toHaveLength(0);
    });

    test("a file that grew after it was picked is refused before upload", async ({ page }) => {
        const sent = recorder(small);
        await instrumentFileReads(page, 1);
        await page.goto(`${small.url}/`);
        await page.locator("#message-files").setInputFiles([file("grows.txt", "abc")]);
        await page.locator("#submit-button").click();
        await expect(page.locator("#create-status")).toHaveText("A file changed after it was added. Remove it and add it again.");
        expect(await page.evaluate(() => window.fileReads)).toBe(1);
        expect(apiRequests(sent())).toHaveLength(0);
    });
});
