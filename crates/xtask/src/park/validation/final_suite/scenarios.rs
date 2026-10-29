use super::*;

fn complete(suite: &mut Suite, id: u8, waiting: Option<String>) -> Result<Option<String>> {
    suite.scenarios[(id - 1) as usize].automatic = true;
    Ok(waiting)
}

pub(super) fn candidate(suite: &Suite) -> Result<&Path> {
    suite.candidate.as_deref().ok_or_else(|| "Prepare the local candidate through A2 or A3, or the installed candidate through B2".into())
}

type Task = (&'static str, bool, fn(&mut Suite, &Path) -> Result<Option<String>>);

fn observation_input(suite: &Suite, application: bool) -> Result<Value> {
    let tools: BTreeMap<_, _> = [&suite.settings.geckodriver, &suite.settings.firefox, &suite.settings.fixtures.terminal.program]
        .into_iter().map(|path| (path.to_string_lossy().into_owned(), executable(path).and_then(|path| digest(&path)).ok())).collect();
    Ok(json!({"source":suite.source,"settings":serde_json::to_value(&suite.settings).map_err(error)?,
        "fixture":if application { None } else { suite.test_hash.as_ref() },
        "candidate":if application { suite.payload_hash.as_ref() } else { None },
        "provider":if application { suite.settings.provider.as_ref().map(|path| json!({"path":path,"sha256":digest(path).ok()})) } else { None },
        "tools":if application { Some(tools) } else { None },
        "boot":fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()}))
}

fn grouped(suite: &mut Suite, directory: &Path, id: u8, tasks: &[Task]) -> Result<Option<String>> {
    let path = suite.directory.join("observations.json");
    let mut observations: BTreeMap<String, Value> = if path.exists() { load(&path)? } else { BTreeMap::new() };
    let mut failures = Vec::new();
    let mut blocked = Vec::new();
    for (name, application, task) in tasks {
        check_interruption()?;
        let key = format!("{}/{name}", IDS[(id - 1) as usize]);
        let input = observation_input(suite, *application)?;
        if *application && observations.get(&key).is_some_and(|record| record["status"] == "pass" && record["input"] == input
            && record["receipt"].as_str().zip(record["receipt_sha256"].as_str()).is_some_and(|(path, hash)| digest(Path::new(path)).ok().as_deref() == Some(hash))) {
            println!("Retained observation pass: {key}");
            continue;
        }
        let attempt = directory.join(name); private(&attempt)?;
        let started = Instant::now();
        let result = task(suite, &attempt);
        let (status, detail) = match result {
            Ok(None) => ("pass", "Required observations passed".to_owned()),
            Ok(Some(reason)) => { blocked.push(format!("{name}: {reason}")); ("blocked", reason) }
            Err(reason) => { failures.push(format!("{name}: {reason}")); ("fail", reason) }
        };
        let receipt = attempt.join("observation.json");
        let mut record = json!({"status":status,"detail":detail,"input":observation_input(suite,*application)?,"elapsed_ms":started.elapsed().as_millis(),"receipt":receipt});
        write_json(&receipt, &record)?;
        record["receipt_sha256"] = json!(digest(&receipt)?);
        observations.insert(key.clone(), record);
        write_json(&path, &observations)?;
        println!("{key}: {status} — {detail}");
    }
    require(failures.is_empty(), failures.join("\n"))?;
    complete(suite, id, (!blocked.is_empty()).then(|| blocked.join("\n")))
}

pub(super) fn controls(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    grouped(suite, directory, 1, &[("navigation", false, arrangement), ("canvas", false, canvas), ("settings", false, settings), ("properties", false, properties)])
}

pub(super) fn applications(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    grouped(suite, directory, 2, &[("input", false, input), ("windows", false, windows), ("toolkit", false, toolkit), ("capture", false, sharing),
        ("browser", true, ordinary_browser), ("provider", true, providers)])
}

fn ensure_candidate(suite: &mut Suite, directory: &Path) -> Result<()> {
    if suite.candidate.is_some() { return Ok(()); }
    let selected = match &suite.settings.executable {
        Some(path) => { require(path.is_absolute(), "Use an absolute candidate executable path")?; executable(path)? }
        None => cargo_executable(directory, &["run", "--locked", "--release", "--message-format=json", "-p", "lince", "--features", "instinct,park", "--", "--build-info"], "lince")?,
    };
    select_candidate(suite, &selected, directory)
}

fn select_candidate(suite: &mut Suite, selected: &Path, directory: &Path) -> Result<()> {
    let info = inspect_build(selected)?;
    require(info["optimized"] == true, "Browser and pacing workflows need an optimized release candidate")?;
    let payload = process_executable(selected)?;
    let payload_hash = digest(&payload)?;
    if suite.payload_hash.as_ref().is_some_and(|hash| hash != &payload_hash) {
        for scenario in &mut suite.scenarios[1..3] {
            scenario.status = "stale".into();
            scenario.automatic = false;
            scenario.detail = "The compositor payload changed; repeat the affected automatic observations".into();
        }
    }
    suite.candidate_hash = Some(digest(selected)?);
    suite.payload_hash = Some(payload_hash);
    suite.candidate = Some(selected.to_path_buf());
    write_json(&directory.join("build-info.json"), &info)?;
    save(suite)
}

fn cargo_executable(directory: &Path, arguments: &[&str], name: &str) -> Result<PathBuf> {
    let log = directory.join(format!("compile-{name}.log"));
    command(cargo_command().args(arguments), &log, 7200)?;
    fs::read_to_string(&log).map_err(error)?.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|value| value["reason"] == "compiler-artifact" && value["target"]["name"] == name)
        .filter_map(|value| value["executable"].as_str().map(PathBuf::from)).last()
        .ok_or_else(|| format!("Cargo did not report executable {name}; {}", log.display()))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureResult {
    status: String,
    detail: String,
    binary_sha256: String,
    input: String,
    elapsed_ms: u128,
    log: PathBuf,
    log_sha256: Option<String>,
}

pub(super) fn fixtures(suite: &mut Suite, directory: &Path, selectors: &[&str], qml: bool) -> Result<()> {
    if suite.park_tests.is_none() {
        let source = match prebuilt_probe("park-fixtures")? {
            Some(binary) => binary,
            None => cargo_executable(directory, &["test", "--locked", "--message-format=json", "-p", "lince-park", "--features", "session,server-tests,clipboard-gate", "--lib", "--no-run"], "lince_park")?,
        };
        let binary = suite.directory.join("park-fixtures");
        fs::copy(source, &binary).map_err(error)?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o500)).map_err(error)?;
        suite.test_hash = Some(digest(&binary)?);
        suite.park_tests = Some(binary);
        save(suite)?;
    }
    let binary = suite.park_tests.as_ref().ok_or("Missing frozen fixture executable")?;
    let binary_sha256 = digest(binary)?;
    require(Some(&binary_sha256) == suite.test_hash.as_ref(), "Frozen fixture executable changed")?;
    let toolkit = if qml {
        suite.settings.fixtures.toolkit_qml.as_ref().map(|runner| {
            let path = executable(&runner.program)?;
            Ok::<_, String>(json!({"program":path,"sha256":digest(&path)?,"arguments":runner.arguments}))
        }).transpose()?
    } else { None };
    let environment: BTreeMap<_, _> = ["WGPU_BACKEND", "VK_DRIVER_FILES", "VK_ICD_FILENAMES", "LD_LIBRARY_PATH", "WAYLAND_DISPLAY", "DISPLAY"]
        .into_iter().map(|name| (name, env::var(name).ok())).collect();
    let input = serde_json::to_string(&json!({"toolkit":toolkit,"environment":environment,"content":"","boot":fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()})).map_err(error)?;
    let ledger_path = suite.directory.join("fixtures.json");
    let mut ledger: BTreeMap<String, FixtureResult> = if ledger_path.exists() { load(&ledger_path)? } else { BTreeMap::new() };
    let captures = directory.join("captures"); private(&captures)?;
    let mut failures = Vec::new();
    for (index, selector) in selectors.iter().enumerate() {
        check_interruption()?;
        let key = format!("{selector};qml={qml}");
        if ledger.get(&key).is_some_and(|result| result.status == "pass" && result.binary_sha256 == binary_sha256 && result.input == input && result.log_sha256.is_some() && digest(&result.log).ok() == result.log_sha256) {
            println!("Retained fixture pass: {selector}");
            continue;
        }
        let log = directory.join(format!("fixture-{index}.log"));
        let started = Instant::now();
        println!("Running fixture: {selector}; log={}", log.display());
        let result = (|| {
            let mut process = Command::new(binary);
            process.args([*selector, "--exact", "--include-ignored", "--test-threads=1", "--nocapture"])
                .env("RUST_BACKTRACE", "1")
                .env("LINCE_CONTENT", "")
                .env("LINCE_PARK_TEST_ARTIFACTS", &captures);
            if qml {
                let runner = suite.settings.fixtures.toolkit_qml.as_ref().ok_or("Set fixtures.toolkit_qml to the chosen Qt Wayland runner")?;
                process.env("LINCE_PARK_TEST_QML", executable(&runner.program)?)
                    .env("LINCE_PARK_TEST_QML_ARGUMENTS", serde_json::to_string(&runner.arguments).map_err(error)?);
            }
            command(&mut process, &log, 1200)?;
            let text = fs::read_to_string(&log).map_err(error)?;
            require(text.lines().any(|line| line.starts_with(&format!("test {selector} ..."))), format!("The required fixture did not execute: {selector}"))?;
            require(text.lines().any(|line| line.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored;")), format!("The exact fixture {selector} did not execute one required case"))
        })();
        let (status, detail) = match result {
            Ok(()) => ("pass", "One exact fixture passed".to_owned()),
            Err(failure) => {
                failures.push(format!("{selector}: {failure}"));
                ("fail", failure)
            }
        };
        ledger.insert(key, FixtureResult {
            status: status.into(), detail, binary_sha256: binary_sha256.clone(), input: input.clone(),
            elapsed_ms: started.elapsed().as_millis(), log_sha256: digest(&log).ok(), log,
        });
        write_json(&ledger_path, &ledger)?;
        if status == "fail" {
            eprintln!("Fixture failed; continuing independent observations: {selector}");
        }
    }
    require(failures.is_empty(), failures.join("\n"))
}

pub(super) fn package(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    let selected = match &suite.settings.executable {
        Some(path) => { require(path.is_absolute(), "The selected installed executable must be absolute")?; executable(path)? }
        None => {
            require(env::var_os("LINCE_PARK_VALIDATION_BINARIES").is_none(), "Set executable to the CI-built installed candidate when using CI validation helpers")?;
            let log = directory.join("package.log");
            nix(directory, &["packages.${builtins.currentSystem}.lince-ui-source"], &log, 14400)?;
            let outputs = nix_outputs(&log)?;
            let output = outputs.first().ok_or("Nix did not report the installed source package")?;
            output.join("bin/lince")
        }
    };
    select_candidate(suite, &selected, directory)?;
    let info = inspect_build(&selected)?;
    if prebuilt_probe("park-fixtures")?.is_none() {
        command(cargo_command().args(["check", "--locked", "-p", "lince", "--features", "instinct,park"]), &directory.join("check.log"), 7200)?;
        nix(directory, &["checks.${builtins.currentSystem}.park-session", "checks.${builtins.currentSystem}.park-launch", "checks.${builtins.currentSystem}.park-source-install"], &directory.join("nix-session.log"), 3600)?;
    } else {
        let directory = PathBuf::from(env::var_os("LINCE_PARK_VALIDATION_BINARIES").ok_or("Missing CI validation directory")?);
        let revision = fs::read_to_string(directory.join("revision")).map_err(error)?;
        require(info["revision"].as_str() == Some(revision.trim()), "CI helpers and installed candidate must come from the same revision")?;
    }
    let info_log = directory.join("installed-build-info.log");
    command(Command::new(&selected).arg("--build-info").current_dir("/"), &info_log, 15)?;
    let fixture = directory.join("fixtures.json");
    let mut configuration = suite.settings.fixtures.clone();
    configuration.showoff = false;
    configuration.lifecycle_rounds = 1;
    configuration.automation = false;
    write_json(&fixture, &configuration)?;
    suite.prepared = Some(prepare_into(&fixture, Some(&selected), Some(directory))?);
    save(suite)?;
    suite.scenarios[4].automatic = true;
    manual::guide(suite, 5, directory)
}

fn nix(directory: &Path, attributes: &[&str], log: &Path, seconds: u64) -> Result<()> {
    let outputs = attributes.iter().map(|attribute| format!("flake.{attribute}")).collect::<Vec<_>>().join(" ");
    let expression = format!("let source = builtins.path {{ path = builtins.getEnv \"LINCE_PARK_SUITE_SOURCE\"; name = \"lince-park-validation-source\"; filter = path: type: !(builtins.elem (baseNameOf path) [ \".git\" \"target\" \".direnv\" \".devenv\" \".cache-lince-android\" ]); }}; flake = builtins.getFlake (\"path:\" + builtins.unsafeDiscardStringContext (toString source)); in [ {outputs} ]");
    private(directory)?;
    command(Command::new("nix").args(["build", "--impure", "--no-link", "--json", "--expr"]).arg(expression)
        .env("LINCE_PARK_SUITE_SOURCE", env::current_dir().map_err(error)?), log, seconds)
}

fn nix_outputs(log: &Path) -> Result<Vec<PathBuf>> {
    let output = fs::read_to_string(log).map_err(error)?.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|value| value.is_array()).last().ok_or("Nix output has no JSON result")?;
    output.as_array().ok_or("Invalid Nix output array")?.iter().map(|entry| {
        entry["outputs"]["out"].as_str().map(PathBuf::from).ok_or_else(|| "Nix did not report an out path".into())
    }).collect()
}

