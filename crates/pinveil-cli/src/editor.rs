//! `send --edit` (APP.md, "Editor mode"): the text is written in `$VISUAL` or `$EDITOR`
//! through a unique, owner-only temp file that is deleted on success and on error. Editor
//! swap and backup files, and crashes, can still leave plaintext behind; the docs say so.

use std::io::Read;
use std::process::Command;

use zeroize::Zeroizing;

use crate::error::CliError;

#[cfg(unix)]
const FALLBACK: &str = "vi";
#[cfg(windows)]
const FALLBACK: &str = "notepad";

/// The editor command: `$VISUAL`, then `$EDITOR`, then the platform fallback. Split on
/// whitespace, so `code --wait` works.
fn editor_command() -> Vec<String> {
    ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(std::env::var_os)
        .filter_map(|value| value.into_string().ok())
        .find(|value| !value.trim().is_empty())
        .unwrap_or_else(|| FALLBACK.to_owned())
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Opens the editor on an empty temp file and returns what was saved.
pub fn edit() -> Result<Zeroizing<String>, CliError> {
    let fail = |what: &str, e: &dyn std::fmt::Display| CliError::usage(format!("{what}: {e}"));
    // Created owner-only on Unix; removed when `temp` drops, whatever happens below.
    let temp = tempfile::Builder::new()
        .prefix("pinveil-")
        .suffix(".txt")
        .tempfile()
        .map_err(|e| fail("cannot create a temp file for the editor", &e))?;
    let command = editor_command();
    let status = Command::new(&command[0])
        .args(&command[1..])
        .arg(temp.path())
        .status()
        .map_err(|e| fail(&format!("cannot start the editor {:?}", command[0]), &e))?;
    if !status.success() {
        return Err(CliError::usage(format!(
            "the editor exited with {status}; nothing was sent"
        )));
    }
    let mut bytes = Zeroizing::new(Vec::new());
    temp.reopen()
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|e| fail("cannot read the edited text", &e))?;
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| CliError::usage("the edited text is not valid UTF-8"))?;
    Ok(Zeroizing::new(text))
}
