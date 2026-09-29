use super::*;

#[test]
fn create_is_scoped_and_never_overwrites_an_existing_file() {
    let inside = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let scope = Scope::open(inside.path()).unwrap();
    let path = inside.path().join("new.txt");
    let file = scope.create(&path).unwrap();
    assert_eq!(file.read().unwrap().text.as_ref(), "");
    std::fs::write(&path, "keep").unwrap();
    assert!(scope.create(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
    assert!(scope.create(&outside.path().join("new.txt")).is_err());
}

#[test]
fn watchers_report_atomic_replacements_and_directory_creation() {
    use std::{
        collections::BTreeSet,
        sync::mpsc,
        time::{Duration, Instant},
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("watched.txt");
    std::fs::write(&path, "old").unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut watch = crate::watch::Watch::new(move || {
        let _ = sender.send(());
    })
    .unwrap();
    watch
        .set(BTreeSet::from([dir.path().to_path_buf()]))
        .unwrap();
    assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());
    let tmp = dir.path().join("replacement");
    std::fs::write(&tmp, "new").unwrap();
    std::fs::rename(tmp, &path).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut seen = false;
    while Instant::now() < deadline {
        receiver.recv_timeout(Duration::from_millis(100)).ok();
        let changes = watch.drain();
        if changes.paths.contains(&path) {
            seen = true;
            break;
        }
    }
    assert!(seen);
    watch.set(BTreeSet::new()).unwrap();
}

#[test]
fn saves_preserve_bom_crlf_permissions_and_refuse_external_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"\xef\xbb\xbfhello\r\n").unwrap();
    let scope = Scope::open(dir.path()).unwrap();
    let file = scope.bind(&path).unwrap();
    let original = file.read().unwrap();
    assert!(original.bom);
    assert_eq!(original.text.as_ref(), "hello\r\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    let saved = file.save(&original, "new\r\n").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"\xef\xbb\xbfnew\r\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    std::fs::write(&path, "external").unwrap();
    assert!(file.save(&saved, "local").is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "external");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn invalid_utf8_binary_and_deleted_files_cannot_be_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file");
    std::fs::write(&path, "text").unwrap();
    let file = Scope::open(dir.path()).unwrap().bind(&path).unwrap();
    let snapshot = file.read().unwrap();
    std::fs::write(&path, [255]).unwrap();
    assert!(file.read().is_err());
    std::fs::write(&path, [0]).unwrap();
    assert!(file.read().is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(file.save(&snapshot, "text").is_err());
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn paths_and_replaced_symlinks_cannot_escape_a_root() {
    let inside = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret");
    std::fs::write(&secret, "outside").unwrap();
    let link = inside.path().join("link");
    std::os::unix::fs::symlink(&secret, &link).unwrap();
    let scope = Scope::open(inside.path()).unwrap();
    assert!(scope.bind(&link).is_err());
    assert!(scope.bind(&secret).is_err());
    let path = inside.path().join("text");
    std::fs::write(&path, "inside").unwrap();
    let file = scope.bind(&path).unwrap();
    let snapshot = file.read().unwrap();
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&secret, &path).unwrap();
    assert!(file.save(&snapshot, "changed").is_err());
    assert_eq!(std::fs::read_to_string(secret).unwrap(), "outside");
}

#[test]
fn explorer_is_lazy_and_applies_ignore_rules() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("nested")).unwrap();
    std::fs::write(dir.path().join("nested/deep.txt"), "").unwrap();
    std::fs::write(dir.path().join("ignored"), "").unwrap();
    std::fs::write(dir.path().join(".gitignore"), "ignored\n").unwrap();
    let scope = Scope::open(dir.path()).unwrap();
    let listed = crate::explorer::list(&scope, dir.path(), false).unwrap();
    assert_eq!(listed.entries.len(), 2);
    assert!(listed.entries[0].directory);
    assert_eq!(
        listed.entries[0].path.as_os_str(),
        dir.path().join("nested").as_os_str()
    );
    assert!(!listed.entries.iter().any(|e| e.path.ends_with("deep.txt")));
    assert_eq!(
        crate::explorer::list(&scope, dir.path(), true)
            .unwrap()
            .entries
            .len(),
        3
    );
    let result = crate::explorer::search(&scope, "deep", false, &Default::default()).unwrap();
    assert_eq!(result.entries.len(), 1);
}
#[test]
fn oversized_files_have_a_bounded_preview_and_cannot_be_saved_as_that_preview() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.txt");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len((crate::MAX_FILE_BYTES + 100) as u64).unwrap();
    let scope = Scope::open(dir.path()).unwrap();
    let binding = scope.bind(&path).unwrap();
    assert!(binding.read().is_err());
    let preview = binding.preview().unwrap();
    assert!(preview.text.len() <= crate::MAX_WINDOW_BYTES);
    assert!(binding.save(&preview, "short").is_err());
    assert_eq!(
        std::fs::metadata(path).unwrap().len(),
        (crate::MAX_FILE_BYTES + 100) as u64
    );
}
