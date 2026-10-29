use super::*;
use lince_interface::host::control::{Context, Disposition, Entry, Error as ControlError, Invocation, Response};

const CHECKS: [&str; 11] = ["authenticated-peer", "bounded-snapshot", "native-windows", "workspace-output-refresh", "subscription-resync", "actual-command", "explicit-restoration", "denied-launch", "denied-output", "native-lifecycle", "connection-closed"];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Prepared {
    probe_sha256: String,
    protocol_sha256: String,
    nested_report_sha256: String,
}

pub(super) fn grants() -> Vec<String> {
    ["window-list", "desktop-control", "window-control"].into_iter().map(str::to_owned).collect()
}

pub(super) fn configuration(configuration: &Configuration) -> Result<()> {
    if configuration.automation && configuration.helper.is_some() {
        return Err("The automation fixture and a selected provider helper use separate prepared runs; keep the provider choice and use automation:true in a second private fixture.".into());
    }
    Ok(())
}

fn prepare_probe(directory: &Path) -> Result<String> {
    if let Some(probe) = prebuilt_probe("control-probe")? {
        let target = directory.join("bin/control-probe");
        fs::copy(probe, &target).map_err(error)?;
        return digest(&target);
    }
    let mut command = cargo_command();
    command.args(["run", "--locked", "--message-format=json", "-p", "lince-park", "--features", "server-tests", "--example", "park_control_probe", "--", "--help"]);
    let output = command.output().map_err(error)?;
    fs::write(directory.join("prepare-control-probe.log"), &output.stderr).map_err(error)?;
    if !output.status.success() { return Err("Preparing the automation probe failed".into()); }
    let binary = output.stdout.split(|byte| *byte == b'\n').filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|value| value["reason"] == "compiler-artifact" && value["target"]["name"] == "park_control_probe")
        .filter_map(|value| value["executable"].as_str().map(PathBuf::from)).last().ok_or("Cargo did not report the automation probe executable")?;
    let target = directory.join("bin/control-probe");
    fs::copy(binary, &target).map_err(error)?;
    digest(&target)
}

#[derive(Clone, Copy)]
struct Expected<'a> {
    payload: &'a Path,
    payload_sha256: &'a str,
    probe: &'a Path,
    probe_sha256: &'a str,
    native: &'a Path,
    native_sha256: &'a str,
    protocol_sha256: &'a str,
    data: &'a Path,
}

fn process(value: &serde_json::Value, path: &Path, hash: &str) -> Result<u32> {
    let pid = value["pid"].as_u64().and_then(|pid| u32::try_from(pid).ok()).filter(|pid| *pid > 1).ok_or("Missing probe process identity")?;
    if value["start"].as_u64().is_none_or(|start| start == 0)
        || value["payload"].as_str().map(Path::new) != Some(path)
        || value["sha256"] != hash
    { return Err("The automation report does not belong to the prepared process/payload".into()); }
    Ok(pid)
}

fn snapshot(value: &serde_json::Value) -> Result<(u64, Vec<Entry>, Context)> {
    let revision = value["revision"].as_u64().filter(|revision| *revision > 0).ok_or("Missing automation snapshot revision")?;
    let entries: Vec<Entry> = serde_json::from_value(value["entries"].clone()).map_err(error)?;
    if entries.len() > 8192 || !entries.iter().any(|entry| matches!(entry, Entry::Session { locked: false, active: true })) { return Err("The automation snapshot is oversized or inactive".into()); }
    let context = entries.iter().find_map(|entry| match entry { Entry::Context { value: Some(context) } => Some(context.clone()), _ => None }).filter(Context::valid).ok_or("The automation snapshot has no valid target")?;
    Ok((revision, entries, context))
}

fn same_view(left: &Context, right: &Context) -> bool {
    (left.owner, &left.canvas, left.workspace, left.output) == (right.owner, &right.canvas, right.workspace, right.output)
}

