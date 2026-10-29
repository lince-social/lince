use super::*;

const PROVIDERS: &[&str] = &["launcher", "clipboard_history", "ime", "sharing_consent", "sharing_pixels", "helper_reconnect", "notifications_polkit"];
const PHYSICAL: &[&str] = &["both_monitors", "output_controls_and_rollback", "hotplug_and_last_output", "vt_and_fresh_input", "suspend_and_fresh_input", "locker_on_every_head", "locked_wake_no_flash", "locker_loss_stays_protected", "power_provider_cycles", "optional_hardware_limits"];
const DAILY: &[&str] = &["clean_greeter_login", "ordinary_saved_workspace", "ordinary_menus_and_dialogs", "chosen_keyboard_clipboard_ime_and_drops", "launch_focus_move", "tiled_free_and_canvas", "video_and_external_services", "lock_and_recovery", "idle_without_repaint_loop", "no_crash_or_data_loss", "no_wrong_input_or_private_leak", "no_relaunch_or_stuck_input", "sandbox_autostart_scope", "save_aware_logout", "niri_sandbox_return", "next_login_and_rollback"];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    outcome: String,
    detail: String,
    artifact: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attestation {
    scenario: u8,
    source: String,
    payload_sha256: String,
    session_report: Option<PathBuf>,
    observations: BTreeMap<String, Observation>,
}

fn questions(id: u8) -> Result<Vec<&'static str>> {
    match id {
        4 => Ok(PHYSICAL.to_vec()),
        5 => Ok(DAILY.iter().chain(PROVIDERS).copied().collect()),
        _ => Err("Guided observations belong only to B1 and B2".into()),
    }
}

pub(super) fn guide(suite: &Suite, id: u8, directory: &Path) -> Result<Option<String>> {
    let observations = questions(id)?.iter().map(|key| ((*key).into(), Observation { outcome: "waiting".into(), detail: String::new(), artifact: None })).collect();
    let attestation = Attestation { scenario: id, source: suite.source.clone(), payload_sha256: suite.payload_hash.clone().ok_or("Prepare the candidate before the scheduled session")?, session_report: None, observations };
    write_json(&directory.join("observations.json"), &attestation)?;
    let instructions = match id {
        4 => "During the scheduled Park session, verify both monitors and pointer mapping, one position/resolution/scale preview with rollback, one hotplug, VT return and one suspend/wake with the chosen locker. Record one ordinary and one locked display-power recovery. Check protection on both heads; do not deliberately crash the locker in a working session. Locker-loss and denied-capture behavior have separate automatic fixtures. A spare-TTY rehearsal is optional, not an additional mandatory session.",
        5 => "After reviewed host activation, select Park through the existing greeter and external UWSM. Use final observe against the actual compositor PID and UWSM user unit for 900 seconds. Include ordinary browser/terminal work, canvas editing, a saved workspace, a short idle interval, video, chosen keyboard/clipboard/IME/drop behavior and the selected external tools or consent UI. Describe unselected optional tools explicitly. Then save-aware logout, return to Niri/Sandbox, log in again, verify saved state and retain the previous NixOS generation. Submit evidence only after those actions. This is a fifteen-minute sample, not a long-term endurance claim.",
        _ => unreachable!(),
    };
    fs::write(directory.join("instructions.txt"), format!("Workflow {}: {}\n{instructions}\nRecord each observed result with a concrete note and an existing evidence file. These are human observations, not automated optical/backend confirmations.\nSubmit with cargo xtask park validate final attest {} {} {}\n", IDS[(id - 1) as usize], NAMES[(id - 1) as usize], quote(&suite.directory), IDS[(id - 1) as usize], quote(&directory.join("observations.json")))).map_err(error)?;
    println!("{}", directory.join("instructions.txt").display());
    Ok(Some(format!("Awaiting guided observations: {}", directory.join("observations.json").display())))
}