fn browser_run(suite: &mut Suite, directory: &Path, plain: bool, provider: bool, rounds: u32, seconds: u32) -> Result<PathBuf> {
    private(directory)?;
    ensure_candidate(suite, directory)?;
    let clients = suite.directory.join("clients");
    private(&clients.join("bin"))?;
    if !clients.join("bin/capture-probe").is_file() { prepare_capture_probe(&clients)?; }
    let mut arguments = vec![candidate(suite)?.as_os_str().to_owned(), "--geckodriver".into(), executable(&suite.settings.geckodriver)?.into_os_string(),
        "--firefox".into(), executable(&suite.settings.firefox)?.into_os_string(), "--terminal".into(), executable(&suite.settings.fixtures.terminal.program)?.into_os_string(),
        "--capture-probe".into(), clients.join("bin/capture-probe").into_os_string(), "--outputs".into(), "2".into(),
        "--seconds".into(), seconds.to_string().into(), "--lifecycle-rounds".into(), rounds.to_string().into()];
    if plain { arguments.push("--plain".into()); }
    if rounds > 0 && !provider {
        arguments.extend(["--terminal-check".into(), "--terminal-arguments".into(), serde_json::to_string(&suite.settings.fixtures.terminal.arguments).map_err(error)?.into()]);
    }
    if provider { arguments.extend(["--provider".into(), suite.settings.provider.as_ref().ok_or("Set provider to the selected provider fixture JSON")?.as_os_str().to_owned()]); }
    let run = browser::run_into(&arguments, directory)?;
    let report = evidence_report(&run.join("results.json"))?;
    require(report["functional_automation"] == "passed" && report["user_manager_unchanged"] == true, "Real-client workflow failed or could not establish host-manager isolation")?;
    require(report["video_start"]["videoFrames"].as_u64().zip(report["video_end"]["videoFrames"].as_u64()).is_some_and(|(before, after)| after > before), "Browser video did not report fresh decoded content")?;
    Ok(run)
}

