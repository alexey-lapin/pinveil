// WEB-roundtrip with the real CLI: a link the browser made opens in the CLI, and a link the
// CLI made opens in the browser. The CLI runs on a pseudo-terminal through the test-only
// `tty-run` helper (crates/pty-probe), which works on every target, Windows included.
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createMessage, expect, file, openMessage, test } from "./harness.js";

const CLI = process.env.PINVEIL_CLI_BIN;
const TTY_RUN = process.env.PINVEIL_TTY_RUN_BIN;

test.skip(!CLI || !TTY_RUN, "set PINVEIL_CLI_BIN and PINVEIL_TTY_RUN_BIN to run CLI round trips");
// One engine is enough for the CLI half; the browser half is covered in every engine elsewhere.
test.skip(({ browserName }) => browserName !== "chromium", "CLI round trips run in Chromium only");

let scratch;
test.beforeEach(() => {
    scratch = mkdtempSync(join(tmpdir(), "pinveil-cli-"));
});
test.afterEach(() => rmSync(scratch, { recursive: true, force: true }));

/**
 * Runs the CLI on a pty. `answers` are [prompt, reply] pairs. Asynchronous on purpose: the
 * recording proxy runs in this Node process, so blocking here would stall the CLI's requests.
 */
async function tty(args, { stdin, stdout, answers = [] } = {}) {
    const options = [];
    if (stdin) options.push("--stdin", stdin);
    if (stdout) options.push("--stdout", stdout);
    for (const [prompt, reply] of answers) options.push("--answer", prompt, reply);
    const child = spawn(TTY_RUN, [...options, "--", CLI, ...args], {
        env: { ...process.env, PINVEIL_CONFIG_DIR: join(scratch, "config") },
        cwd: scratch,
    });
    let out = "";
    let err = "";
    child.stdout.on("data", (chunk) => {
        out += chunk;
    });
    child.stderr.on("data", (chunk) => {
        err += chunk;
    });
    const status = await new Promise((resolve) => child.on("close", resolve));
    expect(status, err).toBe(0);
    return JSON.parse(out);
}

test("WEB-roundtrip.browser-to-cli text and files open in pinveil get", async ({ page, server }) => {
    const { link, pin } = await createMessage(page, server, {
        // A textarea normalizes CRLF to LF, so the browser side sends LF.
        text: "from the browser\nünïcødé",
        files: [file("a.txt", "file a"), file("ünï 🗝.bin", Buffer.from([0, 1, 2, 255]))],
    });
    const out = join(scratch, "out");
    const text = join(scratch, "text.txt");
    const run = await tty(["get", "--insecure-http", "--out", out], {
        stdout: text,
        answers: [
            ["Link:", link],
            ["PIN:", pin],
        ],
    });
    expect(run.exit, run.plain).toBe(0);
    expect(readFileSync(text, "utf8")).toBe("from the browser\nünïcødé");
    expect(readFileSync(join(out, "a.txt"), "utf8")).toBe("file a");
    expect([...readFileSync(join(out, "ünï 🗝.bin"))]).toEqual([0, 1, 2, 255]);
});

test("WEB-roundtrip.cli-to-browser a pinveil send link opens in the browser", async ({ page, server }) => {
    const input = join(scratch, "in.txt");
    writeFileSync(input, "from the cli\n");
    const attachment = join(scratch, "notes ünï.txt");
    writeFileSync(attachment, "attached");
    const linkFile = join(scratch, "link.txt");
    const run = await tty(["send", "--server", server.url, "--insecure-http", attachment], { stdin: input, stdout: linkFile });
    expect(run.exit, run.plain).toBe(0);
    // Windows conpty may render the space as a cursor move, hence \s*.
    const pin = run.plain.match(/PIN:\s*(\d{6})/)[1];
    const link = readFileSync(linkFile, "utf8").trim();

    await openMessage(page, link, pin);
    await expect(page.locator("#message-output")).toHaveText("from the cli\n");
    await expect(page.locator("#download-list .file-name")).toHaveText(["notes ünï.txt"]);
    await expect(page.locator("#download-list .file-name")).toHaveAttribute("title", "text/plain");
});