pub(super) fn seat(suite: &mut Suite) -> Result<()> {
    let _signals = interrupt_handler()?;
    let directory = artifact(suite, 4)?;
    suite.scenarios[3].status = "running".into();
    suite.scenarios[3].evidence.push(directory.clone());
    save(suite)?;
    let outcome = (|| {
        require(std::io::stdin().is_terminal(), "Run the physical scenario only from the scheduled spare foreground TTY")?;
        let prepared = suite.prepared.as_ref().ok_or("Missing prepared physical bundle")?;
        super::super::seat(prepared)?;
        let result: Value = load(&prepared.join("results.json"))?;
        let phases = result["phases"].as_array().ok_or("Missing physical execution phases")?;
        require(!phases.iter().any(|phase| phase["outcome"] == "failed"), "Physical execution has failed phases")?;
        require(phases.iter().any(|phase| phase["name"] == "session-exit" && phase["outcome"] == "passed"), "Physical execution has no confirmed save-aware exit")?;
        suite.scenarios[3].evidence.push(prepared.clone());
        guide(suite, 4, &directory)
    })();
    match outcome {
        Ok(detail) => { suite.scenarios[3].automatic = true; suite.scenarios[3].status = "waiting".into(); suite.scenarios[3].detail = detail.unwrap_or_default(); }
        Err(failure) => { suite.scenarios[3].status = "fail".into(); suite.scenarios[3].automatic = false; suite.scenarios[3].detail = failure; }
    }
    save(suite)?;
    require(suite.scenarios[3].status != "fail", suite.scenarios[3].detail.clone())
}

pub(super) fn observe(suite: &mut Suite, arguments: &[OsString]) -> Result<()> {
    require(arguments.len() % 2 == 1, "Observe needs a PID followed by option/value pairs")?;
    let pid = arguments.first().ok_or("Missing Park PID")?;
    let mut observer = vec![scenarios::candidate(suite)?.as_os_str().to_owned(), pid.clone()];
    observer.extend_from_slice(&arguments[1..]);
    if !arguments.iter().any(|argument| argument == "--seconds") { observer.extend(["--seconds".into(), "900".into()]); }
    require(arguments.iter().any(|argument| argument == "--unit"), "Choose the actual UWSM compositor user unit with --unit")?;
    let target = pid.to_str().and_then(|value| value.parse::<u32>().ok()).filter(|pid| *pid > 1).ok_or("Invalid observed PID")?;
    let process = PathBuf::from(format!("/proc/{target}"));
    require(fs::metadata(&process).map_err(error)?.uid() == rustix::process::geteuid().as_raw(), "Observe only this user's compositor")?;
    let environment = fs::read(process.join("environ")).map_err(error)?;
    require(environment.split(|byte| *byte == 0).any(|entry| entry == b"XDG_CURRENT_DESKTOP=Lince"), "The observed process is not a Lince session")?;
    let arguments_bytes = fs::read(process.join("cmdline")).map_err(error)?;
    require(arguments_bytes.split(|byte| *byte == 0).any(|entry| entry == b"--park") && !arguments_bytes.split(|byte| *byte == 0).any(|entry| entry == b"--park-nested"), "Daily-use observation requires the physical Park process")?;
    let directory = artifact(suite, 5)?;
    suite.scenarios[4].status = "running".into();
    suite.scenarios[4].evidence.push(directory.clone());
    save(suite)?;
    let outcome = (|| {
        let observed = observe::run_into(&observer, Some(&directory))?;
        let result: Value = load(&observed.join("session.json"))?;
        require(result["environment"]["observed_mode"] == "physical" && result["environment"]["XDG_CURRENT_DESKTOP"] == "Lince", "Daily-use observation must belong to the physical Lince session")?;
        guide(suite, 5, &directory)?;
        let mut draft: Attestation = load(&directory.join("observations.json"))?;
        draft.session_report = Some(observed.join("session.json"));
        write_json(&directory.join("observations.json"), &draft)?;
        Ok::<_, String>(observed)
    })();
    match outcome {
        Ok(observed) => {
            suite.scenarios[4].evidence.push(observed.clone());
            suite.scenarios[4].status = "waiting".into();
            suite.scenarios[4].automatic = true;
            suite.scenarios[4].detail = format!("Observation captured in {}; complete the guided logout/return evidence", observed.display());
            let physical = artifact(suite, 4)?;
            guide(suite, 4, &physical)?;
            let mut draft: Attestation = load(&physical.join("observations.json"))?;
            draft.session_report = Some(observed.join("session.json"));
            write_json(&physical.join("observations.json"), &draft)?;
            suite.scenarios[3].automatic = true;
            suite.scenarios[3].status = "waiting".into();
            suite.scenarios[3].detail = format!("Physical-session evidence captured; complete {}", physical.join("observations.json").display());
            suite.scenarios[3].evidence.push(physical);
        }
        Err(failure) => { suite.scenarios[4].status = "fail".into(); suite.scenarios[4].automatic = false; suite.scenarios[4].detail = failure; }
    }
    save(suite)?;
    require(suite.scenarios[4].status != "fail", suite.scenarios[4].detail.clone())
}