pub(super) fn windows(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[
        APPEARANCE_CLIENT_TEST,
        "server::tests::transfers_tests::dispatch::windowed_fullscreen::bounded_fullscreen_keeps_actual_popup_pixels_and_recipients_requested_edits_and_explicit_mode_transitions_in_flat_and_spatial_slots",
    ], false)?;
    Ok(None)
}

fn toolkit(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    if suite.settings.fixtures.toolkit_qml.is_none() { return Ok(Some("Set fixtures.toolkit_qml before finishing ordinary real-toolkit window acceptance".into())); }
    fixtures(suite, directory, &[
        "server::tests::manipulation_tests::tiled::toolkit::flat_qt_controls_popups_owned_dialogs_and_client_moves_keep_normal_input",
    ], true)?;
    Ok(None)
}

pub(super) fn arrangement(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[
        "presentation::tests::widths::width_presets_use_native_fields_validation_reordering_and_narrow_modal_controls",
        "presentation::tests::tiling::real_tiling_controls_and_chords_restore_the_free_canvas_without_recreating_windows",
        "presentation::tests::monitors::monitor_chords_preserve_workspace_editor_follow_and_geometry_through_bar_changes_and_reconnect",
        OUTPUT_AFFINITY_TEST,
        "presentation::tests::workspace_navigation::workspace_navigation_operation_commands_preserve_editor_drafts_outputs_and_live_availability",
        WORKSPACE_HISTORY_TEST,
        COMPACT_CLIENT_TESTS[0], SCRATCH_WORKFLOW_TESTS[0], WINDOWED_FULLSCREEN_CLIENT_TESTS[2],
    ], false)?;
    Ok(None)
}

