//! Unix: every operation is relative to the directory's file descriptor.

use std::fs::File;
use std::io::{self, Read};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use rustix::fs::{
    AtFlags, CWD, FileType, Mode, OFlags, mkdirat, openat, renameat, statat, unlinkat,
};

pub struct Dir {
    fd: OwnedFd,
}

impl Dir {
    pub fn open(path: &Path) -> io::Result<(Self, PathBuf)> {
        // Following a link here is the documented "resolved once".
        let fd = openat(
            CWD,
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        Ok((Self { fd }, path.to_path_buf()))
    }

    pub fn contains(&self, name: &str) -> io::Result<bool> {
        match statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Ok(true),
            Err(rustix::io::Errno::NOENT) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn is_dir(&self, name: &str) -> io::Result<bool> {
        match statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => Ok(FileType::from_raw_mode(stat.st_mode) == FileType::Directory),
            Err(rustix::io::Errno::NOENT) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn create_private(&self, name: &str) -> io::Result<File> {
        let fd = openat(
            &self.fd,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?;
        Ok(File::from(fd))
    }

    pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        Ok(renameat(&self.fd, from, &self.fd, to)?)
    }

    pub fn remove(&self, name: &str) -> io::Result<()> {
        Ok(unlinkat(&self.fd, name, AtFlags::empty())?)
    }

    pub fn read(&self, name: &str) -> io::Result<Vec<u8>> {
        let fd = openat(
            &self.fd,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let mut bytes = Vec::new();
        File::from(fd).read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

pub fn create_private_dir(path: &Path) -> io::Result<()> {
    Ok(mkdirat(CWD, path, Mode::from_raw_mode(0o700))?)
}
