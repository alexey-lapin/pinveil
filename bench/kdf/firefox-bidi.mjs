import { spawn } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// Stock Firefox has WebDriver BiDi, but not Playwright's patched Juggler transport.
// https://developer.mozilla.org/en-US/docs/Web/WebDriver/How_to/Create_BiDi_connection
export async function systemFirefox(executable, url, { helperSource, endpoints, runs }) {
    const profile = await mkdtemp(fileURLToPath(new URL(".build/firefox-profile-", import.meta.url)));
    await writeFile(join(profile, "user.js"), [
        'user_pref("browser.shell.checkDefaultBrowser", false);',
        'user_pref("browser.aboutwelcome.enabled", false);',
        'user_pref("datareporting.policy.dataSubmissionEnabled", false);',
        'user_pref("toolkit.telemetry.enabled", false);',
        'user_pref("app.update.auto", false);',
    ].join("\n"));
    const child = spawn(executable, ["--headless", "--no-remote", "-profile", profile, "--remote-debugging-port", "0"], {
        stdio: ["ignore", "pipe", "pipe"],
    });
    let socket;
    let log = "";
    const pending = new Map();
    try {
        const endpoint = await new Promise((resolve, reject) => {
            const timeout = setTimeout(() => reject(new Error(`Firefox startup timeout: ${log.slice(-1500)}`)), 15_000);
            const finish = (error, value) => {
                clearTimeout(timeout);
                error ? reject(error) : resolve(value);
            };
            const output = (chunk) => {
                log += chunk.toString();
                const match = /WebDriver BiDi listening on (ws:\/\/127\.0\.0\.1:\d+)/.exec(log);
                if (match) finish(null, `${match[1]}/session`);
            };
            child.stdout.on("data", output);
            child.stderr.on("data", output);
            child.once("error", (error) => finish(error));
            child.once("exit", (code) => finish(new Error(`Firefox exited ${code}: ${log.slice(-1500)}`)));
        });
        socket = new WebSocket(endpoint);
        socket.addEventListener("message", (event) => {
            const message = JSON.parse(event.data);
            const receiver = pending.get(message.id);
            if (!receiver) return;
            pending.delete(message.id);
            clearTimeout(receiver.timeout);
            message.type === "error" ? receiver.reject(new Error(JSON.stringify(message))) : receiver.resolve(message.result);
        });
        await new Promise((resolve, reject) => {
            socket.addEventListener("open", resolve, { once: true });
            socket.addEventListener("error", reject, { once: true });
        });
        let id = 0;
        const send = (method, params) => new Promise((resolve, reject) => {
            const requestId = ++id;
            const timeout = setTimeout(() => {
                pending.delete(requestId);
                reject(new Error(`BiDi ${method} timed out`));
            }, 60_000);
            pending.set(requestId, { resolve, reject, timeout });
            socket.send(JSON.stringify({ id: requestId, method, params }));
        });
        const session = await send("session.new", { capabilities: {} });
        const { context } = await send("browsingContext.create", { type: "tab" });
        await send("browsingContext.navigate", { context, url, wait: "complete" });
        const evaluate = async (expression) => {
            const evaluation = await send("script.evaluate", { expression, target: { context }, awaitPromise: true });
            if (evaluation.type === "exception") throw new Error(JSON.stringify(evaluation.exceptionDetails));
            return evaluation.result;
        };
        await evaluate(`globalThis.pinveilKdfDerive = (() => {
            const encoder = new TextEncoder(); return (${helperSource});
        })(); null`);
        // Time each awaited derivation from Node and disclose the included BiDi round trip.
        const roundTripSamplesMs = [];
        for (let i = 0; i < runs; i++) {
            const start = performance.now();
            await evaluate("null");
            roundTripSamplesMs.push(performance.now() - start);
        }
        const results = [];
        for (const iterations of endpoints) {
            const deriveExpression = (sample) => `(() => {
                const salt = Uint8Array.from({ length: 16 }, (_, i) => i);
                new DataView(salt.buffer).setUint32(12, ${sample}, false);
                globalThis.pinveilKdfSalt = salt;
                return globalThis.pinveilKdfDerive("alpha-bravo-charlie-delta", "123456", salt, ${iterations})
                    .then(key => { globalThis.pinveilKdfKey = key; return null; });
            })()`;
            await evaluate(deriveExpression(0));
            const samplesMs = [];
            for (let i = 0; i < runs; i++) {
                const start = performance.now();
                await evaluate(deriveExpression(i + 1));
                samplesMs.push(performance.now() - start);
            }
            const wrapped = await evaluate(`(async () => {
                const content = await crypto.subtle.importKey("raw",
                    Uint8Array.from({ length: 32 }, (_, i) => i + 32), "AES-GCM", true, ["encrypt"]);
                const wrapped = new Uint8Array(await crypto.subtle.wrapKey("raw", content, globalThis.pinveilKdfKey, "AES-KW"));
                const hex = bytes => Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
                return JSON.stringify({ wrappedKeyHex: hex(wrapped), saltHex: hex(globalThis.pinveilKdfSalt) });
            })()`);
            results.push({ iterations, samplesMs,
                medianMs: [...samplesMs].sort((a, b) => a - b)[Math.floor(runs / 2)],
                clock: "Node performance.now around BiDi script.evaluate; includes transport overhead",
                ...JSON.parse(wrapped.value) });
        }
        return { version: session.capabilities.browserVersion, results,
            roundTrip: { samplesMs: roundTripSamplesMs,
                medianMs: [...roundTripSamplesMs].sort((a, b) => a - b)[Math.floor(runs / 2)] } };
    } finally {
        for (const receiver of pending.values()) clearTimeout(receiver.timeout);
        socket?.close();
        const exited = new Promise((resolve) => child.once("exit", resolve));
        child.kill("SIGTERM");
        if (child.exitCode === null && child.signalCode === null) {
            let killTimer;
            await Promise.race([exited, new Promise((resolve) => {
                killTimer = setTimeout(() => { child.kill("SIGKILL"); resolve(); }, 2000);
            })]);
            clearTimeout(killTimer);
        }
        child.stdout.destroy();
        child.stderr.destroy();
        child.unref();
        await rm(profile, { recursive: true, force: true });
    }
}
