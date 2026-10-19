use std::{env, path::PathBuf, process::Command};

#[test]
#[ignore = "Requires pinned ESP-IDF Mbed TLS sources; executes Linux host fixtures without accessing device NVS"]
fn firmware_identity_preserves_keys_and_refuses_corrupt_or_interrupted_storage() {
    let sdk = PathBuf::from(env::var_os("IDF_PATH").expect("Run in nix develop .#ailuros-rust"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crypto = sdk.join("components/mbedtls/mbedtls");
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("identity-test");
    let mut command = Command::new("cc");
    command
        .args([
            "-std=c11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pthread",
            "-DMBEDTLS_CONFIG_FILE=\"mbedtls_host.h\"",
        ])
        .arg("-I")
        .arg(root.join("tests/configuration_host/include"))
        .arg("-I")
        .arg(root.join("tests/identity_host/include"))
        .arg("-I")
        .arg(root.join("firmware/esp-idf/components/lince_core/include"))
        .arg("-I")
        .arg(sdk.join("components/json/cJSON"))
        .arg("-I")
        .arg(crypto.join("include"))
        .arg("-I")
        .arg(crypto.join("library"))
        .arg(root.join("tests/identity_host/main.c"))
        .arg(root.join("firmware/esp-idf/components/lince_core/identity.c"));
    for source in [
        "bignum",
        "bignum_core",
        "bignum_mod",
        "bignum_mod_raw",
        "ecp",
        "ecp_curves",
        "ecdsa",
        "sha256",
        "platform_util",
        "constant_time",
        "asn1parse",
        "asn1write",
    ] {
        command.arg(crypto.join("library").join(format!("{source}.c")));
    }
    assert!(
        command
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap()
            .success()
    );
    assert!(Command::new(executable).status().unwrap().success());
}
