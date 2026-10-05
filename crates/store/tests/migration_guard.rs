#[path = "../build_support/migration_guard.rs"]
mod migration_guard;

use sha2::{Digest, Sha384};
use std::fs;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("migrations")).unwrap();
    fs::write(root.path().join("migrations/0001_init.sql"), b"SELECT 1;\n").unwrap();
    fs::write(
        root.path().join("migrations.sha384"),
        entry("0001_init.sql", b"SELECT 1;\n"),
    )
    .unwrap();
    root
}

fn entry(filename: &str, bytes: &[u8]) -> String {
    let checksum: String = Sha384::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{checksum}  {filename}\n")
}

#[test]
fn current_repository_migrations_are_locked() {
    migration_guard::verify(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
}

#[test]
fn unchanged_migrations_pass() {
    migration_guard::verify(fixture().path()).unwrap();
}

#[test]
fn even_whitespace_edits_fail() {
    let root = fixture();
    fs::write(
        root.path().join("migrations/0001_init.sql"),
        b"SELECT 1;\n\n",
    )
    .unwrap();
    let error = migration_guard::verify(root.path()).unwrap_err();
    assert!(error.contains("0001_init.sql was modified"));
    assert!(error.contains("new numbered migration"));
}

#[test]
fn deleted_or_renamed_migrations_fail() {
    let root = fixture();
    let original = root.path().join("migrations/0001_init.sql");
    let renamed = root.path().join("migrations/0001_renamed.sql");
    fs::rename(&original, &renamed).unwrap();
    assert!(
        migration_guard::verify(root.path())
            .unwrap_err()
            .contains("cannot read locked migration")
    );
    fs::remove_file(renamed).unwrap();
    assert!(
        migration_guard::verify(root.path())
            .unwrap_err()
            .contains("cannot read locked migration")
    );
}

#[test]
fn new_migration_requires_appended_checksum() {
    let root = fixture();
    let bytes = b"SELECT 2;\n";
    fs::write(root.path().join("migrations/0002_next.sql"), bytes).unwrap();
    assert!(
        migration_guard::verify(root.path())
            .unwrap_err()
            .contains("0002_next.sql is not registered")
    );
    let lock = format!(
        "{}{}",
        entry("0001_init.sql", b"SELECT 1;\n"),
        entry("0002_next.sql", bytes)
    );
    fs::write(root.path().join("migrations.sha384"), lock).unwrap();
    migration_guard::verify(root.path()).unwrap();
}

#[test]
fn duplicate_or_out_of_order_versions_fail() {
    for filename in ["0001_duplicate.sql", "0000_earlier.sql"] {
        let root = fixture();
        fs::write(root.path().join("migrations").join(filename), b"SELECT 2;").unwrap();
        let lock = format!(
            "{}{}",
            entry("0001_init.sql", b"SELECT 1;\n"),
            entry(filename, b"SELECT 2;")
        );
        fs::write(root.path().join("migrations.sha384"), lock).unwrap();
        assert!(
            migration_guard::verify(root.path())
                .unwrap_err()
                .contains("strictly increasing")
        );
    }
}

#[test]
fn invalid_lock_entries_fail() {
    for lock in [
        String::new(),
        "invalid  0001_init.sql\n".to_owned(),
        entry("../0001_init.sql", b"SELECT 1;\n"),
    ] {
        let root = fixture();
        fs::write(root.path().join("migrations.sha384"), lock).unwrap();
        assert!(migration_guard::verify(root.path()).is_err());
    }
}
