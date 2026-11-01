use crate::{Result, ui_process};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use lince_interface::testing::{
    Backend, Dispatch, Limits, Route, Runner as Driver, Selector, Step,
};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

const PACKAGE: &str = "social.lince.mobile.smoketest";
const COMPONENT: &str = "social.lince.mobile.smoketest/social.lince.mobile.MainActivity";
const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;

struct Runner {
    adb: PathBuf,
    serial: String,
    output: PathBuf,
    sequence: u64,
    seen_pid: Option<String>,
    allow_restart: bool,
}

enum ReplyFailure {
    Rejected(String),
    Uncertain(String),
}

impl ReplyFailure {
    fn message(self) -> String {
        match self {
            Self::Rejected(message) | Self::Uncertain(message) => message,
        }
    }
}

enum SmokeAction {
    Press {
        primary: String,
        alternate: Option<String>,
    },
    Edit {
        key: String,
        value: String,
    },
}

struct Android<'a, 'p> {
    runner: &'a mut Runner,
    predicate: &'p dyn Fn(&Value) -> bool,
    last: Value,
    progress: bool,
    delay: Duration,
    failure: Option<String>,
}

fn buttons(value: &Value, label: &str) -> usize {
    value["buttons"].as_array().map_or(0, |buttons| {
        buttons.iter().filter(|button| *button == label).count()
    })
}

fn summary(value: &Value) -> String {
    json!({"ready":value["ready"],"setup":value["setup"],"page":value["page"],"status":value["status"],"buttons":value["buttons"],"field_keys":value["fields"].as_array().map(|fields| fields.iter().filter_map(|field| field["key"].as_str()).collect::<Vec<_>>())}).to_string()
}

impl Backend for Android<'_, '_> {
    type Snapshot = Value;
    type Action = SmokeAction;
    type Condition = ();

    fn snapshot(&mut self, remaining: Duration) -> Result<Value> {
        if let Some(error) = self.failure.take() {
            return Err(error);
        }
        let mut value = self
            .runner
            .request(json!({"command":"snapshot"}), remaining)
            .map_err(ReplyFailure::message)?;
        if let Some(object) = value.as_object_mut() {
            object.remove("id");
        }
        self.progress = value != self.last;
        self.last = value.clone();
        Ok(value)
    }

    fn dispatch(
        &mut self,
        snapshot: &Value,
        _selector: &Selector,
        action: &SmokeAction,
        route: Route,
        remaining: Duration,
    ) -> Dispatch {
        let request = match action {
            SmokeAction::Press { primary, alternate } => {
                let label = match route {
                    Route::Primary => primary,
                    Route::Fallback => match alternate {
                        Some(label) => label,
                        None => {
                            return Dispatch::Unavailable(
                                "No alternate button was declared".into(),
                            );
                        }
                    },
                };
                match buttons(snapshot, label) {
                    0 => return Dispatch::Unavailable(format!("Button {label} is unavailable")),
                    1 => {}
                    count => {
                        return Dispatch::Rejected(format!(
                            "Button {label} is ambiguous ({count} matches)"
                        ));
                    }
                }
                json!({"command":"press","label":label})
            }
            SmokeAction::Edit { key, value } => {
                let count = snapshot["fields"].as_array().map_or(0, |fields| {
                    fields
                        .iter()
                        .filter(|field| field["key"] == key.as_str())
                        .count()
                });
                if count != 1 {
                    return Dispatch::Unavailable(format!(
                        "Field {key} requires one match; found {count}"
                    ));
                }
                json!({"command":"edit","key":key,"value":value})
            }
        };
        match self.runner.request(request, remaining) {
            Ok(_) => Dispatch::Accepted,
            Err(ReplyFailure::Rejected(error)) => Dispatch::Rejected(error),
            Err(ReplyFailure::Uncertain(error)) => {
                self.failure = Some(format!(
                    "Input outcome is uncertain; refusing to replay: {error}"
                ));
                Dispatch::Accepted
            }
        }
    }

