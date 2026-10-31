//! Protocol constants shared by every client (APP.md, "Validation"). Operators can't change
//! these; server configuration may only set creation limits at or below the ceilings.
//! `web/static/protocol.js` mirrors this file.

/// The envelope schema version this build writes and reads.
pub const PROTOCOL_VERSION: u32 = 2;

/// `kdfAlgorithm` label.
pub const KDF_ALGORITHM: &str = "PBKDF2-SHA-256";
/// `encryptionAlgorithm` label.
pub const ENCRYPTION_ALGORITHM: &str = "AES-256-GCM";
/// `wrappingAlgorithm` label.
pub const WRAPPING_ALGORITHM: &str = "AES-KW";

/// Accepted PBKDF2 iteration range. A blob outside it is refused before any key
/// derivation, so a hostile sender can't make a recipient burn unbounded CPU. Final
/// bounds come from benchmarks; the minimum follows OWASP's PBKDF2-SHA-256 guidance.
pub const KDF_ITERATIONS_MIN: u32 = 600_000;
/// Upper end of the accepted PBKDF2 iteration range.
pub const KDF_ITERATIONS_MAX: u32 = 2_000_000;

/// PBKDF2 salt length.
pub const KDF_SALT_LEN: usize = 16;
/// AES-GCM nonce length.
pub const IV_LEN: usize = 12;
/// AES-GCM tag length.
pub const TAG_LEN: usize = 16;
/// Raw content key length (AES-256).
pub const CONTENT_KEY_LEN: usize = 32;
/// AES-KW output for a 32-byte key.
pub const WRAPPED_KEY_LEN: usize = 40;

/// Largest outer header accepted. A canonical header is under 300 bytes.
pub const MAX_OUTER_HEADER_BYTES: usize = 1024;

/// Decoding ceiling on the number of files in a message.
pub const MAX_FILES_CEILING: usize = 256;
/// Decoding ceiling on the inner manifest size.
pub const MAX_MANIFEST_BYTES_CEILING: usize = 256 * 1024;
/// Longest file name, in UTF-8 bytes.
pub const MAX_NAME_BYTES: usize = 255;
/// Longest MIME type, in bytes.
pub const MAX_MIME_TYPE_BYTES: usize = 255;
