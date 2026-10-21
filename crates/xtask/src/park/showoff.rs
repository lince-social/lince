use super::super::{cargo_command, checked};
use serde_json::json;
use std::{
    env,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(super) fn run(arguments: &[OsString]) -> Result<()> {
    let mut nested = false;
    let mut audit = false;
    let mut duration = None;
    let mut outputs = None;
    let mut directory = None;
    let mut firefox = OsString::from("firefox");
    let mut terminal = OsString::from("xdg-terminal-exec");
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--nested") => nested = true,
            Some("--audit") => audit = true,
            Some("--directory") => {
                directory = Some(PathBuf::from(
                    arguments.next().ok_or("--directory needs a path")?,
                ))
            }
            Some("--firefox") => {
                firefox = arguments
                    .next()
                    .ok_or("--firefox needs an executable")?
                    .clone()
            }
            Some("--terminal") => {
                terminal = arguments
                    .next()
                    .ok_or("--terminal needs an executable")?
                    .clone()
            }
            Some("--duration") => {
                duration = Some(arguments.next().ok_or("--duration needs seconds")?.clone())
            }
            Some("--outputs") => {
                outputs = Some(arguments.next().ok_or("--outputs needs a count")?.clone())
            }
            Some("--help" | "-h") => {
                println!(
                    "cargo xtask park showoff [--nested] [--audit] [--directory PATH] [--firefox PROGRAM] [--terminal PROGRAM] [--duration SECONDS] [--outputs COUNT]"
                );
                println!(
                    "Runs the release compositor with a saved showoff workspace and a private Firefox profile. No time limit unless requested for a nested run."
                );
                return Ok(());
            }
            _ => {
                return Err(
                    format!("Unknown showoff argument: {}", argument.to_string_lossy()).into(),
                );
            }
        }
    }
    if !nested && (duration.is_some() || outputs.is_some()) {
        return Err("--duration and --outputs apply to --nested only".into());
    }
    let directory = directory
        .or_else(|| {
            env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/state"))
                })
                .map(|path| path.join("lince-park-showoff"))
        })
        .ok_or("Choose --directory for the showoff data")?;
    fs::create_dir_all(&directory)?;
    let directory = fs::canonicalize(directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let cell = directory.join("cell");
    let profile = directory.join("firefox");
    fs::create_dir_all(&cell)?;
    fs::create_dir_all(&profile)?;
    if !profile.join("user.js").exists() {
        fs::write(profile.join("user.js"), FIREFOX_PREFS)?;
    }
    let commands = cell.join("park.json");
    if !commands.exists() {
        fs::write(
            &commands,
            serde_json::to_vec_pretty(&json!({"terminal": {
                "program": "env", "arguments": ["-u", "GBM_BACKENDS_PATH", "-u", "LD_LIBRARY_PATH", "-u", "__EGL_VENDOR_LIBRARY_DIRS", terminal]
            }}))?,
        )?;
    }
    if audit {
        let probe = example("session_probe")?;
        let capture = example("capture_probe")?;
        let run = directory.join("runs").join(format!(
            "{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            std::process::id()
        ));
        fs::create_dir_all(&run)?;
        fs::write(
            directory.join("latest-run.txt"),
            run.to_string_lossy().as_bytes(),
        )?;
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&commands)?)?;
        settings["ready"] = json!({"program": env::current_exe()?, "arguments": ["park", "showoff-ready",
            run, probe, capture], "capabilities": ["capture", "window-list"]});
        fs::write(&commands, serde_json::to_vec_pretty(&settings)?)?;
    } else {
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&commands)?)?;
        if settings["ready"]["arguments"][1] == "showoff-ready" {
            settings
                .as_object_mut()
                .ok_or("The Park command settings must be an object")?
                .remove("ready");
            fs::write(&commands, serde_json::to_vec_pretty(&settings)?)?;
        }
    }
    let page = directory.join("browser-test.html");
    fs::write(&page, PAGE)?;
    eprintln!("Showoff data: {}", directory.display());
    eprintln!(
        "Release mode; Firefox uses a private profile. Use the showoff controls or Alt+E. Log out with Super+Shift+Escape."
    );
    let mut command = cargo_command();
    command
        .args([
            "run",
            "--release",
            "--locked",
            "-p",
            "lince",
            "--features",
            "park",
            "--",
        ])
        .arg(if nested { "--park-nested" } else { "--park" })
        .args(["--showoff", "--directory"])
        .arg(&cell)
        .args(["--listen-addr", "127.0.0.1:0", "--peer-port", "0"]);
    if let Some(duration) = duration {
        command.arg("--park-duration").arg(duration);
    }
    if let Some(outputs) = outputs {
        command.arg("--park-outputs").arg(outputs);
    }
    command
        .args([
            "--",
            "env",
            "-u",
            "GBM_BACKENDS_PATH",
            "-u",
            "LD_LIBRARY_PATH",
            "-u",
            "__EGL_VENDOR_LIBRARY_DIRS",
        ])
        .arg(firefox)
        .args(["--no-remote", "--profile"])
        .arg(profile)
        .arg(page);
    checked(&mut command).map_err(Into::into)
}