pub(super) fn settings(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[OPENING_RULE_WORKFLOW_TEST, OPENING_RULE_READINESS_TEST, CONFIGURATION_RENDER_TESTS[2], CONFIGURATION_INPUT_TESTS[0], PERMISSION_INSPECTOR_TEST,
        "presentation::tests::shortcuts::shortcut_capture_uses_real_pointer_keyboard_and_rendered_modal_at_wide_and_narrow_sizes"], false)?;
    Ok(None)
}

fn properties(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    if suite.settings.fixtures.toolkit_qml.is_none() { return Ok(Some("Set fixtures.toolkit_qml to complete captured Record/Properties interaction".into())); }
    fixtures(suite, directory, &["server::tests::manipulation_tests::tiled::toolkit::properties::flat_free_qt_properties_keep_the_original_record_through_focus_and_close"], true)?;
    Ok(None)
}

pub(super) fn canvas(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[
        "presentation::tests::showoff::showoff_exercises_rendering_assets_camera_editing_tiling_and_persistence",
        "presentation::tests::shaders::shader_sources_match_final_free_tiled_maximized_and_overview_return_pixels_at_fractional_scale",
        "presentation::tests::appearance::geometry::moving_follow_uses_the_current_rendered_target_for_120_single_frames_with_appearance_enabled",
    ], false)?;
    Ok(None)
}

