use lince_ailuros::{profile, project};
use std::{fs, path::Path, process::Command};

fn configure(project: &Path) -> (String, String) {
    let build = project.join("build/check-build");
    let harness = project.join("build/check-src");
    fs::create_dir_all(&harness).unwrap();
    let script = harness.join("CMakeLists.txt");
    fs::write(&script, format!(
        "cmake_minimum_required(VERSION 3.19)\nproject(ailuros_configuration_check NONE)\ninclude(\"{}/cmake/board.cmake\")\nfile(WRITE \"${{CMAKE_BINARY_DIR}}/selected-sdkconfig\" \"${{SDKCONFIG}}\")\n",
        project.display(),
    )).unwrap();
    let output = Command::new("cmake")
        .arg("-S")
        .arg(harness)
        .arg("-B")
        .arg(&build)
        .arg("-DSDKCONFIG=obsolete-sdkconfig")
        .arg("-DSDKCONFIG_DEFAULTS=obsolete-defaults")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        fs::read_to_string(build.join("sdkconfig.board")).unwrap(),
        fs::read_to_string(build.join("selected-sdkconfig")).unwrap(),
    )
}

#[test]
#[ignore = "Requires CMake 3.19 or newer; run explicitly inside the Nix development shell"]
fn board_switches_and_custom_defaults_cannot_reuse_stale_sdk_configuration() {
    let folder = tempfile::tempdir().unwrap();
    let project = folder.path();
    project::export(project).unwrap();
    let (base, base_config) = configure(project);
    assert!(base.contains("CONFIG_SPIRAM=n"));
    assert!(base.contains("CONFIG_LINCE_CAMERA_XCLK_HZ=0"));
    project::select_board(project, "goouuu-esp32s3-cam-v1.5").unwrap();
    let (camera, camera_config) = configure(project);
    assert!(camera.contains("CONFIG_SPIRAM_MODE_OCT=y"));
    assert!(camera.contains("CONFIG_SPIRAM_SPEED_80M=y"));
    assert!(camera.contains("CONFIG_LINCE_CAMERA_XCLK_HZ=8000000"));
    assert_ne!(base_config, camera_config);
    let mut custom = project::board(project).unwrap();
    custom.psram.as_mut().unwrap().mode = profile::PsramMode::Quad;
    custom.psram.as_mut().unwrap().speed_mhz = 40;
    custom.camera.as_mut().unwrap().xclk_hz = 16_000_000;
    fs::write(
        project.join("board.json"),
        serde_json::to_vec(&custom).unwrap(),
    )
    .unwrap();
    let (settings, custom_config) = configure(project);
    assert!(settings.contains("CONFIG_SPIRAM_MODE_QUAD=y"));
    assert!(settings.contains("CONFIG_SPIRAM_SPEED_40M=y"));
    assert!(settings.contains("CONFIG_LINCE_CAMERA_XCLK_HZ=16000000"));
    assert_ne!(camera_config, custom_config);
    project::select_board(project, profile::DEFAULT_BOARD).unwrap();
    assert_eq!(configure(project), (base, base_config.clone()));
    let defaults = project.join("sdkconfig.defaults");
    let mut text = fs::read_to_string(&defaults).unwrap();
    text.push_str("CONFIG_ESP_MAIN_TASK_STACK_SIZE=20480\n");
    fs::write(defaults, text).unwrap();
    assert_ne!(configure(project).1, base_config);
    let before = configure(project).1;
    let camera_options = project.join("components/lince_camera/Kconfig");
    let text = fs::read_to_string(&camera_options).unwrap();
    fs::write(camera_options, text.replace("default 0", "default 8000000")).unwrap();
    assert_ne!(configure(project).1, before);
}

#[test]
#[ignore = "Requires CMake 3.19 or newer; run explicitly inside the Nix development shell"]
fn invalid_board_clock_is_rejected_before_generating_firmware() {
    let folder = tempfile::tempdir().unwrap();
    project::export_for_board(folder.path(), "goouuu-esp32s3-cam-v1.5").unwrap();
    let mut board = project::board(folder.path()).unwrap();
    board.camera.as_mut().unwrap().xclk_hz = 0;
    fs::write(
        folder.path().join("board.json"),
        serde_json::to_vec(&board).unwrap(),
    )
    .unwrap();
    let build = folder.path().join("build");
    fs::create_dir(&build).unwrap();
    let script = build.join("check.cmake");
    fs::write(
        &script,
        format!(
            "cmake_minimum_required(VERSION 3.19)\ninclude(\"{}/cmake/board.cmake\")\n",
            folder.path().display()
        ),
    )
    .unwrap();
    let result = Command::new("cmake")
        .arg("-P")
        .arg(script)
        .current_dir(build)
        .output()
        .unwrap();
    assert!(!result.status.success());
}
