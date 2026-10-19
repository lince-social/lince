use std::{env, path::PathBuf, process::Command};

#[test]
#[ignore = "Requires the pinned ESP-IDF source for its actual cJSON implementation; executes isolated host storage fixtures"]
fn firmware_destination_updates_preserve_credentials_and_the_trusted_clock_floor() {
    let sdk = PathBuf::from(env::var_os("IDF_PATH").expect("Run in nix develop .#ailuros-rust"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("configuration-test");
    let status = Command::new("cc")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(root.join("tests/configuration_host/include"))
        .arg("-I")
        .arg(root.join("firmware/esp-idf/components/lince_core/include"))
        .arg("-I")
        .arg(sdk.join("components/json/cJSON"))
        .arg(root.join("tests/configuration_host/main.c"))
        .arg(root.join("firmware/esp-idf/components/lince_core/configuration.c"))
        .arg(sdk.join("components/json/cJSON/cJSON.c"))
        .args(["-lm", "-o"])
        .arg(&executable)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(Command::new(executable).status().unwrap().success());
}
