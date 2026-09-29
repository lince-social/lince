use super::*;
use crate::{Buffer, Edit};

fn changed(value: &str) -> Buffer {
    let mut buffer = Buffer::new(value).unwrap();
    buffer
        .edit(Edit {
            range: 0..5,
            text: "local".into(),
        })
        .unwrap();
    buffer
}

#[test]
fn restart_preserves_unsaved_unicode_and_reconciles_new_disk_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file.txt");
    let buffer = changed("alpha\n猫\r\n");
    {
        let store = Store::open(&directory.path().join("recovery")).unwrap();
        store
            .write(&Draft {
                path: path.clone(),
                checkpoint: buffer.checkpoint(),
                bom: true,
                detached: false,
            })
            .unwrap();
    }
    let store = Store::open(&directory.path().join("recovery")).unwrap();
    let (drafts, errors) = store.load().unwrap();
    assert!(errors.is_empty());
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].path, path);
    assert!(drafts[0].bom);
    let mut restored = drafts[0].checkpoint.restore().unwrap();
    assert_eq!(restored.text(), "local\n猫\r\n");
    assert!(restored.is_dirty());
    let plan = restored
        .reconciliation()
        .compute("alpha\nexternal\r\n".into())
        .unwrap();
    assert!(restored.accept(plan).unwrap());
    assert_eq!(restored.text(), "local\nexternal\r\n");
    assert!(restored.conflict().is_none());
}

#[test]
fn unresolved_conflicts_survive_restarting_without_overwriting_local_text() {
    let mut buffer = changed("alpha\nbeta\n");
    let plan = buffer
        .reconciliation()
        .compute("external\nbeta\n".into())
        .unwrap();
    buffer.accept(plan).unwrap();
    let restored = buffer.checkpoint().restore().unwrap();
    assert_eq!(restored.text(), "local\nbeta\n");
    assert_eq!(restored.conflict().unwrap().disk, "external\nbeta\n");
    assert!(restored.prepare_save().is_err());
}

#[test]
fn corrupt_entries_are_retained_and_do_not_hide_valid_drafts() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let path = directory.path().join("file.txt");
    let draft = Draft {
        path: path.clone(),
        checkpoint: changed("alpha").checkpoint(),
        bom: false,
        detached: true,
    };
    store.write(&draft).unwrap();
    std::fs::write(directory.path().join("bad.draft"), b"invalid").unwrap();
    let (drafts, errors) = store.load().unwrap();
    assert_eq!(drafts.len(), 1);
    assert!(drafts[0].detached);
    assert_eq!(errors.len(), 1);
    assert!(directory.path().join("bad.draft").exists());
    store.remove(&path).unwrap();
    assert!(store.load().unwrap().0.is_empty());
}

#[test]
fn recovery_locks_out_a_second_writer_and_rejects_damaged_checksums() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    assert!(Store::open(directory.path()).is_err());
    let draft = Draft {
        path: directory.path().join("file.txt"),
        checkpoint: changed("alpha").checkpoint(),
        bom: false,
        detached: false,
    };
    let mut bytes = encode(&draft).unwrap();
    let n = bytes.len() - 1;
    bytes[n] ^= 1;
    assert!(decode(&bytes).is_err());
    drop(store);
    assert!(Store::open(directory.path()).is_ok());
}

#[cfg(unix)]
#[test]
fn checkpoints_are_private_and_symlinks_are_not_followed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let directory = tempfile::tempdir().unwrap();
    let recovery = directory.path().join("recovery");
    let store = Store::open(&recovery).unwrap();
    let path = directory.path().join("file.txt");
    store
        .write(&Draft {
            path: path.clone(),
            checkpoint: changed("alpha").checkpoint(),
            bom: false,
            detached: false,
        })
        .unwrap();
    assert_eq!(
        std::fs::metadata(&recovery).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(recovery.join(filename(&path).unwrap()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let outside = directory.path().join("private");
    std::fs::write(&outside, b"secret").unwrap();
    symlink(&outside, recovery.join("link.draft")).unwrap();
    assert_eq!(store.load().unwrap().1.len(), 1);
    assert_eq!(std::fs::read(outside).unwrap(), b"secret");
}
