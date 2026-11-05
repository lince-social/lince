use super::*;

#[test]
fn screenshot_actions_work_without_configuration_and_require_an_unlocked_session() {
    let settings = super::super::Settings::default();
    for action in [Action::ScreenshotScreen, Action::ScreenshotRegion] {
        let command = settings.command(action, false).unwrap();
        assert_eq!(command.capabilities, [Capability::Capture]);
        assert_eq!(
            Options::from_command(action, &command),
            Some(Options::default())
        );
        assert!(settings.command(action, true).is_err());
    }
    assert!(settings.command(Action::Editor, false).is_err());
    assert!(
        Options {
            directory: "relative".into(),
            show_pointer: false
        }
        .validate()
        .is_err()
    );
}

#[test]
fn screen_capture_targets_the_focused_output_without_changing_region_selection() {
    let options = Options::default();
    let mut screen = options.command(Action::ScreenshotScreen).unwrap();
    Options::target_output(Action::ScreenshotScreen, &mut screen, "DP-2");
    assert_eq!(screen.arguments.last().unwrap(), "DP-2");
    let mut region = options.command(Action::ScreenshotRegion).unwrap();
    let original = region.clone();
    Options::target_output(Action::ScreenshotRegion, &mut region, "DP-2");
    assert_eq!(region, original);
}

#[cfg(unix)]
fn executable(directory: &std::path::Path, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = directory.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(unix)]
#[test]
fn provider_pipeline_preserves_grim_authority_treats_paths_literally_and_cleans_failures() {
    let directory = tempfile::tempdir().unwrap();
    executable(
        directory.path(),
        "grim",
        "printf '%s\\n' \"${WAYLAND_SOCKET-unset}\" \"$@\" > \"$CAPTURE_LOG\"; for last do :; done; printf PNG > \"$last\"; exit \"${GRIM_STATUS-0}\"",
    );
    executable(
        directory.path(),
        "slurp",
        "printf '%s' \"${WAYLAND_SOCKET-unset}\" > \"$SELECTOR_LOG\"; [ \"${CANCEL_SELECTION-0}\" = 0 ] || exit 1; printf '10,20 300x200\\n'",
    );
    let output = directory.path().join("pictures $(touch escaped)");
    let log = directory.path().join("grim.log");
    let selector = directory.path().join("slurp.log");
    let options = Options {
        directory: output.to_string_lossy().into_owned(),
        show_pointer: true,
    };
    let run = |action, cancel: bool, fail: bool| {
        let options = Options {
            show_pointer: action == Action::ScreenshotRegion,
            ..options.clone()
        };
        let mut description = options.command(action).unwrap();
        Options::target_output(action, &mut description, "DP-2");
        std::process::Command::new(description.program)
            .args(description.arguments)
            .current_dir(directory.path())
            .env(
                "PATH",
                std::env::join_paths(
                    std::iter::once(directory.path().to_path_buf())
                        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                )
                .unwrap(),
            )
            .env("WAYLAND_SOCKET", "77")
            .env("CAPTURE_LOG", &log)
            .env("SELECTOR_LOG", &selector)
            .env("CANCEL_SELECTION", if cancel { "1" } else { "0" })
            .env("GRIM_STATUS", if fail { "1" } else { "0" })
            .status()
            .unwrap()
    };
    assert!(run(Action::ScreenshotRegion, false, false).success());
    assert_eq!(std::fs::read_to_string(&selector).unwrap(), "unset");
    let captured = std::fs::read_to_string(&log).unwrap();
    assert!(captured.starts_with("77\n-c\n-g\n10,20 300x200\n"));
    assert!(!directory.path().join("escaped").exists());
    let files = || std::fs::read_dir(&output).unwrap().count();
    assert_eq!(files(), 1);
    assert!(run(Action::ScreenshotRegion, true, false).success());
    assert_eq!(files(), 1);
    assert!(!run(Action::ScreenshotScreen, false, true).success());
    assert_eq!(files(), 1);
    assert!(run(Action::ScreenshotScreen, false, false).success());
    assert_eq!(files(), 2);
    assert!(
        std::fs::read_to_string(&log)
            .unwrap()
            .starts_with("77\n-o\nDP-2\n")
    );
}
