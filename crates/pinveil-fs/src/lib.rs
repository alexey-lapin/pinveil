//! Private, directory-anchored file operations for decrypted output and editor files
//! (APP.md, "File output" and "Editor mode").
//!
//! A [`Dir`] is opened once and pinned: on Unix every operation is relative to its file
//! descriptor (`openat`, `renameat`, `unlinkat`), and on Windows the directory handle is held
//! without delete sharing, so neither the directory nor any of its ancestors can be renamed or
//! replaced while it is open. A link given as the directory itself is resolved once, when it
//! is opened. Names inside it are never followed: new files are created exclusively, and a
//! replacement renames over the entry itself.
//!
//! Files and directories are created owner-only from the start: mode 0600/0700 on Unix, and
//! on Windows a protected access list that grants only the current user (inherited by
//! anything created inside a private directory).

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as sys;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as sys;

/// An opened, pinned directory.
pub struct Dir {
    path: PathBuf,
    inner: sys::Dir,
}

impl Dir {
    /// Opens an existing directory. A symlink naming the directory is resolved now and never
    /// again.
    pub fn open(path: &Path) -> io::Result<Self> {
        let (inner, path) = sys::Dir::open(path)?;
        Ok(Self { path, inner })
    }

    /// The directory as opened, for messages.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether `name` exists in the directory, as any kind of entry, links included.
    pub fn contains(&self, name: &str) -> io::Result<bool> {
        self.inner.contains(check(name)?)
    }

    /// Creates `name` with `data`. Fails if anything has that name, a link included. On a
    /// write failure the new file is removed.
    pub fn create_new(&self, name: &str, data: &[u8]) -> io::Result<()> {
        let name = check(name)?;
        let file = self.inner.create_private(name)?;
        let written = write_all(file, data);
        if written.is_err() {
            let _ = self.inner.remove(name);
        }
        written
    }

    /// Replaces whatever `name` is (a file or a link, not a directory) with a new private
    /// file holding `data`, through a temp file in the same directory and a rename.
    pub fn replace(&self, name: &str, data: &[u8]) -> io::Result<()> {
        let name = check(name)?;
        if self.inner.is_dir(name)? {
            return Err(io::Error::other("a directory has that name"));
        }
        let (temp, file) = loop {
            let candidate = temp_name();
            match self.inner.create_private(&candidate) {
                Ok(file) => break (candidate, file),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        };
        let result = write_all(file, data).and_then(|()| self.inner.rename(&temp, name));
        if result.is_err() {
            let _ = self.inner.remove(&temp);
        }
        result
    }

    /// Removes the file or link `name`.
    pub fn remove(&self, name: &str) -> io::Result<()> {
        self.inner.remove(check(name)?)
    }

    /// Reads the file `name`, whatever replaced it since it was created (an editor may save
    /// by writing a new file and renaming it over the old one). Links are not followed.
    pub fn read(&self, name: &str) -> io::Result<Vec<u8>> {
        self.inner.read(check(name)?)
    }
}

/// How many hard links name the file at `path` (links are followed). A file reachable by
/// several names can't be identified by any one of them.
pub fn link_count(path: &Path) -> io::Result<u64> {
    sys::link_count(path)
}

/// Creates a new private directory at `path` (owner-only; on Windows, inherited by what is
/// created inside). Fails if anything exists there.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    sys::create_private_dir(path)
}

/// Names are single components: the export-name policy already guarantees this, and the
/// check keeps every operation inside the directory.
fn check(name: &str) -> io::Result<&str> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0'])
        || (cfg!(windows) && name.contains(':'));
    if bad {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a plain file name",
        ))
    } else {
        Ok(name)
    }
}

fn write_all(mut file: File, data: &[u8]) -> io::Result<()> {
    file.write_all(data)?;
    file.sync_all()
}

/// A temp name unlikely to collide; collisions are retried by the caller anyway.
fn temp_name() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!(
        ".pinveil-{}-{}-{nanos}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests;
