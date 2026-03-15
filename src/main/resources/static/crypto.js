const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder();

export function encodeEnvelope(metadata, payloadBytes) {
    const metadataBytes = textEncoder.encode(JSON.stringify(metadata));
    const result = new Uint8Array(4 + metadataBytes.length + payloadBytes.length);
    new DataView(result.buffer).setUint32(0, metadataBytes.length, false);
    result.set(metadataBytes, 4);
    result.set(payloadBytes, 4 + metadataBytes.length);
    return result;
}

function decodeEnvelope(bytes) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const metadataLength = view.getUint32(0, false);
    const metadataStart = 4;
    const metadataEnd = metadataStart + metadataLength;
    const metadata = JSON.parse(textDecoder.decode(bytes.slice(metadataStart, metadataEnd)));
    const payload = bytes.slice(metadataEnd);
    return { metadata, payload };
}

export async function encryptPayload(payloadBytes, passphrase, pin, pbkdf2Iterations) {
    const kdfSalt = crypto.getRandomValues(new Uint8Array(16));
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const kek = await deriveKeyEncryptionKey(passphrase, pin, kdfSalt, pbkdf2Iterations);
    const cek = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, true, ["encrypt", "decrypt"]);
    const wrappedContentKey = new Uint8Array(await crypto.subtle.wrapKey("raw", cek, kek, "AES-KW"));
    const ciphertext = new Uint8Array(await crypto.subtle.encrypt(
        {
            name: "AES-GCM",
            iv,
            additionalData: wrappedContentKey
        },
        cek,
        payloadBytes
    ));

    const cryptoHeader = {
        wrappedContentKey: base64Encode(wrappedContentKey),
        kdfSalt: base64Encode(kdfSalt),
        iv: base64Encode(iv),
        kdfAlgorithm: "PBKDF2-SHA-256",
        kdfIterations: pbkdf2Iterations,
        encryptionAlgorithm: "AES-256-GCM",
        wrappingAlgorithm: "AES-KW",
        schemaVersion: 1
    };

    return encodeEnvelope(cryptoHeader, ciphertext);
}

export async function decryptPayload(blobBytes, passphrase, pin) {
    const outerEnvelope = decodeEnvelope(blobBytes);
    const header = outerEnvelope.metadata;
    const ciphertext = outerEnvelope.payload;

    const wrappedContentKey = base64Decode(header.wrappedContentKey);
    const iv = base64Decode(header.iv);
    const kdfSalt = base64Decode(header.kdfSalt);
    const kek = await deriveKeyEncryptionKey(passphrase, pin, kdfSalt, header.kdfIterations);
    const cek = await crypto.subtle.unwrapKey(
        "raw",
        wrappedContentKey,
        kek,
        "AES-KW",
        { name: "AES-GCM", length: 256 },
        false,
        ["decrypt"]
    );

    try {
        const plaintext = new Uint8Array(await crypto.subtle.decrypt(
            {
                name: "AES-GCM",
                iv,
                additionalData: wrappedContentKey
            },
            cek,
            ciphertext
        ));
        return decodeEnvelope(plaintext);
    } catch {
        throw new Error("Unable to decrypt this message. The PIN or passphrase may be wrong.");
    }
}

export function generatePassphrase(words) {
    return Array.from({ length: 4 }, () => randomWord(words)).join("-");
}

function randomWord(words) {
    const bytes = new Uint32Array(1);
    crypto.getRandomValues(bytes);
    return words[bytes[0] % words.length];
}

async function deriveKeyEncryptionKey(passphrase, pin, salt, iterations) {
    const baseKey = await crypto.subtle.importKey(
        "raw",
        textEncoder.encode(`${passphrase}${pin}`),
        "PBKDF2",
        false,
        ["deriveKey"]
    );

    return crypto.subtle.deriveKey(
        {
            name: "PBKDF2",
            hash: "SHA-256",
            salt,
            iterations
        },
        baseKey,
        { name: "AES-KW", length: 256 },
        false,
        ["wrapKey", "unwrapKey"]
    );
}

function base64Encode(bytes) {
    let binary = "";
    for (const byte of bytes) {
        binary += String.fromCharCode(byte);
    }
    return btoa(binary);
}

function base64Decode(value) {
    const binary = atob(value);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
        bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
}
