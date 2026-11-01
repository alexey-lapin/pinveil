//! Generators for passphrases, PINs, and message ids.

use zeroize::Zeroizing;

use crate::entropy::{Entropy, EntropyError, uniform_below};
use crate::wordlist::{WORDLIST_LEN, Wordlist};

/// Words in a passphrase (APP.md, "Keys").
pub const PASSPHRASE_WORDS: usize = 4;
/// Digits in a PIN.
pub const PIN_DIGITS: usize = 6;
const PIN_SPACE: u32 = 1_000_000;

/// Four random words joined with `-`.
pub fn passphrase(
    wordlist: &Wordlist<'_>,
    entropy: &mut impl Entropy,
) -> Result<Zeroizing<String>, EntropyError> {
    let words = random_words(wordlist, entropy, PASSPHRASE_WORDS)?;
    Ok(Zeroizing::new(words.join("-")))
}

/// A PIN, uniform over `000000` to `999999`.
pub fn pin(entropy: &mut impl Entropy) -> Result<Zeroizing<String>, EntropyError> {
    let value = uniform_below(entropy, PIN_SPACE)?;
    Ok(Zeroizing::new(format!("{value:06}")))
}

/// Whether `pin` is exactly six ASCII digits.
#[must_use]
pub fn is_valid_pin(pin: &str) -> bool {
    pin.len() == PIN_DIGITS && pin.bytes().all(|b| b.is_ascii_digit())
}

/// A message id such as `barcode-unbalance-cloak-102856`: `words` random words and a
/// zero-padded number with `digits` digits.
///
/// # Panics
///
/// If `digits` is not between 1 and 9.
pub fn message_id(
    wordlist: &Wordlist<'_>,
    entropy: &mut impl Entropy,
    words: usize,
    digits: u32,
) -> Result<String, EntropyError> {
    assert!(
        (1..=9).contains(&digits),
        "message id digits must be 1 to 9"
    );
    let mut parts = random_words(wordlist, entropy, words)?;
    let number = uniform_below(entropy, 10u32.pow(digits))?;
    let width = digits as usize;
    let suffix = format!("{number:0width$}");
    parts.push(&suffix);
    Ok(parts.join("-"))
}

fn random_words<'a>(
    wordlist: &Wordlist<'a>,
    entropy: &mut impl Entropy,
    count: usize,
) -> Result<Vec<&'a str>, EntropyError> {
    let bound = u32::try_from(WORDLIST_LEN).expect("wordlist length fits u32");
    (0..count)
        .map(|_| uniform_below(entropy, bound).map(|i| wordlist.word(i as usize)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{is_valid_pin, message_id, passphrase, pin};
    use crate::entropy::{EntropyError, ReplayEntropy, SystemEntropy};
    use crate::wordlist::Wordlist;

    #[test]
    fn gen_pin_boundaries() {
        // floor(2^32 / 10^6) * 10^6 = 4_294_000_000
        let mut entropy = ReplayEntropy::of_u32s(&[0]);
        assert_eq!(pin(&mut entropy).unwrap().as_str(), "000000");
        let mut entropy = ReplayEntropy::of_u32s(&[4_293_999_999]);
        assert_eq!(pin(&mut entropy).unwrap().as_str(), "999999");
        let mut entropy = ReplayEntropy::of_u32s(&[4_294_000_000, 42]);
        assert_eq!(pin(&mut entropy).unwrap().as_str(), "000042");
    }

    #[test]
    fn gen_pin_entropy_failure_aborts() {
        assert_eq!(
            pin(&mut ReplayEntropy::new([0, 1])).unwrap_err(),
            EntropyError
        );
    }

    #[test]
    fn gen_pin_format() {
        for _ in 0..1000 {
            assert!(is_valid_pin(&pin(&mut SystemEntropy).unwrap()));
        }
        for bad in ["", "12345", "1234567", "12345a", "١٢٣٤٥٦", " 12345"] {
            assert!(!is_valid_pin(bad), "{bad:?}");
        }
    }

    #[test]
    fn gen_passphrase_uses_the_validated_list() {
        let list = Wordlist::embedded().unwrap();
        let mut entropy = ReplayEntropy::of_u32s(&[0, 1, 7775, 7776]);
        assert_eq!(
            passphrase(&list, &mut entropy).unwrap().as_str(),
            "abacus-abdomen-zoom-abacus"
        );
        assert_eq!(
            passphrase(&list, &mut ReplayEntropy::of_u32s(&[0, 1])).unwrap_err(),
            EntropyError
        );
    }

    #[test]
    fn srv_ids_message_id_format() {
        let list = Wordlist::embedded().unwrap();
        let mut entropy = ReplayEntropy::of_u32s(&[0, 1, 2, 42]);
        assert_eq!(
            message_id(&list, &mut entropy, 3, 6).unwrap(),
            "abacus-abdomen-abdominal-000042"
        );
        let id = message_id(&list, &mut SystemEntropy, 3, 6).unwrap();
        let parts: Vec<_> = id.rsplitn(2, '-').collect();
        assert_eq!(parts[0].len(), 6);
        assert!(parts[0].bytes().all(|b| b.is_ascii_digit()));
    }
}
