use super::*;
use std::os::unix::{ffi::OsStringExt, fs::DirBuilderExt};

#[derive(Debug, PartialEq)]
struct Identity {
    start: u64,
    device: u64,
    inode: u64,
}

fn start_time(stat: &str) -> Option<u64> {
    stat.rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

fn payload_digest(path: &Path) -> Result<String> {
    let output = Command::new("timeout")
        .args(["15s", "sha256sum", "--zero", "--"])
        .arg(path)
        .output()
        .map_err(error)?;
    if !output.status.success() {
        return Err(format!(
            "Could not hash the observed payload: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let bytes = output
        .stdout
        .get(..64)
        .filter(|bytes| bytes.iter().all(u8::is_ascii_hexdigit))
        .ok_or("Invalid payload checksum output")?;
    String::from_utf8(bytes.to_vec()).map_err(error)
}

fn identity(process: &Path) -> Result<Identity> {
    if fs::metadata(process).map_err(error)?.uid() != rustix::process::geteuid().as_raw() {
        return Err("Observe only a process owned by the current user".into());
    }
    let stat = fs::read_to_string(process.join("stat")).map_err(error)?;
    let executable = fs::metadata(process.join("exe")).map_err(error)?;
    Ok(Identity {
        start: start_time(&stat).ok_or("Invalid process start time")?,
        device: executable.dev(),
        inode: executable.ino(),
    })
}

fn unit_state(unit: Option<&OsString>) -> serde_json::Value {
    let Some(unit) = unit else {
        return serde_json::Value::Null;
    };
    match Command::new("timeout").args(["10s", "systemctl", "--user", "show"])
        .args(["--property=ActiveState,SubState,MainPID,ExecMainCode,ExecMainStatus,InvocationID,ControlGroup", "--"])
        .arg(unit).output() {
        Ok(output) => serde_json::json!({"exit": output.status.code(),
            "properties": String::from_utf8_lossy(&output.stdout), "error": String::from_utf8_lossy(&output.stderr)}),
        Err(failure) => serde_json::json!({"unavailable": failure.to_string()}),
    }
}

fn session_state(session: Option<&String>) -> serde_json::Value {
    let Some(session) = session else {
        return serde_json::Value::Null;
    };
    match Command::new("timeout")
        .args([
            "10s",
            "loginctl",
            "show-session",
            "--property=Id,User,Service,Class,Type,Desktop,Seat,VTNr,Active,State",
            "--",
        ])
        .arg(session)
        .output()
    {
        Ok(output) => serde_json::json!({"exit": output.status.code(),
            "properties": String::from_utf8_lossy(&output.stdout), "error": String::from_utf8_lossy(&output.stderr)}),
        Err(failure) => serde_json::json!({"unavailable": failure.to_string()}),
    }
}

fn process_environment(process: &Path) -> Result<(BTreeMap<String, String>, Option<PathBuf>)> {
    let bytes = fs::read(process.join("environ")).map_err(error)?;
    let values: BTreeMap<_, _> = bytes
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            let position = entry.iter().position(|byte| *byte == b'=')?;
            let key = std::str::from_utf8(&entry[..position]).ok()?;
            let value = std::str::from_utf8(&entry[position + 1..]).ok()?;
            Some((key.to_owned(), value.to_owned()))
        })
        .collect();
    let arguments: Vec<_> = fs::read(process.join("cmdline"))
        .map_err(error)?
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .map(|value| OsString::from_vec(value.to_vec()))
        .take_while(|value| value != "--")
        .collect();
    let directory = arguments
        .windows(2)
        .find(|pair| pair[0] == "--directory")
        .map(|pair| PathBuf::from(&pair[1]))
        .or_else(|| {
            values
                .get("XDG_CONFIG_HOME")
                .filter(|value| Path::new(value).is_absolute())
                .map(|value| PathBuf::from(value).join("lince"))
        })
        .or_else(|| {
            values
                .get("HOME")
                .filter(|value| Path::new(value).is_absolute())
                .map(|value| PathBuf::from(value).join(".config/lince"))
        });
    let directory = directory.and_then(|path| {
        if path.is_absolute() {
            Some(path)
        } else {
            fs::read_link(process.join("cwd"))
                .ok()
                .map(|cwd| cwd.join(path))
        }
    });
    let mut environment: BTreeMap<_, _> = values
        .into_iter()
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "WAYLAND_DISPLAY"
                    | "XDG_CURRENT_DESKTOP"
                    | "XDG_SESSION_DESKTOP"
                    | "XDG_SESSION_TYPE"
                    | "XDG_SESSION_ID"
            )
        })
        .collect();
    let mode = if arguments.iter().any(|value| value == "--park-nested") {
        "nested"
    } else if arguments.iter().any(|value| value == "--park") {
        "physical"
    } else {
        "other"
    };
    environment.insert("observed_mode".into(), mode.into());
    Ok((environment, directory))
}

