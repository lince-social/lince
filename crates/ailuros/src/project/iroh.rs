use super::{Result, run};
use crate::serial::Cancellation;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const RUST_TOOLCHAIN: &str = "esp-1.93.0.0";
pub const RUST_VERSION: &str = "1.93.0.0";
pub const RUST_TARGET: &str = "xtensa-esp32s3-espidf";

fn configuration_digest(root: &Path) -> Result<String> {
    super::source_digest_matching(root, |path| {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        path == Path::new("board.json")
            || (path.starts_with("iroh")
                && (name.starts_with("sdkconfig.defaults")
                    || name.starts_with("sdkconfig.input")
                    || matches!(
                        name,
                        "Cargo.toml"
                            | "Cargo.lock"
                            | "rust-toolchain.toml"
                            | "config.toml"
                            | "build.rs"
                            | "bindings.h"
                            | "partitions.csv"
                            | "components_esp32s3.lock"
                            | "xtensa-esp32s3-espidf.json"
                    )))
            || path == Path::new("protocol/Cargo.toml")
            || (path.starts_with("components")
                && matches!(
                    name,
                    "Kconfig" | "Kconfig.projbuild" | "CMakeLists.txt" | "idf_component.yml"
                ))
    })
}

fn generated_sdk(release: &Path) -> Result<PathBuf> {
    let mut configurations = Vec::new();
    for entry in fs::read_dir(release.join("build")).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with("esp-idf-sys-"))
        {
            let configuration = entry.path().join("out/sdkconfig");
            if configuration.is_file() {
                configurations.push(configuration);
            }
        }
    }
    if configurations.len() != 1 {
        return Err("Cannot identify the generated ESP-IDF SDK configuration uniquely".into());
    }
    Ok(configurations.remove(0))
}

pub(super) fn source_files() -> Vec<(&'static str, &'static str)> {
    macro_rules! source {
        ($path:literal) => {
            (
                concat!("iroh/", $path),
                include_str!(concat!("../../firmware/iroh/", $path)),
            )
        };
    }
    vec![
        source!("Cargo.toml"),
        source!("Cargo.lock"),
        source!(".cargo/config.toml"),
        source!(".gitignore"),
        source!("rust-toolchain.toml"),
        source!("targets/xtensa-esp32s3-espidf.json"),
        source!("build.rs"),
        source!("bindings.h"),
        source!("components_esp32s3.lock"),
        source!("partitions.csv"),
        source!("sdkconfig.defaults"),
        source!("src/lib.rs"),
        source!("src/network.rs"),
        source!("src/client.rs"),
        source!("src/power.rs"),
        source!("src/readiness.rs"),
        source!("src/device.rs"),
        source!("src/device/camera.rs"),
        source!("src/verification.rs"),
        source!("src/verification/cache.rs"),
        source!("fixtures/ecdsa.json"),
        source!("src/bin/transport_check.rs"),
        source!("src/bin/camera.rs"),
        source!("tests/certificate_verification.rs"),
        source!("tests/camera_client.rs"),
        source!("tests/public_relay.rs"),
        source!("examples/device_check.rs"),
        source!("CREDITS.txt"),
        source!("resources.json"),
        source!("licenses/iroh-MIT.txt"),
        source!("licenses/iroh-BSD3.txt"),
        source!("licenses/rusty-esp-iroh-MIT.txt"),
        source!("licenses/rustls-MIT.txt"),
        source!("licenses/rustls-rustcrypto-MIT.txt"),
    ]
}

pub fn compile_qualification(root: &Path, cancellation: &Cancellation) -> Result<super::Artifact> {
    compile(root, cancellation, super::FirmwareMode::IrohQualification)
}

pub fn compile_camera(root: &Path, cancellation: &Cancellation) -> Result<super::Artifact> {
    compile(root, cancellation, super::FirmwareMode::IrohCamera)
}

