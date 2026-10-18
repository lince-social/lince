use super::*;
use lince_map::pack::{self, Format, InstallProgress, Manifest};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};

pub(super) fn manifest(root: &Path, format: Format, paths: &[&str], dataset: &str) -> Manifest {
    let value = Manifest {
        version: 1,
        format,
        schema: "shortbread-1.1".into(),
        dataset: dataset.into(),
        source: "Local test fixture".into(),
        date: "2026-10-17".into(),
        coverage: Bounds {
            west: -180.0,
            east: 180.0,
            south: -85.0,
            north: 85.0,
        },
        min_zoom: 0,
        max_zoom: 14,
        license: "CC0 test data".into(),
        credits: "Lince fixture".into(),
        files: paths
            .iter()
            .map(|path| {
                let bytes = std::fs::read(root.join(path)).unwrap();
                pack::File {
                    path: (*path).into(),
                    bytes: bytes.len() as u64,
                    expanded_bytes: bytes.len() as u64,
                    sha256: Sha256::digest(&bytes)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                }
            })
            .collect(),
    };
    value.validate().unwrap();
    std::fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    value
}

fn directory(root: &Path, dataset: &str) -> Manifest {
    std::fs::create_dir_all(root.join("4/2")).unwrap();
    std::fs::write(root.join("4/2/3.pbf"), fixture()).unwrap();
    manifest(root, Format::Directory, &["4/2/3.pbf"], dataset)
}

async fn install(
    root: &Path,
    managed: &Path,
    quota: u64,
    cancel: bool,
) -> Result<std::path::PathBuf, String> {
    pack::install(
        root.into(),
        managed.into(),
        quota,
        Arc::new(AtomicBool::new(cancel)),
        Arc::new(InstallProgress::default()),
    )
    .await
}

#[tokio::test]
async fn manifest_rejects_traversal_symlinks_unknown_files_schema_and_bombs_before_activation() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    let managed = temporary.path().join("managed/packs");
    let good = directory(&source, "good");
    for field in [
        "schema", "path", "size", "expanded", "hash", "zoom", "coverage",
    ] {
        let mut changed = good.clone();
        match field {
            "schema" => changed.schema = "unknown".into(),
            "path" => changed.files[0].path = "../../outside.pbf".into(),
            "size" => changed.files[0].bytes = u64::MAX,
            "expanded" => {
                changed.files[0].expanded_bytes = lince_map::source::MAX_DECODED_BYTES as u64 + 1
            }
            "hash" => changed.files[0].sha256 = "z".repeat(64),
            "zoom" => changed.max_zoom = 20,
            "coverage" => changed.coverage.north = f64::NAN,
            _ => unreachable!(),
        }
        assert!(changed.validate().is_err(), "{field}");
    }
    std::fs::write(source.join("unlisted"), "unexpected").unwrap();
    assert!(
        install(&source, &managed, 16 * 1024 * 1024, false)
            .await
            .is_err()
    );
    std::fs::remove_file(source.join("unlisted")).unwrap();
    #[cfg(unix)]
    {
        std::fs::remove_file(source.join("4/2/3.pbf")).unwrap();
        let outside = temporary.path().join("outside.pbf");
        std::fs::write(&outside, fixture()).unwrap();
        std::os::unix::fs::symlink(&outside, source.join("4/2/3.pbf")).unwrap();
        assert!(
            install(&source, &managed, 16 * 1024 * 1024, false)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(outside).unwrap(), fixture());
        std::fs::remove_file(source.join("4/2/3.pbf")).unwrap();
    }
    std::fs::write(source.join("4/2/3.pbf"), b"corrupt").unwrap();
    assert!(
        install(&source, &managed, 16 * 1024 * 1024, false)
            .await
            .is_err()
    );
    assert!(!temporary.path().join("outside.pbf").is_dir());
    assert!(std::fs::read_dir(&managed).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".install-")
    }));
}

#[tokio::test]
async fn replacement_cancel_copy_failure_quota_and_reopen_preserve_the_selected_pack() {
    let temporary = tempfile::tempdir().unwrap();
    let first = temporary.path().join("A");
    let second = temporary.path().join("B");
    let cache = temporary.path().join("maps");
    let managed = cache.join("packs");
    directory(&first, "A");
    directory(&second, "B");
    let installed = install(&first, &managed, 16 * 1024 * 1024, false)
        .await
        .unwrap();
    let settings_path = cache.join("settings.json");
    let a = Settings {
        local_directory: installed.clone(),
        ..Default::default()
    };
    a.save_atomic(&settings_path).unwrap();
    let previous = std::fs::read(&settings_path).unwrap();
    let reader_a = TileStore::new(a.clone(), &cache).unwrap();
    let tile = Tile { z: 4, x: 2, y: 3 };
    for cancel in [true, false] {
        assert!(
            install(
                &second,
                &managed,
                if cancel { 16 * 1024 * 1024 } else { 1 },
                cancel
            )
            .await
            .is_err()
        );
        assert_eq!(std::fs::read(&settings_path).unwrap(), previous);
        assert!(reader_a.load(tile).await.is_ok());
    }
    std::fs::remove_file(second.join("4/2/3.pbf")).unwrap();
    assert!(
        install(&second, &managed, 16 * 1024 * 1024, false)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&settings_path).unwrap(), previous);
    directory(&second, "B");
    let b_path = install(&second, &managed, 16 * 1024 * 1024, false)
        .await
        .unwrap();
    assert_ne!(installed, b_path);
    let b = Settings {
        local_directory: b_path,
        ..a.clone()
    };
    b.save_atomic(&settings_path).unwrap();
    let restored: Settings =
        serde_json::from_slice(&std::fs::read(settings_path).unwrap()).unwrap();
    assert_eq!(restored.source_id(), b.source_id());
    assert_ne!(restored.source_id(), a.source_id());
    assert!(
        TileStore::new(restored, &cache)
            .unwrap()
            .load(tile)
            .await
            .is_ok()
    );
    assert!(reader_a.load(tile).await.is_ok());
    assert!(std::fs::read_dir(managed).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".install-")
    }));
}

#[tokio::test]
async fn offline_defaults_and_same_url_dataset_changes_never_reuse_another_versions_tiles() {
    let temporary = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let (url, server) = server(fixture(), count.clone()).await;
    let defaults = Settings::default();
    assert!(!defaults.online);
    assert!(defaults.server.is_empty());
    let tile = Tile { z: 4, x: 2, y: 3 };
    let configured = Settings {
        server: url,
        dataset_version: "v1".into(),
        ..defaults
    };
    assert!(
        TileStore::new(configured.clone(), temporary.path())
            .unwrap()
            .load(tile)
            .await
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let online = Settings {
        online: true,
        ..configured.clone()
    };
    assert!(
        TileStore::new(online.clone(), temporary.path())
            .unwrap()
            .load(tile)
            .await
            .is_ok()
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(
        TileStore::new(configured.clone(), temporary.path())
            .unwrap()
            .load(tile)
            .await
            .is_ok()
    );
    let changed = Settings {
        dataset_version: "v2".into(),
        ..configured
    };
    assert_ne!(changed.source_id(), online.source_id());
    assert!(
        TileStore::new(changed, temporary.path())
            .unwrap()
            .load(tile)
            .await
            .is_err()
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    server.abort();
}
