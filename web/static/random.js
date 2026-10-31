/**
 * Passphrase and PIN generation. The rejection-sampling algorithm matches `pinveil-core`'s
 * `uniform_below`: big-endian u32 draws, discarding draws at or above the largest multiple of
 * the bound, so there is no modulo bias. `fill` defaults to the browser CSPRNG; tests inject
 * fixed bytes. A failing `fill` throws, which aborts creation.
 */

export const PASSPHRASE_WORDS = 4;
const PIN_SPACE = 1_000_000;

function systemFill(bytes) {
    crypto.getRandomValues(bytes);
}

/** A uniformly distributed integer in `0..n`. */
export function uniformBelow(n, fill = systemFill) {
    if (!Number.isInteger(n) || n <= 0 || n > 2 ** 32) {
        throw new RangeError("uniformBelow needs a bound between 1 and 2^32");
    }
    const limit = 2 ** 32 - (2 ** 32 % n);
    const bytes = new Uint8Array(4);
    for (;;) {
        fill(bytes);
        const value = new DataView(bytes.buffer).getUint32(0, false);
        if (value < limit) {
            return value % n;
        }
    }
}

/** Four random words joined with `-`, from a list validated by `parseWordList`. */
export function generatePassphrase(words, fill = systemFill) {
    return Array.from({ length: PASSPHRASE_WORDS }, () => words[uniformBelow(words.length, fill)]).join("-");
}

/** A PIN, uniform over 000000 to 999999. */
export function generatePin(fill = systemFill) {
    return String(uniformBelow(PIN_SPACE, fill)).padStart(6, "0");
}

/** Whether `pin` is exactly six ASCII digits. */
export function isValidPin(pin) {
    return /^[0-9]{6}$/.test(pin);
}