fn validate(report: &serde_json::Value, expected: Expected<'_>, physical: Option<(&Path, u32)>) -> Result<()> {
    if report["status"] != "passed" || report["protocol_sha256"] != expected.protocol_sha256 || report["native_exit"] != 0 { return Err("The automation report did not pass its prepared protocol/lifecycle scope".into()); }
    let compositor = process(&report["compositor"], expected.payload, expected.payload_sha256)?;
    let probe = process(&report["probe"], expected.probe, expected.probe_sha256)?;
    let native = process(&report["native"], expected.native, expected.native_sha256)?;
    if report["compositor"]["data_directory"].as_str().map(Path::new) != Some(expected.data) { return Err("The automation report belongs to a different private Cell directory".into()); }
    if [compositor == probe, compositor == native, probe == native].into_iter().any(|same| same) { return Err("The compositor, SDK probe and native client must be separate processes".into()); }
    let socket = report["socket"].as_str().map(Path::new).filter(|path| path.is_absolute()).ok_or("The automation report has no absolute private socket")?;
    if physical.is_some_and(|(actual_socket, pid)| actual_socket != socket || pid != compositor) { return Err("The automation report belongs to a different physical Park session".into()); }
    let checks: Vec<String> = serde_json::from_value(report["checks"].clone()).map_err(error)?;
    if checks != CHECKS { return Err("The automation report omitted or substituted a required observation".into()); }
    let (before_revision, before, original) = snapshot(&report["before"])?;
    let (toggle_revision, _, toggled) = snapshot(&report["after_toggle"])?;
    let (restore_revision, _, restored) = snapshot(&report["after_restore"])?;
    let (closed_revision, closed, _) = snapshot(&report["after_close"])?;
    if toggle_revision < before_revision || restore_revision < toggle_revision || closed_revision < restore_revision
        || !same_view(&original, &toggled) || !same_view(&original, &restored)
    { return Err("The original view or monotonic snapshot revision was lost during explicit restoration".into()); }
    if before.iter().filter(|entry| matches!(entry, Entry::Window { app_id, .. } if app_id == "social.lince.park.validation")).count() != 2
        || closed.iter().any(|entry| matches!(entry, Entry::Window { app_id, .. } if app_id == "social.lince.park.validation"))
        || !before.iter().any(|entry| matches!(entry, Entry::Workspace { id, .. } if *id == original.workspace))
        || !before.iter().any(|entry| matches!(entry, Entry::Output { id, enabled: true, refresh_mhz: Some(refresh), modes, .. } if Some(*id) == original.output && *refresh > 0 && !modes.is_empty()))
    { return Err("The automation report did not observe the actual native lifecycle and enabled output/workspace/refresh".into()); }
    let subscription = &report["subscription"];
    if subscription["revision"].as_u64().is_none_or(|revision| revision == 0 || revision > before_revision)
        || subscription["scope"].as_u64().is_none_or(|scope| scope & u64::from(lince_interface::host::control::CHANGE_RESYNC) == 0)
    { return Err("The automation probe did not consume its initial snapshot-resync event".into()); }
    let calls = report["calls"].as_array().filter(|calls| calls.len() == 4).ok_or("Expected exactly four correlated invocations without a replay")?;
    for (call, (tag, command, context, denied)) in calls.iter().zip([
        ("toggle", "layout.toggle-tiling", &original, false), ("restore", "layout.toggle-tiling", &toggled, false),
        ("deny-launch", "launch.launcher", &restored, true), ("deny-output", "focus.monitor-left", &restored, true),
    ]) {
        let invocation: Invocation = serde_json::from_value(call["invocation"].clone()).map_err(error)?;
        invocation.validate().map_err(|failure| failure.message)?;
        let response: Response = serde_json::from_value(call["response"].clone()).map_err(error)?;
        let result = if denied { matches!(response, Response::Failed { failure } if failure.code == ControlError::Denied) }
            else { matches!(response, Response::Complete { outcome: Disposition::Applied }) };
        if call["tag"] != tag || invocation.command != command || &invocation.context != context
            || invocation.arguments != lince_interface::host::control::Arguments::None
            || call["outcome_unknown"] == true || call.get("transport_error").is_some()
            || call["elapsed_ms"].as_u64().is_none_or(|elapsed| elapsed > 12_000) || !result
        { return Err(format!("The automation {tag} observation does not prove its captured command/permission outcome")); }
    }
    Ok(())
}

