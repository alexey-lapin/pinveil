//! Sealing and opening blobs (APP.md, "Keys"). Mirrors `web/static/crypto.js`.
//!
//! PBKDF2-HMAC-SHA-256 over `passphrase + pin` derives a key-encryption key, AES-KW wraps a
//! fresh content key, and AES-256-GCM encrypts the payload with the wrapped key as
//! associated data. Randomness is drawn in a fixed order (salt, IV, content key), so tests
//! that replay entropy get byte-exact blobs in both clients.

use aes_gcm::aead::AeadInOut;
use aes_gcm::{Aes256Gcm, KeyInit};
use aes_kw::KwAes256;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::entropy::{Entropy, EntropyError};
use crate::envelope::{
    HeaderError, OuterHeader, decode_outer, encode_outer, is_accepted_kdf_iterations,
};
use crate::generate::is_valid_pin;
use crate::payload::{Payload, PayloadError, decode_payload};
use crate::protocol::{CONTENT_KEY_LEN, IV_LEN, KDF_SALT_LEN, WRAPPED_KEY_LEN};

/// Why sealing failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SealError {
    #[error(transparent)]
    Entropy(#[from] EntropyError),
    #[error("the PIN must be exactly six digits")]
    InvalidPin,
    #[error("the KDF iteration count is outside the accepted range")]
    KdfIterationsOutOfRange,
}

/// Why opening failed, by validation stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    /// Stage one: refused before any key derivation.
    #[error(transparent)]
    Header(#[from] HeaderError),
    #[error("the PIN must be exactly six digits")]
    InvalidPin,
    /// Wrong passphrase or PIN, or a tampered blob. Deliberately not more specific.
    #[error("unable to decrypt this message")]
    Decrypt,
    /// Stage two: refused after decryption, before anything is shown or written.
    #[error(transparent)]
    Payload(#[from] PayloadError),
}

impl OpenError {
    /// The validation stage, as named in the cross-client vectors.
    #[must_use]
    pub fn stage(&self) -> &'static str {
        match self {
            Self::Header(_) => "header",
            Self::InvalidPin => "input",
            Self::Decrypt => "decrypt",
            Self::Payload(_) => "payload",
        }
    }
}

/// Per-message randomness, in draw order.
pub(crate) struct SealMaterial {
    pub(crate) kdf_salt: [u8; KDF_SALT_LEN],
    pub(crate) iv: [u8; IV_LEN],
    pub(crate) content_key: Zeroizing<[u8; CONTENT_KEY_LEN]>,
}

impl SealMaterial {
    fn draw(entropy: &mut impl Entropy) -> Result<Self, EntropyError> {
        let mut material = Self {
            kdf_salt: [0; KDF_SALT_LEN],
            iv: [0; IV_LEN],
            content_key: Zeroizing::new([0; CONTENT_KEY_LEN]),
        };
        entropy.fill(&mut material.kdf_salt)?;
        entropy.fill(&mut material.iv)?;
        entropy.fill(material.content_key.as_mut_slice())?;
        Ok(material)
    }
}

/// Encrypts encoded payload bytes into a blob.
pub fn seal(
    payload_bytes: &[u8],
    passphrase: &str,
    pin: &str,
    kdf_iterations: u32,
    entropy: &mut impl Entropy,
) -> Result<Vec<u8>, SealError> {
    if !is_valid_pin(pin) {
        return Err(SealError::InvalidPin);
    }
    if !is_accepted_kdf_iterations(kdf_iterations) {
        return Err(SealError::KdfIterationsOutOfRange);
    }
    let material = SealMaterial::draw(entropy)?;
    Ok(seal_with(
        payload_bytes,
        passphrase,
        pin,
        kdf_iterations,
        &material,
    ))
}

pub(crate) fn seal_with(
    payload_bytes: &[u8],
    passphrase: &str,
    pin: &str,
    kdf_iterations: u32,
    material: &SealMaterial,
) -> Vec<u8> {
    let kek = derive_kek(passphrase, pin, &material.kdf_salt, kdf_iterations);
    let mut wrapped_content_key = [0u8; WRAPPED_KEY_LEN];
    KwAes256::new_from_slice(kek.as_slice())
        .expect("32-byte KEK")
        .wrap_key(material.content_key.as_slice(), &mut wrapped_content_key)
        .expect("32-byte content key wraps to 40 bytes");

    let cipher = Aes256Gcm::new_from_slice(material.content_key.as_slice()).expect("32-byte key");
    let mut ciphertext = payload_bytes.to_vec();
    cipher
        .encrypt_in_place(&material.iv.into(), &wrapped_content_key, &mut ciphertext)
        .expect("payload is within AES-GCM limits");

    let header = OuterHeader {
        wrapped_content_key,
        kdf_salt: material.kdf_salt,
        iv: material.iv,
        kdf_iterations,
    };
    encode_outer(&header, &ciphertext)
}

/// Validates a blob's header, then decrypts it. Returns the inner payload bytes, which
/// still need [`decode_payload`]; [`open_payload`] does both.
pub fn open(blob: &[u8], passphrase: &str, pin: &str) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    let envelope = decode_outer(blob)?;
    if !is_valid_pin(pin) {
        return Err(OpenError::InvalidPin);
    }
    let header = &envelope.header;
    let kek = derive_kek(passphrase, pin, &header.kdf_salt, header.kdf_iterations);
    let mut content_key = Zeroizing::new([0u8; CONTENT_KEY_LEN]);
    KwAes256::new_from_slice(kek.as_slice())
        .expect("32-byte KEK")
        .unwrap_key(&header.wrapped_content_key, content_key.as_mut_slice())
        .map_err(|_| OpenError::Decrypt)?;

    let cipher = Aes256Gcm::new_from_slice(content_key.as_slice()).expect("32-byte key");
    let mut plaintext = Zeroizing::new(envelope.ciphertext.to_vec());
    cipher
        .decrypt_in_place(
            &header.iv.into(),
            &header.wrapped_content_key,
            &mut *plaintext,
        )
        .map_err(|_| OpenError::Decrypt)?;
    Ok(plaintext)
}

