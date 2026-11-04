//! Message id templates (APP.md, "Message ids").
//!
//! An operator-written template such as `{word:3}-{digits:6}` or `msg_{alnum:8}` is parsed once
//! at startup into segments. Each placeholder reports its entropy, and the total is the sum.
//!
//! Summing is sound because a fixed template maps distinct draws to distinct ids. The wordlist
//! is prefix-free (enforced by [`Wordlist::parse`]), so words decode from the left. Literals
//! have known content, and digit and alphanumeric segments have fixed lengths. This holds even
//! for adjacent placeholders such as `{word:2}{word}` and for words containing a hyphen.

use std::fmt;

use crate::entropy::{Entropy, EntropyError, uniform_below};
use crate::generate::random_words;
use crate::wordlist::{WORDLIST_LEN, Wordlist};

/// The default template. It produces ids such as `barcode-unbalance-cloak-102856`.
pub const DEFAULT_TEMPLATE: &str = "{word:3}-{digits:6}";
/// Longest template text accepted, checked before parsing.
pub const MAX_TEMPLATE_BYTES: usize = 256;
/// Longest id a template may produce, checked against its worst case.
pub const MAX_ID_LEN: usize = 128;
/// Largest count a placeholder accepts.
pub const MAX_COUNT: u8 = 64;

const LOWER_ALNUM: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";
const MIXED_ALNUM: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Whether `b` may appear in a message id: the unreserved URL characters
/// `A-Z a-z 0-9 - . _ ~`. Template literals use the same set.
#[must_use]
pub fn is_id_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

/// Whether `id` has the shape any template can produce: nonempty, only [`is_id_byte`]
/// characters, and not a dot segment. Clients check this, never the server's current
/// template, so ids made under an older template stay valid.
#[must_use]
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id != "." && id != ".." && id.bytes().all(is_id_byte)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Words(u8),
    Digits(u8),
    Alnum { len: u8, upper: bool },
}

/// A validated id template. Only [`IdTemplate::parse`] builds one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdTemplate {
    source: String,
    segments: Vec<Segment>,
    max_len: usize,
}

/// Why a template was refused. `offset` is the byte offset in the template, when the problem
/// has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateError {
    pub offset: Option<usize>,
    pub reason: String,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(offset) => write!(f, "{} (at byte {offset})", self.reason),
            None => f.write_str(&self.reason),
        }
    }
}

impl std::error::Error for TemplateError {}

fn fail<T>(offset: Option<usize>, reason: impl Into<String>) -> Result<T, TemplateError> {
    Err(TemplateError {
        offset,
        reason: reason.into(),
    })
}