    fn condition(&self, snapshot: &Value, _: &()) -> Result<bool> {
        Ok((self.predicate)(snapshot))
    }

    fn pump(&mut self, remaining: Duration) -> Result<()> {
        if self.progress {
            self.delay = Duration::ZERO;
            return Ok(());
        }
        self.delay = (self.delay + Duration::from_millis(50)).min(Duration::from_millis(200));
        thread::sleep(self.delay.min(remaining));
        Ok(())
    }

    fn diagnostics(&self) -> String {
        match &self.failure {
            Some(error) => format!("{error}; {}", summary(&self.last)),
            None => summary(&self.last),
        }
    }
}

impl Runner {
    fn adb_output(&self, args: &[&str], budget: Duration) -> Result<Output> {
        ui_process::output(
            Command::new(&self.adb)
                .args(["-s", &self.serial])
                .args(args),
            budget,
            OUTPUT_LIMIT,
        )
    }

    fn adb_with_budget(&self, args: &[&str], budget: Duration) -> Result<String> {
        let result = self.adb_output(args, budget)?;
        if !result.status.success() {
            return Err(format!(
                "ADB command failed with status {:?}",
                result.status.code()
            ));
        }
        Ok(String::from_utf8_lossy(&result.stdout).into_owned())
    }

    fn adb(&self, args: &[&str]) -> Result<String> {
        self.adb_with_budget(args, Duration::from_secs(20))
    }