fn compile(
    root: &Path,
    cancellation: &Cancellation,
    mode: super::FirmwareMode,
) -> Result<super::Artifact> {
    let (feature, binary, image_name) = match mode {
        super::FirmwareMode::IrohQualification => {
            ("device-check", "transport-check", "iroh-qualification.bin")
        }
        super::FirmwareMode::IrohCamera => ("camera", "camera", "iroh-camera.bin"),
        _ => return Err("Choose an iroh firmware runtime".into()),
    };
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let package: super::Package = serde_json::from_slice(&super::regular_bytes(
        &root.join("lince-firmware.json"),
        16384,
    )?)
    .map_err(|_| "Invalid firmware package manifest")?;
    package.validate()?;
    let selected = super::board(&root)?;
    let qualified = crate::profile::builtin("goouuu-esp32s3-cam-v1.5")?;
    if serde_json::to_value(selected).map_err(|error| error.to_string())?
        != serde_json::to_value(qualified).map_err(|error| error.to_string())?
    {
        return Err(
            "Iroh firmware currently requires the verified GOOUUU ESP32-S3-CAM V1.5 profile".into(),
        );
    }
    let frontend = root.join("iroh");
    let target: serde_json::Value = serde_json::from_slice(&super::regular_bytes(
        &frontend.join("targets/xtensa-esp32s3-espidf.json"),
        16384,
    )?)
    .map_err(|_| "Invalid Rust firmware target")?;
    if target["arch"] != "xtensa"
        || target["cpu"] != "esp32s3"
        || target["os"] != "espidf"
        || target["features"] != "-s32c1i,-forced-atomics"
        || target["max-atomic-width"] != 32
    {
        return Err(
            "Rust firmware requires the PSRAM-safe ESP32-S3 target and SDK software atomics".into(),
        );
    }
    super::regular_bytes(&frontend.join("Cargo.toml"), 65536)?;
    let toolchain: toml::Value = toml::from_str(
        std::str::from_utf8(&super::regular_bytes(
            &frontend.join("rust-toolchain.toml"),
            8192,
        )?)
        .map_err(|_| "Invalid Rust firmware toolchain file")?,
    )
    .map_err(|_| "Invalid Rust firmware toolchain file")?;
    if toolchain
        .get("toolchain")
        .and_then(|value| value.get("channel"))
        .and_then(toml::Value::as_str)
        != Some(RUST_TOOLCHAIN)
    {
        return Err("Rust firmware must select the qualified ESP Rust 1.93.0.0 toolchain before compilation".into());
    }
    let mut version = Command::new("idf.py");
    version.arg("--version");
    if !run(version, cancellation)?
        .trim()
        .ends_with(&format!("v{}", super::SDK))
    {
        return Err(
            "Run Rust firmware compilation inside nix develop .#ailuros-rust with ESP-IDF 5.5.2"
                .into(),
        );
    }
    let before = super::source_digest(&root)?;
    let build = root.join("build");
    let prepared = prepare(&build.join("toolchain"), cancellation)?;
    let target = build.join(format!("rust-{}", configuration_digest(&root)?));
    let mut compile = Command::new("rustup");
    compile
        .args([
            "run",
            RUST_TOOLCHAIN,
            "cargo",
            "rustc",
            "--locked",
            "--release",
            "--target",
            "targets/xtensa-esp32s3-espidf.json",
            "--features",
            feature,
            "--bin",
            binary,
            "--target-dir",
        ])
        .arg(&target)
        .current_dir(&frontend)
        .env_remove("CC")
        .env_remove("CXX")
        .env("LIBCLANG_PATH", prepared.libclang)
        .env(
            "RUSTFLAGS",
            "--cfg espidf_time64 -C no-vectorize-loops -C no-vectorize-slp",
        )
        .env("ESP_IDF_TOOLS_INSTALL_DIR", "fromenv")
        .env("CARGO_WORKSPACE_DIR", &frontend)
        .env("ESP_IDF_SDKCONFIG", frontend.join("sdkconfig.input"))
        .env(
            "ESP_IDF_SDKCONFIG_DEFAULTS",
            frontend.join("sdkconfig.defaults"),
        );
    run(compile, cancellation)?;
    let release = target.join(RUST_TARGET).join("release");
    let configuration = String::from_utf8(super::regular_bytes(
        &generated_sdk(&release)?,
        1024 * 1024,
    )?)
    .map_err(|_| "Invalid Rust firmware SDK configuration")?;
    for expected in [
        "CONFIG_ESPTOOLPY_FLASHMODE=\"dio\"",
        "CONFIG_ESPTOOLPY_FLASHFREQ=\"80m\"",
        "CONFIG_ESPTOOLPY_FLASHSIZE=\"16MB\"",
        "CONFIG_SPIRAM=y",
        "CONFIG_SPIRAM_MODE_OCT=y",
        "CONFIG_SPIRAM_SPEED_80M=y",
        "CONFIG_LINCE_CAMERA_XCLK_HZ=8000000",
        "CONFIG_STDATOMIC_S32C1I_SPIRAM_WORKAROUND=y",
        "CONFIG_MBEDTLS_ECP_FIXED_POINT_OPTIM=y",
    ] {
        if !configuration.lines().any(|line| line == expected) {
            return Err("Rust firmware SDK settings differ from the qualification board; review its defaults and compile again".into());
        }
    }
    let application = build.join(image_name);
    let mut image = Command::new("python");
    image
        .args([
            "-m",
            "esptool",
            "--chip",
            "esp32s3",
            "elf2image",
            "--flash_mode",
            "dio",
            "--flash_freq",
            "80m",
            "--flash_size",
            "16MB",
            "--output",
        ])
        .arg(&application)
        .arg(release.join(binary));
    run(image, cancellation)?;
    let mut images = Vec::new();
    for (offset, source, name) in [
        (0, release.join("bootloader.bin"), "iroh-bootloader.bin"),
        (
            0x8000,
            release.join("partition-table.bin"),
            "iroh-partition-table.bin",
        ),
        (0x10000, application, image_name),
    ] {
        let bytes = super::regular_bytes(&source, 16 * 1024 * 1024)?;
        if source != build.join(name) {
            fs::write(build.join(name), &bytes).map_err(|error| error.to_string())?;
        }
        images.push(super::Image {
            offset,
            path: name.into(),
            sha384: super::digest(&bytes),
        });
    }
    if before != super::source_digest(&root)? {
        return Err("Firmware sources changed during Rust compilation; compile again".into());
    }
    super::finish_artifact(&root, mode, 16 * 1024 * 1024, images, &before)
}

