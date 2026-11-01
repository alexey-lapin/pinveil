//! `send --edit` (APP.md, "Editor mode"): the text is written in `$VISUAL` or `$EDITOR`.
//!
//! The editor gets a file in a fresh private directory (owner-only, and on Windows inherited
//! by whatever the editor creates there), so swap and backup files the editor puts next to
//! the file are private too and are removed with the directory, on success and on error. The
//! file is read back by name, so editors that save by writing a new file and renaming it over
//! the old one work. A crash can still leave plaintext behind; the docs say so.

use std::path::{Path, PathBuf};
use std::process::Command;

use pinveil_fs::{Dir, create_private_dir};
use zeroize::Zeroizing;

use crate::error::CliError;

#[cfg(unix)]
const FALLBACK: &str = "vi";
#[cfg(windows)]
const FALLBACK: &str = "notepad";

const FILE_NAME: &str = "message.txt";

/// The editor command: `$VISUAL`, then `$EDITOR`, then the platform fallback.
fn editor_command() -> Result<Vec<String>, CliError> {
    let value = ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(std::env::var_os)
        .filter_map(|value| value.into_string().ok())
        .find(|value| !value.trim().is_empty())
        .unwrap_or_else(|| FALLBACK.to_owned());
    let words = split_command(&value)
        .ok_or_else(|| CliError::usage("$VISUAL or $EDITOR has an unclosed quote"))?;
    if words.is_empty() {
        return Err(CliError::usage("$VISUAL or $EDITOR is empty"));
    }
    Ok(words)
}

/// Splits an editor setting into the program and its arguments: words separated by
/// whitespace, where single or double quotes group a word that contains spaces, such as
/// `"C:\Program Files\Editor\editor.exe" --wait`. Backslashes are ordinary characters, so
/// Windows paths need no escaping. Returns `None` for an unclosed quote. No shell is involved:
/// the file path is passed as its own argument, never spliced into a command line.
fn split_command(value: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote = None;
    for c in value.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            (None, c) => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_word {
        words.push(word);
    }
    Some(words)
}

/// A private directory removed, with everything in it, when dropped.
struct Workspace {
    path: PathBuf,
}

impl Workspace {
    fn create() -> Result<Self, CliError> {
        let base = std::env::temp_dir();
        for attempt in 0..16u32 {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos());
            let path = base.join(format!(
                "pinveil-edit-{}-{nanos}-{attempt}",
                std::process::id()
            ));
            match create_private_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(CliError::usage(format!(
                        "cannot create a private directory for the editor: {e}"
                    )));
                }
            }
        }
        Err(CliError::usage(
            "cannot create a private directory for the editor",
        ))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Opens the editor on an empty private file and returns what was saved.
pub fn edit() -> Result<Zeroizing<String>, CliError> {
    let command = editor_command()?;
    let workspace = Workspace::create()?;
    let fail = |what: &str, e: &dyn std::fmt::Display| CliError::usage(format!("{what}: {e}"));
    let dir =
        Dir::open(&workspace.path).map_err(|e| fail("cannot open the editor directory", &e))?;
    dir.create_new(FILE_NAME, b"")
        .map_err(|e| fail("cannot create the editor file", &e))?;
    let status = Command::new(&command[0])
        .args(&command[1..])
        .arg(Path::new(&workspace.path).join(FILE_NAME))
        .status()
        .map_err(|e| fail(&format!("cannot start the editor {:?}", command[0]), &e))?;
    if !status.success() {
        return Err(CliError::usage(format!(
            "the editor exited with {status}; nothing was sent"
        )));
    }
    let bytes = Zeroizing::new(
        dir.read(FILE_NAME)
            .map_err(|e| fail("cannot read the edited text", &e))?,
    );
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| CliError::usage("the edited text is not valid UTF-8"))?;
    Ok(Zeroizing::new(text))
}

#[cfg(test)]
mod tests {
    use super::split_command;

    #[test]
    fn cli_editor_command_parsing() {
        let words = |s: &str| split_command(s).unwrap();
        assert_eq!(words("vim"), ["vim"]);
        assert_eq!(words("code --wait"), ["code", "--wait"]);
        assert_eq!(
            words(r#""C:\Program Files\Editor\ed.exe" --wait"#),
            [r"C:\Program Files\Editor\ed.exe", "--wait"]
        );
        assert_eq!(
            words("'/tmp/editor with spaces.sh' -n"),
            ["/tmp/editor with spaces.sh", "-n"]
        );
        assert_eq!(words(r#"a"b c"d"#), ["ab cd"]);
        assert_eq!(words("  "), Vec::<String>::new());
        assert!(split_command(r#""unclosed"#).is_none());
    }
}