fn configuration(
    arguments: &[OsString],
) -> Result<(PathBuf, rustix::process::Pid, u64, Option<OsString>)> {
    if arguments.len() < 2 || !arguments.len().is_multiple_of(2) {
        return Err(
            "observe needs an absolute executable, PID and optional flag/value pairs".into(),
        );
    }
    let executable = PathBuf::from(&arguments[0]);
    let pid = arguments[1]
        .to_str()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .and_then(rustix::process::Pid::from_raw)
        .ok_or("Invalid observed PID")?;
    let mut seconds = 5400;
    let mut unit = None;
    for pair in arguments[2..].chunks_exact(2) {
        if pair[0] == "--seconds" {
            seconds = pair[1]
                .to_str()
                .and_then(|value| value.parse().ok())
                .ok_or("Invalid observation duration")?;
        } else if pair[0] == "--unit" {
            let name = pair[1].to_str().ok_or("Invalid user unit")?;
            if name.len() > 256
                || name.contains(['/', '\0'])
                || !(name.ends_with(".service") || name.ends_with(".scope"))
            {
                return Err("Choose a user service or scope name".into());
            }
            unit = Some(pair[1].clone());
        } else {
            return Err("Unknown observation option".into());
        }
    }
    if !executable.is_absolute() || !(5..=21600).contains(&seconds) {
        return Err("Use an absolute executable and 5–21600 observation seconds".into());
    }
    Ok((executable, pid, seconds, unit))
}