pub(super) fn prepare(directory: &Path, candidate: &Path, payload: &Path) -> Result<Prepared> {
    let probe_sha256 = prepare_probe(directory)?;
    let xml = directory.join("bin/park-control-v1.xml");
    fs::copy(env::current_dir().map_err(error)?.join("crates/park/protocols/lince-park-control-v1.xml"), &xml).map_err(error)?;
    let protocol_sha256 = digest(&xml)?;
    let data = directory.join("nested-automation-data");
    fs::create_dir(&data).map_err(error)?;
    let report = directory.join("nested-automation.json");
    write_json(&data.join("park.json"), &serde_json::json!({"ready":{"program":directory.join("bin/control-probe"),"arguments":[report,directory.join("bin/session-probe")],"capabilities":grants()}}))?;
    let payload_sha256 = digest(payload)?;
    let mut command = Command::new(executable(Path::new("dbus-run-session"))?);
    command.arg("--").arg(candidate).args(["--park-nested", "--park-duration", "120", "--park-outputs", "2", "--no-tray", "--directory"]).arg(&data).args(["--port", "0", "--peer-port", "0"])
        .env_remove("LINCE_DATA_DIR").env_remove("LINCE_PORT").env_remove("WAYLAND_SOCKET").env_remove("LINCE_PARK_BOOTSTRAP_PID").env("RUST_MIN_STACK", "33554432");
    logged(&mut command, &directory.join("nested-automation.log"))?;
    let (status, elapsed_ms) = bounded(&mut command, Duration::from_secs(145))?;
    if !status.success() { return Err(format!("The selected candidate's nested automation failed: {status}")); }
    let observed = load(&report)?;
    validate(&observed, Expected { payload, payload_sha256: &payload_sha256, probe: &directory.join("bin/control-probe"), probe_sha256: &probe_sha256,
        native: &directory.join("bin/session-probe"), native_sha256: &digest(&directory.join("bin/session-probe"))?, protocol_sha256: &protocol_sha256, data: &data }, None)?;
    if digest(payload)? != payload_sha256 { return Err("The selected compositor payload changed during automation validation".into()); }
    write_json(&directory.join("nested-automation-scope.json"), &serde_json::json!({"elapsed_ms":elapsed_ms,"scope":"Selected optimized candidate, nested Wayland, SDK and native SHM lifecycle; physical output/input, pixels and frame pacing remain separate."}))?;
    Ok(Prepared { probe_sha256, protocol_sha256, nested_report_sha256: digest(&report)? })
}

pub(super) fn verify_artifacts(run: &Run) -> Result<()> {
    if digest(&run.directory.join("bin/control-probe"))? != run.automation.probe_sha256
        || digest(&run.directory.join("bin/park-control-v1.xml"))? != run.automation.protocol_sha256
        || digest(&run.directory.join("nested-automation.json"))? != run.automation.nested_report_sha256
    { return Err("The prepared automation executable/protocol/report changed".into()); }
    Ok(())
}

fn expected<'a>(run: &'a Run, probe: &'a Path, native: &'a Path, data: &'a Path) -> Expected<'a> {
    Expected { payload: &run.process_executable, payload_sha256: &run.process_sha256,
        probe, probe_sha256: &run.automation.probe_sha256,
        native, native_sha256: &run.probe_sha256, protocol_sha256: &run.automation.protocol_sha256, data }
}