pub struct PreparedToolchain {
    pub compiler: PathBuf,
    pub libclang: PathBuf,
    pub export: PathBuf,
}

fn libclang(compiler: &Path) -> Result<PathBuf> {
    let root = compiler
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid ESP Rust compiler path")?;
    let candidates = fs::read_dir(root.join("xtensa-esp32-elf-clang"))
        .map_err(|_| "ESP toolchain has no Xtensa libclang; run prepare-iroh-toolchain again")?;
    let mut libraries = Vec::new();
    for candidate in candidates {
        let path = candidate
            .map_err(|error| error.to_string())?
            .path()
            .join("esp-clang/lib/libclang.so");
        if path.is_file() {
            libraries.push(path);
        }
    }
    if libraries.len() != 1 {
        return Err("ESP toolchain must contain exactly one Xtensa libclang installation".into());
    }
    Ok(libraries.remove(0))
}

fn verify_version(output: &str) -> Result<()> {
    if output.split_whitespace().nth(1) != Some("1.93.0-nightly")
        || !output.trim().ends_with("(1.93.0.0)")
    {
        return Err("Ailuros iroh firmware requires the pinned ESP Rust 1.93.0.0 toolchain".into());
    }
    Ok(())
}

pub fn prepare(directory: &Path, cancellation: &Cancellation) -> Result<PreparedToolchain> {
    cancellation.check()?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let directory = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let export = directory.join("esp-rust-export.sh");
    let mut install = Command::new("espup");
    install
        .args([
            "install",
            "--targets",
            "esp32s3",
            "--std",
            "--name",
            RUST_TOOLCHAIN,
            "--toolchain-version",
            RUST_VERSION,
            "--export-file",
        ])
        .arg(&export);
    run(install, cancellation)?;
    let mut locate = Command::new("rustup");
    locate.args(["which", "--toolchain", RUST_TOOLCHAIN, "rustc"]);
    let compiler = PathBuf::from(run(locate, cancellation)?.trim());
    if !compiler.is_absolute() || !compiler.is_file() {
        return Err("Rustup did not return the installed ESP compiler".into());
    }
    let mut version = Command::new(&compiler);
    version.arg("--version");
    verify_version(&run(version, cancellation)?)?;
    let library = libclang(&compiler)?;
    if let Some(paths) = std::env::var_os("AILUROS_LIBCLANG_RPATH") {
        let paths = paths.to_str().ok_or("Invalid Nix libclang runtime path")?;
        if paths.is_empty()
            || paths
                .split(':')
                .any(|path| !Path::new(path).is_absolute() || !Path::new(path).is_dir())
        {
            return Err("Nix libclang runtime paths must be existing absolute directories".into());
        }
        let mut inspect = Command::new("patchelf");
        inspect.arg("--print-rpath").arg(&library);
        let current = run(inspect, cancellation)?;
        let mut entries: Vec<&str> = current
            .trim()
            .split(':')
            .filter(|entry| !entry.is_empty())
            .collect();
        for path in paths.split(':') {
            if !entries.contains(&path) {
                entries.push(path);
            }
        }
        let mut patch = Command::new("patchelf");
        patch
            .arg("--set-rpath")
            .arg(entries.join(":"))
            .arg(&library);
        run(patch, cancellation)?;
    }
    Ok(PreparedToolchain {
        compiler,
        libclang: library.parent().ok_or("Missing libclang directory")?.into(),
        export,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_frontend_reuses_shared_components_and_hashes_all_release_inputs() {
        let directory = tempfile::tempdir().unwrap();
        super::super::export_for_board(directory.path(), "goouuu-esp32s3-cam-v1.5").unwrap();
        let manifest = fs::read_to_string(directory.path().join("iroh/Cargo.toml")).unwrap();
        assert!(manifest.contains("../components/lince_core"));
        assert!(manifest.contains("../components/lince_camera"));
        assert!(!manifest.contains("../esp-idf"));
        assert!(manifest.contains("path = \"../protocol\""));
        assert!(
            directory
                .path()
                .join("components/lince_core/bootstrap.c")
                .is_file()
        );
        assert!(
            directory
                .path()
                .join("components/lince_camera/camera.c")
                .is_file()
        );
        for name in [
            "iroh/Cargo.lock",
            "iroh/rust-toolchain.toml",
            "iroh/targets/xtensa-esp32s3-espidf.json",
            "iroh/sdkconfig.defaults",
            "iroh/partitions.csv",
            "iroh/.cargo/config.toml",
            "iroh/src/lib.rs",
            "iroh/src/network.rs",
            "iroh/src/power.rs",
            "iroh/src/readiness.rs",
            "iroh/src/verification.rs",
            "iroh/fixtures/ecdsa.json",
            "components/lince_core/verification.c",
            "protocol/Cargo.toml",
            "protocol/src/lib.rs",
            "iroh/src/bin/transport_check.rs",
            "iroh/licenses/rusty-esp-iroh-MIT.txt",
        ] {
            let path = directory.path().join(name);
            let original = fs::read(&path).unwrap();
            let before = super::super::source_digest(directory.path()).unwrap();
            let mut changed = original.clone();
            changed.push(b'\n');
            fs::write(&path, changed).unwrap();
            assert_ne!(
                super::super::source_digest(directory.path()).unwrap(),
                before,
                "{name}"
            );
            fs::write(&path, original).unwrap();
        }
        let cancellation = Cancellation::default();
        let base = tempfile::tempdir().unwrap();
        super::super::export(base.path()).unwrap();
        let error = compile_qualification(base.path(), &cancellation).unwrap_err();
        assert!(error.contains("verified GOOUUU"));
        assert!(!base.path().join("build").exists());
        let target_path = directory
            .path()
            .join("iroh/targets/xtensa-esp32s3-espidf.json");
        let original = fs::read(&target_path).unwrap();
        let mut target: serde_json::Value = serde_json::from_slice(&original).unwrap();
        target["features"] = "+s32c1i".into();
        fs::write(&target_path, serde_json::to_vec(&target).unwrap()).unwrap();
        let error = compile_qualification(directory.path(), &cancellation).unwrap_err();
        assert!(error.contains("PSRAM-safe"));
        assert!(!directory.path().join("build").exists());
        fs::write(target_path, original).unwrap();
        fs::write(
            directory.path().join("iroh/rust-toolchain.toml"),
            "[toolchain]\nchannel = \"esp\"\n",
        )
        .unwrap();
        let error = compile_qualification(directory.path(), &cancellation).unwrap_err();
        assert!(error.contains("qualified ESP Rust"));
        assert!(!directory.path().join("build").exists());
    }

    #[test]
    fn compiler_gate_rejects_the_known_codegen_regression_and_unpinned_versions() {
        verify_version("rustc 1.93.0-nightly (2b43689c5 2026-01-27) (1.93.0.0)").unwrap();
        assert!(verify_version("rustc 1.93.0-nightly (other 2026-01-01)").is_err());
        assert!(verify_version("rustc 1.97.0 (abcd 2026-09-01)").is_err());
        assert!(verify_version("rustc 1.93.1 (abcd 2026-01-01)").is_err());
        assert!(verify_version("unrecognized compiler").is_err());
    }

    #[test]
    fn standalone_cargo_caches_do_not_become_firmware_inputs() {
        let directory = tempfile::tempdir().unwrap();
        super::super::export_for_board(directory.path(), "goouuu-esp32s3-cam-v1.5").unwrap();
        let before = super::super::source_digest(directory.path()).unwrap();
        for name in [
            "iroh/target/deps/example.o",
            "iroh/.embuild/esp-idf/cache",
            "target/debug/cache",
            "protocol/target/deps/example.o",
            "protocol/Cargo.lock",
        ] {
            let path = directory.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "generated cache").unwrap();
        }
        assert_eq!(
            super::super::source_digest(directory.path()).unwrap(),
            before
        );
        fs::write(directory.path().join("iroh/src/lib.rs"), "edited firmware").unwrap();
        assert_ne!(
            super::super::source_digest(directory.path()).unwrap(),
            before
        );
    }

    #[test]
    fn sdk_configuration_inputs_select_fresh_caches_but_workflow_edits_reuse_them() {
        let directory = tempfile::tempdir().unwrap();
        super::super::export_for_board(directory.path(), "goouuu-esp32s3-cam-v1.5").unwrap();
        let before = configuration_digest(directory.path()).unwrap();
        fs::write(directory.path().join("iroh/src/lib.rs"), "edited workflow").unwrap();
        fs::write(
            directory.path().join("protocol/src/lib.rs"),
            "edited framing",
        )
        .unwrap();
        assert_eq!(configuration_digest(directory.path()).unwrap(), before);
        for name in [
            "iroh/sdkconfig.defaults",
            "iroh/partitions.csv",
            "iroh/rust-toolchain.toml",
            "iroh/targets/xtensa-esp32s3-espidf.json",
            "components/lince_camera/Kconfig",
            "protocol/Cargo.toml",
        ] {
            let path = directory.path().join(name);
            let original = fs::read(&path).unwrap();
            let mut changed = original.clone();
            changed.push(b'\n');
            fs::write(&path, changed).unwrap();
            assert_ne!(
                configuration_digest(directory.path()).unwrap(),
                before,
                "{name}"
            );
            fs::write(&path, original).unwrap();
        }
        fs::write(
            directory
                .path()
                .join("iroh/sdkconfig.defaults.release.esp32s3"),
            "CONFIG_SPIRAM=n\n",
        )
        .unwrap();
        assert_ne!(configuration_digest(directory.path()).unwrap(), before);
        let release = directory.path().join("build/release");
        let sdk = release.join("build/esp-idf-sys-first/out/sdkconfig");
        fs::create_dir_all(sdk.parent().unwrap()).unwrap();
        fs::write(&sdk, "CONFIG_SPIRAM=y\n").unwrap();
        assert_eq!(generated_sdk(&release).unwrap(), sdk);
        let other = release.join("build/esp-idf-sys-other/out/sdkconfig");
        fs::create_dir_all(other.parent().unwrap()).unwrap();
        fs::write(other, "CONFIG_SPIRAM=n\n").unwrap();
        assert!(generated_sdk(&release).is_err());
    }
}
