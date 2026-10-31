//! The EFF large wordlist, embedded and validated (APP.md invariant 12).

use std::collections::HashSet;

/// Number of entries in the EFF large wordlist.
pub const WORDLIST_LEN: usize = 7776;

const EMBEDDED: &str = include_str!("../../../assets/eff_large_wordlist.txt");

/// A validated wordlist: exactly [`WORDLIST_LEN`] distinct, well-formed words.
#[derive(Debug, Clone)]
pub struct Wordlist<'a> {
    words: Vec<&'a str>,
}

/// Why a wordlist was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WordlistError {
    #[error("wordlist has {0} entries, expected {WORDLIST_LEN}")]
    WrongCount(usize),
    #[error("wordlist line {0} is malformed")]
    MalformedLine(usize),
    #[error("wordlist line {0} repeats a dice code or word")]
    Duplicate(usize),
}

impl<'a> Wordlist<'a> {
    /// Parses and validates `dice<TAB>word` lines. A single trailing newline is allowed.
    pub fn parse(text: &'a str) -> Result<Self, WordlistError> {
        let body = text.strip_suffix('\n').unwrap_or(text);
        let mut words = Vec::with_capacity(WORDLIST_LEN);
        let mut seen_codes = HashSet::with_capacity(WORDLIST_LEN);
        let mut seen_words = HashSet::with_capacity(WORDLIST_LEN);
        for (index, line) in body.split('\n').enumerate() {
            let number = index + 1;
            let (code, word) = line
                .split_once('\t')
                .filter(|(code, word)| is_dice_code(code) && is_word(word))
                .ok_or(WordlistError::MalformedLine(number))?;
            if !seen_codes.insert(code) || !seen_words.insert(word) {
                return Err(WordlistError::Duplicate(number));
            }
            words.push(word);
        }
        if words.len() != WORDLIST_LEN {
            return Err(WordlistError::WrongCount(words.len()));
        }
        Ok(Self { words })
    }

    /// Number of words (always [`WORDLIST_LEN`]).
    #[must_use]
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Always `false`: a validated wordlist is never empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// The word at `index`, which must be below [`WORDLIST_LEN`].
    #[must_use]
    pub fn word(&self, index: usize) -> &'a str {
        self.words[index]
    }
}

impl Wordlist<'static> {
    /// The wordlist embedded in this binary, validated. Callers should validate once at
    /// startup and keep the result.
    pub fn embedded() -> Result<Self, WordlistError> {
        Self::parse(EMBEDDED)
    }
}

fn is_dice_code(code: &str) -> bool {
    code.len() == 5 && code.bytes().all(|b| (b'1'..=b'6').contains(&b))
}

/// Lowercase ASCII letters, optionally joined by single hyphens (e.g. `t-shirt`).
fn is_word(word: &str) -> bool {
    !word.is_empty()
        && word
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::{EMBEDDED, WORDLIST_LEN, Wordlist, WordlistError};

    fn replace_line(line: usize, with: &str) -> String {
        EMBEDDED
            .lines()
            .enumerate()
            .map(|(i, l)| if i + 1 == line { with } else { l })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn inv_12_embedded_list_is_valid() {
        let list = Wordlist::embedded().unwrap();
        assert_eq!(list.len(), WORDLIST_LEN);
        assert_eq!(list.word(0), "abacus");
        assert_eq!(list.word(WORDLIST_LEN - 1), "zoom");
    }

    #[test]
    fn inv_12_embedded_invalid_refused_short() {
        let short: String = EMBEDDED
            .lines()
            .take(WORDLIST_LEN - 1)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            Wordlist::parse(&short).unwrap_err(),
            WordlistError::WrongCount(WORDLIST_LEN - 1)
        );
        assert!(Wordlist::parse("").is_err());
        assert!(Wordlist::parse("11111\tabacus\n").is_err());
    }

    #[test]
    fn inv_12_embedded_invalid_refused_duplicates() {
        let dup_word = replace_line(2, "11112\tabacus");
        assert_eq!(
            Wordlist::parse(&dup_word).unwrap_err(),
            WordlistError::Duplicate(2)
        );
        let dup_code = replace_line(2, "11111\tabdomen");
        assert_eq!(
            Wordlist::parse(&dup_code).unwrap_err(),
            WordlistError::Duplicate(2)
        );
    }

    #[test]
    fn inv_12_embedded_invalid_refused_malformed() {
        for bad in [
            "11112 abdomen",    // space, not tab
            "11117\tabdomen",   // not a die face
            "1111\tabdomen",    // short code
            "11112\tAbdomen",   // uppercase
            "11112\tab-",       // dangling hyphen
            "11112\t",          // empty word
            "11112\tabdomen\r", // CRLF line ending
        ] {
            assert_eq!(
                Wordlist::parse(&replace_line(2, bad)).unwrap_err(),
                WordlistError::MalformedLine(2),
                "{bad:?}"
            );
        }
        let blank_line = format!("{EMBEDDED}\n");
        assert!(Wordlist::parse(&blank_line).is_err());
    }

    #[test]
    fn hyphenated_eff_words_are_accepted() {
        let list = Wordlist::embedded().unwrap();
        let words: Vec<_> = (0..list.len()).map(|i| list.word(i)).collect();
        for hyphenated in ["drop-down", "felt-tip", "t-shirt", "yo-yo"] {
            assert!(words.contains(&hyphenated));
        }
    }
}
