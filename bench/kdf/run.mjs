import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash, webcrypto } from "node:crypto";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { cpus, arch, platform } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { measureKdf, calibrateCpu } from "./webcrypto.mjs";
import { systemFirefox } from "./firefox-bidi.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const build = join(here, ".build");
const { values } = parseArgs({ options: {
    runs: { type: "string", default: "7" },
    output: { type: "string", default: join(build, "results.json") },
    firefox: { type: "string" },
} });
const runs = Number(values.runs);
assert(Number.isSafeInteger(runs) && runs >= 5 && runs <= 1001 && runs % 2 === 1, "--runs must be odd, between 5 and 1001");
await mkdir(build, { recursive: true });

function command(program, args, options = {}) {
    const result = spawnSync(program, args, { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024, ...options });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`${program} failed (${result.status}): ${result.stderr}\n${result.stdout}`);
    return result.stdout.trim();
}

function extractFunction(source, marker) {
    const start = source.indexOf(marker);
    assert(start >= 0, `Missing production helper ${marker}`);
    const end = source.indexOf("\n}", start);
    assert(end > start, `Missing helper end ${marker}`);
    return source.slice(start, end + 2);
}
const jsSource = await readFile(join(root, "web/static/crypto.js"), "utf8");
const rustSource = await readFile(join(root, "crates/pinveil-core/src/crypto.rs"), "utf8");
const jsHelper = extractFunction(jsSource, "async function deriveKek(");
const rustHelper = extractFunction(rustSource, "fn derive_kek(");
const { KDF_ITERATIONS_MIN, KDF_ITERATIONS_MAX } = await import("../../web/static/protocol.js");
const endpoints = [KDF_ITERATIONS_MIN, KDF_ITERATIONS_MAX];
const report = {
    measuredAt: new Date().toISOString(),
    sourceRevision: command("git", ["rev-parse", "HEAD"]),
    machine: { cpu: cpus()[0]?.model, logicalCpus: cpus().length, arch: arch(), platform: platform(),
        os: platform() === "darwin" ? command("sw_vers", []) : command("uname", ["-a"]) },
    node: process.version,
    rustc: command("rustc", ["--version"]),
    runs, warmupsPerEndpoint: 1, endpoints,
    salts: "16 bytes: bytes 0..11 contain 0..11; bytes 12..15 are a big-endian sample index, warmup 0, timed 1..runs",
    sourceHashes: Object.fromEntries([["js", jsHelper], ["rust", rustHelper]].map(([name, source]) =>
        [name, createHash("sha256").update(source).digest("hex")])),
    environments: [],
};
const persist = () => writeFile(resolve(values.output), `${JSON.stringify(report, null, 2)}\n`);
function record(environment) {
    report.environments.push(environment);
    console.log(environment.name, environment.results?.map(({ iterations, medianMs }) => `${iterations}: ${medianMs.toFixed(2)} ms`).join(", ") ?? environment.error);
}

const deriveKek = new Function("encoder", "crypto", `return (${jsHelper});`)(new TextEncoder(), webcrypto);
const nodeResults = [];
for (const iterations of endpoints) {
    const result = await measureKdf(deriveKek, iterations, runs);
    // Independent raw-byte oracle, outside timing, also checks the Rust KEK.
    const material = await webcrypto.subtle.importKey("raw", new TextEncoder().encode("alpha-bravo-charlie-delta123456"), "PBKDF2", false, ["deriveBits"]);
    const bytes = await webcrypto.subtle.deriveBits({ name: "PBKDF2", hash: "SHA-256", iterations,
        salt: Buffer.from(result.saltHex, "hex") }, material, 256);
    result.keyHex = Buffer.from(bytes).toString("hex");
    const oracleKek = await webcrypto.subtle.importKey("raw", bytes, "AES-KW", false, ["wrapKey"]);
    const contentKey = await webcrypto.subtle.importKey("raw", Uint8Array.from({ length: 32 }, (_, i) => i + 32),
        "AES-GCM", true, ["encrypt"]);
    const oracleWrapped = await webcrypto.subtle.wrapKey("raw", contentKey, oracleKek, "AES-KW");
    assert.equal(result.wrappedKeyHex, Buffer.from(oracleWrapped).toString("hex"), "JS helper differs from the raw-byte oracle");
    nodeResults.push(result);
}
record({ name: "Node Web Crypto", version: process.version, results: nodeResults });
await persist();

console.log("Building the locked pinveil-core release dependencies...");
const cargoOutput = command("cargo", ["build", "--release", "--locked", "--offline", "-p", "pinveil-core", "--message-format=json"]);
const artifacts = cargoOutput.split("\n").filter(Boolean).map((line) => JSON.parse(line))
    .filter((item) => item.reason === "compiler-artifact");