pub(super) fn input(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[
        "server::tests::keyboard_tests::keyboard_map_and_repeat_changes_reach_a_real_client_and_keep_physical_key_identity",
        "server::tests::visible_inhibitors_respect_reference_counts_input_idle_and_session_lock",
        FILE_HOVER_LIFETIME_TESTS[2],
        "server::tests::transfers_tests::dispatch::real_file_hover_survives_pending_presentation_and_uses_the_dispatcher_for_activation_and_drop",
        "presentation::tests::keyboard::keyboard_settings_use_native_fields_and_the_host_transaction_at_wide_and_narrow_sizes",
        CONFIGURATION_INPUT_TESTS[0],
        NATIVE_POINTER_OWNERSHIP_TESTS[0],
    ], false)?;
    Ok(None)
}

fn sharing(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[CAPTURE_CLIENT_TEST, CAPTURE_LIFETIME_TESTS[0], APPEARANCE_CAPTURE_TESTS[0],
        "server::tests::locker_disconnect_keeps_the_session_locked_and_new_outputs_protected"], false)?;
    Ok(None)
}

fn ordinary_browser(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    browser_run(suite, directory, true, false, 1, 15)?;
    Ok(None)
}

fn providers(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    if suite.settings.provider.is_none() { return Ok(Some("Select the user's provider fixture before verifying external-tool compatibility".into())); }
    let run = browser_run(suite, directory, true, true, 1, 15)?;
    let result = evidence_report(&run.join("results.json"))?;
    require(result["bar_placement"] == "passed", "The selected provider did not prove reservation/restoration")?;
    if result["clipboard_roundtrip"] != "passed" { return Ok(Some("Configure the provider's clipboard_read, clipboard_write and clipboard_text to complete the cross-application transfer".into())); }
    Ok(None)
}

