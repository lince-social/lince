use super::{Result, error};
use std::{env, ffi::OsString, fs, path::{Path, PathBuf}, process::Command};

pub(super) fn run(arguments: &[OsString]) -> Result<()> {
    if arguments.is_empty() || arguments[0] == "--help" {
        println!(
            "cargo xtask park validate browser /installed/bin/lince --geckodriver PATH [--firefox PROGRAM] [--terminal PROGRAM] [--capture-probe PATH] [--provider FIXTURE.json] [--host-close PROGRAM] [--host-prepare PROGRAM] [--ownership] [--save-failure] [--reopen-sandbox] [--lifecycle-rounds 0] [--plain] [--keep-size] [--fullscreen] [--outputs 1] [--seconds 75]"
        );
        return Ok(());
    }
    let stamp = super::SystemTime::now().duration_since(super::UNIX_EPOCH).map_err(error)?.as_nanos();
    let directory = env::current_dir().map_err(error)?.join("target/park-validation/browser").join(format!("{stamp}-{}", std::process::id()));
    run_into(arguments, &directory).map(|_| ())
}

pub(super) fn run_into(arguments: &[OsString], directory: &Path) -> Result<PathBuf> {
    let executable = Path::new(arguments.first().ok_or("Missing installed executable")?);
    if !executable.is_absolute() {
        return Err("Use an absolute installed executable path".into());
    }
    if super::inspect_build(executable)?["optimized"] != true {
        return Err("Installed browser validation requires an optimized executable".into());
    }
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
    let mut command = Command::new(env::current_exe().map_err(error)?);
    command
        .args(["maintenance", "browser"])
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
    super::logged(&mut command, &directory.join("browser-driver.log"))?;
    let requested = arguments.windows(2).find(|pair| pair[0] == "--seconds").and_then(|pair| pair[1].to_str()?.parse::<u64>().ok()).unwrap_or(75);
    let (status, _) = super::bounded(&mut command, std::time::Duration::from_secs(requested.saturating_add(900)))?;
    if status.success() {
        let summary: serde_json::Value = serde_json::from_slice(&fs::read(directory.join("browser-driver.log")).map_err(error)?).map_err(error)?;
        let run = PathBuf::from(summary["directory"].as_str().ok_or("Browser result has no run directory")?);
        let run = fs::canonicalize(run).map_err(error)?;
        if !run.starts_with(fs::canonicalize(directory).map_err(error)?.join("runs")) {
            return Err("Browser result escaped its private run directory".into());
        }
        Ok(run)
    } else {
        Err(format!("Installed browser validation failed: {status}"))
    }
}
