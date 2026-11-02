// Create in the browser, open in the browser: content kinds, PINs, downloads, and the zip.
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";

import {
    apiRequests,
    createMessage,
    expect,
    expectNoLeaks,
    file,
    fragmentOf,
    openMessage,
    test,
} from "./harness.js";

const SECRET_TEXT = "pinveil-plaintext-sentinel ünïcødé 🗝";

/** The decrypted download list as `[{name, exportName, size}]`. */
async function downloadList(page) {
    return page.locator("#download-list li").evaluateAll((items) =>
        items.map((item) => ({
            name: item.querySelector(".file-name").textContent,
            exportName: item.querySelector("button").dataset.exportName,
        })),
    );
}

async function download(page, trigger) {
    const [saved] = await Promise.all([page.waitForEvent("download"), trigger.click()]);
    return { name: saved.suggestedFilename(), bytes: await readFile(await saved.path()) };
}

test("WEB-roundtrip.text-only with no plaintext, passphrase, or third party on the wire", async ({ page, context, server, requests, sent }) => {
    const { link, pin } = await createMessage(page, server, { text: SECRET_TEXT });
    // EFF words may contain a hyphen themselves (t-shirt), so only the shape is checked.
    expect(link).toMatch(new RegExp(`^${server.url}/message/[a-z-]+-\\d+#[a-z]+(-[a-z]+){3,}$`));
    const reader = await context.newPage();
    await openMessage(reader, link, pin);
    await expect(reader.locator("#message-output")).toHaveText(SECRET_TEXT);
    await expect(reader.locator("#file-output")).toBeHidden();

    // INV-1.browser-request-capture, INV-2.browser-requests, INV-11.request-origins
    expectNoLeaks({ requests, wire: sent() }, server, [SECRET_TEXT, "pinveil-plaintext-sentinel", fragmentOf(link)]);
    const [create, retrieve] = apiRequests(sent());
    expect(create.method).toBe("POST");
    expect(retrieve.method).toBe("GET");
    expect(retrieve.headers["x-message-pin"]).toBe(pin);
});

test("WEB-roundtrip.files-only with empty, duplicate, and Unicode names, per-file and zip downloads", async ({
    page,
    context,
    server,
    requests,
    sent,
}) => {
    const files = [
        file("ünïcødé 🗝 report.txt", "first file\n", "application/x-pinveil-mime-sentinel"),
        file("empty.bin", Buffer.alloc(0), "application/octet-stream"),
        file("dup.txt", "one"),
        file("dup.txt", "two"),
        file("CON.txt", "device name"),
    ];
    const { link, pin } = await createMessage(page, server, { files });
    const reader = await context.newPage();
    await openMessage(reader, link, pin);
    await expect(reader.locator("#download-list li")).toHaveCount(files.length);
    await expect(reader.locator("#text-output")).toBeHidden();

    // Original names are shown as text; downloads use the export-name policy.
    expect(await downloadList(reader)).toEqual([
        { name: "ünïcødé 🗝 report.txt", exportName: "ünïcødé 🗝 report.txt" },
        { name: "empty.bin", exportName: "empty.bin" },
        { name: "dup.txt", exportName: "dup.txt" },
        { name: "dup.txt", exportName: "dup (2).txt" },
        { name: "CON.txt", exportName: "_CON.txt" },
    ]);
    const buttons = reader.locator("#download-list button");
    for (const [i, expected] of [[0, "first file\n"], [1, ""], [3, "two"]]) {
        const saved = await download(reader, buttons.nth(i));
        expect(saved.bytes.toString()).toBe(expected);
    }

    // WEB-zip: "Download all" is read back by Python's zipfile.
    const zip = await download(reader, reader.locator("#download-all-button"));
    expect(zip.name).toBe("pinveil-files.zip");
    // Windows runners install Python as `python`.
    const listing = spawnSync(
        process.platform === "win32" ? "python" : "python3",
        [
            "-I",
            "-c",
            "import sys,zipfile,json,io\nz=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read()))\nassert z.testzip() is None\nprint(json.dumps([[i.filename,z.read(i).decode()] for i in z.infolist()]))",
        ],
        { input: zip.bytes, encoding: "utf8" },
    );
    expect(listing.status, listing.stderr).toBe(0);
    expect(JSON.parse(listing.stdout)).toEqual([
        ["ünïcødé 🗝 report.txt", "first file\n"],
        ["empty.bin", ""],
        ["dup.txt", "one"],
        ["dup (2).txt", "two"],
        ["_CON.txt", "device name"],
    ]);

    // Names, MIME types, and contents never reach the server, the multipart filename included.
    expectNoLeaks({ requests, wire: sent() }, server, ["ünïcødé", "report.txt", "dup.txt", "application/x-pinveil-mime-sentinel", "first file", "device name", fragmentOf(link)]);
    const [create] = apiRequests(sent());
    expect(create.body.toString("latin1")).toContain('filename="blob"');
});

test("WEB-roundtrip.text-and-files together; a single file has no zip button", async ({ page, context, server }) => {
    const { link, pin } = await createMessage(page, server, {
        text: "see attached",
        files: [file("key.pem", "-----BEGIN KEY-----")],
    });
    const reader = await context.newPage();
    await openMessage(reader, link, pin);
    await expect(reader.locator("#message-output")).toHaveText("see attached");
    await expect(reader.locator("#download-list li")).toHaveCount(1);
    await expect(reader.locator("#download-all-button")).toBeHidden();
});

