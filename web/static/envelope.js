/**
 * The outer envelope: the opaque blob the server stores. Mirrors
 * `pinveil-core/src/envelope.rs`.
 *
 * `[u32 BE header length][canonical JSON header][AES-GCM ciphertext with tag]`
 *
 * Decoding is validation stage one: nothing here does cryptographic work. The header must be
 * byte-identical to its canonical form (fixed key order, no whitespace).
 */

import {
    ENCRYPTION_ALGORITHM,
    IV_LEN,
    KDF_ALGORITHM,
    KDF_ITERATIONS_MAX,
    KDF_ITERATIONS_MIN,
    KDF_SALT_LEN,
    MAX_OUTER_HEADER_BYTES,
    PROTOCOL_VERSION,
    PinveilError,
    TAG_LEN,
    WRAPPED_KEY_LEN,
    WRAPPING_ALGORITHM,
} from "./protocol.js";

const HEADER_KEYS = [
    "wrappedContentKey",
    "kdfSalt",
    "iv",
    "kdfAlgorithm",
    "kdfIterations",
    "encryptionAlgorithm",
    "wrappingAlgorithm",
    "schemaVersion",
];

const encoder = new TextEncoder();
const strictDecoder = new TextDecoder("utf-8", { fatal: true });

function headerError(reason) {
    return new PinveilError("header", reason);
}

/** Whether `iterations` is in the accepted KDF range. */
export function isAcceptedKdfIterations(iterations) {
    return Number.isInteger(iterations) && iterations >= KDF_ITERATIONS_MIN && iterations <= KDF_ITERATIONS_MAX;
}

function headerJson(header) {
    return JSON.stringify({
        wrappedContentKey: base64Encode(header.wrappedContentKey),
        kdfSalt: base64Encode(header.kdfSalt),
        iv: base64Encode(header.iv),
        kdfAlgorithm: KDF_ALGORITHM,
        kdfIterations: header.kdfIterations,
        encryptionAlgorithm: ENCRYPTION_ALGORITHM,
        wrappingAlgorithm: WRAPPING_ALGORITHM,
        schemaVersion: PROTOCOL_VERSION,
    });
}

/** Frames a header (`{wrappedContentKey, kdfSalt, iv, kdfIterations}`) and ciphertext into a blob. */
export function encodeOuter(header, ciphertext) {
    const json = encoder.encode(headerJson(header));
    const blob = new Uint8Array(4 + json.length + ciphertext.length);
    new DataView(blob.buffer).setUint32(0, json.length, false);
    blob.set(json, 4);
    blob.set(ciphertext, 4 + json.length);
    return blob;
}

/** The exact size of the blob that sealing a `payloadLength`-byte inner payload produces. */
export function blobLength(payloadLength, kdfIterations) {
    const json = headerJson({
        wrappedContentKey: new Uint8Array(WRAPPED_KEY_LEN),
        kdfSalt: new Uint8Array(KDF_SALT_LEN),
        iv: new Uint8Array(IV_LEN),
        kdfIterations,
    });
    return 4 + encoder.encode(json).length + payloadLength + TAG_LEN;
}

/** Decodes and validates a blob (stage one). Throws a `PinveilError` with stage "header". */
export function decodeOuter(blob) {
    if (blob.length < 4) {
        throw headerError("truncated");
    }
    const headerLength = new DataView(blob.buffer, blob.byteOffset, blob.byteLength).getUint32(0, false);
    if (headerLength > MAX_OUTER_HEADER_BYTES) {
        throw headerError("header-too-large");
    }
    if (blob.length - 4 < headerLength) {
        throw headerError("truncated");
    }
    const headerBytes = blob.subarray(4, 4 + headerLength);
    const ciphertext = blob.subarray(4 + headerLength);

    let parsed;
    try {
        parsed = JSON.parse(strictDecoder.decode(headerBytes));
    } catch {
        throw headerError("malformed");
    }
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed) || !("schemaVersion" in parsed)) {
        throw headerError("malformed");
    }
    if (parsed.schemaVersion !== PROTOCOL_VERSION) {
        throw headerError("unsupported-version");
    }
    const keys = Object.keys(parsed);
    if (keys.length !== HEADER_KEYS.length || !HEADER_KEYS.every((key) => keys.includes(key))) {
        throw headerError("malformed");
    }
    for (const key of HEADER_KEYS.filter((k) => k !== "kdfIterations" && k !== "schemaVersion")) {
        if (typeof parsed[key] !== "string") {
            throw headerError("malformed");
        }
    }
    if (!Number.isSafeInteger(parsed.kdfIterations) || parsed.kdfIterations < 0) {
        throw headerError("malformed");
    }
    if (
        parsed.kdfAlgorithm !== KDF_ALGORITHM ||
        parsed.encryptionAlgorithm !== ENCRYPTION_ALGORITHM ||
        parsed.wrappingAlgorithm !== WRAPPING_ALGORITHM
    ) {
        throw headerError("unsupported-algorithm");
    }
    if (!isAcceptedKdfIterations(parsed.kdfIterations)) {
        throw headerError("kdf-iterations-out-of-range");
    }
    const header = {
        wrappedContentKey: decodeField(parsed.wrappedContentKey, WRAPPED_KEY_LEN),
        kdfSalt: decodeField(parsed.kdfSalt, KDF_SALT_LEN),
        iv: decodeField(parsed.iv, IV_LEN),
        kdfIterations: parsed.kdfIterations,
    };
    if (!bytesEqual(encoder.encode(headerJson(header)), headerBytes)) {
        throw headerError("not-canonical");
    }
    if (ciphertext.length < TAG_LEN) {
        throw headerError("truncated");
    }
    return { header, ciphertext };
}

function decodeField(value, length) {
    let bytes;
    try {
        bytes = base64Decode(value);
    } catch {
        throw headerError("bad-field-size");
    }
    if (bytes.length !== length) {
        throw headerError("bad-field-size");
    }
    return bytes;
}

function bytesEqual(a, b) {
    return a.length === b.length && a.every((byte, i) => byte === b[i]);
}

export function base64Encode(bytes) {
    let binary = "";
    for (const byte of bytes) {
        binary += String.fromCharCode(byte);
    }
    return btoa(binary);
}

export function base64Decode(value) {
    const binary = atob(value);
    return Uint8Array.from(binary, (char) => char.charCodeAt(0));
}