/// [`open`] followed by [`decode_payload`]: both validation stages.
pub fn open_payload(blob: &[u8], passphrase: &str, pin: &str) -> Result<Payload, OpenError> {
    let plaintext = open(blob, passphrase, pin)?;
    Ok(decode_payload(&plaintext)?)
}

fn derive_kek(passphrase: &str, pin: &str, salt: &[u8], iterations: u32) -> Zeroizing<[u8; 32]> {
    let input = Zeroizing::new(format!("{passphrase}{pin}"));
    let mut kek = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha256>(input.as_bytes(), salt, iterations, kek.as_mut_slice());
    kek
}

#[cfg(test)]
mod tests {
    use super::{OpenError, SealError, open, open_payload, seal};
    use crate::entropy::{ReplayEntropy, SystemEntropy};
    use crate::envelope::decode_outer;
    use crate::payload::{CreationLimits, FileEntry, Payload, encode_payload};
    use crate::protocol::KDF_ITERATIONS_MIN;

    const PASSPHRASE: &str = "alpha-bravo-charlie-delta";
    const PIN: &str = "123456";

    fn sample() -> Payload {
        Payload {
            text: Some("hello".into()),
            files: vec![FileEntry {
                name: "a.txt".into(),
                mime_type: "text/plain".into(),
                data: b"abc".to_vec(),
            }],
        }
    }

    fn sealed() -> Vec<u8> {
        let bytes = encode_payload(&sample(), CreationLimits::CEILINGS).unwrap();
        seal(
            &bytes,
            PASSPHRASE,
            PIN,
            KDF_ITERATIONS_MIN,
            &mut SystemEntropy,
        )
        .unwrap()
    }

    #[test]
    fn inv_4_round_trip_needs_both_inputs() {
        let blob = sealed();
        assert_eq!(open_payload(&blob, PASSPHRASE, PIN).unwrap(), sample());
        assert_eq!(
            open_payload(&blob, PASSPHRASE, "123457").unwrap_err(),
            OpenError::Decrypt
        );
        assert_eq!(
            open_payload(&blob, "alpha-bravo-charlie-echo", PIN).unwrap_err(),
            OpenError::Decrypt
        );
    }

    #[test]
    fn inv_14_tamper_before_render() {
        let blob = sealed();
        let header_len = u32::from_be_bytes(blob[..4].try_into().unwrap()) as usize;
        // Flip the last tag byte, a ciphertext byte, and swap in another blob's header.
        let mut tag = blob.clone();
        *tag.last_mut().unwrap() ^= 1;
        let mut body = blob.clone();
        body[4 + header_len] ^= 1;
        let other = sealed();
        let mut mixed = other[..4 + header_len].to_vec();
        mixed.extend_from_slice(&blob[4 + header_len..]);
        for tampered in [tag, body, mixed] {
            assert_eq!(
                open(&tampered, PASSPHRASE, PIN).unwrap_err(),
                OpenError::Decrypt
            );
        }
    }

    #[test]
    fn seal_draws_salt_iv_key_in_order_and_is_deterministic() {
        let entropy_bytes: Vec<u8> = (0..60).collect();
        let bytes = encode_payload(&sample(), CreationLimits::CEILINGS).unwrap();
        let first = seal(
            &bytes,
            PASSPHRASE,
            PIN,
            KDF_ITERATIONS_MIN,
            &mut ReplayEntropy::new(entropy_bytes.clone()),
        )
        .unwrap();
        let second = seal(
            &bytes,
            PASSPHRASE,
            PIN,
            KDF_ITERATIONS_MIN,
            &mut ReplayEntropy::new(entropy_bytes),
        )
        .unwrap();
        assert_eq!(first, second);
        let header = decode_outer(&first).unwrap().header;
        assert_eq!(
            header.kdf_salt,
            std::array::from_fn::<u8, 16, _>(|i| u8::try_from(i).unwrap())
        );
        assert_eq!(
            header.iv,
            std::array::from_fn::<u8, 12, _>(|i| u8::try_from(16 + i).unwrap())
        );
    }

    #[test]
    fn seal_refuses_bad_inputs_and_failed_entropy() {
        assert_eq!(
            seal(
                b"x",
                PASSPHRASE,
                "12345",
                KDF_ITERATIONS_MIN,
                &mut SystemEntropy
            )
            .unwrap_err(),
            SealError::InvalidPin
        );
        assert_eq!(
            seal(b"x", PASSPHRASE, PIN, 1000, &mut SystemEntropy).unwrap_err(),
            SealError::KdfIterationsOutOfRange
        );
        assert!(matches!(
            seal(
                b"x",
                PASSPHRASE,
                PIN,
                KDF_ITERATIONS_MIN,
                &mut ReplayEntropy::new(vec![0; 59])
            )
            .unwrap_err(),
            SealError::Entropy(_)
        ));
    }

    #[test]
    fn open_refuses_a_bad_pin_without_deriving() {
        assert_eq!(
            open(&sealed(), PASSPHRASE, "12a456").unwrap_err(),
            OpenError::InvalidPin
        );
    }
}
