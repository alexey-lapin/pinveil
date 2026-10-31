//! Windows: the directory handle is held without delete sharing, which stops the directory
//! and its ancestors from being renamed or replaced while it is open, and new files and
//! directories get a protected access list granting only the current user.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

pub struct Dir {
    path: PathBuf,
    /// Held for the life of the `Dir`; it is what pins the path.
    _pin: File,
}

impl Dir {
    pub fn open(path: &Path) -> io::Result<(Self, PathBuf)> {
        // Resolved once; every later operation uses this path while the handle pins it.
        let path = std::fs::canonicalize(path)?;
        let pin = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&path)?;
        if !pin.metadata()?.is_dir() {
            return Err(io::Error::other("not a directory"));
        }
        Ok((
            Self {
                path: path.clone(),
                _pin: pin,
            },
            path,
        ))
    }

    pub fn contains(&self, name: &str) -> io::Result<bool> {
        match std::fs::symlink_metadata(self.path.join(name)) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn is_dir(&self, name: &str) -> io::Result<bool> {
        match std::fs::symlink_metadata(self.path.join(name)) {
            Ok(meta) => Ok(meta.is_dir()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn create_private(&self, name: &str) -> io::Result<File> {
        acl::create_file(&self.path.join(name))
    }

    pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        // MoveFileEx with REPLACE_EXISTING: a link at `to` is replaced, never written through.
        std::fs::rename(self.path.join(from), self.path.join(to))
    }

    pub fn remove(&self, name: &str) -> io::Result<()> {
        std::fs::remove_file(self.path.join(name))
    }

    pub fn read(&self, name: &str) -> io::Result<Vec<u8>> {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(self.path.join(name))?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("not a regular file"));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

pub fn create_private_dir(path: &Path) -> io::Result<()> {
    acl::create_dir(path)
}

fn wide(path: &OsStr) -> Vec<u16> {
    path.encode_wide().chain(std::iter::once(0)).collect()
}

/// The Win32 calls, the only unsafe code in the workspace's shipped binaries. Each block is
/// a direct FFI call with locally owned, null-terminated buffers; every returned handle or
/// allocation is closed or freed on all paths.
#[allow(unsafe_code)]
mod acl {
    use std::fs::File;
    use std::io;
    use std::os::windows::io::FromRawHandle;
    use std::path::Path;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL,
        FILE_FLAG_OPEN_REPARSE_POINT,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    use super::wide;

    /// A security descriptor from `ConvertStringSecurityDescriptorToSecurityDescriptorW`,
    /// freed with `LocalFree`.
    struct Descriptor(PSECURITY_DESCRIPTOR);

    impl Drop for Descriptor {
        fn drop(&mut self) {
            // SAFETY: the pointer came from the conversion call and is freed exactly once.
            unsafe { LocalFree(self.0) };
        }
    }

    /// The current user's SID as a string, such as `S-1-5-21-...`.
    fn user_sid() -> io::Result<String> {
        let mut token: HANDLE = null_mut();
        // SAFETY: GetCurrentProcess returns a pseudo-handle; `token` is a valid out pointer.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // A u64 buffer keeps TOKEN_USER (pointer-aligned) correctly aligned.
        let mut buffer = vec![0u64; 64];
        let mut needed = 0u32;
        let size = u32::try_from(buffer.len() * 8).expect("small buffer");
        // SAFETY: the buffer is writable for `size` bytes; `needed` is a valid out pointer.
        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                size,
                &raw mut needed,
            )
        };
        let error = io::Error::last_os_error();
        // SAFETY: `token` is a handle this function opened.
        unsafe { CloseHandle(token) };
        if ok == 0 {
            return Err(error);
        }
        // SAFETY: on success the buffer starts with a TOKEN_USER whose SID points into it.
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut text: *mut u16 = null_mut();
        // SAFETY: `sid` is valid while `buffer` lives; `text` is a valid out pointer.
        if unsafe { ConvertSidToStringSidW(sid, &raw mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `text` is a null-terminated UTF-16 string allocated by the call above,
        // read up to its terminator and then freed once.
        let string = unsafe {
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let string = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text.cast());
            string
        };
        Ok(string)
    }

    /// A protected DACL with one entry: full access for the current user, inherited by files
    /// and directories created inside (OICI).
    fn private_descriptor() -> io::Result<Descriptor> {
        let sddl: Vec<u16> = format!("D:P(A;OICI;FA;;;{})", user_sid()?)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: `sddl` is null-terminated; `descriptor` is a valid out pointer.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Descriptor(descriptor))
    }

    fn attributes(descriptor: &Descriptor) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).expect("small struct"),
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        }
    }

    /// Creates a new file (never opening an existing one or a link) with the private DACL,
    /// open for writing and not shared.
    pub fn create_file(path: &Path) -> io::Result<File> {
        let descriptor = private_descriptor()?;
        let attributes = attributes(&descriptor);
        let path = wide(path.as_os_str());
        // SAFETY: `path` is null-terminated and `attributes` points at a live descriptor.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_WRITE,
                0,
                &raw const attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `handle` is a new, valid file handle owned by nothing else.
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    /// Creates a new directory with the private DACL.
    pub fn create_dir(path: &Path) -> io::Result<()> {
        let descriptor = private_descriptor()?;
        let attributes = attributes(&descriptor);
        let path = wide(path.as_os_str());
        // SAFETY: `path` is null-terminated and `attributes` points at a live descriptor.
        if unsafe { CreateDirectoryW(path.as_ptr(), &raw const attributes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
