//! The inner payload: what the ciphertext decrypts to (APP.md, "Inner payload").
//!
//! `[u32 BE manifest length][canonical JSON manifest][text bytes, if any][file bytes, in order]`
//!
//! Decoding is validation stage two (APP.md invariant 14): it runs after authenticated
//! decryption and before anything is rendered or written. It enforces the protocol
//! ceilings; encoding enforces the (lower or equal) creation limits the server advertises.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::protocol::{
    MAX_FILES_CEILING, MAX_MANIFEST_BYTES_CEILING, MAX_MIME_TYPE_BYTES, MAX_NAME_BYTES,
};

/// Largest length a manifest may declare: JavaScript's `Number.MAX_SAFE_INTEGER`, so both
/// clients read every accepted length exactly.
const MAX_DECLARED_LENGTH: u64 = (1 << 53) - 1;

/// One file in a message.
#[derive(Debug, Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct FileEntry {
    pub name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// A decrypted message: optional text plus zero or more files, at least one of the two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct Payload {
    pub text: Option<String>,
    pub files: Vec<FileEntry>,
}

/// Limits a server advertises for new messages. Always at or below the protocol ceilings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreationLimits {
    max_files: usize,
    max_manifest_bytes: usize,
}

impl CreationLimits {
    /// The protocol ceilings themselves.
    pub const CEILINGS: Self = Self {
        max_files: MAX_FILES_CEILING,
        max_manifest_bytes: MAX_MANIFEST_BYTES_CEILING,
    };

    /// Limits for new messages. Returns `None` if either is zero or above its ceiling.
    #[must_use]
    pub fn new(max_files: usize, max_manifest_bytes: usize) -> Option<Self> {
        let valid = (1..=MAX_FILES_CEILING).contains(&max_files)
            && (1..=MAX_MANIFEST_BYTES_CEILING).contains(&max_manifest_bytes);
        valid.then_some(Self {
            max_files,
            max_manifest_bytes,
        })
    }
}

