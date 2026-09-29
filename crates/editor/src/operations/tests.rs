use super::*;

#[test]
fn folders_and_files_can_be_moved_without_overwriting_destinations() {
    let root = tempfile::tempdir().unwrap();
    let scope = Scope::open(root.path()).unwrap();
    scope.create_directory(&root.path().join("from")).unwrap();
    scope.create_directory(&root.path().join("to")).unwrap();
    std::fs::write(root.path().join("from/file.txt"), "keep").unwrap();
    let entry = scope.entry(&root.path().join("from/file.txt")).unwrap();
    scope
        .move_entry(&entry, &root.path().join("to/renamed.txt"))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("to/renamed.txt")).unwrap(),
        "keep"
    );
    assert!(!entry.path.exists());
    let folder = scope.entry(&root.path().join("to")).unwrap();
    assert!(
        scope
            .move_entry(&folder, &root.path().join("from"))
            .is_err()
    );
    assert!(
        scope
            .move_entry(&folder, &root.path().join("to/inside"))
            .is_err()
    );
    scope
        .move_entry(&folder, &root.path().join("moved"))
        .unwrap();
    assert!(root.path().join("moved/renamed.txt").exists());
}

#[test]
fn deletion_is_recoverable_and_undo_refuses_a_recreated_destination() {
    let root = tempfile::tempdir().unwrap();
    let scope = Scope::open(root.path()).unwrap();
    let path = root.path().join("folder");
    scope.create_directory(&path).unwrap();
    std::fs::write(path.join("file.txt"), "original").unwrap();
    let ticket = scope.trash(&scope.entry(&path).unwrap()).unwrap();
    assert!(
        crate::explorer::list(&scope, root.path(), false)
            .unwrap()
            .entries
            .iter()
            .all(|entry| entry.path.file_name().unwrap() != ".lince-trash")
    );
    assert!(
        crate::explorer::search(
            &scope,
            "file.txt",
            false,
            &std::sync::atomic::AtomicBool::new(false)
        )
        .unwrap()
        .entries
        .is_empty()
    );
    assert!(!path.exists());
    assert_eq!(
        std::fs::read_to_string(ticket.stored.join("file.txt")).unwrap(),
        "original"
    );
    scope.create_directory(&path).unwrap();
    assert!(scope.restore_trash(&ticket).is_err());
    std::fs::remove_dir(&path).unwrap();
    scope.restore_trash(&ticket).unwrap();
    assert_eq!(
        std::fs::read_to_string(path.join("file.txt")).unwrap(),
        "original"
    );
}

#[test]
fn mutations_refuse_root_and_parent_paths_and_replaced_entries() {
    let root = tempfile::tempdir().unwrap();
    let scope = Scope::open(root.path()).unwrap();
    assert!(scope.entry(root.path()).is_err());
    assert!(
        scope
            .create_directory(&root.path().join("../escape"))
            .is_err()
    );
    let path = root.path().join("file.txt");
    std::fs::write(&path, "original").unwrap();
    let entry = scope.entry(&path).unwrap();
    std::fs::rename(&path, root.path().join("original.txt")).unwrap();
    std::fs::write(&path, "replacement").unwrap();
    assert!(
        scope
            .move_entry(&entry, &root.path().join("moved.txt"))
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replacement");
}

#[cfg(unix)]
#[test]
fn symlink_targets_outside_the_scope_are_never_changed() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "keep").unwrap();
    symlink(outside.path(), root.path().join("link")).unwrap();
    let scope = Scope::open(root.path()).unwrap();
    assert!(scope.entry(&root.path().join("link/secret")).is_err());
    assert!(
        scope
            .create_directory(&root.path().join("link/new"))
            .is_err()
    );
    let ticket = scope
        .trash(&scope.entry(&root.path().join("link")).unwrap())
        .unwrap();
    assert!(!root.path().join("link").exists());
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret")).unwrap(),
        "keep"
    );
    scope.restore_trash(&ticket).unwrap();
    assert!(root.path().join("link").is_symlink());
}
