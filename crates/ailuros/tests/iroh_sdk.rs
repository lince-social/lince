use lince_ailuros::{
    project::{self, FirmwareMode},
    serial::Cancellation,
};
use std::fs;
use std::path::PathBuf;

#[test]
#[ignore = "Requires nix develop .#ailuros-rust, ESP Rust installation and a full optimized SDK link; never flashes a device"]
fn exported_iroh_qualification_links_and_produces_guarded_images() {
    let directory = tempfile::tempdir().unwrap();
    let root = std::env::var_os("AILUROS_QUALIFICATION_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| directory.path().join("project"));
    project::export_for_board(&root, "goouuu-esp32s3-cam-v1.5").unwrap();
    let artifact = project::iroh::compile_qualification(&root, &Cancellation::default()).unwrap();
    assert_eq!(artifact.mode, FirmwareMode::IrohQualification);
    assert_eq!(artifact.flash_bytes, 16 * 1024 * 1024);
    let images = artifact.verify(&root).unwrap();
    assert_eq!(images.len(), 3);
    assert!(
        artifact
            .images
            .iter()
            .all(|image| [0, 0x8000, 0x10000].contains(&image.offset))
    );
    for name in [
        "iroh/src/lib.rs",
        "iroh/partitions.csv",
        "iroh/rust-toolchain.toml",
    ] {
        let path = root.join(name);
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.push(b'\n');
        fs::write(&path, changed).unwrap();
        assert!(artifact.verify(&root).is_err(), "{name}");
        fs::write(&path, original).unwrap();
    }
    artifact.verify(&root).unwrap();
    eprintln!(
        "Qualification images verified at {} · {}",
        root.display(),
        artifact.fingerprint().unwrap()
    );
}

#[test]
#[ignore = "Requires nix develop .#ailuros-rust and a full optimized SDK link; never flashes a device"]
fn exported_iroh_camera_links_and_preserves_the_enrollment_partition() {
    let directory = tempfile::tempdir().unwrap();
    let root = std::env::var_os("AILUROS_CAMERA_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| directory.path().join("project"));
    project::export_for_board(&root, "goouuu-esp32s3-cam-v1.5").unwrap();
    let artifact = project::iroh::compile_camera(&root, &Cancellation::default()).unwrap();
    assert_eq!(artifact.mode, FirmwareMode::IrohCamera);
    assert_eq!(artifact.flash_bytes, 16 * 1024 * 1024);
    assert_eq!(artifact.verify(&root).unwrap().len(), 3);
    assert!(
        artifact
            .images
            .iter()
            .all(|image| [0, 0x8000, 0x10000].contains(&image.offset))
    );
    assert!(
        artifact
            .images
            .iter()
            .any(|image| image.path == "iroh-camera.bin")
    );
    eprintln!(
        "Camera images verified at {} · {}",
        root.display(),
        artifact.fingerprint().unwrap()
    );
}
