/**
 * Sealing and opening blobs through Web Crypto. Mirrors `pinveil-core/src/crypto.rs`.
 *
 * PBKDF2-HMAC-SHA-256 over `passphrase + pin` derives a key-encryption key, AES-KW wraps a
 * fresh content key, and AES-256-GCM encrypts the payload with the wrapped key as associated
 * data. Randomness is drawn in a fixed order (salt, IV, content key) through `fill`, which
 * defaults to the browser CSPRNG; tests inject fixed bytes to reproduce the shared vectors.
 */

import { decodeOuter, encodeOuter, isAcceptedKdfIterations } from "./envelope.js";
import { decodePayload } from "./payload.js";
import { CONTENT_KEY_LEN, IV_LEN, KDF_SALT_LEN, PinveilError } from "./protocol.js";
import { isValidPin } from "./random.js";

const encoder = new TextEncoder();

function systemFill(bytes) {
    crypto.getRandomValues(bytes);
}

/** Encrypts encoded payload bytes into a blob. */
export async function sealPayload(payloadBytes, passphrase, pin, kdfIterations, fill = systemFill) {
    if (!isValidPin(pin)) {
        throw new PinveilError("create", "invalid-pin", "The PIN must be exactly six digits.");
    }
    if (!isAcceptedKdfIterations(kdfIterations)) {
        throw new PinveilError("create", "kdf-iterations-out-of-range");
    }
    const kdfSalt = new Uint8Array(KDF_SALT_LEN);
    const iv = new Uint8Array(IV_LEN);
    const contentKeyBytes = new Uint8Array(CONTENT_KEY_LEN);
    fill(kdfSalt);
    fill(iv);
    fill(contentKeyBytes);
    try {
        const kek = await deriveKek(passphrase, pin, kdfSalt, kdfIterations);
        const contentKey = await crypto.subtle.importKey("raw", contentKeyBytes, "AES-GCM", true, ["encrypt"]);
        const wrappedContentKey = new Uint8Array(await crypto.subtle.wrapKey("raw", contentKey, kek, "AES-KW"));
        const ciphertext = new Uint8Array(
            await crypto.subtle.encrypt(
                { name: "AES-GCM", iv, additionalData: wrappedContentKey, tagLength: 128 },
                contentKey,
                payloadBytes,
            ),
        );
        return encodeOuter({ wrappedContentKey, kdfSalt, iv, kdfIterations }, ciphertext);
    } finally {
        contentKeyBytes.fill(0);
    }
}

/**
 * Validates a blob's header (stage one, no crypto), then decrypts it. Returns the inner
 * payload bytes; `openPayload` also runs stage two.
 */
export async function openBlob(blob, passphrase, pin) {
    const { header, ciphertext } = decodeOuter(blob);
    if (!isValidPin(pin)) {
        throw new PinveilError("input", "invalid-pin", "The PIN must be exactly six digits.");
    }
    const kek = await deriveKek(passphrase, pin, header.kdfSalt, header.kdfIterations);
    let contentKey;
    try {
        contentKey = await crypto.subtle.unwrapKey(
            "raw",
            header.wrappedContentKey,
            kek,
            "AES-KW",
            { name: "AES-GCM", length: 256 },
            false,
            ["decrypt"],
        );
    } catch {
        throw decryptError();
    }
    try {
        return new Uint8Array(
            await crypto.subtle.decrypt(
                { name: "AES-GCM", iv: header.iv, additionalData: header.wrappedContentKey, tagLength: 128 },
                contentKey,
                ciphertext,
            ),
        );
    } catch {
        throw decryptError();
    }
}

/** `openBlob` followed by `decodePayload`: both validation stages. */
export async function openPayload(blob, passphrase, pin) {
    return decodePayload(await openBlob(blob, passphrase, pin));
}

function decryptError() {
    return new PinveilError("decrypt", "failed", "Unable to decrypt this message. The PIN or passphrase may be wrong.");
}

async function deriveKek(passphrase, pin, salt, iterations) {
    const input = encoder.encode(`${passphrase}${pin}`);
    try {
        const baseKey = await crypto.subtle.importKey("raw", input, "PBKDF2", false, ["deriveKey"]);
        return await crypto.subtle.deriveKey(
            { name: "PBKDF2", hash: "SHA-256", salt, iterations },
            baseKey,
            { name: "AES-KW", length: 256 },
            false,
            ["wrapKey", "unwrapKey"],
        );
    } finally {
        input.fill(0);
    }
}
