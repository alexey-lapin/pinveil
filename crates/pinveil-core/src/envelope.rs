//! The outer envelope: the opaque blob the server stores (APP.md, "Outer envelope").
//!
//! `[u32 BE header length][canonical JSON header][AES-GCM ciphertext with tag]`
//!
//! Decoding is validation stage one (APP.md invariant 14): everything here is checked
//! before any key derivation. The header must be byte-identical to its canonical form
//! (fixed key order, no whitespace), so the browser and the CLI accept exactly the same
//! inputs.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use crate::protocol::{
    ENCRYPTION_ALGORITHM, IV_LEN, KDF_ALGORITHM, KDF_ITERATIONS_MAX, KDF_ITERATIONS_MIN,
    KDF_SALT_LEN, MAX_OUTER_HEADER_BYTES, PROTOCOL_VERSION, TAG_LEN, WRAPPED_KEY_LEN,
    WRAPPING_ALGORITHM,
};

/// The decoded, validated outer header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OuterHeader {
    pub wrapped_content_key: [u8; WRAPPED_KEY_LEN],
    pub kdf_salt: [u8; KDF_SALT_LEN],
    pub iv: [u8; IV_LEN],
    pub kdf_iterations: u32,
}

/// A decoded blob: the validated header and the ciphertext it describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OuterEnvelope<'a> {
    pub header: OuterHeader,
    pub ciphertext: &'a [u8],
}

/// Why a blob was refused before decryption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HeaderError {
    #[error("the blob is too short")]
    Truncated,
    #[error("the blob header is too large")]
    HeaderTooLarge,
    #[error("the blob header is not valid JSON of the expected shape")]
    Malformed,
    #[error("the blob header is not in canonical form")]
    NotCanonical,
    #[error("unsupported schema version")]
    UnsupportedVersion,
    #[error("unsupported algorithm")]
    UnsupportedAlgorithm,
    #[error("the KDF iteration count is outside the accepted range")]
    KdfIterationsOutOfRange,
    #[error("a header field has the wrong size")]
    BadFieldSize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HeaderJson {
    wrapped_content_key: String,
    kdf_salt: String,
    iv: String,
    kdf_algorithm: String,
    kdf_iterations: u64,
    encryption_algorithm: String,
    wrapping_algorithm: String,
    schema_version: u64,
}

impl OuterHeader {
    fn to_json(&self) -> Vec<u8> {
        let json = HeaderJson {
            wrapped_content_key: STANDARD.encode(self.wrapped_content_key),
            kdf_salt: STANDARD.encode(self.kdf_salt),
            iv: STANDARD.encode(self.iv),
            kdf_algorithm: KDF_ALGORITHM.to_owned(),
            kdf_iterations: u64::from(self.kdf_iterations),
            encryption_algorithm: ENCRYPTION_ALGORITHM.to_owned(),
            wrapping_algorithm: WRAPPING_ALGORITHM.to_owned(),
            schema_version: u64::from(PROTOCOL_VERSION),
        };
        serde_json::to_vec(&json).expect("header serializes")
    }
}

/// Whether `iterations` is in the accepted KDF range.
#[must_use]
pub fn is_accepted_kdf_iterations(iterations: u32) -> bool {
    (KDF_ITERATIONS_MIN..=KDF_ITERATIONS_MAX).contains(&iterations)
}

/// Frames a header and ciphertext into a blob.
#[must_use]
pub fn encode_outer(header: &OuterHeader, ciphertext: &[u8]) -> Vec<u8> {
    let json = header.to_json();
    let mut blob = Vec::with_capacity(4 + json.len() + ciphertext.len());
    blob.extend_from_slice(
        &u32::try_from(json.len())
            .expect("header fits u32")
            .to_be_bytes(),
    );
    blob.extend_from_slice(&json);
    blob.extend_from_slice(ciphertext);
    blob
}

/// The exact size of the blob that sealing a `payload_len`-byte inner payload produces.
/// Clients compare this with `maxPayloadBytes` before encrypting.
#[must_use]
pub fn blob_len(payload_len: usize, kdf_iterations: u32) -> usize {
    let header = OuterHeader {
        wrapped_content_key: [0; WRAPPED_KEY_LEN],
        kdf_salt: [0; KDF_SALT_LEN],
        iv: [0; IV_LEN],
        kdf_iterations,
    };
    4 + header.to_json().len() + payload_len + TAG_LEN
}

