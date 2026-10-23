use super::{Result, error};
use std::{env, ffi::OsString, fs, path::Path, process::Command};

pub(super) fn run(arguments: &[OsString]) -> Result<()> {
    if arguments.is_empty() || arguments[0] == "--help" {
        println!(
            "cargo xtask park validate browser /installed/bin/lince --geckodriver PATH [--firefox PROGRAM] [--terminal PROGRAM] [--capture-probe PATH] [--provider FIXTURE.json] [--host-close PROGRAM] [--host-prepare PROGRAM] [--ownership] [--save-failure] [--reopen-sandbox] [--lifecycle-rounds 0] [--plain] [--keep-size] [--fullscreen] [--outputs 1] [--seconds 75]"
        );
        return Ok(());
    }
    let executable = Path::new(&arguments[0]);
    if !executable.is_absolute() {
        return Err("Use an absolute installed executable path".into());
    }
    if super::inspect_build(executable)?["optimized"] != true {
        return Err("Installed browser validation requires an optimized executable".into());
    }
    let directory = env::current_dir()
        .map_err(error)?
        .join("target/park-validation/browser");
    let fixtures = directory.join("fixtures");
    fs::create_dir_all(directory.join("bin")).map_err(error)?;
    fs::create_dir_all(&fixtures).map_err(error)?;
    fs::write(fixtures.join("page.html"), super::super::showoff::PAGE).map_err(error)?;
    fs::write(
        fixtures.join("user.js"),
        super::super::showoff::FIREFOX_PREFS,
    )
    .map_err(error)?;
    fs::write(
        fixtures.join("video.webm"),
        include_bytes!("../../../fixtures/showoff.webm"),
    )
    .map_err(error)?;
    let script = directory.join("browser.py");
    fs::write(&script, include_str!("browser.py")).map_err(error)?;
    let mut command = Command::new("python3");
    command
        .arg(script)
        .args(arguments)
        .arg("--fixtures")
        .arg(fixtures);
    for (flag, fallback) in [
        ("--firefox", "firefox"),
        ("--terminal", "xdg-terminal-exec"),
    ] {
        if !arguments.iter().any(|argument| argument == flag) {
            command.args([flag, fallback]);
        }
    }
    if !arguments
        .iter()
        .any(|argument| argument == "--capture-probe")
    {
        super::prepare_capture_probe(&directory)?;
        command
            .arg("--capture-probe")
            .arg(directory.join("bin/capture-probe"));
    }
    let status = command.status().map_err(error)?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Installed browser validation failed: {status}"))
    }
}
