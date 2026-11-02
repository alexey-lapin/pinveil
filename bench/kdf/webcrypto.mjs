// These functions also run in browser pages. Keep them independent of module bindings.
export async function measureKdf(deriveKek, iterations, runs) {
    const saltFor = (sample) => {
        const salt = Uint8Array.from({ length: 16 }, (_, i) => i);
        new DataView(salt.buffer).setUint32(12, sample, false);
        return salt;
    };
    const derive = (salt) => deriveKek("alpha-bravo-charlie-delta", "123456", salt, iterations);
    await derive(saltFor(0)); // One untimed warm-up with a different salt.
    const highResolutionSamplesMs = [];
    const wallClockSamplesMs = [];
    let key;
    let salt;
    for (let i = 0; i < runs; i++) {
        salt = saltFor(i + 1); // Avoid NSS's cache for repeated PBKDF2 inputs.
        const wallStart = Date.now();
        const start = performance.now();
        key = await derive(salt);
        highResolutionSamplesMs.push(performance.now() - start);
        wallClockSamplesMs.push(Date.now() - wallStart);
    }
    // Retain both clocks to detect timer precision problems.
    const useWallClock = highResolutionSamplesMs.every((sample) => sample === 0);
    const samplesMs = useWallClock ? wallClockSamplesMs : highResolutionSamplesMs;
    if (samplesMs.every((sample) => sample === 0)) throw new Error("Both browser clocks returned only zero durations");
    // Check the non-extractable KEK outside the timed region by wrapping a fixed content key.
    const contentKey = await crypto.subtle.importKey(
        "raw", Uint8Array.from({ length: 32 }, (_, i) => i + 32), "AES-GCM", true, ["encrypt"],
    );
    const wrapped = new Uint8Array(await crypto.subtle.wrapKey("raw", contentKey, key, "AES-KW"));
    return {
        iterations,
        samplesMs,
        clock: useWallClock ? "Date.now" : "performance.now",
        highResolutionSamplesMs,
        wallClockSamplesMs,
        saltHex: Array.from(salt, (byte) => byte.toString(16).padStart(2, "0")).join(""),
        medianMs: [...samplesMs].sort((a, b) => a - b)[Math.floor(runs / 2)],
        wrappedKeyHex: Array.from(wrapped, (byte) => byte.toString(16).padStart(2, "0")).join(""),
    };
}

export function calibrateCpu(runs) {
    const samplesMs = [];
    for (let run = -1; run < runs; run++) {
        let value = 1234567;
        const start = performance.now();
        for (let i = 0; i < 8_000_000; i++) value = Math.imul(value ^ i, 1664525) + 1013904223 | 0;
        const elapsed = performance.now() - start;
        globalThis.kdfCalibrationSink = value;
        if (run >= 0) samplesMs.push(elapsed);
    }
    return {
        samplesMs,
        medianMs: [...samplesMs].sort((a, b) => a - b)[Math.floor(runs / 2)],
    };
}