/// Decodes and validates a blob (stage one). Does no cryptographic work.
pub fn decode_outer(blob: &[u8]) -> Result<OuterEnvelope<'_>, HeaderError> {
    let (prefix, rest) = blob
        .split_first_chunk::<4>()
        .ok_or(HeaderError::Truncated)?;
    let header_len = u32::from_be_bytes(*prefix) as usize;
    if header_len > MAX_OUTER_HEADER_BYTES {
        return Err(HeaderError::HeaderTooLarge);
    }
    if rest.len() < header_len {
        return Err(HeaderError::Truncated);
    }
    let (header_bytes, ciphertext) = rest.split_at(header_len);

    // Read the version on its own first, so a future version reports as unsupported
    // rather than as malformed.
    let loose: serde_json::Value =
        serde_json::from_slice(header_bytes).map_err(|_| HeaderError::Malformed)?;
    let version = loose
        .as_object()
        .ok_or(HeaderError::Malformed)?
        .get("schemaVersion")
        .ok_or(HeaderError::Malformed)?;
    if version.as_u64() != Some(u64::from(PROTOCOL_VERSION)) {
        return Err(HeaderError::UnsupportedVersion);
    }

    let json: HeaderJson =
        serde_json::from_slice(header_bytes).map_err(|_| HeaderError::Malformed)?;
    if json.kdf_algorithm != KDF_ALGORITHM
        || json.encryption_algorithm != ENCRYPTION_ALGORITHM
        || json.wrapping_algorithm != WRAPPING_ALGORITHM
    {
        return Err(HeaderError::UnsupportedAlgorithm);
    }
    let kdf_iterations = u32::try_from(json.kdf_iterations)
        .ok()
        .filter(|&n| is_accepted_kdf_iterations(n))
        .ok_or(HeaderError::KdfIterationsOutOfRange)?;
    let header = OuterHeader {
        wrapped_content_key: decode_field(&json.wrapped_content_key)?,
        kdf_salt: decode_field(&json.kdf_salt)?,
        iv: decode_field(&json.iv)?,
        kdf_iterations,
    };
    if header.to_json() != header_bytes {
        return Err(HeaderError::NotCanonical);
    }
    if ciphertext.len() < TAG_LEN {
        return Err(HeaderError::Truncated);
    }
    Ok(OuterEnvelope { header, ciphertext })
}

fn decode_field<const N: usize>(value: &str) -> Result<[u8; N], HeaderError> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|_| HeaderError::BadFieldSize)?;
    bytes.try_into().map_err(|_| HeaderError::BadFieldSize)
}

#[cfg(test)]
mod tests {
    use super::{HeaderError, OuterHeader, blob_len, decode_outer, encode_outer};
    use crate::protocol::{KDF_ITERATIONS_MAX, KDF_ITERATIONS_MIN, TAG_LEN};

    fn header(kdf_iterations: u32) -> OuterHeader {
        OuterHeader {
            wrapped_content_key: [7; 40],
            kdf_salt: [1; 16],
            iv: [2; 12],
            kdf_iterations,
        }
    }

    /// Frames arbitrary header text, for building malformed blobs.
    fn framed(header_json: &str, ciphertext_len: usize) -> Vec<u8> {
        let mut blob = u32::try_from(header_json.len())
            .unwrap()
            .to_be_bytes()
            .to_vec();
        blob.extend_from_slice(header_json.as_bytes());
        blob.extend(std::iter::repeat_n(0u8, ciphertext_len));
        blob
    }

    fn canonical_json(kdf_iterations: u32) -> String {
        let blob = encode_outer(&header(kdf_iterations), &[]);
        String::from_utf8(blob[4..].to_vec()).unwrap()
    }

    #[test]
    fn fmt_outer_round_trips_and_is_canonical() {
        let ciphertext = [9u8; 40];
        let blob = encode_outer(&header(600_000), &ciphertext);
        let decoded = decode_outer(&blob).unwrap();
        assert_eq!(decoded.header, header(600_000));
        assert_eq!(decoded.ciphertext, ciphertext);
        assert_eq!(
            canonical_json(600_000),
            r#"{"wrappedContentKey":"BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBw==","kdfSalt":"AQEBAQEBAQEBAQEBAQEBAQ==","iv":"AgICAgICAgICAgIC","kdfAlgorithm":"PBKDF2-SHA-256","kdfIterations":600000,"encryptionAlgorithm":"AES-256-GCM","wrappingAlgorithm":"AES-KW","schemaVersion":2}"#
        );
    }

    #[test]
    fn fmt_outer_blob_len_is_exact() {
        for iterations in [KDF_ITERATIONS_MIN, 1_234_567, KDF_ITERATIONS_MAX] {
            for payload in [0, 1, 1000] {
                let blob = encode_outer(&header(iterations), &vec![0; payload + TAG_LEN]);
                assert_eq!(blob.len(), blob_len(payload, iterations));
            }
        }
    }

