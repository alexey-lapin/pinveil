//! The export-name policy (APP.md, "Export names"): turns names from a manifest into names
//! that are safe to create on any supported platform. The CLI writes files under these
//! names and the browser uses them for downloads and the zip; `web/static/export-names.js`
//! implements the same rules, and `tests/vectors/export-names.json` pins both.

use std::collections::HashSet;

use crate::protocol::MAX_NAME_BYTES;

const WINDOWS_INVALID: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
const WINDOWS_DEVICES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Maps each name to a safe, unique export name, in order. Names that differ only by case
/// count as duplicates, because Windows and macOS filesystems usually ignore case.
#[must_use]
pub fn export_names<S: AsRef<str>>(names: &[S]) -> Vec<String> {
    let mut used = HashSet::with_capacity(names.len());
    names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let safe = sanitize(name.as_ref(), index);
            let unique = disambiguate(&safe, &used);
            used.insert(unique.to_lowercase());
            unique
        })
        .collect()
}

fn sanitize(name: &str, index: usize) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if WINDOWS_INVALID.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut safe = trim_end(&replaced).to_owned();
    if safe.is_empty() {
        return format!("file-{}", index + 1);
    }
    let stem = safe
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    if WINDOWS_DEVICES.contains(&stem.to_lowercase().as_str()) {
        safe.insert(0, '_');
    }
    trim_end(truncate(&safe, MAX_NAME_BYTES)).to_owned()
}

fn disambiguate(name: &str, used: &HashSet<String>) -> String {
    if !used.contains(&name.to_lowercase()) {
        return name.to_owned();
    }
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    // At most `used.len() + 1` numbers can be taken, so this always finds one.
    let mut n = 2usize;
    loop {
        let suffix = format!(" ({n})");
        let room = MAX_NAME_BYTES.saturating_sub(suffix.len() + extension.len());
        let candidate = format!("{}{suffix}{extension}", truncate(stem, room));
        if !used.contains(&candidate.to_lowercase()) {
            return candidate;
        }
        n += 1;
    }
}

/// Windows drops trailing dots and spaces, so names must not end with them.
fn trim_end(name: &str) -> &str {
    name.trim_end_matches(['.', ' '])
}

fn truncate(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let end = (0..=max_bytes)
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0);
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::export_names;

    #[derive(serde::Deserialize)]
    struct Vectors {
        cases: Vec<Entry>,
    }

    #[derive(serde::Deserialize)]
    struct Entry {
        case: String,
        input: Vec<String>,
        output: Vec<String>,
    }

    #[test]
    fn cli_output_export_names_match_shared_vectors() {
        let vectors: Vectors =
            serde_json::from_str(include_str!("../../../tests/vectors/export-names.json")).unwrap();
        assert!(vectors.cases.len() >= 10);
        for case in vectors.cases {
            assert_eq!(export_names(&case.input), case.output, "{}", case.case);
        }
    }

    #[test]
    fn export_names_are_unique_short_and_safe() {
        let names = ["a", "A", "a", "a (2)", "CON", "con", "..", "", "x/y", "x_y"];
        let out = export_names(&names);
        let lowered: std::collections::HashSet<_> = out.iter().map(|n| n.to_lowercase()).collect();
        assert_eq!(lowered.len(), out.len(), "{out:?}");
        for name in &out {
            assert!(!name.is_empty() && name.len() <= 255);
            assert!(!name.contains(['/', '\\', ':']));
            assert!(!name.ends_with(['.', ' ']));
        }
    }
}