pub(super) fn collect(run: &Run, socket: &Path, pid: u32, children: &mut Children, results: &mut Results) -> Result<()> {
    verify_artifacts(run)?;
    let probe = run.directory.join("bin/control-probe");
    let native = run.directory.join("bin/session-probe");
    validate(&load(&run.directory.join("nested-automation.json"))?, expected(run, &probe, &native, &run.directory.join("nested-automation-data")), None)?;
    results.phases.push(Phase { name: "nested-candidate-automation".into(), outcome: "passed".into(), elapsed_ms: 0,
        detail: format!("Frozen nested-automation.json belongs to the selected {} payload; no physical/pixel/pacing pass is inferred", if run.installed { "installed" } else { "source" }) });
    if !run.configuration.automation {
        results.phases.push(Phase { name: "physical-desktop-automation".into(), outcome: "unrun".into(), elapsed_ms: 0,
            detail: "Select automation:true in a separate private fixture to use the authorized ready hook; other selected provider helpers remain unchanged.".into() });
        return Ok(());
    }
    let start = Instant::now();
    let report = run.directory.join("physical-automation.json");
    while !report.is_file() {
        check_interruption()?;
        if children.0[0].child.try_wait().map_err(error)?.is_some() { return Err("Park exited before the physical-session automation report".into()); }
        if start.elapsed() > Duration::from_secs(140) { return Err("The authorized physical-session automation probe did not finish".into()); }
        thread::sleep(Duration::from_millis(25));
    }
    validate(&load(&report)?, expected(run, &probe, &native, &run.directory.join("data")), Some((socket, pid)))?;
    let metadata = fs::read_to_string(format!("/proc/{pid}/stat")).map_err(error)?;
    let observed: serde_json::Value = load(&report)?;
    if metadata.rsplit_once(") ").and_then(|(_, fields)| fields.split_whitespace().nth(19)).and_then(|value| value.parse::<u64>().ok()) != observed["compositor"]["start"].as_u64()
        || digest(&PathBuf::from(format!("/proc/{pid}/exe")))? != run.process_sha256
    { return Err("The physical automation report no longer belongs to the live compositor generation".into()); }
    results.phases.push(Phase { name: "physical-desktop-automation".into(), outcome: "passed".into(), elapsed_ms: start.elapsed().as_millis(),
        detail: "physical-automation.json verifies SDK commands, denied scopes, native lifecycle and explicit cleanup on this payload/session; scanout, toolkit UX, providers and frame pacing remain separate.".into() });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lince_interface::host::{control::{Arguments, Mode}, mapping::QuarterTurn};

    const HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    fn expected() -> Expected<'static> {
        Expected { payload: Path::new("/fixture/compositor"), payload_sha256: HASH, probe: Path::new("/fixture/control-probe"), probe_sha256: HASH,
            native: Path::new("/fixture/session-probe"), native_sha256: HASH, protocol_sha256: HASH, data: Path::new("/fixture/data") }
    }

    fn report() -> serde_json::Value {
        let context = Context { owner: 101, canvas: "candidate-private-canvas".into(), workspace: 7, output: Some(1), item: None };
        let entries = vec![
            Entry::Session { locked: false, active: true }, Entry::Context { value: Some(context.clone()) },
            Entry::Workspace { id: 7, name: "Private workspace".into(), active: true, output: Some(1) },
            Entry::Output { id: 1, name: "Park-1".into(), position: [0, 0], size: [1000, 720], scale: 1.0, rotation: Some(QuarterTurn::Normal),
                adaptive_sync_requested: false, adaptive_sync: Default::default(), enabled: true, refresh_mhz: Some(60_000), modes: vec![Mode { size: [1000, 720], refresh_mhz: 60_000, preferred: true }] },
            Entry::Window { surface: 40, title: "Native first".into(), app_id: "social.lince.park.validation".into(), parent: None },
            Entry::Window { surface: 41, title: "Native second".into(), app_id: "social.lince.park.validation".into(), parent: None },
        ];
        let calls = [("toggle", "layout.toggle-tiling", false), ("restore", "layout.toggle-tiling", false), ("deny-launch", "launch.launcher", true), ("deny-output", "focus.monitor-left", true)].into_iter().map(|(tag, command, denied)| {
            let response = if denied { Response::Failed { failure: lince_interface::host::control::Failure::new(ControlError::Denied, "The fixture did not grant this permission") } }
                else { Response::Complete { outcome: Disposition::Applied } };
            serde_json::json!({"tag":tag,"elapsed_ms":10,"invocation":Invocation { command:command.into(), context:context.clone(), arguments:Arguments::None },"response":response})
        }).collect::<Vec<_>>();
        serde_json::json!({"status":"passed","compositor":{"pid":71,"start":100,"payload":"/fixture/compositor","sha256":HASH,"data_directory":"/fixture/data"},
            "probe":{"pid":72,"start":101,"payload":"/fixture/control-probe","sha256":HASH},"native":{"pid":73,"start":102,"payload":"/fixture/session-probe","sha256":HASH},
            "protocol_sha256":HASH,"native_exit":0,"socket":"/run/private/wayland-park-71","checks":CHECKS,
            "subscription":{"revision":1,"scope":lince_interface::host::control::CHANGE_RESYNC},
            "before":{"revision":2,"entries":entries},"after_toggle":{"revision":3,"entries":entries},"after_restore":{"revision":4,"entries":entries},
            "after_close":{"revision":5,"entries":entries.iter().filter(|entry| !matches!(entry, Entry::Window { .. })).collect::<Vec<_>>()},"calls":calls})
    }

    #[test]
    fn report_acceptance_requires_every_named_observation_and_refuses_empty_failed_or_replayed_calls() {
        let report = report();
        assert!(validate(&report, expected(), None).is_ok());
        assert!(validate(&report, expected(), Some((Path::new("/run/private/wayland-park-71"), 71))).is_ok());
        for index in 0..CHECKS.len() {
            let mut missing = report.clone();
            missing["checks"].as_array_mut().unwrap().remove(index);
            assert!(validate(&missing, expected(), None).is_err());
        }
        for missing in [serde_json::json!({}), serde_json::json!({"status":"passed","checks":[]}), serde_json::json!({"status":"unrun"})] {
            assert!(validate(&missing, expected(), None).is_err());
        }
        for index in 0..4 {
            let mut missing = report.clone();
            missing["calls"].as_array_mut().unwrap().remove(index);
            assert!(validate(&missing, expected(), None).is_err());
            let mut replayed = report.clone();
            replayed["calls"].as_array_mut().unwrap().push(report["calls"][index].clone());
            assert!(validate(&replayed, expected(), None).is_err());
            let mut uncertain = report.clone();
            uncertain["calls"][index]["outcome_unknown"] = serde_json::json!(true);
            assert!(validate(&uncertain, expected(), None).is_err());
        }
    }

    #[test]
    fn report_acceptance_binds_payload_protocol_session_targets_permission_results_and_actual_native_cleanup() {
        let report = report();
        for (pointer, replacement) in [
            ("/status", serde_json::json!("failed")), ("/compositor/sha256", serde_json::json!("another")),
            ("/probe/sha256", serde_json::json!("another")), ("/native/sha256", serde_json::json!("another")),
            ("/protocol_sha256", serde_json::json!("another")), ("/compositor/payload", serde_json::json!("/fixture/different-compositor")),
            ("/compositor/data_directory", serde_json::json!("/fixture/another-cell")),
            ("/native/pid", serde_json::json!(71)), ("/compositor/start", serde_json::json!(0)), ("/native_exit", serde_json::json!(1)),
            ("/before/entries/3/enabled", serde_json::json!(false)), ("/before/entries/3/refresh_mhz", serde_json::Value::Null),
            ("/before/entries/3/modes", serde_json::json!([])), ("/before/entries/1/value/workspace", serde_json::json!(8)),
            ("/after_toggle/revision", serde_json::json!(1)), ("/after_restore/entries/1/value/output", serde_json::json!(2)),
            ("/after_close/revision", serde_json::json!(1)), ("/subscription/scope", serde_json::json!(0)),
            ("/calls/0/invocation/context/owner", serde_json::json!(102)), ("/calls/1/invocation/command", serde_json::json!("item.close")),
            ("/calls/0/response/outcome", serde_json::json!("dispatched")), ("/calls/2/response/failure/code", serde_json::json!("unavailable")),
            ("/calls/3/elapsed_ms", serde_json::json!(12001)),
        ] {
            let mut changed = report.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(validate(&changed, expected(), None).is_err(), "Unexpectedly accepted {pointer}");
        }
        for pointer in ["/before/entries", "/after_close/entries"] {
            let mut changed = report.clone();
            if pointer.starts_with("/before") { changed.pointer_mut(pointer).unwrap().as_array_mut().unwrap().pop(); }
            else { changed.pointer_mut(pointer).unwrap().as_array_mut().unwrap().push(report["before"]["entries"][4].clone()); }
            assert!(validate(&changed, expected(), None).is_err());
        }
        assert!(validate(&report, expected(), Some((Path::new("/run/private/another-display"), 71))).is_err());
        assert!(validate(&report, expected(), Some((Path::new("/run/private/wayland-park-71"), 81))).is_err());
    }

    #[test]
    fn choosing_the_physical_automation_fixture_preserves_other_helper_choices_and_extends_its_watchdog() {
        let mut configuration: Configuration = serde_json::from_str(r#"{"terminal":{"program":"chosen-terminal"}}"#).unwrap();
        assert!(!configuration.automation);
        let ordinary = configuration.session_seconds();
        configuration.automation = true;
        assert!(super::configuration(&configuration).is_ok());
        assert_eq!(configuration.session_seconds(), ordinary + 150);
        configuration.helper = Some(Helper { target: Program { program:"chosen-provider".into(), arguments:Vec::new() }, capabilities:vec!["input-method".into()] });
        assert!(super::configuration(&configuration).is_err());
        configuration.automation = false;
        assert!(super::configuration(&configuration).is_ok());
        assert_eq!(configuration.helper.as_ref().unwrap().target.program, Path::new("chosen-provider"));
        let helper = Helper { target:Program { program:"chosen-helper".into(), arguments:Vec::new() }, capabilities:grants() };
        assert!(helper.valid());
    }
}