pub(super) fn run(arguments: &[OsString]) -> Result<()> {
    if arguments.is_empty() || arguments[0] == "--help" {
        println!(
            "cargo xtask park validate observe /installed/bin/lince PID [--seconds 5400] [--unit USER_UNIT]"
        );
        println!(
            "Read-only process/resource observation; login, physical behavior and saved logout still require acceptance evidence."
        );
        return Ok(());
    }
    let (executable, pid, seconds, unit) = configuration(arguments)?;
    let process = PathBuf::from(format!("/proc/{}", pid.as_raw_nonzero()));
    let expected = identity(&process)?;
    let payload = process_executable(&executable)?;
    let metadata = fs::metadata(&payload).map_err(error)?;
    if (metadata.dev(), metadata.ino()) != (expected.device, expected.inode) {
        return Err("The observed process does not run the selected executable payload".into());
    }
    let information = inspect_build(&executable)?;
    let checksum = payload_digest(&payload)?;
    let (environment, cell_directory) = process_environment(&process)?;
    if identity(&process)? != expected {
        return Err("The observed process changed during identity inspection".into());
    }
    let base = env::current_dir()
        .map_err(error)?
        .join("target/park-validation");
    fs::create_dir_all(&base).map_err(error)?;
    let metadata = fs::symlink_metadata(&base).map_err(error)?;
    if !metadata.is_dir() || metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err("Observation state must be an owned directory, not a symlink".into());
    }
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).map_err(error)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_nanos();
    let directory = base.join(format!("session-{stamp}-{}", std::process::id()));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(error)?;
    let mut log = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(directory.join("resources.jsonl"))
        .map_err(error)?;
    let clock_ticks = Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u64>()
                .ok()
        });
    let mut report = serde_json::json!({"pid": pid.as_raw_nonzero().get(), "process_start": expected.start,
        "observer_pid": std::process::id(), "started_unix_ns": stamp,
        "executable": executable, "payload": payload, "payload_sha256": checksum, "build_info": information,
        "environment": environment, "cell_directory": cell_directory,
        "process_cgroup": fs::read_to_string(process.join("cgroup")).ok(),
        "cell_directory_scope": "Derived from explicit argv or target environment; ownership/persistence acceptance remains separate.",
        "user_unit": unit.as_ref().map(|value| value.to_string_lossy()), "unit_before": unit_state(unit.as_ref()),
        "logind_session_before": session_state(environment.get("XDG_SESSION_ID")),
        "hardware_before": diagnostics::hardware(), "requested_seconds": seconds, "status": "observing", "samples": 0,
        "resources": directory.join("resources.jsonl"), "physical_present_fps": null,
        "cpu_ticks_per_second": clock_ticks,
        "checks": {"clean_login":"pending", "ordinary_application_launch":"pending", "selected_services":"pending",
            "lock_and_recovery":"pending", "work_session":"pending", "saved_logout":"pending",
            "niri_sandbox_return":"pending", "next_login_and_rollback":"pending"}, "daily_use_recommendation":"pending"});
    let _signals = interrupt_handler()?;
    write_json(&directory.join("session.json"), &report)?;
    println!(
        "Observing {} in {}",
        pid.as_raw_nonzero(),
        directory.display()
    );
    let started = Instant::now();
    let mut samples = Vec::new();
    loop {
        if identity(&process).ok().as_ref() != Some(&expected) {
            report["status"] =
                "target exited, became inaccessible or changed identity; exit/save result unproven"
                    .into();
            break;
        }
        let sample = resource_sample(pid, started.elapsed().as_millis());
        if identity(&process).ok().as_ref() != Some(&expected) {
            report["status"] = "target changed during sampling; incomplete sample discarded".into();
            break;
        }
        serde_json::to_writer(&mut log, &sample).map_err(error)?;
        log.write_all(b"\n").map_err(error)?;
        log.flush().map_err(error)?;
        report["latest_resource"] = serde_json::to_value(&sample).map_err(error)?;
        samples.push(sample);
        report["samples"] = samples.len().into();
        report["elapsed_ms"] =
            serde_json::to_value(started.elapsed().as_millis()).map_err(error)?;
        write_json(&directory.join("session.json"), &report)?;
        if check_interruption().is_err() {
            report["status"] = "observer interrupted; target not stopped".into();
            break;
        }
        if started.elapsed() >= Duration::from_secs(seconds) {
            report["status"] = "observation duration completed; target not stopped".into();
            break;
        }
        thread::sleep(
            Duration::from_secs(5)
                .min(Duration::from_secs(seconds).saturating_sub(started.elapsed())),
        );
    }
    report["unit_after"] = unit_state(unit.as_ref());
    report["logind_session_after"] = session_state(environment.get("XDG_SESSION_ID"));
    report["hardware_after"] = diagnostics::hardware();
    report["resource_summary"] = resource_summary(&samples).into();
    write_json(&directory.join("session.json"), &report)?;
    println!("{}", directory.join("session.json").display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_hash_preserves_literal_payload_paths() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("park-observer-hash-{stamp}-{}", std::process::id()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let path = directory.join("a\\b\n$(touch unexpected)");
        fs::write(&path, b"Observed executable identity").unwrap();
        assert_eq!(payload_digest(&path).unwrap(), digest(&path).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn observation_identity_rejects_pid_reuse_and_exec_replacement() {
        use std::io::{BufRead, BufReader};
        let process = PathBuf::from(format!("/proc/{}", std::process::id()));
        let current = identity(&process).unwrap();
        assert_eq!(identity(&process).unwrap(), current);
        let mut command = Command::new("sh");
        command
            .args(["-c", "printf 'ready\\n'; read -r trigger; exec sleep 30"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        let mut children = Children(vec![ManagedChild::spawn(&mut command).unwrap()]);
        let child = &mut children.0[0].child;
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready, "ready\n");
        let process = PathBuf::from(format!("/proc/{}", child.id()));
        let original = identity(&process).unwrap();
        child.stdin.take().unwrap().write_all(b"go\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let replaced = identity(&process).unwrap();
            if replaced != original {
                assert_eq!(replaced.start, original.start);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "The child did not replace its executable"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let stat = format!(
            "1 (a ) b) S {}",
            (0..19)
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );
        assert_eq!(start_time(&stat), Some(18));
    }

    #[test]
    fn observation_arguments_bound_duration_and_only_name_user_units() {
        let parse =
            |values: &[&str]| configuration(&values.iter().map(OsString::from).collect::<Vec<_>>());
        assert!(
            parse(&[
                "/bin/lince",
                "1",
                "--seconds",
                "5400",
                "--unit",
                "wayland-wm@lince.service"
            ])
            .is_ok()
        );
        for values in [
            vec!["relative", "1"],
            vec!["/bin/lince", "0"],
            vec!["/bin/lince", "-1"],
            vec!["/bin/lince", "1", "--seconds", "21601"],
            vec!["/bin/lince", "1", "--unit", "/tmp/a.service"],
            vec!["/bin/lince", "1", "--stop", "yes"],
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
        }
    }
}
