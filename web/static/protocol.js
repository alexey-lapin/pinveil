/**
 * Protocol constants shared by every client. Mirrors `pinveil-core/src/protocol.rs`;
 * the cross-client vectors catch any drift.
 */

export const PROTOCOL_VERSION = 2;

export const KDF_ALGORITHM = "PBKDF2-SHA-256";
export const ENCRYPTION_ALGORITHM = "AES-256-GCM";
export const WRAPPING_ALGORITHM = "AES-KW";

export const KDF_ITERATIONS_MIN = 600_000;
export const KDF_ITERATIONS_MAX = 2_000_000;

export const KDF_SALT_LEN = 16;
export const IV_LEN = 12;
export const TAG_LEN = 16;
export const CONTENT_KEY_LEN = 32;
export const WRAPPED_KEY_LEN = 40;

export const MAX_OUTER_HEADER_BYTES = 1024;

export const MAX_FILES_CEILING = 256;
export const MAX_MANIFEST_BYTES_CEILING = 256 * 1024;
export const MAX_NAME_BYTES = 255;
export const MAX_MIME_TYPE_BYTES = 255;

/**
 * A refused message. `stage` says where: "header" (before any key derivation), "decrypt"
 * (wrong passphrase or PIN, or tampering), "payload" (after decryption), or "create".
 * `reason` is a short machine-readable code.
 */
export class PinveilError extends Error {
    constructor(stage, reason, message = `${stage}: ${reason}`) {
        super(message);
        this.name = "PinveilError";
        this.stage = stage;
        this.reason = reason;
    }
}
