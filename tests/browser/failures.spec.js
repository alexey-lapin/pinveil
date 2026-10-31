// Failures that must stop before anything is uploaded or consumed, and limits from the config.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { apiRequests, expect, file, recorder, startServer, test } from "./harness.js";

const WORDLIST = fileURLToPath(new URL("../../assets/eff_large_wordlist.txt", import.meta.url));

/** Serves `body` with `status` in place of the real wordlist. */
async function replaceWordlist(page, status, body) {
    await page.route("**/static/eff_large_wordlist.txt", (route) => route.fulfill({ status, body, contentType: "text/plain" }));
}

async function submitText(page, server) {
    await page.goto(`${server.url}/`);
    await page.locator("#message-text").fill("never uploaded");
    await page.locator("#submit-button").click();
}

test.describe("INV-12 wordlist failures stop creation before upload", () => {
    test("INV-12.js-http-404", async ({ page, server, sent }) => {
        await replaceWordlist(page, 404, "missing");
        await submitText(page, server);
        await expect(page.locator("#create-status")).toContainText("HTTP 404");
        expect(apiRequests(sent())).toHaveLength(0);
    });

    test("INV-12.js-truncated", async ({ page, server, sent }) => {
        const lines = (await readFile(WORDLIST, "utf8")).trimEnd().split("\n");
        await replaceWordlist(page, 200, `${lines.slice(0, 7775).join("\n")}\n`);
        await submitText(page, server);
        await expect(page.locator("#create-status")).toContainText("7775 entries");
        expect(apiRequests(sent())).toHaveLength(0);
    });

    test("INV-12.js-duplicates", async ({ page, server, sent }) => {
        const lines = (await readFile(WORDLIST, "utf8")).trimEnd().split("\n");
        lines[1] = `${lines[1].split("\t")[0]}\t${lines[0].split("\t")[1]}`;
        await replaceWordlist(page, 200, `${lines.join("\n")}\n`);
        await submitText(page, server);
        await expect(page.locator("#create-status")).toContainText("repeats");
        expect(apiRequests(sent())).toHaveLength(0);
    });
});

test("WEB-retrieve.missing-fragment errors at once and can't consume the message", async ({ page, server, sent }) => {
    await page.goto(`${server.url}/message/some-id-1234`);
    await expect(page.locator("#retrieve-status")).toContainText("link is incomplete");
    await expect(page.locator("#retrieve-button")).toBeDisabled();
    await expect(page.locator("#retrieve-pin")).toBeDisabled();
    expect(apiRequests(sent())).toHaveLength(0);
});

test.describe("limits come from the server's config", () => {
    let small;
    test.beforeAll(async () => {
        small = await startServer({
            PINVEIL_MESSAGES__MAX_PAYLOAD_BYTES: "4000",
            PINVEIL_MESSAGES__MAX_FILES: "2",
            PINVEIL_MESSAGES__MIN_TTL: "5m",
            PINVEIL_MESSAGES__MAX_TTL: "2h",
            PINVEIL_MESSAGES__TTL_PRESETS: "5m,90m",
            PINVEIL_MESSAGES__DEFAULT_TTL: "90m",
        });
    });
    test.afterAll(() => small.stop());

    test("WEB-create.exact-size an oversized message is refused before upload", async ({ page }) => {
        const sent = recorder(small);
        await page.goto(`${small.url}/`);
        await page.locator("#message-files").setInputFiles([file("big.bin", Buffer.alloc(5000))]);
        await expect(page.locator("#create-status")).toContainText("This server accepts up to 3.9 KB");
        await page.locator("#submit-button").click();
        await expect(page.locator("#create-status")).toContainText("once encrypted");
        expect(apiRequests(sent())).toHaveLength(0);
    });

    test("WEB-create.limits-from-config file count over the server's max is refused", async ({ page }) => {
        const sent = recorder(small);
        await page.goto(`${small.url}/`);
        await expect(page.locator("#file-limits")).toContainText("Up to 2 files");
        await page.locator("#message-files").setInputFiles([file("a", "1"), file("b", "2"), file("c", "3")]);
        await page.locator("#submit-button").click();
        await expect(page.locator("#create-status")).toHaveText("This server accepts up to 2 files per message.");
        expect(apiRequests(sent())).toHaveLength(0);
    });

    test("WEB-create.ttl presets, default, and custom range follow the config", async ({ page }) => {
        const sent = recorder(small);
        await page.goto(`${small.url}/`);
        const options = await page.locator("#ttl-preset option").evaluateAll((all) => all.map((o) => [o.value, o.textContent]));
        expect(options).toEqual([
            ["300", "5 minutes"],
            ["5400", "90 minutes"],
            ["custom", "Custom"],
        ]);
        await expect(page.locator("#ttl-preset")).toHaveValue("5400");
        await page.locator("#ttl-preset").selectOption("custom");
        await expect(page.locator("#custom-ttl-range")).toHaveText("From 5 to 120 minutes.");
        await page.locator("#message-text").fill("x");
        await page.locator("#custom-ttl-minutes").fill("121");
        await page.locator("#submit-button").click();
        await expect(page.locator("#create-status")).toContainText("from 5 to 120");
        expect(apiRequests(sent())).toHaveLength(0);

        // An accepted custom value reaches the server as seconds.
        await page.locator("#custom-ttl-minutes").fill("7");
        await page.locator("#submit-button").click();
        await expect(page.locator("#result-ready")).toBeVisible();
        const [create] = apiRequests(sent());
        expect(create.body.toString("latin1")).toMatch(/name="ttl"\r\n\r\n420\r\n/);
    });
});
