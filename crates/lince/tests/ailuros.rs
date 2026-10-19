use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
    time::Duration,
};

const FIRST_KEY: &str = "036b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296";
const SECOND_KEY: &str = "037cf27b188d034f7e8a52380304b51ac3c08969e277f21b35a60b48fc47669978";

fn command(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lince"))
        .arg("--directory")
        .arg(directory)
        .arg("ailuros")
        .args(arguments)
        .env("XDG_CONFIG_HOME", directory.join("config"))
        .output()
        .unwrap()
}

fn inspect(directory: &Path) -> Value {
    let output = command(directory, &["inspect"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn owner_cli_rejects_duplicate_enrollment_and_keeps_failed_camera_setup_disabled() {
    let directory = tempfile::tempdir().unwrap();
    let expiry = (nucleus::execution::now() + Duration::from_secs(86400)).to_rfc3339();
    let output = command(
        directory.path(),
        &["enroll", FIRST_KEY, "Camera", "camera", &expiry],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let first = inspect(directory.path());
    let rows = first["endpoints"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["enabled"], false);
    let signal = rows[0]["signal_uid"].as_str().unwrap().to_string();
    let duplicate = command(
        directory.path(),
        &["enroll", FIRST_KEY, "Duplicate", "another-camera", &expiry],
    );
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already enrolled"));
    assert_eq!(
        inspect(directory.path())["endpoints"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let failed = command(
        directory.path(),
        &["enroll", SECOND_KEY, "Another camera", "camera", &expiry],
    );
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("Configuration changed"));
    let after = inspect(directory.path());
    let rows = after["endpoints"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|row| row["enabled"] == false && row["signal_uid"] == signal)
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let store = store::Store::open(&format!(
            "sqlite://{}",
            directory.path().join("lince.db").display()
        ))
        .await
        .unwrap();
        let mut connection = store.pool.acquire().await.unwrap();
        let configuration = store::signal_streams::get_on(&mut connection, &signal)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(configuration.value_type, "jpeg");
        assert_eq!(configuration.revision, 1);
        drop(connection);
        store.pool.close().await;
    });
}

#[test]
fn owner_cli_refuses_an_active_cell_before_opening_its_database() {
    let directory = tempfile::tempdir().unwrap();
    let lock = lince_interface::activation::claim_headless(directory.path()).unwrap();
    let output = command(directory.path(), &["inspect"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Another Lince instance"));
    assert!(!directory.path().join("lince.db").exists());
    drop(lock);
}