/// Why a payload was refused, while encoding or after decryption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PayloadError {
    #[error("the message has no text and no files")]
    Empty,
    #[error("the message has too many files")]
    TooManyFiles,
    #[error("the message manifest is too large")]
    ManifestTooLarge,
    #[error("a file name is empty or too long")]
    BadName,
    #[error("a MIME type is empty or too long")]
    BadMimeType,
    #[error("the payload is too short")]
    Truncated,
    #[error("the manifest is not valid JSON of the expected shape")]
    Malformed,
    #[error("the manifest is not in canonical form")]
    NotCanonical,
    #[error("the declared lengths don't match the payload")]
    LengthMismatch,
    #[error("the text is not valid UTF-8")]
    InvalidText,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestJson {
    text: Option<TextJson>,
    files: Vec<FileJson>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TextJson {
    byte_length: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileJson {
    name: String,
    mime_type: String,
    byte_length: u64,
}

fn manifest_of(payload: &Payload) -> ManifestJson {
    ManifestJson {
        text: payload.text.as_ref().map(|text| TextJson {
            byte_length: text.len() as u64,
        }),
        files: payload
            .files
            .iter()
            .map(|file| FileJson {
                name: file.name.clone(),
                mime_type: file.mime_type.clone(),
                byte_length: file.data.len() as u64,
            })
            .collect(),
    }
}

fn check_entries(payload: &Payload) -> Result<(), PayloadError> {
    if payload.text.as_deref().is_none_or(str::is_empty) && payload.files.is_empty() {
        return Err(PayloadError::Empty);
    }
    for file in &payload.files {
        if !(1..=MAX_NAME_BYTES).contains(&file.name.len()) {
            return Err(PayloadError::BadName);
        }
        if !(1..=MAX_MIME_TYPE_BYTES).contains(&file.mime_type.len()) {
            return Err(PayloadError::BadMimeType);
        }
    }
    Ok(())
}

/// Encodes a payload for sealing, enforcing `limits`.
pub fn encode_payload(
    payload: &Payload,
    limits: CreationLimits,
) -> Result<Zeroizing<Vec<u8>>, PayloadError> {
    check_entries(payload)?;
    if payload.files.len() > limits.max_files {
        return Err(PayloadError::TooManyFiles);
    }
    let manifest =
        Zeroizing::new(serde_json::to_vec(&manifest_of(payload)).expect("manifest serializes"));
    if manifest.len() > limits.max_manifest_bytes {
        return Err(PayloadError::ManifestTooLarge);
    }
    let body_len: usize = payload.text.as_ref().map_or(0, String::len)
        + payload.files.iter().map(|f| f.data.len()).sum::<usize>();
    let mut bytes = Zeroizing::new(Vec::with_capacity(4 + manifest.len() + body_len));
    bytes.extend_from_slice(
        &u32::try_from(manifest.len())
            .expect("manifest fits u32")
            .to_be_bytes(),
    );
    bytes.extend_from_slice(&manifest);
    if let Some(text) = &payload.text {
        bytes.extend_from_slice(text.as_bytes());
    }
    for file in &payload.files {
        bytes.extend_from_slice(&file.data);
    }
    Ok(bytes)
}

/// Decodes and validates decrypted bytes (stage two), against the protocol ceilings.
pub fn decode_payload(bytes: &[u8]) -> Result<Payload, PayloadError> {
    let (prefix, rest) = bytes
        .split_first_chunk::<4>()
        .ok_or(PayloadError::Truncated)?;
    let manifest_len = u32::from_be_bytes(*prefix) as usize;
    if manifest_len > MAX_MANIFEST_BYTES_CEILING {
        return Err(PayloadError::ManifestTooLarge);
    }
    if rest.len() < manifest_len {
        return Err(PayloadError::Truncated);
    }
    let (manifest_bytes, mut body) = rest.split_at(manifest_len);
    let manifest: ManifestJson =
        serde_json::from_slice(manifest_bytes).map_err(|_| PayloadError::Malformed)?;
    if manifest.files.len() > MAX_FILES_CEILING {
        return Err(PayloadError::TooManyFiles);
    }

    let lengths = manifest
        .text
        .iter()
        .map(|t| t.byte_length)
        .chain(manifest.files.iter().map(|f| f.byte_length));
    let mut total: u64 = 0;
    for length in lengths {
        if length > MAX_DECLARED_LENGTH {
            return Err(PayloadError::Malformed);
        }
        total = total
            .checked_add(length)
            .ok_or(PayloadError::LengthMismatch)?;
    }
    if total != body.len() as u64 {
        return Err(PayloadError::LengthMismatch);
    }

    let text = match &manifest.text {
        Some(entry) => {
            let (text, rest) = body.split_at(usize::try_from(entry.byte_length).expect("checked"));
            body = rest;
            Some(String::from_utf8(text.to_vec()).map_err(|_| PayloadError::InvalidText)?)
        }
        None => None,
    };
    let mut files = Vec::with_capacity(manifest.files.len());
    for entry in &manifest.files {
        let (data, rest) = body.split_at(usize::try_from(entry.byte_length).expect("checked"));
        body = rest;
        files.push(FileEntry {
            name: entry.name.clone(),
            mime_type: entry.mime_type.clone(),
            data: data.to_vec(),
        });
    }
    let payload = Payload { text, files };
    check_entries(&payload)?;
    let canonical =
        Zeroizing::new(serde_json::to_vec(&manifest_of(&payload)).expect("manifest serializes"));
    if canonical.as_slice() != manifest_bytes {
        return Err(PayloadError::NotCanonical);
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::{CreationLimits, FileEntry, Payload, PayloadError, decode_payload, encode_payload};
    use crate::protocol::{MAX_FILES_CEILING, MAX_MANIFEST_BYTES_CEILING};

    fn file(name: &str, data: &[u8]) -> FileEntry {
        FileEntry {
            name: name.into(),
            mime_type: "text/plain".into(),
            data: data.to_vec(),
        }
    }

    /// Frames arbitrary manifest text and body, for building malformed payloads.
    fn framed(manifest: &str, body: &[u8]) -> Vec<u8> {
        let mut bytes = u32::try_from(manifest.len())
            .unwrap()
            .to_be_bytes()
            .to_vec();
        bytes.extend_from_slice(manifest.as_bytes());
        bytes.extend_from_slice(body);
        bytes
    }

    #[test]
    fn fmt_inner_round_trips_text_files_and_both() {
        let payloads = [
            Payload {
                text: Some("hello".into()),
                files: vec![],
            },
            Payload {
                text: None,
                files: vec![
                    file("a.txt", b"abc"),
                    file("a.txt", b""),
                    file("ünïcødé 🗝.bin", &[0, 255]),
                ],
            },
            Payload {
                text: Some("note".into()),
                files: vec![file("key.pem", b"-----")],
            },
        ];
        for payload in payloads {
            let bytes = encode_payload(&payload, CreationLimits::CEILINGS).unwrap();
            assert_eq!(decode_payload(&bytes).unwrap(), payload);
        }
    }

    #[test]
    fn fmt_inner_canonical_bytes() {
        let payload = Payload {
            text: Some("hi".into()),
            files: vec![file("a.txt", b"abc")],
        };
        let bytes = encode_payload(&payload, CreationLimits::CEILINGS).unwrap();
        let expected = framed(
            r#"{"text":{"byteLength":2},"files":[{"name":"a.txt","mimeType":"text/plain","byteLength":3}]}"#,
            b"hiabc",
        );
        assert_eq!(*bytes, expected);
        let no_text = Payload {
            text: None,
            files: vec![file("a", b"")],
        };
        let bytes = encode_payload(&no_text, CreationLimits::CEILINGS).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains(r#"{"text":null,"files":["#));
    }

    #[test]
    fn fmt_inner_creation_limits_vs_ceilings() {
        assert!(CreationLimits::new(MAX_FILES_CEILING, MAX_MANIFEST_BYTES_CEILING).is_some());
        assert!(CreationLimits::new(MAX_FILES_CEILING + 1, 1024).is_none());
        assert!(CreationLimits::new(10, MAX_MANIFEST_BYTES_CEILING + 1).is_none());
        assert!(CreationLimits::new(0, 1024).is_none());

        let limits = CreationLimits::new(2, 1024).unwrap();
        let three = Payload {
            text: None,
            files: vec![file("a", b""), file("b", b""), file("c", b"")],
        };
        assert_eq!(
            encode_payload(&three, limits).unwrap_err(),
            PayloadError::TooManyFiles
        );
        // A message made under the ceilings still decodes when a server lowers its limits:
        // decoding only checks the ceilings.
        let bytes = encode_payload(&three, CreationLimits::CEILINGS).unwrap();
        assert!(decode_payload(&bytes).is_ok());

        let small = CreationLimits::new(10, 40).unwrap();
        let long_name = Payload {
            text: None,
            files: vec![file(&"n".repeat(60), b"")],
        };
        assert_eq!(
            encode_payload(&long_name, small).unwrap_err(),
            PayloadError::ManifestTooLarge
        );
    }

    #[test]
    fn fmt_inner_names_and_emptiness() {
        let limits = CreationLimits::CEILINGS;
        let max_name = "é".repeat(127) + "x"; // 255 bytes
        assert_eq!(max_name.len(), 255);
        assert!(
            encode_payload(
                &Payload {
                    text: None,
                    files: vec![file(&max_name, b"")]
                },
                limits
            )
            .is_ok()
        );
        let too_long = "é".repeat(128); // 256 bytes
        for (payload, error) in [
            (Payload::default(), PayloadError::Empty),
            (
                Payload {
                    text: Some(String::new()),
                    files: vec![],
                },
                PayloadError::Empty,
            ),
            (
                Payload {
                    text: None,
                    files: vec![file("", b"x")],
                },
                PayloadError::BadName,
            ),
            (
                Payload {
                    text: None,
                    files: vec![file(&too_long, b"x")],
                },
                PayloadError::BadName,
            ),
            (
                Payload {
                    text: None,
                    files: vec![FileEntry {
                        name: "a".into(),
                        mime_type: String::new(),
                        data: vec![],
                    }],
                },
                PayloadError::BadMimeType,
            ),
        ] {
            assert_eq!(encode_payload(&payload, limits).unwrap_err(), error);
        }
    }

    #[test]
    fn inv_14_bad_manifest_before_output() {
        let good = r#"{"text":{"byteLength":2},"files":[{"name":"a.txt","mimeType":"text/plain","byteLength":3}]}"#;
        let too_many = format!(
            r#"{{"text":null,"files":[{}]}}"#,
            vec![r#"{"name":"a","mimeType":"b","byteLength":0}"#; MAX_FILES_CEILING + 1].join(",")
        );
        let cases: Vec<(Vec<u8>, PayloadError)> = vec![
            (vec![0, 0], PayloadError::Truncated),
            (vec![0, 0, 0, 50, b'{'], PayloadError::Truncated),
            (vec![0x7f, 0xff, 0xff, 0xff], PayloadError::ManifestTooLarge),
            (framed("nope", b""), PayloadError::Malformed),
            (framed(good, b"hiab"), PayloadError::LengthMismatch),
            (framed(good, b"hiabcd"), PayloadError::LengthMismatch),
            (
                framed(
                    &good.replace(r#""byteLength":3"#, r#""byteLength":-1"#),
                    b"hiabc",
                ),
                PayloadError::Malformed,
            ),
            (
                framed(
                    &good.replace(r#""byteLength":3"#, r#""byteLength":9007199254740992"#),
                    b"hiabc",
                ),
                PayloadError::Malformed,
            ),
            (
                framed(
                    &good
                        .replace(r#""byteLength":2"#, r#""byteLength":9007199254740991"#)
                        .replace(r#""byteLength":3"#, r#""byteLength":9007199254740991"#),
                    b"",
                ),
                PayloadError::LengthMismatch,
            ),
            (
                framed(&good.replace(r#""name":"a.txt""#, r#""name":"""#), b"hiabc"),
                PayloadError::BadName,
            ),
            (
                framed(&good.replace("text/plain", ""), b"hiabc"),
                PayloadError::BadMimeType,
            ),
            (
                framed(r#"{"text":null,"files":[]}"#, b""),
                PayloadError::Empty,
            ),
            (
                framed(&good.replace(',', ", "), b"hiabc"),
                PayloadError::NotCanonical,
            ),
            (
                framed(
                    r#"{"files":[{"name":"a.txt","mimeType":"text/plain","byteLength":3}]}"#,
                    b"abc",
                ),
                PayloadError::NotCanonical,
            ),
            (
                framed(&good.replace("}]}", r#","extra":1}]}"#), b"hiabc"),
                PayloadError::Malformed,
            ),
            (
                framed(good, &[0xff, 0xfe, b'a', b'b', b'c']),
                PayloadError::InvalidText,
            ),
            (framed(&too_many, b""), PayloadError::TooManyFiles),
        ];
        for (bytes, expected) in cases {
            assert_eq!(
                decode_payload(&bytes).unwrap_err(),
                expected,
                "{}",
                String::from_utf8_lossy(&bytes)
            );
        }
    }

    proptest::proptest! {
        #[test]
        fn fmt_fuzz_inner_never_panics(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..600)) {
            let _ = decode_payload(&bytes);
        }

        #[test]
        fn fmt_fuzz_inner_round_trips(text in proptest::option::of(".{1,40}"), names in proptest::collection::vec("[^\u{0}]{1,30}", 0..5), data in proptest::collection::vec(proptest::num::u8::ANY, 0..50)) {
            let files: Vec<_> = names.iter().map(|n| file(n, &data)).collect();
            let payload = Payload { text, files };
            if let Ok(bytes) = encode_payload(&payload, CreationLimits::CEILINGS) {
                proptest::prop_assert_eq!(decode_payload(&bytes).unwrap(), payload);
            }
        }
    }
}