fn example(name: &str) -> Result<PathBuf> {
    let output = cargo_command()
        .args([
            "run",
            "--release",
            "--locked",
            "--message-format=json",
            "-p",
            "lince-park",
            "--features",
            "session,server-tests,lince-interface/native-media",
            "--example",
            name,
            "--",
            "--help",
        ])
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Err(format!("Could not compile the release {name} probe").into());
    }
    String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|value| {
            (value["reason"] == "compiler-artifact" && value["target"]["name"] == name)
                .then(|| value["executable"].as_str().map(PathBuf::from))
                .flatten()
        })
        .ok_or_else(|| format!("The {name} executable was not reported").into())
}

pub(super) fn ready(arguments: &[OsString]) -> Result<()> {
    if arguments.len() != 3 {
        return Err(
            "The showoff readiness hook needs its directory and two probe executables".into(),
        );
    }
    let directory = PathBuf::from(&arguments[0]);
    let socket = env::var_os("WAYLAND_DISPLAY").ok_or("The showoff display is unavailable")?;
    let mut report = json!({"socket": socket, "pid": rustix::process::getppid().map(|pid| pid.as_raw_nonzero().get()), "status": "running"});
    fs::write(
        directory.join("ready.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::thread::sleep(Duration::from_secs(5));
    let probe_log = fs::File::create(directory.join("native-window-lifecycle.log"))?;
    let status = Command::new(&arguments[1])
        .env_remove("WAYLAND_SOCKET")
        .stdout(probe_log.try_clone()?)
        .stderr(probe_log)
        .status()?;
    report["native_window_lifecycle"] = json!(status.code());
    std::thread::sleep(Duration::from_secs(2));
    let capture_log = fs::File::create(directory.join("capture.log"))?;
    let status = Command::new(&arguments[2])
        .arg(directory.join("capture.json"))
        .arg("--image")
        .arg(directory.join("showoff.ppm"))
        .args(["--application", "firefox"])
        .stdout(capture_log.try_clone()?)
        .stderr(capture_log)
        .status()?;
    report["capture"] = json!(status.code());
    report["status"] = json!(
        if status.success() && report["native_window_lifecycle"] == 0 {
            "passed"
        } else {
            "failed"
        }
    );
    fs::write(
        directory.join("audit.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

const FIREFOX_PREFS: &str = r#"user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
"#;

const PAGE: &str = r#"<!doctype html>
<html lang="en"><meta charset="utf-8"><title>Firefox · Park showoff</title>
<style>html{color-scheme:dark;font:18px system-ui;background:#182335;color:#e8eaf1}body{max-width:52rem;margin:3rem auto;padding:1rem}h1{color:#bc9af4}input,button,textarea{font:inherit;padding:.7rem;border:1px solid #7186a3;border-radius:.4rem}textarea{display:block;width:90%;height:8rem;margin:1rem 0}canvas{width:100%;background:#101827;border-radius:.7rem}li{margin:.5rem 0}</style>
<h1>Firefox running inside Park</h1><p>This is a normal Wayland Firefox window using an isolated test profile. Browse normally from its address bar.</p>
<p><input placeholder="Test keyboard focus here" aria-label="Keyboard focus test"> <button id="count">Clicks: 0</button></p>
<textarea aria-label="Clipboard and selection test">Edit, select, copy and paste this text. Open another tab or window, resize, tile and fullscreen Firefox.</textarea>
<canvas id="motion" width="800" height="160" aria-label="Animated browser canvas"></canvas>
<ul><li>Use Park's Canvas, Tile and Columns controls to change layout.</li><li>Super + H/J/K/L focuses items; add Shift to move them.</li><li>Super + Backspace returns focus to the canvas; Alt + E opens editing.</li><li>Use Scatter physics, drag Sands, add areas, and explore the GLB in 3D.</li><li>This page works offline. Test real sites by typing an address.</li></ul>
<script>let clicks=0;document.querySelector('#count').onclick=e=>e.target.textContent='Clicks: '+ ++clicks;const c=document.querySelector('canvas'),x=c.getContext('2d');function draw(t){x.clearRect(0,0,800,160);for(let i=0;i<7;i++){x.fillStyle=['#bc9af4','#68c3df','#f0ba66'][i%3];x.beginPath();x.arc(70+i*108,80+Math.sin(t/700+i)*38,18,0,Math.PI*2);x.fill()}requestAnimationFrame(draw)}requestAnimationFrame(draw);</script></html>"#;