fn owned_artifact(path: &Path, suite: &Suite) -> Result<PathBuf> {
    require(path.is_absolute(), "Evidence paths must be absolute")?;
    let metadata = fs::symlink_metadata(path).map_err(error)?;
    require(metadata.is_file() && metadata.uid() == rustix::process::geteuid().as_raw() && metadata.len() > 0, "Evidence must be a nonempty owned regular file")?;
    let canonical = fs::canonicalize(path).map_err(error)?;
    require(canonical.starts_with(&suite.directory), "Copy observation artifacts into this suite's private directory")?;
    Ok(canonical)
}

fn property<'a>(report: &'a Value, key: &str) -> Result<&'a str> {
    report["unit_before"]["properties"].as_str().and_then(|properties| properties.lines().find_map(|line| line.strip_prefix(format!("{key}=").as_str())))
        .filter(|value| !value.is_empty()).ok_or_else(|| format!("Missing observed user-unit property {key}"))
}

fn session_journal(suite: &Suite, observed: &Value, directory: &Path) -> Result<PathBuf> {
    require(observed["payload_sha256"].as_str() == suite.payload_hash.as_deref(), "Observed login uses a different compositor payload")?;
    let pid = observed["pid"].as_i64().ok_or("Missing observed compositor PID")?;
    require(property(observed, "MainPID")?.parse::<i64>().map_err(error)? == pid, "Selected UWSM unit did not own the observed compositor")?;
    let invocation = property(observed, "InvocationID")?;
    require(invocation.len() == 32 && invocation.bytes().all(|byte| byte.is_ascii_hexdigit()), "Invalid observed UWSM invocation ID")?;
    let log = directory.join("managed-session.log");
    command(Command::new("journalctl").args(["--user", "--no-pager", "--output=cat", &format!("_SYSTEMD_INVOCATION_ID={invocation}"), "--grep=Park (UWSM readiness completed|orderly logout completed|power (request|observed):)" ]), &log, 30)?;
    Ok(log)
}

fn power(log: &str) -> Result<Value> {
    let mut off = BTreeMap::new();
    let mut requests = BTreeMap::new();
    let mut outputs: BTreeMap<String, (usize, bool, bool)> = BTreeMap::new();
    let mut maximum_ms = 0.0_f64;
    for line in log.lines() {
        let request = line.strip_prefix("Park power request: ");
        let Some(line) = request.or_else(|| line.strip_prefix("Park power observed: ")) else { continue; };
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() >= 6 && fields[2] == "epoch" {
            let epoch = fields[3].parse::<u64>().map_err(error)?;
            let time = fields.last().ok_or("Missing power timestamp")?.parse::<f64>().map_err(error)?;
            require(time.is_finite(), "Invalid power timestamp")?;
            let key = (fields[0].to_owned(), fields[1].to_owned(), epoch);
            if request.is_some() { requests.insert(key, time); continue; }
            let Some(started) = requests.remove(&key) else { continue; };
            let elapsed = time - started;
            require((0.0..=2.0).contains(&elapsed), "A matched provider power observation exceeded its original two-second deadline")?;
            maximum_ms = maximum_ms.max(elapsed * 1000.0);
            if fields[1] == "off" { off.insert(fields[0].to_owned(), epoch); }
            if fields[1] == "on" && off.get(fields[0]).is_some_and(|previous| epoch > *previous) {
                off.remove(fields[0]);
                let cycles = outputs.entry(fields[0].to_owned()).or_default();
                cycles.0 += 1;
                cycles.1 |= fields.get(5) == Some(&"true");
                cycles.2 |= fields.get(5) == Some(&"false");
            }
        }
    }
    require(outputs.values().any(|(cycles, protected, public)| *cycles >= 2 && *protected && *public), "Physical power trace needs two matched advancing cycles on one output, including protected and ordinary wake")?;
    Ok(json!({"outputs":outputs,"maximum_request_confirmation_ms":maximum_ms}))
}

