use std::fs;

use super::{Dir, create_private_dir};

#[test]
fn create_new_refuses_existing_entries_and_links() {
    let root = tempfile::tempdir().unwrap();
    let dir = Dir::open(root.path()).unwrap();
    dir.create_new("a.txt", b"one").unwrap();
    assert_eq!(fs::read(root.path().join("a.txt")).unwrap(), b"one");
    let again = dir.create_new("a.txt", b"two").unwrap_err();
    assert_eq!(again.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(dir.contains("a.txt").unwrap());
    assert!(!dir.contains("b.txt").unwrap());
    for bad in ["", ".", "..", "x/y", "x\\y", "nul\0"] {
        assert!(dir.create_new(bad, b"").is_err(), "{bad:?}");
    }
}

#[cfg(unix)]
#[test]
fn unix_files_are_owner_only_and_links_are_never_followed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let dir = Dir::open(root.path()).unwrap();
    dir.create_new("private", b"x").unwrap();
    let mode = fs::metadata(root.path().join("private"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    let target = root.path().join("target");
    fs::write(&target, b"keep").unwrap();
    symlink(&target, root.path().join("link")).unwrap();
    symlink(root.path().join("missing"), root.path().join("dangling")).unwrap();
    assert!(dir.create_new("dangling", b"x").is_err());
    assert!(!root.path().join("missing").exists());
    assert!(dir.read("link").is_err(), "read never follows a link");

    dir.replace("link", b"new").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"keep");
    assert!(
        fs::symlink_metadata(root.path().join("link"))
            .unwrap()
            .is_file()
    );
    let mode = fs::metadata(root.path().join("link"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn replace_refuses_directories_and_leaves_no_temp_files() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("sub")).unwrap();
    fs::write(root.path().join("old"), b"old").unwrap();
    let dir = Dir::open(root.path()).unwrap();
    assert!(dir.replace("sub", b"x").is_err());
    dir.replace("old", b"new").unwrap();
    assert_eq!(fs::read(root.path().join("old")).unwrap(), b"new");
    let names: Vec<_> = fs::read_dir(root.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn read_sees_a_file_replaced_by_rename() {
    let root = tempfile::tempdir().unwrap();
    let dir = Dir::open(root.path()).unwrap();
    dir.create_new("edit.txt", b"").unwrap();
    // What an editor that saves by replacement does.
    fs::write(root.path().join("edit.txt.new"), b"saved").unwrap();
    fs::rename(
        root.path().join("edit.txt.new"),
        root.path().join("edit.txt"),
    )
    .unwrap();
    assert_eq!(dir.read("edit.txt").unwrap(), b"saved");
}

#[cfg(unix)]
#[test]
fn unix_a_substituted_directory_does_not_redirect_writes() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let selected = root.path().join("selected");
    let held = root.path().join("held");
    let outside = root.path().join("outside");
    fs::create_dir(&selected).unwrap();
    fs::create_dir(&outside).unwrap();
    let dir = Dir::open(&selected).unwrap();
    // Swap the directory for a link while it is open.
    fs::rename(&selected, &held).unwrap();
    symlink(&outside, &selected).unwrap();
    dir.create_new("a.txt", b"secret").unwrap();
    dir.replace("b.txt", b"secret").unwrap();
    assert_eq!(fs::read(held.join("a.txt")).unwrap(), b"secret");
    assert_eq!(fs::read(held.join("b.txt")).unwrap(), b"secret");
    assert_eq!(
        fs::read_dir(&outside).unwrap().count(),
        0,
        "nothing leaked outside"
    );

    let private = root.path().join("private");
    create_private_dir(&private).unwrap();
    let mode = fs::metadata(&private).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[cfg(windows)]
#[test]
fn windows_an_open_directory_and_its_parent_cannot_be_renamed() {
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("parent");
    let selected = parent.join("selected");
    fs::create_dir_all(&selected).unwrap();
    let dir = Dir::open(&selected).unwrap();
    assert!(fs::rename(&selected, root.path().join("moved")).is_err());
    assert!(fs::rename(&parent, root.path().join("moved-parent")).is_err());
    dir.create_new("a.txt", b"x").unwrap();
    assert!(selected.join("a.txt").exists());
}

/// The access-list entries `icacls` prints for `path`, one per line.
#[cfg(windows)]
fn access_entries(path: &std::path::Path) -> Vec<String> {
    let out = std::process::Command::new("icacls")
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let path_text = path.display().to_string();
    text.lines()
        .map(|line| line.replace(&path_text, ""))
        .map(|line| line.trim().to_owned())
        .filter(|line| line.contains(":("))
        .collect()
}

#[cfg(windows)]
#[test]
fn windows_files_and_dirs_are_owner_only_even_under_a_permissive_parent() {
    let root = tempfile::tempdir().unwrap();
    // Everyone may read and write here, and that is inherited.
    let status = std::process::Command::new("icacls")
        .arg(root.path())
        .args(["/grant", "*S-1-1-0:(OI)(CI)F"])
        .status()
        .unwrap();
    assert!(status.success());
    fs::write(root.path().join("ordinary"), b"x").unwrap();
    assert!(
        access_entries(&root.path().join("ordinary")).len() > 1,
        "the parent's grant is inherited by ordinary files"
    );

    let dir = Dir::open(root.path()).unwrap();
    dir.create_new("private", b"x").unwrap();
    dir.replace("ordinary", b"y").unwrap();
    let private_dir = root.path().join("private-dir");
    create_private_dir(&private_dir).unwrap();
    fs::write(private_dir.join("inside"), b"z").unwrap();
    for path in [
        root.path().join("private"),
        root.path().join("ordinary"),
        private_dir.clone(),
        private_dir.join("inside"),
    ] {
        let entries = access_entries(&path);
        assert_eq!(entries.len(), 1, "{}: {entries:?}", path.display());
        assert!(!entries[0].contains("Everyone"), "{entries:?}");
    }
}