test("WEB-create.generated-pin is six digits, regenerates, and opens the message", async ({ page, context, server }) => {
    await page.goto(`${server.url}/`);
    const first = await page.locator("#pin").inputValue();
    expect(first).toMatch(/^\d{6}$/);
    let next = first;
    for (let i = 0; i < 5 && next === first; i += 1) {
        await page.locator("#new-pin-button").click();
        next = await page.locator("#pin").inputValue();
    }
    expect(next).toMatch(/^\d{6}$/);
    expect(next).not.toBe(first);

    const { link, pin } = await createMessage(page, server, { text: "generated" });
    const reader = await context.newPage();
    await openMessage(reader, link, pin);
    await expect(reader.locator("#message-output")).toHaveText("generated");
});

test("WEB-create.pin-override is used and an invalid one is refused before upload", async ({ page, context, server, requests, sent }) => {
    await page.goto(`${server.url}/`);
    await page.locator("#message-text").fill("x");
    await page.locator("#pin").fill("12a456");
    await page.locator("#submit-button").click();
    await expect(page.locator("#create-status")).toHaveText("The PIN must be exactly six digits.");
    expect(apiRequests(sent())).toHaveLength(0);

    const { link, pin } = await createMessage(page, server, { text: "chosen", pin: "007007" });
    expect(pin).toBe("007007");
    const reader = await context.newPage();
    await openMessage(reader, link, "007007");
    await expect(reader.locator("#message-output")).toHaveText("chosen");
});

test("WEB-create.copy-controls, the two-channel warning, and the store notice on the create page", async ({ page, server }) => {
    await page.goto(`${server.url}/`);
    await expect(page.locator("#store-notice")).toContainText("memory only");
    await createMessage(page, server, { text: "copy me" });
    await expect(page.locator("#result-ready")).toContainText("Send the link and the PIN through different channels");
    // The store notice is in "How it works", which stays reachable next to the result.
    await page.locator("footer [data-open-dialog]").click();
    await expect(page.locator("#store-notice")).toBeVisible();
    await expect(page.locator("#store-notice")).toContainText("restart deletes every message");
    await page.keyboard.press("Escape");
    await expect(page.locator("#about-dialog")).toBeHidden();
    await expect(page.locator("#expiry-label")).toContainText("Expires");
    // The whole link is shown, not scrolled out of a one-line field.
    expect(await page.locator("#share-url").evaluate((el) => el.scrollHeight <= el.clientHeight + 1)).toBe(true);
    for (const id of ["#copy-url-button", "#copy-pin-button"]) {
        await page.locator(id).click();
        // Either copied, or (without clipboard permission) the field is selected for a manual copy.
        await expect(page.locator(id)).toHaveText(/Copied|Selected/);
    }
});

test("ACC-wrong-passphrase-consumes: a wrong fragment fails locally and the message is gone", async ({ page, context, server }) => {
    const { link, pin } = await createMessage(page, server, { text: "only once" });
    const wrong = `${link.split("#")[0]}#wrong-words-in-fragment`;
    const reader = await context.newPage();
    await openMessage(reader, wrong, pin);
    await expect(reader.locator("#retrieve-status")).toContainText("Cannot open this message");
    await expect(reader.locator("#retrieve-status")).toContainText("already deleted");
    await expect(reader.locator("#message-output")).toBeHidden();

    const again = await context.newPage();
    await openMessage(again, link, pin);
    await expect(again.locator("#retrieve-status")).toContainText("doesn't exist");
});

test("WEB-retrieve.warning-before-opening and a wrong PIN can be retried", async ({ page, context, server }) => {
    const { link, pin } = await createMessage(page, server, { text: "retry" });
    const reader = await context.newPage();
    await reader.goto(link);
    await expect(reader.locator("#open-warning")).toContainText("Opening deletes it from the server, even if it can't be read");
    await expect(reader.locator("#open-warning")).toContainText("Keep this page open");
    // The longer explanation, including a dropped connection, is in "How it works".
    await reader.locator("footer [data-open-dialog]").click();
    await expect(reader.locator("#about-dialog")).toContainText("connection drops");
    await reader.keyboard.press("Escape");
    const wrongPin = pin === "000000" ? "111111" : "000000";
    await reader.locator("#retrieve-pin").fill(wrongPin);
    await reader.locator("#retrieve-button").click();
    await expect(reader.locator("#retrieve-status")).toContainText("Wrong PIN");
    await reader.locator("#retrieve-pin").fill(pin);
    await reader.locator("#retrieve-button").click();
    await expect(reader.locator("#message-output")).toHaveText("retry");
});

test("WEB-theme cycles System, Light, and Dark, and the choice survives a reload", async ({ page, server }) => {
    await page.goto(`${server.url}/`);
    const button = page.locator("#theme-button");
    const theme = () => page.evaluate(() => document.documentElement.dataset.theme ?? "system");
    expect(await theme()).toBe("system");
    await expect(button).toHaveAttribute("aria-label", "Theme: System. Switch to Light.");
    await button.click();
    expect(await theme()).toBe("light");
    await button.click();
    expect(await theme()).toBe("dark");
    await expect(button).toHaveAttribute("aria-label", "Theme: Dark. Switch to System.");
    await page.reload();
    expect(await theme()).toBe("dark");
    await page.locator("#theme-button").click();
    expect(await theme()).toBe("system");
});

test("WEB-footer shows the server version and the source link on both pages", async ({ page, server }) => {
    await page.goto(`${server.url}/`);
    await expect(page.locator("footer .version")).toHaveText(/^v\d+\.\d+\.\d+/);
    await expect(page.locator('a[href="https://github.com/alexey-lapin/pinveil"]')).toHaveAttribute("rel", "noopener noreferrer");
    await page.goto(`${server.url}/message/any-id#words`);
    await expect(page.locator("footer .version")).toHaveText(/^v\d+\.\d+\.\d+/);
});