pub(super) fn attest(suite: &mut Suite, id: u8, path: &Path) -> Result<()> {
    let required = questions(id)?;
    require(path.is_absolute(), "Use an absolute observation JSON path")?;
    let receipt: Attestation = load(path)?;
    require(receipt.scenario == id && receipt.source == suite.source && Some(&receipt.payload_sha256) == suite.payload_hash.as_ref(), "Observation report belongs to a different scenario/source/payload")?;
    require(suite.scenarios[(id - 1) as usize].automatic, "Run this scenario's automatic preparation before submitting observations")?;
    require(receipt.observations.len() == required.len(), "Observation report omitted or substituted conditions")?;
    let directory = artifact(suite, id)?;
    let mut artifacts = BTreeMap::new();
    for key in &required {
        let observation = receipt.observations.get(*key).ok_or_else(|| format!("Missing observation {key}"))?;
        require(observation.detail.trim().len() >= 12, format!("Describe the actual {key} observation"))?;
        let optional = (*key == "optional_hardware_limits" && id == 4) || (id == 5 && ["clipboard_history", "ime", "notifications_polkit"].contains(key));
        require(observation.outcome == "pass" || optional && observation.outcome == "not_selected", format!("Observation {key} has not passed"))?;
        let artifact = owned_artifact(observation.artifact.as_deref().ok_or_else(|| format!("Attach evidence for {key}"))?, suite)?;
        artifacts.insert((*key).to_owned(), json!({"path":artifact,"sha256":digest(&artifact)?}));
    }
    if id == 4 {
        let log = if let Some(path) = receipt.session_report.as_deref() {
            let path = owned_artifact(path, suite)?;
            require(suite.scenarios[4].evidence.iter().any(|evidence| path.starts_with(evidence)), "Physical report is outside the recorded session observation")?;
            let observed = evidence_report(&path)?;
            fs::read_to_string(session_journal(suite, &observed, &directory)?).map_err(error)?
        } else {
            let prepared = suite.prepared.as_ref().ok_or("Attach the suite's observed physical session report")?;
            fs::read_to_string(prepared.join("session.log")).map_err(error)?
        };
        write_json(&directory.join("power-summary.json"), &power(&log)?)?;
    }
    if id == 5 {
        let path = owned_artifact(receipt.session_report.as_deref().ok_or("Attach the suite's observed session report")?, suite)?;
        require(suite.scenarios[4].evidence.iter().any(|evidence| path.starts_with(evidence)), "Session report is not from this suite's observer")?;
        let observed: Value = load(&path)?;
        require(observed["environment"]["observed_mode"] == "physical" && observed["environment"]["XDG_CURRENT_DESKTOP"] == "Lince", "Daily-use report is not a physical Park session")?;
        require(observed["elapsed_ms"].as_u64().is_some_and(|elapsed| elapsed >= 900_000) && observed["status"] == "observation duration completed; target not stopped", "Daily use needs a complete uninterrupted fifteen-minute observation")?;
        let log = session_journal(suite, &observed, &directory)?;
        let text = fs::read_to_string(log).map_err(error)?;
        require(text.lines().any(|line| line == "Park UWSM readiness completed") && text.lines().any(|line| line == "Park orderly logout completed"), "Actual managed readiness and save-aware logout are not confirmed by the selected unit's journal")?;
    }
    write_json(&directory.join("observations.json"), &receipt)?;
    write_json(&directory.join("artifacts.json"), &artifacts)?;
    let scenario = &mut suite.scenarios[(id - 1) as usize];
    scenario.status = "pass".into();
    scenario.detail = "Automatic evidence plus the recorded user observations passed; manual observations remain explicitly labelled".into();
    scenario.evidence.push(directory);
    save(suite)?;
    report(suite)
}
