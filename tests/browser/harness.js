// Starts a real pinveil-server for browser tests and drives the two pages.
import { spawn } from "node:child_process";
import { createServer, request as forward } from "node:http";
import { createInterface } from "node:readline";

import { test as base, expect } from "@playwright/test";

/** Settings every test server gets: a pepper and rate limits no test hits. */
const BASE_ENV = {
    PINVEIL_PIN__PEPPER: "browser-test-pepper",
    PINVEIL_RATE_LIMIT__CREATE_PER_WINDOW: "10000",
    PINVEIL_RATE_LIMIT__RETRIEVE_PER_WINDOW: "10000",
};

function serverBinary() {
    const bin = process.env.PINVEIL_SERVER_BIN;
    if (!bin) {
        throw new Error("Set PINVEIL_SERVER_BIN to the pinveil-server binary to test against.");
    }
    return bin;
}

/**
 * Starts a server with `env` on a random loopback port, behind a recording proxy. Resolves
 * to `{url, wire, stop}`: `url` is the proxy, and `wire` lists every request that reached it
 * as `{method, path, headers, body}` with the raw body bytes. Browsers don't expose multipart
 * bodies reliably (Chromium reports none), so leak checks read these instead.
 */
export async function startServer(env = {}) {
    const upstream = await startBinary(env);
    const wire = [];
    const proxy = createServer((req, res) => {
        const chunks = [];
        req.on("data", (chunk) => chunks.push(chunk));
        req.on("end", () => {
            const body = Buffer.concat(chunks);
            wire.push({ method: req.method, path: req.url, headers: req.headers, body });
            const out = forward(`${upstream.url}${req.url}`, { method: req.method, headers: req.headers }, (answer) => {
                res.writeHead(answer.statusCode, answer.headers);
                answer.pipe(res);
            });
            out.on("error", () => res.destroy());
            out.end(body);
        });
    });
    await new Promise((resolve) => proxy.listen(0, "127.0.0.1", resolve));
    return {
        url: `http://127.0.0.1:${proxy.address().port}`,
        wire,
        stop: () => {
            proxy.close();
            proxy.closeAllConnections();
            upstream.stop();
        },
    };
}

function startBinary(env) {
    const child = spawn(serverBinary(), ["--listen", "127.0.0.1:0"], {
        env: { ...process.env, ...BASE_ENV, ...env },
        stdio: ["ignore", "pipe", "pipe"],
    });
    let stderr = "";
    child.stderr.on("data", (chunk) => {
        stderr += chunk;
    });
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`server did not start: ${stderr}`)), 30_000);
        child.on("exit", (code) => reject(new Error(`server exited with ${code}: ${stderr}`)));
        createInterface({ input: child.stdout }).once("line", (line) => {
            clearTimeout(timer);
            const addr = line.trim().replace(/^listening on /, "");
            resolve({
                url: `http://${addr}`,
                stop: () => child.kill(),
            });
        });
    });
}

/**
 * `server` is one default server per worker. `requests` records the URL of every request the
 * browser makes, wherever it goes. `sent()` returns what reached `server` during this test.
 */
export const test = base.extend({
    server: [
        // Playwright requires an object pattern for the (unused) fixtures argument.
        async ({}, use) => {
            const server = await startServer();
            await use(server);
            server.stop();
        },
        { scope: "worker" },
    ],
    requests: async ({ context }, use) => {
        const seen = [];
        context.on("request", (request) => seen.push({ url: request.url(), method: request.method() }));
        await use(seen);
    },
    sent: async ({ server }, use) => {
        await use(recorder(server));
    },
});

export { expect };

/** A file for `setInputFiles`. */
export const file = (name, content, mimeType = "text/plain") => ({
    name,
    mimeType,
    buffer: Buffer.isBuffer(content) ? content : Buffer.from(content),
});

/**
 * Fills the create page and submits it. Resolves to `{link, pin}` once the link is shown.
 * `pin` overrides the generated PIN when given.
 */
export async function createMessage(page, server, { text = "", files = [], pin, ttl } = {}) {
    await page.goto(`${server.url}/`);
    if (text) {
        await page.locator("#message-text").fill(text);
    }
    if (files.length > 0) {
        await page.locator("#message-files").setInputFiles(files);
    }
    if (pin !== undefined) {
        await page.locator("#pin").fill(pin);
    }
    if (ttl !== undefined) {
        await page.locator("#ttl-preset").selectOption(ttl);
    }
    await page.locator("#submit-button").click();
    await expect(page.locator("#result-ready")).toBeVisible();
    return {
        link: await page.locator("#share-url").inputValue(),
        pin: await page.locator("#share-pin").inputValue(),
    };
}

/** Opens `link` and submits `pin`. */
export async function openMessage(page, link, pin) {
    await page.goto(link);
    await page.locator("#retrieve-pin").fill(pin);
    await page.locator("#retrieve-button").click();
}

/** The fragment of a share link. */
export const fragmentOf = (link) => new URL(link).hash.slice(1);

/**
 * Asserts that every request the browser made stayed on the server's origin (INV-11), and that
 * no request that reached the server carried any of `secrets` in its path, headers, or raw
 * body (INV-1, INV-2).
 */
export function expectNoLeaks({ requests, wire }, server, secrets) {
    const origin = new URL(server.url).origin;
    expect(requests.length).toBeGreaterThan(0);
    for (const request of requests) {
        expect(new URL(request.url).origin, request.url).toBe(origin);
    }
    expect(wire.some((r) => r.body.length > 0), "an upload was recorded").toBe(true);
    for (const request of wire) {
        const haystacks = [Buffer.from(request.path), Buffer.from(JSON.stringify(request.headers)), request.body];
        for (const secret of secrets) {
            for (const haystack of haystacks) {
                expect(haystack.includes(Buffer.from(secret)), `${request.method} ${request.path} carries ${JSON.stringify(secret)}`).toBe(false);
            }
        }
    }
}

/** A function returning the requests that reached `server` from now on. */
export function recorder(server) {
    const start = server.wire.length;
    return () => server.wire.slice(start);
}

/** Recorded requests to the message API, in order. */
export const apiRequests = (wire) => wire.filter((r) => r.path.startsWith("/api/messages"));