fn capture_cycles(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[CAPTURE_LIFETIME_TESTS[1]], false)?;
    Ok(None)
}

fn pacing(suite: &mut Suite, directory: &Path, plain: bool) -> Result<Option<String>> {
    let run = browser_run(suite, directory, plain, false, if plain { 3 } else { 0 }, 75)?;
    let trace = fs::read_to_string(run.join("measured.log")).map_err(error)?;
    let result = metrics::evaluate(&trace, 10.0, 60.0, if plain { 57.0 } else { 45.0 })?;
    write_json(&run.join("pacing.json"), &result)?;
    require(result["passed"] == true, format!("Release frame pacing failed: {}; inspect host constraints before attributing nested pacing to physical Park", run.display()))?;
    if plain {
        let report = evidence_report(&run.join("results.json"))?;
        let cycles = report["client_lifecycle"]["rounds"].as_array().ok_or("Missing actual application lifecycle observations")?;
        require(cycles.len() == 3 && report["client_lifecycle"]["status"] == "passed", "Expected three real application lifecycle rounds")?;
        require(report["client_lifecycle"]["resource_summary"]["passed"] == true && report["idle"]["passed"] == true, "Application resources or static-workspace idle did not pass")?;
    }
    Ok(None)
}

fn ordinary_pacing(suite: &mut Suite, directory: &Path) -> Result<Option<String>> { pacing(suite, directory, true) }
fn showoff_pacing(suite: &mut Suite, directory: &Path) -> Result<Option<String>> { pacing(suite, directory, false) }