impl IdTemplate {
    /// Parses and validates `text`. `wordlist` sets the longest possible word, which bounds
    /// the worst-case id length.
    pub fn parse(text: &str, wordlist: &Wordlist<'_>) -> Result<Self, TemplateError> {
        if text.len() > MAX_TEMPLATE_BYTES {
            return fail(
                None,
                format!(
                    "the template is {} bytes; the limit is {MAX_TEMPLATE_BYTES}",
                    text.len()
                ),
            );
        }
        let bytes = text.as_bytes();
        let mut segments = Vec::new();
        let mut literal = String::new();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => {
                    let Some(close) = text[i + 1..].find('}') else {
                        return fail(Some(i), "this { is never closed");
                    };
                    let body_start = i + 1;
                    let body = &text[body_start..body_start + close];
                    if let Some(nested) = body.find('{') {
                        return fail(Some(body_start + nested), "placeholders can't be nested");
                    }
                    if !literal.is_empty() {
                        segments.push(Segment::Literal(std::mem::take(&mut literal)));
                    }
                    segments.push(placeholder(body, body_start)?);
                    i = body_start + close + 1;
                }
                b'}' => return fail(Some(i), "this } has no matching {"),
                b if is_id_byte(b) => {
                    literal.push(char::from(b));
                    i += 1;
                }
                _ => {
                    let c = text[i..].chars().next().expect("i is a char boundary");
                    return fail(
                        Some(i),
                        format!(
                            "{c:?} is not allowed; literal text may use only A-Z a-z 0-9 - . _ ~"
                        ),
                    );
                }
            }
        }
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }
        if !segments.iter().any(|s| !matches!(s, Segment::Literal(_))) {
            return fail(None, "the template needs at least one placeholder");
        }
        let longest_word = wordlist.max_word_len();
        let max_len = segments
            .iter()
            .map(|segment| match segment {
                Segment::Literal(text) => text.len(),
                Segment::Words(n) => usize::from(*n) * (longest_word + 1) - 1,
                Segment::Digits(n) | Segment::Alnum { len: n, .. } => usize::from(*n),
            })
            .sum();
        if max_len > MAX_ID_LEN {
            return fail(
                None,
                format!("the longest id would be {max_len} characters; the limit is {MAX_ID_LEN}"),
            );
        }
        Ok(Self {
            source: text.to_owned(),
            segments,
            max_len,
        })
    }

    /// The template text as given.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// The length of the longest id this template can produce.
    #[must_use]
    pub fn max_len(&self) -> usize {
        self.max_len
    }

    /// Bits of entropy in one id, from exact alphabet sizes.
    #[must_use]
    pub fn bits(&self) -> f64 {
        let word_bits = f64::from(u16::try_from(WORDLIST_LEN).expect("7776 fits u16")).log2();
        self.segments
            .iter()
            .map(|segment| match segment {
                Segment::Literal(_) => 0.0,
                Segment::Words(n) => f64::from(*n) * word_bits,
                Segment::Digits(n) => f64::from(*n) * 10f64.log2(),
                Segment::Alnum { len, upper: false } => f64::from(*len) * 36f64.log2(),
                Segment::Alnum { len, upper: true } => f64::from(*len) * 62f64.log2(),
            })
            .sum()
    }

    /// A random id. Every word and character is drawn uniformly from `entropy`.
    pub fn generate(
        &self,
        wordlist: &Wordlist<'_>,
        entropy: &mut impl Entropy,
    ) -> Result<String, EntropyError> {
        let mut id = String::with_capacity(self.max_len);
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => id.push_str(text),
                Segment::Words(n) => {
                    id.push_str(&random_words(wordlist, entropy, usize::from(*n))?.join("-"));
                }
                Segment::Digits(n) => push_chars(&mut id, entropy, *n, b"0123456789")?,
                Segment::Alnum { len, upper: false } => {
                    push_chars(&mut id, entropy, *len, LOWER_ALNUM)?;
                }
                Segment::Alnum { len, upper: true } => {
                    push_chars(&mut id, entropy, *len, MIXED_ALNUM)?;
                }
            }
        }
        Ok(id)
    }
}

fn push_chars(
    id: &mut String,
    entropy: &mut impl Entropy,
    count: u8,
    alphabet: &[u8],
) -> Result<(), EntropyError> {
    let bound = u32::try_from(alphabet.len()).expect("alphabets are small");
    for _ in 0..count {
        let index = uniform_below(entropy, bound)?;
        id.push(char::from(alphabet[index as usize]));
    }
    Ok(())
}

/// Parses a placeholder body such as `word`, `word:3`, or `alnum:8`. `start` is the body's
/// byte offset in the template.
fn placeholder(body: &str, start: usize) -> Result<Segment, TemplateError> {
    let (name, count) = match body.split_once(':') {
        Some((name, count)) => (name, Some(count)),
        None => (body, None),
    };
    let count_start = start + name.len() + 1;
    let count = match count {
        None => None,
        Some(text) => Some(parse_count(text, count_start)?),
    };
    let needs_count = |name: &str| {
        count.map_or_else(
            || {
                fail(
                    Some(start),
                    format!("{{{name}}} needs a count, such as {{{name}:8}}"),
                )
            },
            Ok,
        )
    };
    match name {
        "word" => Ok(Segment::Words(count.unwrap_or(1))),
        "digits" => Ok(Segment::Digits(needs_count(name)?)),
        "alnum" => Ok(Segment::Alnum {
            len: needs_count(name)?,
            upper: false,
        }),
        "Alnum" => Ok(Segment::Alnum {
            len: needs_count(name)?,
            upper: true,
        }),
        _ => fail(
            Some(start),
            format!("unknown placeholder {{{body}}}; use word, digits, alnum, or Alnum"),
        ),
    }
}

