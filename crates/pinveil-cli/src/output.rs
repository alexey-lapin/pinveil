//! Writing decrypted files (APP.md, "File output").
//!
//! Every destination is planned before anything is written, from names that already passed
//! the export-name policy. The destination directory is opened and pinned first
//! ([`pinveil_fs::Dir`]), so replacing it or an ancestor while the collision prompt waits
//! can't redirect a write. New files are created exclusively and owner-only; `--force` writes
//! a private temp file in the directory and renames it over the old entry, replacing a link
//! itself rather than writing through it. If a write fails, the files this run created are
//! removed.

use std::fs;
use std::path::{Path, PathBuf};

use pinveil_core::export_names::export_names;
use pinveil_core::payload::FileEntry;
use pinveil_fs::Dir;

use crate::error::{CliError, ExitClass};

/// The answer to "some files already exist".
pub enum Collision {
    Overwrite,
    Elsewhere(PathBuf),
    Quit,
}

fn output_error(message: impl Into<String>) -> CliError {
    CliError::new(ExitClass::Output, message)
}

/// Opens (creating it if needed) and pins the destination directory. A link naming the
/// directory itself is resolved here, once.
fn open_dir(path: &Path) -> Result<Dir, CliError> {
    fs::create_dir_all(path)
        .and_then(|()| Dir::open(path))
        .map_err(|e| output_error(format!("cannot use the directory {}: {e}", path.display())))
}

