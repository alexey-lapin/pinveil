//! Cross-client vectors (implementation plan, Phase 1 step 7; coverage `FMT-vectors`).
//!
//! - `tests/vectors/js-made.json` comes from the browser modules. Core must open every
//!   positive blob, re-seal it byte for byte, and refuse every negative at its stage.
//! - `tests/vectors/rust-made.json` comes from here. The JS suite holds the browser to it.
//!   Run with `PINVEIL_WRITE_VECTORS=1` to rewrite it after an intended format change.

use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use pinveil_core::crypto::{open_payload, seal};
use pinveil_core::entropy::{Entropy, EntropyError};
use pinveil_core::payload::{CreationLimits, FileEntry, Payload, encode_payload};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct VectorFile {
    description: String,
    positive: Vec<Positive>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    negative: Vec<Negative>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Positive {
    case: String,
    passphrase: String,
    pin: String,
    kdf_iterations: u32,
    entropy: String,
    payload: PayloadSpec,
    blob: String,
}

#[derive(Serialize, Deserialize)]
struct PayloadSpec {
    text: Option<String>,
    files: Vec<FileSpec>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileSpec {
    name: String,
    mime_type: String,
    data: String,
}

#[derive(Serialize, Deserialize)]
struct Negative {
    case: String,
    passphrase: String,
    pin: String,
    blob: String,
    stage: String,
}

/// Replays fixed bytes, then fails like a broken CSPRNG.
struct Replay(Vec<u8>);

impl Entropy for Replay {
    fn fill(&mut self, buf: &mut [u8]) -> Result<(), EntropyError> {
        if self.0.len() < buf.len() {
            return Err(EntropyError);
        }
        buf.copy_from_slice(&self.0[..buf.len()]);
        self.0.drain(..buf.len());
        Ok(())
    }
}

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/vectors")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(vectors_dir().join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

fn b64(value: &str) -> Vec<u8> {
    STANDARD.decode(value).expect("vector base64")
}

fn from_hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).expect("vector hex"))
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        write!(out, "{b:02x}").expect("writing to a String");
        out
    })
}

fn payload_of(spec: &PayloadSpec) -> Payload {
    Payload {
        text: spec.text.clone(),
        files: spec
            .files
            .iter()
            .map(|f| FileEntry {
                name: f.name.clone(),
                mime_type: f.mime_type.clone(),
                data: b64(&f.data),
            })
            .collect(),
    }
}

fn spec_of(payload: &Payload) -> PayloadSpec {
    PayloadSpec {
        text: payload.text.clone(),
        files: payload
            .files
            .iter()
            .map(|f| FileSpec {
                name: f.name.clone(),
                mime_type: f.mime_type.clone(),
                data: STANDARD.encode(&f.data),
            })
            .collect(),
    }
}

fn seal_case(
    payload: &Payload,
    passphrase: &str,
    pin: &str,
    kdf_iterations: u32,
    entropy: &[u8],
) -> Vec<u8> {
    let bytes = encode_payload(payload, CreationLimits::CEILINGS).expect("vector payload encodes");
    seal(
        &bytes,
        passphrase,
        pin,
        kdf_iterations,
        &mut Replay(entropy.to_vec()),
    )
    .expect("vector seals")
}

fn check_positive(file: &VectorFile) {
    assert!(!file.positive.is_empty());
    for case in &file.positive {
        let blob = b64(&case.blob);
        let expected = payload_of(&case.payload);
        let opened = open_payload(&blob, &case.passphrase, &case.pin)
            .unwrap_or_else(|e| panic!("{}: open failed: {e}", case.case));
        assert_eq!(opened, expected, "{}: payload", case.case);
        let resealed = seal_case(
            &expected,
            &case.passphrase,
            &case.pin,
            case.kdf_iterations,
            &from_hex(&case.entropy),
        );
        assert!(resealed == blob, "{}: re-sealed bytes differ", case.case);
    }
}