/// A count is one or two ASCII digits, with no sign and no leading zero, from 1 to 64.
fn parse_count(text: &str, offset: usize) -> Result<u8, TemplateError> {
    let well_formed = (1..=2).contains(&text.len())
        && text.bytes().all(|b| b.is_ascii_digit())
        && !text.starts_with('0');
    match text.parse::<u8>() {
        Ok(n) if well_formed && n <= MAX_COUNT => Ok(n),
        _ => fail(
            Some(offset),
            format!("the count {text:?} must be a number from 1 to {MAX_COUNT}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_TEMPLATE, IdTemplate, MAX_ID_LEN, MAX_TEMPLATE_BYTES, TemplateError, is_valid_id,
    };
    use crate::entropy::{EntropyError, ReplayEntropy, SystemEntropy};
    use crate::wordlist::Wordlist;

    type Allowed = fn(u8) -> bool;

    fn parse(text: &str) -> Result<IdTemplate, TemplateError> {
        IdTemplate::parse(text, &Wordlist::embedded().unwrap())
    }

    fn generate(text: &str, draws: &[u32]) -> String {
        parse(text)
            .unwrap()
            .generate(
                &Wordlist::embedded().unwrap(),
                &mut ReplayEntropy::of_u32s(draws),
            )
            .unwrap()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn srv_ids_default_template_keeps_the_old_format() {
        assert_eq!(
            generate(DEFAULT_TEMPLATE, &[0, 1, 2, 0, 0, 0, 0, 4, 2]),
            "abacus-abdomen-abdominal-000042"
        );
        let list = Wordlist::embedded().unwrap();
        let template = parse(DEFAULT_TEMPLATE).unwrap();
        for _ in 0..200 {
            let id = template.generate(&list, &mut SystemEntropy).unwrap();
            let (words, number) = id.rsplit_once('-').unwrap();
            assert_eq!(number.len(), 6);
            assert!(number.bytes().all(|b| b.is_ascii_digit()));
            assert!(words.split('-').count() >= 3);
            assert!(id.len() <= template.max_len());
        }
        assert!(close(
            template.bits(),
            3.0 * 7776f64.log2() + 6.0 * 10f64.log2()
        ));
        assert!((template.bits() - 58.706_006).abs() < 1e-6);
    }

    #[test]
    fn every_placeholder_and_literals() {
        assert_eq!(generate("msg_{alnum:3}", &[0, 35, 26]), "msg_a90");
        assert_eq!(generate("{Alnum:3}", &[0, 26, 61]), "Aa9");
        assert_eq!(generate("{digits:2}.~", &[9, 0]), "90.~");
        assert_eq!(generate("{word}", &[7775]), "zoom");
        assert_eq!(
            generate("{word:2}{word}", &[0, 1, 2]),
            "abacus-abdomenabdominal"
        );
        assert_eq!(generate("x{word}_{digits:1}", &[0, 3]), "xabacus_3");
    }

    #[test]
    fn hyphenated_words_generate_whole() {
        let list = Wordlist::embedded().unwrap();
        let index = (0..list.len())
            .find(|&i| list.word(i) == "t-shirt")
            .unwrap();
        let draw = u32::try_from(index).unwrap();
        assert_eq!(generate("{word:2}", &[draw, draw]), "t-shirt-t-shirt");
    }

    #[test]
    fn outputs_stay_in_their_alphabets() {
        let list = Wordlist::embedded().unwrap();
        let cases: [(&str, Allowed); 3] = [
            ("{digits:64}", |b| b.is_ascii_digit()),
            ("{alnum:64}", |b| {
                b.is_ascii_lowercase() || b.is_ascii_digit()
            }),
            ("{Alnum:64}", |b| b.is_ascii_alphanumeric()),
        ];
        for (text, allowed) in cases {
            let template = parse(text).unwrap();
            for _ in 0..50 {
                let id = template.generate(&list, &mut SystemEntropy).unwrap();
                assert_eq!(id.len(), 64);
                assert!(id.bytes().all(allowed), "{text}: {id}");
                assert!(is_valid_id(&id));
            }
        }
    }

    #[test]
    fn entropy_is_summed_from_exact_alphabet_sizes() {
        let t = parse("a{word:2}-{digits:3}-{alnum:4}-{Alnum:5}").unwrap();
        let expected =
            2.0 * 7776f64.log2() + 3.0 * 10f64.log2() + 4.0 * 36f64.log2() + 5.0 * 62f64.log2();
        assert!(close(t.bits(), expected));
        assert!(close(parse("lit{digits:1}").unwrap().bits(), 10f64.log2()));
    }

    #[test]
    fn worst_case_length_uses_the_longest_word() {
        let list = Wordlist::embedded().unwrap();
        assert_eq!(list.max_word_len(), 9);
        assert_eq!(parse("{word:3}").unwrap().max_len(), 29);
        assert_eq!(
            parse("ab{word}-{digits:6}").unwrap().max_len(),
            2 + 9 + 1 + 6
        );
        // 12 words: 12 * 10 - 1 = 119; 13 words: 129.
        assert!(parse("{word:12}").is_ok());
        let error = parse("{word:13}").unwrap_err();
        assert!(error.reason.contains("129"), "{error}");
        let at_limit = format!("{{alnum:64}}{{alnum:{}}}", MAX_ID_LEN - 64);
        assert_eq!(parse(&at_limit).unwrap().max_len(), MAX_ID_LEN);
        assert!(parse(&format!("x{at_limit}")).is_err());
    }

    #[test]
    fn template_input_is_capped_before_parsing() {
        let long = "{digits:1}".repeat(26);
        assert_eq!(long.len(), 260);
        let error = parse(&long).unwrap_err();
        assert_eq!(error.offset, None);
        assert!(error.reason.contains(&MAX_TEMPLATE_BYTES.to_string()));
    }

    #[test]
    fn malformed_templates_name_the_offset() {
        let cases: &[(&str, Option<usize>, &str)] = &[
            ("", None, "at least one placeholder"),
            ("abc", None, "at least one placeholder"),
            ("{digits}", Some(1), "needs a count"),
            ("{alnum}", Some(1), "needs a count"),
            ("{Alnum}", Some(1), "needs a count"),
            ("{words:3}", Some(1), "unknown placeholder"),
            ("{ALNUM:3}", Some(1), "unknown placeholder"),
            ("{}", Some(1), "unknown placeholder"),
            ("x{ word}", Some(2), "unknown placeholder"),
            ("{digits:0}", Some(8), "from 1 to 64"),
            ("{digits:65}", Some(8), "from 1 to 64"),
            ("{digits:07}", Some(8), "from 1 to 64"),
            ("{digits:+7}", Some(8), "from 1 to 64"),
            ("{digits:-7}", Some(8), "from 1 to 64"),
            ("{digits:100}", Some(8), "from 1 to 64"),
            ("{digits: 7}", Some(8), "from 1 to 64"),
            ("{digits:}", Some(8), "from 1 to 64"),
            ("{digits:3:4}", Some(8), "from 1 to 64"),
            ("{word:3", Some(0), "never closed"),
            ("ab}{word}", Some(2), "no matching {"),
            ("{word}}", Some(6), "no matching {"),
            ("{wo{rd}", Some(3), "nested"),
            ("a/{word}", Some(1), "not allowed"),
            ("{word} x", Some(6), "not allowed"),
            ("{word}%41", Some(6), "not allowed"),
            ("é{word}", Some(0), "not allowed"),
        ];
        for (text, offset, reason) in cases {
            let error = parse(text).unwrap_err();
            assert_eq!(error.offset, *offset, "{text:?}: {error}");
            assert!(error.reason.contains(reason), "{text:?}: {error}");
        }
    }

    #[test]
    fn entropy_failure_aborts_generation() {
        let list = Wordlist::embedded().unwrap();
        let template = parse("{alnum:4}").unwrap();
        assert_eq!(
            template
                .generate(&list, &mut ReplayEntropy::of_u32s(&[1, 2, 3]))
                .unwrap_err(),
            EntropyError
        );
    }

    #[test]
    fn valid_id_shape() {
        for good in ["a", "barcode-unbalance-cloak-102856", "Ticket_42.~", "A-Z"] {
            assert!(is_valid_id(good), "{good}");
        }
        for bad in ["", ".", "..", "a/b", "a b", "a%2Fb", "é", "a#b", "a?b"] {
            assert!(!is_valid_id(bad), "{bad}");
        }
    }
}
