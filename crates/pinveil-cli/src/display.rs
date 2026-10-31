//! Making decrypted text safe to print on a terminal (APP.md, `pinveil get`). The sender
//! controls the text, so on a terminal it could carry escape sequences that move the cursor,
//! rewrite earlier lines, set the window title, or hide text with bidirectional overrides.
//! Pipes and files get the bytes unchanged; this applies only when stdout is a terminal.

use std::fmt::Write;

/// Replaces every control character except newline and tab with a visible escape: C0 and DEL
/// in caret notation (`^[`, `^?`), and C1 controls and bidirectional formatting characters
/// as `<U+XXXX>`. A carriage return is kept only as part of a CRLF line ending.
#[must_use]
pub fn neutralize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' | '\t' => out.push(c),
            '\r' if chars.peek() == Some(&'\n') => out.push(c),
            '\u{0}'..='\u{1f}' => {
                out.push('^');
                out.push(char::from(
                    b'@' + u8::try_from(u32::from(c)).expect("C0 is one byte"),
                ));
            }
            '\u{7f}' => out.push_str("^?"),
            '\u{80}'..='\u{9f}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}' => {
                let _ = write!(out, "<U+{:04X}>", u32::from(c));
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::neutralize;

    #[test]
    fn cli_output_terminal_control_sequences_neutralized() {
        assert_eq!(
            neutralize("plain\ttext\nline two\r\n"),
            "plain\ttext\nline two\r\n"
        );
        assert_eq!(
            neutralize("\u{1b}[2J\u{1b}]0;title\u{7}"),
            "^[[2J^[]0;title^G"
        );
        assert_eq!(neutralize("over\rwrite"), "over^Mwrite");
        assert_eq!(neutralize("\u{7f}\u{0}"), "^?^@");
        assert_eq!(neutralize("csi\u{9b}2J"), "csi<U+009B>2J");
        assert_eq!(neutralize("abc\u{202e}fed"), "abc<U+202E>fed");
        assert_eq!(neutralize("ünïcødé 🗝"), "ünïcødé 🗝");
    }
}