#[test]
fn fmt_vectors_js_made_positive_open_and_reseal_exactly() {
    check_positive(&serde_json::from_str(&read("js-made.json")).unwrap());
}

#[test]
fn fmt_vectors_js_made_negative_refused_at_stage() {
    let file: VectorFile = serde_json::from_str(&read("js-made.json")).unwrap();
    assert!(file.negative.len() >= 20);
    for case in &file.negative {
        let started = std::time::Instant::now();
        let error = open_payload(&b64(&case.blob), &case.passphrase, &case.pin)
            .expect_err(&format!("{}: should be refused", case.case));
        assert_eq!(error.stage(), case.stage, "{}: {error}", case.case);
        if case.stage == "header" {
            // Stage one does no key derivation, so even a huge KDF count fails at once.
            assert!(
                started.elapsed() < std::time::Duration::from_millis(100),
                "{}: took {:?}",
                case.case,
                started.elapsed()
            );
        }
    }
}

/// Deterministic entropy for Rust-made case `seed` (a different scheme from the JS file's).
fn entropy_for(seed: u8) -> Vec<u8> {
    (0u8..60)
        .map(|i| seed.wrapping_mul(53).wrapping_add(i.wrapping_mul(29)))
        .collect()
}

fn build_rust_vectors() -> String {
    let file = |name: &str, mime: &str, data: Vec<u8>| FileEntry {
        name: name.into(),
        mime_type: mime.into(),
        data,
    };
    let cases: Vec<(&str, &str, &str, u32, Payload)> = vec![
        (
            "files only: ten small files",
            "cloak-tidal-barcode-unbalance",
            "271828",
            600_000,
            Payload {
                text: None,
                files: (0..10)
                    .map(|i| {
                        file(
                            &format!("part-{i}.txt"),
                            "text/plain",
                            format!("part {i}\n").into_bytes(),
                        )
                    })
                    .collect(),
            },
        ),
        (
            "CRLF text with a 64 KiB file",
            "abacus-zoom-yo-yo-abdomen",
            "000001",
            1_000_000,
            Payload {
                text: Some("line one\r\nline two\r\n".into()),
                files: vec![file(
                    "big.bin",
                    "application/octet-stream",
                    (0..65_536u32)
                        .map(|i| u8::try_from(i % 251).unwrap())
                        .collect(),
                )],
            },
        ),
        (
            "text only, written by the CLI",
            "felt-tip-drop-down-t-shirt-zoom",
            "777777",
            600_000,
            Payload {
                text: Some("hello from the terminal 🗝".into()),
                files: vec![],
            },
        ),
    ];
    let positive = cases
        .into_iter()
        .zip(1u8..)
        .map(|((case, passphrase, pin, kdf_iterations, payload), seed)| {
            let entropy = entropy_for(seed);
            let blob = seal_case(&payload, passphrase, pin, kdf_iterations, &entropy);
            Positive {
                case: case.into(),
                passphrase: passphrase.into(),
                pin: pin.into(),
                kdf_iterations,
                entropy: to_hex(&entropy),
                payload: spec_of(&payload),
                blob: STANDARD.encode(blob),
            }
        })
        .collect();
    let file = VectorFile {
        description: "Made by pinveil-core (crates/pinveil-core/tests/vectors.rs). `entropy` is the salt, IV, and content key, in draw order. Both clients must open each blob to `payload` and re-seal it to the same bytes.".into(),
        positive,
        negative: vec![],
    };
    serde_json::to_string_pretty(&file).unwrap() + "\n"
}

#[test]
fn fmt_vectors_rust_made_reproduce_and_open() {
    let built = build_rust_vectors();
    if std::env::var_os("PINVEIL_WRITE_VECTORS").is_some() {
        std::fs::write(vectors_dir().join("rust-made.json"), &built).unwrap();
    }
    assert!(
        built == read("rust-made.json"),
        "rust-made.json is stale; rerun with PINVEIL_WRITE_VECTORS=1"
    );
    check_positive(&serde_json::from_str(&built).unwrap());
}
