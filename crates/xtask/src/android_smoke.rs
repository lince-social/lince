use crate::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    env,
    ffi::OsString,
    fs,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

const PACKAGE: &str = "social.lince.mobile.smoketest";
const COMPONENT: &str = "social.lince.mobile.smoketest/social.lince.mobile.MainActivity";

struct Runner {
    adb: PathBuf,
    serial: String,
    output: PathBuf,
    sequence: u64,
}

impl Runner {
    fn adb(&self, args: &[&str]) -> Result<String> {
        let result = Command::new(&self.adb)
            .args(["-s", &self.serial])
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() {
            return Err(format!(
                "adb failed: {} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&result.stdout).into_owned())
    }

    fn request(&mut self, mut request: Value) -> Result<Value> {
        self.sequence += 1;
        request["id"] = json!(self.sequence);
        let encoded = STANDARD.encode(serde_json::to_vec(&request).map_err(|e| e.to_string())?);
        self.adb(&[
            "shell",
            "am",
            "start",
            "-f",
            "0x20000000",
            "-n",
            COMPONENT,
            "--es",
            "lince_smoke",
            &encoded,
        ])?;
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut sent = Instant::now();
        while Instant::now() < deadline {
            if request["command"] == "snapshot" && sent.elapsed() >= Duration::from_secs(2) {
                self.adb(&[
                    "shell",
                    "am",
                    "start",
                    "-f",
                    "0x20000000",
                    "-n",
                    COMPONENT,
                    "--es",
                    "lince_smoke",
                    &encoded,
                ])?;
                sent = Instant::now();
            }
            let pid = self.adb(&["shell", "pidof", PACKAGE]).unwrap_or_default();
            if pid.trim().is_empty() {
                thread::sleep(Duration::from_millis(250));
                continue;
            }
            let logs = self.adb(&["logcat", "--pid", pid.trim(), "-d", "-t", "500"])?;
            for line in logs.lines().rev() {
                if let Some((_, body)) = line.split_once("LINCE_SMOKE ") {
                    if let Ok(reply) = serde_json::from_str::<Value>(body) {
                        if reply["id"] == self.sequence {
                            fs::write(
                                self.output.join("snapshot.json"),
                                serde_json::to_vec_pretty(&reply).map_err(|e| e.to_string())?,
                            )
                            .map_err(|e| e.to_string())?;
                            if let Some(error) = reply["error"].as_str() {
                                return Err(error.to_owned());
                            }
                            return Ok(reply);
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err("Timed out waiting for the smoke APK; inspect logcat.txt".into())
    }

    fn wait_for(&mut self, label: &str, predicate: impl Fn(&Value) -> bool) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            let value = self.request(json!({"command":"snapshot"}))?;
            if predicate(&value) {
                return Ok(value);
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err(format!("Timed out waiting for {label}"))
    }

    fn press(&mut self, label: &str) -> Result<()> {
        self.wait_for(label, |v| {
            v["buttons"]
                .as_array()
                .is_some_and(|buttons| buttons.iter().any(|b| b == label))
        })?;
        self.request(json!({"command":"press","label":label}))?;
        Ok(())
    }

    fn exercise(&mut self) -> Result<()> {
        self.adb(&["shell", "am", "start", "-W", "-n", COMPONENT])?;
        thread::sleep(Duration::from_secs(3));
        self.wait_for("initial startup", |v| v["ready"] == true)?;
        self.press("Create my Organ")?;
        self.press("Confirm")?;
        self.wait_for("Organ creation", |v| {
            v["ready"] == true && v["setup"] == false
        })?;
        self.press("Pages")?;
        self.press("Records")?;
        self.press("New Record")?;
        let form = self.wait_for("Record editor", |v| head_key(v).is_some())?;
        let key = head_key(&form).ok_or("missing title field")?;
        let title = "Tarefa de amanhã — café ☕";
        self.request(json!({"command":"edit","key":key,"value":title}))?;
        self.press("Save Title")?;
        self.wait_for("saved Record", |v| {
            v["record"]["head"] == title
                && v["status"].as_str().is_some_and(|s| s.contains("Saved"))
        })?;
        self.adb(&["shell", "am", "force-stop", PACKAGE])?;
        self.adb(&["shell", "am", "start", "-W", "-n", COMPONENT])?;
        thread::sleep(Duration::from_secs(3));
        self.wait_for("persisted Unicode title", |v| {
            v["ready"] == true
                && v["fields"]
                    .as_array()
                    .is_some_and(|fields| fields.iter().any(|f| f["value"] == title))
        })?;
        self.press("Pages")?;
        self.press("Organ")?;
        self.request(json!({"command":"edit","key":"profile/name","value":"Second test profile"}))?;
        self.press("Create and open fresh profile")?;
        self.wait_for("fresh profile onboarding", |v| {
            v["ready"] == true && v["setup"] == true
        })?;
        self.press("Open original profile")?;
        self.wait_for("original profile restored", |v| {
            v["ready"] == true && v["setup"] == false
        })?;
        self.press("Pages")?;
        self.press("Records")?;
        self.wait_for("original Record preserved", |v| {
            v["buttons"]
                .as_array()
                .is_some_and(|buttons| buttons.iter().any(|button| button == title))
        })?;
        Ok(())
    }

    fn collect(&self) -> Result<()> {
        let uid = self.adb(&["shell", "cmd", "package", "list", "packages", "-U", PACKAGE])?;
        if let Some(uid) = uid
            .split("uid:")
            .nth(1)
            .and_then(|v| v.split_whitespace().next())
        {
            let logs = self.adb(&["logcat", &format!("--uid={uid}"), "-d", "-t", "4000"])?;
            fs::write(self.output.join("logcat.txt"), logs).map_err(|e| e.to_string())?;
        }
        let screenshot = Command::new(&self.adb)
            .args(["-s", &self.serial, "exec-out", "screencap", "-p"])
            .output()
            .map_err(|e| e.to_string())?;
        if screenshot.status.success() {
            fs::write(self.output.join("screen.png"), screenshot.stdout)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn head_key(value: &Value) -> Option<&str> {
    value["fields"]
        .as_array()?
        .iter()
        .filter_map(|f| f["key"].as_str())
        .find(|key| key.ends_with("/head"))
}

pub(crate) fn run(args: &[OsString]) -> Result<()> {
    let mut device = None;
    let mut apk = None;
    let mut directory = PathBuf::from("target/android-smoke");
    let mut pagesize = None;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        if flag == "--help" {
            println!(
                "cargo xtask android-smoke --device SERIAL --apk PATH [--output DIR] [--pagesize 16384]"
            );
            return Ok(());
        }
        let value = args.next().ok_or("option requires a value")?;
        match flag.to_str() {
            Some("--device") => device = Some(value.to_string_lossy().into_owned()),
            Some("--apk") => apk = Some(PathBuf::from(value)),
            Some("--output") => directory = PathBuf::from(value),
            Some("--pagesize") => pagesize = Some(value.to_string_lossy().into_owned()),
            _ => return Err(format!("unknown option {}", flag.to_string_lossy())),
        }
    }
    let serial = device
        .ok_or("--device is required; only the separate smoketest application will be reset")?;
    if serial.is_empty()
        || serial.starts_with('-')
        || !serial
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
    {
        return Err("invalid device serial".into());
    }
    let apk = apk
        .ok_or("--apk is required")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let sdk = env::var_os("ANDROID_HOME")
        .or_else(|| env::var_os("ANDROID_SDK_ROOT"))
        .map(PathBuf::from)
        .ok_or("Set ANDROID_HOME to the Android SDK directory")?;
    let badging = Command::new(sdk.join("build-tools/35.0.0/aapt2"))
        .args(["dump", "badging"])
        .arg(&apk)
        .output()
        .map_err(|e| e.to_string())?;
    let expected = format!("package: name='{PACKAGE}' ");
    if !badging.status.success()
        || !String::from_utf8_lossy(&badging.stdout)
            .lines()
            .any(|l| l.starts_with(&expected))
    {
        return Err(
            "Refusing APK: expected the separate social.lince.mobile.smoketest package".into(),
        );
    }
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut runner = Runner {
        adb: sdk.join("platform-tools/adb"),
        serial,
        output: directory,
        sequence: 0,
    };
    let actual = runner.adb(&["shell", "getconf", "PAGE_SIZE"])?;
    if pagesize.is_some_and(|expected| expected != actual.trim()) {
        return Err(format!("Unexpected Android page size: {}", actual.trim()));
    }
    fs::write(
        runner.output.join("device.txt"),
        format!("{}\npage size: {actual}", runner.serial),
    )
    .map_err(|e| e.to_string())?;
    runner.adb(&["install", "-r", apk.to_str().ok_or("invalid APK path")?])?;
    let cleared = runner.adb(&["shell", "pm", "clear", PACKAGE])?;
    if !cleared.contains("Success") {
        return Err(format!("Could not reset smoke data: {cleared}"));
    }
    let result = runner.exercise();
    let collected = runner.collect();
    runner.adb(&["shell", "am", "force-stop", PACKAGE])?;
    result?;
    collected?;
    println!(
        "Android install, launch, edit, restart and separate profiles passed. Evidence: {}",
        runner.output.display()
    );
    Ok(())
}
