/**
 * The inner payload: what the ciphertext decrypts to. Mirrors `pinveil-core/src/payload.rs`.
 *
 * `[u32 BE manifest length][canonical JSON manifest][text bytes, if any][file bytes, in order]`
 *
 * Decoding is validation stage two: it runs after authenticated decryption and before anything
 * is rendered or saved, against the protocol ceilings. Encoding enforces the creation limits
 * the server advertises.
 */

import {
    MAX_FILES_CEILING,
    MAX_MANIFEST_BYTES_CEILING,
    MAX_MIME_TYPE_BYTES,
    MAX_NAME_BYTES,
    PinveilError,
} from "./protocol.js";

export const CEILING_LIMITS = Object.freeze({
    maxFiles: MAX_FILES_CEILING,
    maxManifestBytes: MAX_MANIFEST_BYTES_CEILING,
});

const encoder = new TextEncoder();
const strictDecoder = new TextDecoder("utf-8", { fatal: true });

function payloadError(reason) {
    return new PinveilError("payload", reason);
}

function byteLength(text) {
    return encoder.encode(text).length;
}

function manifestJson(text, files) {
    return JSON.stringify({
        text: text === null ? null : { byteLength: text.length },
        files: files.map((file) => ({ name: file.name, mimeType: file.mimeType, byteLength: file.data.length })),
    });
}

function checkEntries(textBytes, files) {
    if ((textBytes === null || textBytes.length === 0) && files.length === 0) {
        throw payloadError("empty");
    }
    for (const file of files) {
        const nameBytes = byteLength(file.name);
        if (nameBytes < 1 || nameBytes > MAX_NAME_BYTES) {
            throw payloadError("bad-name");
        }
        const mimeBytes = byteLength(file.mimeType);
        if (mimeBytes < 1 || mimeBytes > MAX_MIME_TYPE_BYTES) {
            throw payloadError("bad-mime-type");
        }
    }
}

/**
 * Encodes `{text, files: [{name, mimeType, data}]}` (text may be null) for sealing, enforcing
 * `limits` (`{maxFiles, maxManifestBytes}`, at or below the ceilings).
 */
export function encodePayload({ text = null, files = [] }, limits) {
    const textBytes = text === null ? null : encoder.encode(text);
    checkEntries(textBytes, files);
    if (files.length > limits.maxFiles) {
        throw payloadError("too-many-files");
    }
    const manifest = encoder.encode(manifestJson(textBytes, files));
    if (manifest.length > limits.maxManifestBytes) {
        throw payloadError("manifest-too-large");
    }
    const bodyLength = (textBytes?.length ?? 0) + files.reduce((sum, file) => sum + file.data.length, 0);
    const bytes = new Uint8Array(4 + manifest.length + bodyLength);
    new DataView(bytes.buffer).setUint32(0, manifest.length, false);
    bytes.set(manifest, 4);
    let offset = 4 + manifest.length;
    if (textBytes !== null) {
        bytes.set(textBytes, offset);
        offset += textBytes.length;
    }
    for (const file of files) {
        bytes.set(file.data, offset);
        offset += file.data.length;
    }
    return bytes;
}

/**
 * Decodes and validates decrypted bytes (stage two). Returns `{text, files}`; file data are
 * views into `bytes`. Throws a `PinveilError` with stage "payload".
 */
export function decodePayload(bytes) {
    if (bytes.length < 4) {
        throw payloadError("truncated");
    }
    const manifestLength = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0, false);
    if (manifestLength > MAX_MANIFEST_BYTES_CEILING) {
        throw payloadError("manifest-too-large");
    }
    if (bytes.length - 4 < manifestLength) {
        throw payloadError("truncated");
    }
    const manifestBytes = bytes.subarray(4, 4 + manifestLength);
    let body = bytes.subarray(4 + manifestLength);

    let manifest;
    try {
        manifest = JSON.parse(strictDecoder.decode(manifestBytes));
    } catch {
        throw payloadError("malformed");
    }
    if (!isPlainObject(manifest) || !sameKeys(manifest, ["text", "files"]) || !Array.isArray(manifest.files)) {
        throw payloadError("malformed");
    }
    if (manifest.text !== null && !(isPlainObject(manifest.text) && sameKeys(manifest.text, ["byteLength"]))) {
        throw payloadError("malformed");
    }
    if (manifest.files.length > MAX_FILES_CEILING) {
        throw payloadError("too-many-files");
    }
    for (const file of manifest.files) {
        if (
            !isPlainObject(file) ||
            !sameKeys(file, ["name", "mimeType", "byteLength"]) ||
            typeof file.name !== "string" ||
            typeof file.mimeType !== "string"
        ) {
            throw payloadError("malformed");
        }
    }

    const lengths = [...(manifest.text === null ? [] : [manifest.text.byteLength]), ...manifest.files.map((f) => f.byteLength)];
    let total = 0;
    for (const length of lengths) {
        if (!Number.isSafeInteger(length) || length < 0) {
            throw payloadError("malformed");
        }
        total += length;
    }
    if (total !== body.length) {
        throw payloadError("length-mismatch");
    }

    let text = null;
    let textBytes = null;
    if (manifest.text !== null) {
        textBytes = body.subarray(0, manifest.text.byteLength);
        body = body.subarray(manifest.text.byteLength);
        try {
            text = strictDecoder.decode(textBytes);
        } catch {
            throw payloadError("invalid-text");
        }
    }
    const files = manifest.files.map((entry) => {
        const data = body.subarray(0, entry.byteLength);
        body = body.subarray(entry.byteLength);
        return { name: entry.name, mimeType: entry.mimeType, data };
    });
    checkEntries(textBytes, files);
    if (manifestJson(textBytes, files) !== strictDecoder.decode(manifestBytes)) {
        throw payloadError("not-canonical");
    }
    return { text, files };
}

function isPlainObject(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
}

function sameKeys(object, expected) {
    const keys = Object.keys(object);
    return keys.length === expected.length && expected.every((key) => keys.includes(key));
}