    #[test]
    fn inv_14_truncated_and_oversized_headers() {
        assert_eq!(decode_outer(&[]).unwrap_err(), HeaderError::Truncated);
        assert_eq!(
            decode_outer(&[0, 0, 1]).unwrap_err(),
            HeaderError::Truncated
        );
        assert_eq!(
            decode_outer(&[0, 0, 0, 200, b'{']).unwrap_err(),
            HeaderError::Truncated
        );
        assert_eq!(
            decode_outer(&[0xff, 0xff, 0xff, 0xff]).unwrap_err(),
            HeaderError::HeaderTooLarge
        );
        let no_tag = framed(&canonical_json(600_000), TAG_LEN - 1);
        assert_eq!(decode_outer(&no_tag).unwrap_err(), HeaderError::Truncated);
    }

    #[test]
    fn inv_14_bad_header_before_kdf() {
        let good = canonical_json(600_000);
        let cases: Vec<(String, HeaderError)> = vec![
            ("not json".into(), HeaderError::Malformed),
            ("[]".into(), HeaderError::Malformed),
            (
                good.replace(r#""schemaVersion":2"#, r#""schemaVersion":1"#),
                HeaderError::UnsupportedVersion,
            ),
            (
                good.replace(r#""schemaVersion":2"#, r#""schemaVersion":"2""#),
                HeaderError::UnsupportedVersion,
            ),
            (
                good.replace("AES-256-GCM", "AES-128-GCM"),
                HeaderError::UnsupportedAlgorithm,
            ),
            (
                good.replace("PBKDF2-SHA-256", "Argon2id"),
                HeaderError::UnsupportedAlgorithm,
            ),
            (
                good.replace(r#""AES-KW""#, r#""AES-KWP""#),
                HeaderError::UnsupportedAlgorithm,
            ),
            (
                good.replace("600000", "1"),
                HeaderError::KdfIterationsOutOfRange,
            ),
            (
                good.replace("600000", "1000000000"),
                HeaderError::KdfIterationsOutOfRange,
            ),
            (
                good.replace("600000", "99999999999999"),
                HeaderError::KdfIterationsOutOfRange,
            ),
            (good.replace("600000", "600000.0"), HeaderError::Malformed),
            (good.replace("600000", "-600000"), HeaderError::Malformed),
            (
                good.replace("AgICAgICAgICAgIC", "AgICAgICAgICAgI="),
                HeaderError::BadFieldSize,
            ),
            (
                good.replace("AgICAgICAgICAgIC", "AgICAgICAgICAgICAgI="),
                HeaderError::BadFieldSize,
            ),
            (
                good.replace(r#""iv""#, r#""nonce""#),
                HeaderError::Malformed,
            ),
            (good.replace('}', r#","extra":1}"#), HeaderError::Malformed),
            (good.replace(',', ", "), HeaderError::NotCanonical),
            (
                good.replace(
                    r#"{"wrappedContentKey""#,
                    r#"{"iv":"AgICAgICAgICAgIC","wrappedContentKey""#,
                ),
                HeaderError::Malformed,
            ),
        ];
        for (json, expected) in cases {
            assert_eq!(
                decode_outer(&framed(&json, TAG_LEN)).unwrap_err(),
                expected,
                "{json}"
            );
        }
    }

    #[test]
    fn inv_14_kdf_range_edges() {
        for (iterations, ok) in [
            (KDF_ITERATIONS_MIN - 1, false),
            (KDF_ITERATIONS_MIN, true),
            (KDF_ITERATIONS_MAX, true),
            (KDF_ITERATIONS_MAX + 1, false),
        ] {
            let blob = framed(
                &canonical_json(KDF_ITERATIONS_MIN).replace("600000", &iterations.to_string()),
                TAG_LEN,
            );
            assert_eq!(decode_outer(&blob).is_ok(), ok, "{iterations}");
        }
    }

    proptest::proptest! {
        #[test]
        fn fmt_fuzz_outer_never_panics(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..600)) {
            let _ = decode_outer(&bytes);
        }

        #[test]
        fn fmt_fuzz_outer_with_valid_prefix_never_panics(tail in proptest::collection::vec(proptest::num::u8::ANY, 0..400), len in 0u32..2000) {
            let mut blob = len.to_be_bytes().to_vec();
            blob.extend(tail);
            let _ = decode_outer(&blob);
        }
    }
}