    fn request(
        &mut self,
        mut request: Value,
        budget: Duration,
    ) -> std::result::Result<Value, ReplyFailure> {
        let deadline = Instant::now() + budget;
        self.sequence += 1;
        request["id"] = json!(self.sequence);
        let encoded =
            STANDARD.encode(serde_json::to_vec(&request).map_err(|_| {
                ReplyFailure::Uncertain("Smoke request could not be encoded".into())
            })?);
        let args = [
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
        ];
        self.adb_with_budget(&args, deadline.saturating_duration_since(Instant::now()))
            .map_err(ReplyFailure::Uncertain)?;
        let mut sent = Instant::now();
        let mut pause = Duration::ZERO;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ReplyFailure::Uncertain(
                    "Smoke acknowledgement deadline expired; inspect logcat.txt".into(),
                ));
            }
            let output = self
                .adb_output(&["shell", "pidof", PACKAGE], remaining)
                .map_err(ReplyFailure::Uncertain)?;
            let text = String::from_utf8_lossy(&output.stdout);
            let pids = text.split_whitespace().collect::<Vec<_>>();
            if !output.status.success() && output.status.code() != Some(1) {
                return Err(ReplyFailure::Uncertain(
                    "Could not inspect the smoke APK process".into(),
                ));
            }
            if pids.len() > 1 && !self.allow_restart {
                return Err(ReplyFailure::Uncertain(
                    "Smoke APK has multiple processes during an ordinary action".into(),
                ));
            }
            if let Some(pid) = pids.last() {
                if !pid.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(ReplyFailure::Uncertain(
                        "Smoke APK returned an invalid process identity".into(),
                    ));
                }
                if self.seen_pid.as_deref().is_some_and(|seen| seen != *pid) && !self.allow_restart
                {
                    return Err(ReplyFailure::Uncertain(
                        "Smoke APK restarted unexpectedly; inspect logcat.txt".into(),
                    ));
                }
                self.seen_pid = Some((*pid).into());
                let logs = self
                    .adb_with_budget(
                        &["logcat", "--pid", pid, "-d", "-t", "500"],
                        deadline.saturating_duration_since(Instant::now()),
                    )
                    .map_err(ReplyFailure::Uncertain)?;
                if logs.lines().any(|line| {
                    line.contains("Fatal signal")
                        || line.contains("FATAL EXCEPTION")
                        || line.contains("panicked at")
                }) {
                    return Err(ReplyFailure::Uncertain(
                        "Smoke APK crashed; inspect logcat.txt".into(),
                    ));
                }
                for line in logs.lines().rev() {
                    if let Some((_, body)) = line.split_once("LINCE_SMOKE ") {
                        if let Ok(reply) = serde_json::from_str::<Value>(body) {
                            if reply["id"] == self.sequence {
                                fs::write(
                                    self.output.join("snapshot.json"),
                                    serde_json::to_vec_pretty(&reply).map_err(|_| {
                                        ReplyFailure::Uncertain(
                                            "Smoke snapshot could not be encoded".into(),
                                        )
                                    })?,
                                )
                                .map_err(|_| {
                                    ReplyFailure::Uncertain(
                                        "Smoke snapshot could not be saved".into(),
                                    )
                                })?;
                                if let Some(error) = reply["error"].as_str() {
                                    return Err(ReplyFailure::Rejected(error.into()));
                                }
                                return Ok(reply);
                            }
                        }
                    }
                }
            } else if self.seen_pid.is_some() && !self.allow_restart {
                return Err(ReplyFailure::Uncertain(
                    "Smoke APK exited unexpectedly; inspect logcat.txt".into(),
                ));
            }
            if request["command"] == "snapshot" && sent.elapsed() >= Duration::from_millis(200) {
                self.adb_with_budget(&args, deadline.saturating_duration_since(Instant::now()))
                    .map_err(ReplyFailure::Uncertain)?;
                sent = Instant::now();
            }
            pause = (pause + Duration::from_millis(50)).min(Duration::from_millis(200));
            thread::sleep(pause.min(deadline.saturating_duration_since(Instant::now())));
        }
    }

    fn drive(
        &mut self,
        name: &str,
        action: Option<SmokeAction>,
        budget: Duration,
        predicate: impl Fn(&Value) -> bool,
    ) -> Result<Value> {
        let selector = match &action {
            Some(SmokeAction::Press { primary, .. }) => Selector::new(primary).with_role("button"),
            Some(SmokeAction::Edit { key, .. }) => Selector::new(key).with_role("text"),
            None => Selector::new(name),
        };
        let fallback = matches!(
            &action,
            Some(SmokeAction::Press {
                alternate: Some(_),
                ..
            })
        );
        let step = Step {
            name: name.into(),
            selector,
            action,
            condition: (),
            fallback,
        };
        let mut backend = Android {
            runner: self,
            predicate: &predicate,
            last: Value::Null,
            progress: false,
            delay: Duration::ZERO,
            failure: None,
        };
        let report = Driver::new(Limits {
            timeout: budget,
            max_polls: 512,
        })
        .run(&mut backend, &step);
        eprintln!(
            "Smoke step {}: {:?}; {} polls, {} dispatches, {:.0} ms",
            report.step,
            report.outcome,
            report.metrics.polls,
            report.metrics.dispatches,
            report.metrics.elapsed.as_secs_f64() * 1000.0
        );
        if !report.is_success() {
            return Err(format!(
                "Smoke step {} failed: {:?}; {}",
                report.step, report.outcome, report.diagnostics
            ));
        }
        Ok(backend.last)
    }

    fn wait_for(&mut self, label: &str, predicate: impl Fn(&Value) -> bool) -> Result<Value> {
        self.drive(label, None, Duration::from_secs(45), predicate)
    }

    fn press(&mut self, label: &str, predicate: impl Fn(&Value) -> bool) -> Result<Value> {
        self.press_any(label, None, predicate)
    }

    fn press_any(
        &mut self,
        primary: &str,
        alternate: Option<&str>,
        predicate: impl Fn(&Value) -> bool,
    ) -> Result<Value> {
        self.drive(
            primary,
            Some(SmokeAction::Press {
                primary: primary.into(),
                alternate: alternate.map(str::to_owned),
            }),
            Duration::from_secs(
                if matches!(
                    primary,
                    "Confirm" | "Create and open fresh profile" | "Open original profile"
                ) {
                    45
                } else {
                    15
                },
            ),
            predicate,
        )
    }

    fn edit(&mut self, key: &str, value: &str) -> Result<()> {
        self.drive(
            &format!("edit {key}"),
            Some(SmokeAction::Edit {
                key: key.into(),
                value: value.into(),
            }),
            Duration::from_secs(15),
            |snapshot| {
                snapshot["fields"].as_array().is_some_and(|fields| {
                    fields
                        .iter()
                        .any(|field| field["key"] == key && field["value"] == value)
                })
            },
        )?;
        Ok(())
    }

    fn launch(&mut self) -> Result<()> {
        self.seen_pid = None;
        self.allow_restart = false;
        self.adb(&["shell", "am", "start", "-W", "-n", COMPONENT])?;
        Ok(())
    }

    fn exercise(&mut self) -> Result<()> {
        self.launch()?;
        self.wait_for("initial startup", |v| {
            v["ready"] == true && buttons(v, "Create my Organ") == 1
        })?;
        self.allow_restart = false;
        self.press("Create my Organ", |v| buttons(v, "Confirm") == 1)?;
        self.press("Confirm", |v| v["ready"] == true && v["setup"] == false)?;
        self.press("Pages", |v| buttons(v, "Records") == 1)?;
        self.press("Records", |v| {
            v["page"] == "Records" && buttons(v, "New Record") == 1
        })?;
        let form = self.press("New Record", |v| head_key(v).is_some())?;
        let key = head_key(&form).ok_or("missing title field")?;
        let title = "Tarefa de amanhã — café ☕";
        self.edit(key, title)?;
        self.press_any("Save Title", Some("Save title"), |v| {
            v["record"]["head"] == title
                && v["status"]
                    .as_str()
                    .is_some_and(|status| status.contains("Saved"))
        })?;
        self.adb(&["shell", "am", "force-stop", PACKAGE])?;
        self.launch()?;
        self.wait_for("persisted Unicode title", |v| {
            v["ready"] == true
                && v["fields"]
                    .as_array()
                    .is_some_and(|fields| fields.iter().any(|field| field["value"] == title))
        })?;
        self.allow_restart = false;
        self.press("Pages", |v| buttons(v, "Organ") == 1)?;
        self.press("Organ", |v| {
            v["page"] == "Organ"
                && v["fields"]
                    .as_array()
                    .is_some_and(|fields| fields.iter().any(|field| field["key"] == "profile/name"))
        })?;
        self.edit("profile/name", "Second test profile")?;
        self.allow_restart = true;
        self.press("Create and open fresh profile", |v| {
            v["ready"] == true && v["setup"] == true && buttons(v, "Open original profile") == 1
        })?;
        self.press("Open original profile", |v| {
            v["ready"] == true && v["setup"] == false
        })?;
        self.allow_restart = false;
        self.press("Pages", |v| buttons(v, "Records") == 1)?;
        self.press("Records", |v| buttons(v, title) == 1)?;
        Ok(())
    }

    fn collect(&self) -> Result<()> {
        let uid = self.adb(&["shell", "cmd", "package", "list", "packages", "-U", PACKAGE])?;
        if let Some(uid) = uid
            .split("uid:")
            .nth(1)
            .and_then(|uid| uid.split_whitespace().next())
        {
            let logs = self.adb(&["logcat", &format!("--uid={uid}"), "-d", "-t", "4000"])?;
            fs::write(self.output.join("logcat.txt"), logs).map_err(|error| error.to_string())?;
        }
        let screenshot =
            self.adb_output(&["exec-out", "screencap", "-p"], Duration::from_secs(20))?;
        if screenshot.status.success() {
            fs::write(self.output.join("screen.png"), screenshot.stdout)
                .map_err(|error| error.to_string())?;
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

pub(crate) fn run(root: &Path, args: &[OsString]) -> Result<()> {
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
    let sdk = crate::android::sdk_path(root, None)?;
    let badging = ui_process::output(
        Command::new(sdk.join("build-tools/35.0.0/aapt2"))
            .args(["dump", "badging"])
            .arg(&apk),
        Duration::from_secs(20),
        OUTPUT_LIMIT,
    )?;
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
        seen_pid: None,
        allow_restart: false,
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
    runner.adb_with_budget(
        &["install", "-r", apk.to_str().ok_or("invalid APK path")?],
        Duration::from_secs(120),
    )?;
    let cleared = runner.adb(&["shell", "pm", "clear", PACKAGE])?;
    if !cleared.contains("Success") {
        return Err(format!("Could not reset smoke data: {cleared}"));
    }
    let result = runner.exercise();
    let collected = runner.collect();
    let stopped = runner.adb(&["shell", "am", "force-stop", PACKAGE]);
    result?;
    collected?;
    stopped?;
    println!(
        "Android install, launch, edit, restart and separate profiles passed. Evidence: {}",
        runner.output.display()
    );
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Directory(PathBuf);

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fake(acknowledge_press: bool) -> (Runner, Directory) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = Directory(
            std::env::temp_dir().join(format!("lince-smoke-driver-{}-{stamp}", std::process::id())),
        );
        fs::create_dir(&directory.0).unwrap();
        let executable = directory.0.join("adb");
        let root = format!("'{}'", directory.0.to_string_lossy().replace('\'', "'\\''"));
        let script = r#"#!/bin/sh
root=ROOT_TOKEN
shift 2
case "$1 $2" in
  'shell pidof') printf '12345\n' ;;
  'shell am')
    for encoded do :; done
    request=$(printf %s "$encoded" | base64 -d)
    id=$(printf %s "$request" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
    case "$request" in
      *'"command":"press"'*)
        printf x >> "$root/mutations"
        if test ACK_TOKEN = no; then exit 0; fi
        ;;
    esac
    printf '{"id":%s,"error":null,"buttons":["Save title"],"done":true}' "$id" > "$root/reply"
    ;;
  'logcat --pid') printf 'LINCE_SMOKE '; cat "$root/reply" ;;