pub(super) fn performance(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    grouped(suite, directory, 3, &[("capture-lifetimes", false, capture_cycles), ("ordinary-video", true, ordinary_pacing), ("showoff-video", true, showoff_pacing)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_digests_keep_the_same_sha256_for_small_and_large_escaped_paths() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/park-validation").join(format!("digest-{stamp}"));
        private(&directory).unwrap();
        let file = directory.join("-fixture\nname");
        fs::write(&file, b"abc").unwrap();
        assert_eq!(digest(&file).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        fs::write(&file, vec![0; 8 * 1024 * 1024]).unwrap();
        assert_eq!(digest(&file).unwrap(), "2daeb1f36095b44b318410b3f4e8b5d989dcc7bb023d1426c492dab0a3053e74");
        assert!(digest(&directory.join("missing")).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn observations_continue_after_failures_and_reuse_only_unchanged_passes() {
        fn failure(_: &mut Suite, _: &Path) -> Result<Option<String>> { Err("Independent failure".into()) }
        fn missing(_: &mut Suite, _: &Path) -> Result<Option<String>> { Ok(Some("Missing selected tool".into())) }
        fn success(_: &mut Suite, directory: &Path) -> Result<Option<String>> { fs::write(directory.join("executed"), []).map_err(error)?; Ok(None) }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/park-validation").join(format!("observation-ledger-{stamp}"));
        private(&directory).unwrap();
        let configuration = directory.join("configuration.json"); template(&configuration).unwrap();
        let mut suite = Suite {
            directory: directory.clone(), settings: load(&configuration).unwrap(), source: String::new(), candidate: None,
            candidate_hash: None, payload_hash: None, prepared: None, park_tests: None, test_hash: None,
            scenarios: NAMES.iter().enumerate().map(|(index, name)| Scenario { id: index as u8 + 1, name: (*name).into(), status: "not_run".into(), automatic: false, detail: String::new(), evidence: Vec::new() }).collect(),
        };
        suite.settings.provider = Some(directory.join("absent-provider.json"));
        let first = directory.join("first"); private(&first).unwrap();
        assert!(grouped(&mut suite, &first, 1, &[("failure", false, failure), ("blocked", true, missing), ("success", true, success)]).is_err());
        let ledger: Value = load(&directory.join("observations.json")).unwrap();
        assert_eq!(ledger["A1/failure"]["status"], "fail");
        assert_eq!(ledger["A1/blocked"]["status"], "blocked");
        assert_eq!(ledger["A1/success"]["status"], "pass");
        let retry = directory.join("retry"); private(&retry).unwrap();
        assert!(grouped(&mut suite, &retry, 1, &[("blocked", true, missing), ("success", true, success)]).unwrap().is_some());
        assert!(!retry.join("success/executed").exists());
        suite.payload_hash = Some("changed executable".into());
        assert!(grouped(&mut suite, &retry, 1, &[("success", true, success)]).unwrap().is_none());
        assert!(retry.join("success/executed").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn fixture_failures_do_not_suppress_results_and_only_valid_passes_are_reused() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/park-validation").join(format!("fixture-ledger-{stamp}"));
        private(&directory).unwrap();
        let configuration = directory.join("configuration.json");
        template(&configuration).unwrap();
        let binary = directory.join("fixture");
        let script = "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$0.calls\"\ncase \"$1\" in broken) exit 1;; empty) printf 'test result: ok. 0 passed; 0 failed; 0 ignored;\\n';; *) printf 'test %s ... fixture progress\\nok\\ntest result: ok. 1 passed; 0 failed; 0 ignored;\\n' \"$1\";; esac\n";
        fs::write(&binary, script).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let mut suite = Suite {
            directory: directory.clone(), settings: load(&configuration).unwrap(), source: String::new(),
            candidate: None, candidate_hash: None, payload_hash: None, prepared: None,
            test_hash: Some(digest(&binary).unwrap()), park_tests: Some(binary.clone()), scenarios: Vec::new(),
        };
        let attempt = directory.join("first"); private(&attempt).unwrap();
        let error = fixtures(&mut suite, &attempt, &["broken", "good", "empty", "last"], false).unwrap_err();
        assert!(error.contains("broken") && error.contains("empty"));
        let ledger: BTreeMap<String, FixtureResult> = load(&directory.join("fixtures.json")).unwrap();
        assert_eq!(ledger["good;qml=false"].status, "pass");
        assert_eq!(ledger["last;qml=false"].status, "pass");
        assert_eq!(ledger["empty;qml=false"].status, "fail");
        let attempt = directory.join("retry"); private(&attempt).unwrap();
        assert!(fixtures(&mut suite, &attempt, &["broken", "good", "last"], false).is_err());
        let calls = PathBuf::from(format!("{}.calls", binary.display()));
        assert_eq!(fs::read_to_string(&calls).unwrap(), "broken\ngood\nempty\nlast\nbroken\n");
        fs::write(&ledger["good;qml=false"].log, "changed evidence").unwrap();
        let attempt = directory.join("changed-log"); private(&attempt).unwrap();
        fixtures(&mut suite, &attempt, &["good", "last"], false).unwrap();
        assert!(fs::read_to_string(&calls).unwrap().ends_with("broken\ngood\n"));
        fs::write(&binary, format!("{script}\n")).unwrap();
        let attempt = directory.join("changed-binary"); private(&attempt).unwrap();
        assert!(fixtures(&mut suite, &attempt, &["last"], false).unwrap_err().contains("executable changed"));
        suite.test_hash = Some(digest(&binary).unwrap());
        fixtures(&mut suite, &attempt, &["last"], false).unwrap();
        assert!(fs::read_to_string(calls).unwrap().ends_with("good\nlast\n"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn source_identity_tracks_build_assets_but_not_unrelated_records_or_plans() {
        for path in ["Cargo.lock", "flake.nix", "crates/park/src/server.rs", "crates/store/migrations/0001.sql", "institute/assets/scene.glb", "institute/packaging/linux/lince-park-session"] {
            assert!(source_input(path), "{path}");
        }
        for path in ["institute/anicca/Park.md", "institute/trails/unrelated.lingua", "target/debug/lince", ".git/index"] {
            assert!(!source_input(path), "{path}");
        }
    }
}