const libraries = ["pinveil_core", "pbkdf2", "sha2", "zeroize"].map((name) => {
    const artifact = artifacts.find((item) => item.target.name === name && item.filenames.some((file) => file.endsWith(".rlib")));
    assert(artifact, `Cargo did not report ${name} rlib`);
    return [name, artifact.filenames.find((file) => file.endsWith(".rlib"))];
});
const helperFile = join(build, "derive-kek.rs");
const rustBinary = join(build, "derive-kdf");
await writeFile(helperFile, `${rustHelper}\n`);
command("rustc", [join(here, "derive.rs"), "--edition=2024", "-C", "opt-level=3", "-o", rustBinary,
    ...[...new Set(libraries.map(([, path]) => dirname(path)))].flatMap((path) => ["-L", `dependency=${path}`]),
    ...libraries.flatMap(([name, path]) => ["--extern", `${name}=${path}`])],
{ env: { ...process.env, PINVEIL_KDF_SOURCE: helperFile } });
const rustResults = command(rustBinary, [String(runs)]).split("\n").map((line) => JSON.parse(line));
assert.deepEqual(rustResults.map((r) => r.iterations), endpoints, "Rust and JS KDF constants differ");
for (const result of rustResults) {
    const reference = nodeResults.find((r) => r.iterations === result.iterations);
    assert.equal(result.saltHex, reference.saltHex);
    assert.equal(result.keyHex, reference.keyHex);
}
record({ name: "Rust release CLI derivation", version: report.rustc, results: rustResults });
await persist();

const require = createRequire(join(root, "tests/browser/package.json"));
const { chromium, webkit, firefox } = require("playwright");
report.playwright = require("playwright/package.json").version;
const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" });
    response.end("<!doctype html><html lang=en><title>Pinveil KDF benchmark</title><p>Local benchmark</p></html>");
});
await new Promise((resolve, reject) => { server.once("error", reject); server.listen(0, "127.0.0.1", resolve); });
const url = `http://127.0.0.1:${server.address().port}/`;
const evaluateSource = `async () => {
    const derive = (${jsHelper});
    const encoder = new TextEncoder();
    const measure = (${measureKdf.toString()});
    const results = [];
    for (const iterations of ${JSON.stringify(endpoints)}) results.push(await measure(derive, iterations, ${runs}));
    return results;
}`;
function validateResults(results) {
    assert.equal(results.length, endpoints.length);
    for (const result of results) {
        assert.equal(result.saltHex, nodeResults.find((r) => r.iterations === result.iterations).saltHex);
        assert.equal(result.wrappedKeyHex, nodeResults.find((r) => r.iterations === result.iterations).wrappedKeyHex,
            "Browser and Node derived different KEKs");
    }
}
try {
    for (const [name, engine] of [["Chromium", chromium], ["WebKit", webkit], ["Firefox", firefox]]) {
        let browser;
        try {
            browser = await engine.launch({ headless: true, timeout: 15_000 });
            const page = await browser.newPage();
            await page.goto(url);
            const calibration = name === "Chromium" ? await page.evaluate(calibrateCpu, runs) : undefined;
            const results = await page.evaluate(`(${evaluateSource})()`);
            validateResults(results);
            record({ name, version: browser.version(), results, ...(calibration && { jsCalibration: calibration }) });
            if (name === "Chromium") {
                const cdp = await page.context().newCDPSession(page);
                await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });
                const throttledCalibration = await page.evaluate(calibrateCpu, runs);
                const throttled = await page.evaluate(`(${evaluateSource})()`);
                validateResults(throttled);
                record({ name: "Chromium CDP 4x", version: browser.version(), throttleRate: 4,
                    jsCalibration: throttledCalibration, results: throttled });
            }
        } catch (error) {
            if (name === "Firefox") {
                const executable = values.firefox ?? (platform() === "darwin" ? "/Applications/Firefox.app/Contents/MacOS/firefox" : "firefox");
                try {
                    const fallback = await systemFirefox(executable, url, { helperSource: jsHelper, endpoints, runs });
                    validateResults(fallback.results);
                    record({ name: "Firefox (system, BiDi)", version: fallback.version,
                        bundledLaunchError: error.message.includes("Could not find profile folder")
                            ? "Could not find profile folder" : error.message.split("\n")[0],
                        roundTrip: fallback.roundTrip, results: fallback.results });
                } catch (fallbackError) {
                    const details = `${error.message}\nSystem Firefox: ${fallbackError.message}`;
                    await writeFile(join(build, "firefox-errors.log"), details);
                    const bundled = error.message.includes("Could not find profile folder")
                        ? "Bundled Firefox could not find its profile folder."
                        : error.message.split("\n")[0];
                    const system = fallbackError.message.includes("xattr:")
                        ? "System Firefox startup timed out; its startup logged xattr Operation not permitted errors."
                        : fallbackError.message.slice(0, 250);
                    record({ name, error: `${bundled} ${system}`, diagnosticLog: "bench/kdf/.build/firefox-errors.log" });
                }
            } else {
                record({ name, error: error.message });
            }
        } finally {
            await browser?.close();
            await persist();
        }
    }
} finally {
    await new Promise((resolve) => server.close(resolve));
}
console.log(`Raw samples: ${resolve(values.output)}`);
if (report.environments.some((environment) => environment.error)) process.exitCode = 1;