esac
"#
        .replace("ROOT_TOKEN", &root)
        .replace("ACK_TOKEN", if acknowledge_press { "yes" } else { "no" });
        fs::write(&executable, script).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        (
            Runner {
                adb: executable,
                serial: "fake".into(),
                output: directory.0.clone(),
                sequence: 0,
                seen_pid: None,
                allow_restart: false,
            },
            directory,
        )
    }

    #[test]
    fn alternate_label_is_used_before_acceptance_and_only_one_press_is_sent() {
        let (mut runner, directory) = fake(true);
        runner
            .drive(
                "save",
                Some(SmokeAction::Press {
                    primary: "Save Title".into(),
                    alternate: Some("Save title".into()),
                }),
                Duration::from_secs(2),
                |snapshot| snapshot["done"] == true,
            )
            .unwrap();
        assert_eq!(fs::read(directory.0.join("mutations")).unwrap(), b"x");
    }

    #[test]
    fn missing_acknowledgement_never_replays_an_already_submitted_press() {
        let (mut runner, directory) = fake(false);
        let failure = runner
            .drive(
                "save",
                Some(SmokeAction::Press {
                    primary: "Save title".into(),
                    alternate: Some("Save title".into()),
                }),
                Duration::from_millis(300),
                |snapshot| snapshot["done"] == true,
            )
            .unwrap_err();
        assert!(failure.contains("refusing to replay"));
        assert_eq!(fs::read(directory.0.join("mutations")).unwrap(), b"x");
    }
}