/// Saves `files` into `dir` and returns the paths written. `ask` decides what to do when
/// some names are taken and `force` is off; it gets the directory and the taken names.
pub fn save(
    files: &[FileEntry],
    dir: &Path,
    force: bool,
    mut ask: impl FnMut(&Path, &[String]) -> Result<Collision, CliError>,
) -> Result<Vec<PathBuf>, CliError> {
    let names = export_names(&files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>());
    let mut dir = open_dir(dir)?;
    let replace = loop {
        let mut taken = Vec::new();
        for name in &names {
            if dir
                .contains(name)
                .map_err(|e| output_error(format!("cannot check {name}: {e}")))?
            {
                taken.push(name.clone());
            }
        }
        if taken.is_empty() || force {
            break force;
        }
        match ask(dir.path(), &taken)? {
            Collision::Overwrite => break true,
            Collision::Elsewhere(other) => dir = open_dir(&other)?,
            Collision::Quit => {
                return Err(output_error(
                    "nothing was saved and the message is discarded. The server has already deleted it",
                ));
            }
        }
    };

    let mut created: Vec<&str> = Vec::new();
    let mut written = Vec::with_capacity(files.len());
    for (file, name) in files.iter().zip(&names) {
        let result = dir.contains(name).and_then(|existed| {
            if existed && replace {
                dir.replace(name, &file.data).map(|()| false)
            } else {
                dir.create_new(name, &file.data).map(|()| true)
            }
        });
        match result {
            Ok(new) => {
                if new {
                    created.push(name);
                }
                written.push(dir.path().join(name));
            }
            Err(error) => {
                for name in &created {
                    let _ = dir.remove(name);
                }
                return Err(output_error(format!(
                    "cannot write {}: {error}. Files created by this run were removed",
                    dir.path().join(name).display()
                )));
            }
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use pinveil_core::payload::FileEntry;

    use super::{Collision, save};
    use crate::error::ExitClass;

    fn file(name: &str, data: &[u8]) -> FileEntry {
        FileEntry {
            name: name.into(),
            mime_type: "text/plain".into(),
            data: data.to_vec(),
        }
    }

    fn never(_: &Path, _: &[String]) -> Result<Collision, crate::error::CliError> {
        panic!("no collision expected")
    }

    #[test]
    fn cli_output_writes_export_names_into_a_new_directory() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("new/sub");
        let files = [
            file("../evil.txt", b"a"),
            file("dup", b"1"),
            file("DUP", b"2"),
        ];
        let written = save(&files, &dir, false, never).unwrap();
        let names: Vec<_> = written
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_owned())
            .collect();
        assert_eq!(names, [".._evil.txt", "dup", "DUP (2)"]);
        assert!(written.iter().all(|p| p.parent() == Some(dir.as_path())));
        assert_eq!(fs::read(dir.join("DUP (2)")).unwrap(), b"2");
        assert!(!root.path().join("new/evil.txt").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join("dup")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "owner-only");
        }
    }

    #[test]
    fn cli_output_collision_prompt_choices() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"old").unwrap();
        let files = [file("a.txt", b"new"), file("b.txt", b"b")];

        // Quit keeps everything as it was.
        let error = save(&files, dir.path(), false, |_, taken| {
            assert_eq!(taken, ["a.txt"]);
            Ok(Collision::Quit)
        })
        .unwrap_err();
        assert_eq!(error.class, ExitClass::Output);
        assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"old");
        assert!(!dir.path().join("b.txt").exists());

        // Another directory gets a fresh plan.
        let other = dir.path().join("other");
        let written = save(&files, dir.path(), false, |_, _| {
            Ok(Collision::Elsewhere(other.clone()))
        })
        .unwrap();
        assert!(written.iter().all(|p| p.starts_with(&other)));

        // Overwrite replaces.
        save(&files, dir.path(), false, |_, _| Ok(Collision::Overwrite)).unwrap();
        assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn cli_output_never_writes_through_links() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.txt");
        fs::write(&target, b"keep").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("a.txt")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("missing"), dir.path().join("b.txt")).unwrap();

        // Without --force, both links count as taken, even the dangling one.
        let error = save(&[file("b.txt", b"x")], dir.path(), false, |_, taken| {
            assert_eq!(taken, ["b.txt"]);
            Ok(Collision::Quit)
        })
        .unwrap_err();
        assert_eq!(error.class, ExitClass::Output);
        assert!(!dir.path().join("missing").exists());

        // --force replaces the link itself; the file it pointed to is untouched.
        save(&[file("a.txt", b"new")], dir.path(), true, never).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"keep");
        let replaced = fs::symlink_metadata(dir.path().join("a.txt")).unwrap();
        assert!(replaced.is_file());
        assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"new");
    }

    #[test]
    fn cli_output_partial_failure_removes_created_files() {
        let dir = tempfile::tempdir().unwrap();
        // The second name is taken by a directory, which even --force can't replace.
        fs::create_dir(dir.path().join("b")).unwrap();
        let error = save(
            &[file("a", b"1"), file("b", b"2"), file("c", b"3")],
            dir.path(),
            true,
            never,
        )
        .unwrap_err();
        assert_eq!(error.class, ExitClass::Output);
        assert!(!dir.path().join("a").exists(), "created file removed");
        assert!(!dir.path().join("c").exists());
        assert!(dir.path().join("b").is_dir());
        let leftovers: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no temp files left: {leftovers:?}");
    }

    /// Codex's reproduction: the directory is swapped for a link while the collision prompt
    /// waits. The approved overwrite must land in the directory that was inspected.
    #[cfg(unix)]
    #[test]
    fn cli_output_directory_swapped_during_the_prompt_is_not_followed() {
        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("selected");
        let held = root.path().join("held");
        let outside = root.path().join("outside");
        fs::create_dir_all(&selected).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(selected.join("a.txt"), b"original").unwrap();
        save(
            &[file("a.txt", b"decrypted secret")],
            &selected,
            false,
            |_, _| {
                fs::rename(&selected, &held).unwrap();
                std::os::unix::fs::symlink(&outside, &selected).unwrap();
                Ok(Collision::Overwrite)
            },
        )
        .unwrap();
        assert_eq!(fs::read(held.join("a.txt")).unwrap(), b"decrypted secret");
        assert!(!outside.join("a.txt").exists(), "the write was redirected");
    }
}
