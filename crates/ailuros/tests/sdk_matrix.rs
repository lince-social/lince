use lince_ailuros::{
    profile,
    project::{self, FirmwareMode, Toolchain},
    serial::Cancellation,
};
use std::{fs, path::Path};

fn setting(cache: &str, name: &str) -> String {
    cache
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.split(':').next() == Some(name))
        .unwrap_or_else(|| panic!("Missing SDK configuration setting {name}"))
        .1
        .to_owned()
}

fn compile(root: &Path, mode: FirmwareMode) -> String {
    eprintln!("Checking SDK project {} in {mode:?} mode", root.display());
    let cancellation = Cancellation::default();
    let toolchain = if std::env::var_os("IDF_PATH").is_some() {
        Toolchain::Environment
    } else {
        Toolchain::Nix
    };
    let artifact = match mode {
        FirmwareMode::Workflow => project::compile(root, toolchain, &cancellation),
        FirmwareMode::Tests => project::compile_tests(root, toolchain, &cancellation),
        FirmwareMode::IrohQualification | FirmwareMode::IrohCamera => {
            panic!("Rust firmware uses its own build matrix")
        }
    }
    .unwrap_or_else(|error| panic!("{} {mode:?}: {error}", root.display()));
    assert_eq!(artifact.mode, mode);
    assert_eq!(artifact.verify(root).unwrap().len(), 3);
    let cache = fs::read_to_string(root.join("build/CMakeCache.txt")).unwrap();
    let expected_mode = if mode == FirmwareMode::Tests {
        "ON"
    } else {
        "OFF"
    };
    assert_eq!(setting(&cache, "LINCE_RUN_TESTS"), expected_mode);
    assert_eq!(setting(&cache, "LINCE_COMPILE_TESTS"), expected_mode);
    let sdk = fs::read_to_string(setting(&cache, "SDKCONFIG")).unwrap();
    let board = project::board(root).unwrap();
    let camera_clock = board.camera.as_ref().map_or(0, |camera| camera.xclk_hz);
    assert!(
        sdk.lines()
            .any(|line| line == format!("CONFIG_LINCE_CAMERA_XCLK_HZ={camera_clock}"))
    );
    if board.psram.is_some() {
        assert!(sdk.lines().any(|line| line == "CONFIG_SPIRAM=y"));
        assert!(sdk.lines().any(|line| line == "CONFIG_SPIRAM_MODE_OCT=y"));
        assert!(sdk.lines().any(|line| line == "CONFIG_SPIRAM_SPEED_80M=y"));
    } else {
        assert!(!sdk.lines().any(|line| line == "CONFIG_SPIRAM=y"));
    }
    artifact.fingerprint().unwrap()
}

#[test]
#[ignore = "Compiles ESP-IDF 5.5.2 through the pinned Nix SDK; never flashes hardware"]
fn actual_sdk_resets_test_mode_and_board_settings_between_compilations() {
    let directory = tempfile::tempdir().unwrap();
    let output = std::env::var_os("AILUROS_SDK_MATRIX_OUTPUT").map(std::path::PathBuf::from);
    let directory = output.as_deref().unwrap_or(directory.path());
    if output.is_some() {
        fs::create_dir(directory).unwrap();
    }
    for id in [profile::DEFAULT_BOARD, "goouuu-esp32s3-cam-v1.5"] {
        let root = directory.join(id);
        fs::create_dir(&root).unwrap();
        project::export_for_board(&root, id).unwrap();
        let workflow = compile(&root, FirmwareMode::Workflow);
        let tests = compile(&root, FirmwareMode::Tests);
        assert_ne!(workflow, tests);
        compile(&root, FirmwareMode::Workflow);
        if id == "goouuu-esp32s3-cam-v1.5" {
            project::select_board(&root, profile::DEFAULT_BOARD).unwrap();
            compile(&root, FirmwareMode::Workflow);
            project::select_board(&root, id).unwrap();
            compile(&root, FirmwareMode::Workflow);
        }
    }
}
