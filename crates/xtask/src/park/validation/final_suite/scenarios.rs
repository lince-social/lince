use super::*;

pub(super) fn candidate(suite: &Suite) -> Result<&Path> {
    suite.candidate.as_deref().ok_or_else(|| "Scenario 01 has not prepared the installed candidate".into())
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

fn fixtures(suite: &mut Suite, directory: &Path, selectors: &[&str], qml: bool) -> Result<()> {
    if suite.park_tests.is_none() {
        let source = cargo_executable(directory, &["test", "--locked", "--release", "--message-format=json", "-p", "lince-park", "--features", "session,server-tests,clipboard-gate", "--lib", "--no-run"], "lince_park")?;
        let binary = suite.directory.join("park-fixtures");
        fs::copy(source, &binary).map_err(error)?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o500)).map_err(error)?;
        suite.test_hash = Some(digest(&binary)?);
        suite.park_tests = Some(binary);
        save(suite)?;
    }
    let binary = suite.park_tests.as_ref().ok_or("Missing frozen fixture executable")?;
    let captures = directory.join("captures"); private(&captures)?;
    for (index, selector) in selectors.iter().enumerate() {
        let log = directory.join(format!("fixture-{index}.log"));
        let mut process = Command::new(binary);
        process.args([*selector, "--exact", "--ignored", "--test-threads=1"])
            .env("LINCE_PARK_TEST_ARTIFACTS", &captures);
        if qml {
            let runner = suite.settings.fixtures.toolkit_qml.as_ref().ok_or("Set fixtures.toolkit_qml to the chosen Qt Wayland runner")?;
            process.env("LINCE_PARK_TEST_QML", executable(&runner.program)?)
                .env("LINCE_PARK_TEST_QML_ARGUMENTS", serde_json::to_string(&runner.arguments).map_err(error)?);
        }
        command(&mut process, &log, 1200)?;
        let text = fs::read_to_string(log).map_err(error)?;
        inspector_result(&text, selector)?;
        require(text.lines().any(|line| line.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored;")), format!("The exact fixture {selector} did not execute one required case"))?;
    }
    Ok(())
}

pub(super) fn package(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    let selected = match &suite.settings.executable {
        Some(path) => { require(path.is_absolute(), "The selected installed executable must be absolute")?; executable(path)? }
        None => cargo_executable(directory, &["run", "--locked", "--release", "--message-format=json", "-p", "lince", "--features", "instinct,park", "--", "--build-info"], "lince")?,
    };
    let info = inspect_build(&selected)?;
    let payload = process_executable(&selected)?;
    suite.candidate_hash = Some(digest(&selected)?);
    suite.payload_hash = Some(digest(&payload)?);
    suite.candidate = Some(selected.clone());
    write_json(&directory.join("build-info.json"), &info)?;
    save(suite)?;
    command(cargo_command().args(["check", "--locked", "-p", "lince", "--features", "instinct,park"]), &directory.join("check.log"), 7200)?;
    command(Command::new("nix").args(["build", "--no-link", "--json", ".#checks.x86_64-linux.park-session", ".#checks.x86_64-linux.park-launch", ".#checks.x86_64-linux.park-source-install"]), &directory.join("nix-session.log"), 3600)?;
    let fixture = directory.join("fixtures.json");
    let mut configuration = suite.settings.fixtures.clone();
    configuration.showoff = false;
    configuration.lifecycle_rounds = 1;
    configuration.automation = false;
    write_json(&fixture, &configuration)?;
    suite.prepared = Some(prepare_into(&fixture, Some(&selected), Some(directory))?);
    save(suite)?;
    Ok(Some("Package/session/nested probes completed; actual managed UWSM startup is confirmed by scenario 10".into()))
}

fn browser_run(suite: &Suite, directory: &Path, plain: bool, provider: bool, rounds: u32) -> Result<PathBuf> {
    private(directory)?;
    let prepared = suite.prepared.as_ref().ok_or("Missing prepared probes")?;
    let mut arguments = vec![candidate(suite)?.as_os_str().to_owned(), "--geckodriver".into(), executable(&suite.settings.geckodriver)?.into_os_string(),
        "--firefox".into(), executable(&suite.settings.firefox)?.into_os_string(), "--terminal".into(), executable(&suite.settings.fixtures.terminal.program)?.into_os_string(),
        "--capture-probe".into(), prepared.join("bin/capture-probe").into_os_string(), "--outputs".into(), "2".into(),
        "--seconds".into(), if rounds == 0 { "75" } else { "15" }.into(), "--lifecycle-rounds".into(), rounds.to_string().into()];
    if plain { arguments.push("--plain".into()); }
    if provider { arguments.extend(["--provider".into(), suite.settings.provider.as_ref().ok_or("Set provider to the selected provider fixture JSON")?.as_os_str().to_owned()]); }
    let run = browser::run_into(&arguments, directory)?;
    let report: Value = load(&run.join("results.json"))?;
    require(report["functional_automation"] == "passed" && report["user_manager_unchanged"] == true, "Real-client workflow failed or could not establish host-manager isolation")?;
    require(report["video_start"]["frames"].as_u64().zip(report["video_end"]["frames"].as_u64()).is_some_and(|(before, after)| after > before), "Browser video did not report fresh decoded content")?;
    Ok(run)
}

pub(super) fn windows(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    let qml = suite.settings.fixtures.toolkit_qml.is_some();
    fixtures(suite, directory, &[
        APPEARANCE_CLIENT_TEST,
        "server::tests::transfers_tests::dispatch::windowed_fullscreen::bounded_fullscreen_keeps_actual_popup_pixels_and_recipients_requested_edits_and_explicit_mode_transitions_in_flat_and_spatial_slots",
    ], false)?;
    if !qml { return Ok(Some("Set fixtures.toolkit_qml before finishing ordinary real-toolkit window acceptance".into())); }
    let qt = directory.join("qt"); private(&qt)?;
    fixtures(suite, &qt, &[
        "server::tests::manipulation_tests::tiled::toolkit::flat_qt_controls_popups_owned_dialogs_and_client_moves_keep_normal_input",
        "server::tests::manipulation_tests::tiled::toolkit::spatial_qt_controls_popups_owned_dialogs_and_client_moves_keep_normal_input",
    ], true)?;
    browser_run(suite, &directory.join("browser"), true, false, 1)?;
    Ok(None)
}

pub(super) fn arrangement(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[
        "presentation::tests::tiling::real_tiling_controls_and_chords_restore_the_free_canvas_without_recreating_windows",
        WORKSPACE_HISTORY_TEST, OUTPUT_AFFINITY_TEST,
        "presentation::tests::workspace_navigation::workspace_navigation_operation_commands_preserve_editor_drafts_outputs_and_live_availability",
        COMPACT_CLIENT_TESTS[0], SCRATCH_WORKFLOW_TESTS[0], WINDOWED_FULLSCREEN_CLIENT_TESTS[2],
    ], false)?;
    Ok(None)
}

pub(super) fn settings(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[OPENING_RULE_WORKFLOW_TEST, OPENING_RULE_READINESS_TEST, CONFIGURATION_RENDER_TESTS[2], CONFIGURATION_INPUT_TESTS[0], PERMISSION_INSPECTOR_TEST], false)?;
    if suite.settings.fixtures.toolkit_qml.is_none() { return Ok(Some("Set fixtures.toolkit_qml to complete captured Record/Properties interaction".into())); }
    let qt = directory.join("qt"); private(&qt)?;
    fixtures(suite, &qt, &["server::tests::manipulation_tests::tiled::toolkit::properties::flat_free_qt_properties_keep_the_original_record_through_focus_and_close"], true)?;
    Ok(None)
}

pub(super) fn canvas(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[
        "presentation::tests::showoff::showoff_exercises_rendering_assets_camera_editing_tiling_and_persistence",
        "presentation::tests::shaders::shader_sources_match_final_free_tiled_maximized_and_overview_return_pixels_at_multiple_scales",
        "presentation::tests::appearance::geometry::moving_follow_uses_the_current_rendered_target_for_120_single_frames_with_appearance_enabled",
    ], false)?;
    Ok(None)
}

pub(super) fn input(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[
        FILE_HOVER_LIFETIME_TESTS[2],
        "presentation::tests::keyboard::keyboard_settings_use_native_fields_and_the_host_transaction_at_wide_and_narrow_sizes",
        CONFIGURATION_INPUT_TESTS[0],
        "server::tests::transfers_tests::dispatch::ime::native_preedit_commit_delete_and_popup_follow_the_real_focused_editor_without_stealing_application_input",
    ], false)?;
    Ok(None)
}

pub(super) fn providers(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[CAPTURE_CLIENT_TEST, CAPTURE_LIFETIME_TESTS[0], APPEARANCE_CAPTURE_TESTS[0]], false)?;
    if suite.settings.provider.is_none() { return Ok(Some("Select the user's provider fixture before verifying external-tool compatibility".into())); }
    let run = browser_run(suite, &directory.join("provider"), true, true, 1)?;
    let result: Value = load(&run.join("results.json"))?;
    require(result["bar_placement"] == "passed", "The selected provider did not prove reservation/restoration")?;
    manual::guide(suite, 7, directory)
}

pub(super) fn performance(suite: &mut Suite, directory: &Path) -> Result<Option<String>> {
    fixtures(suite, directory, &[CAPTURE_LIFETIME_TESTS[1]], false)?;
    let mut measurements = Vec::new();
    for plain in [true, false] {
        for repetition in 1..=3 {
            let run = browser_run(suite, &directory.join(format!("{}-{repetition}", if plain { "ordinary" } else { "showoff" })), plain, false, 0)?;
            let trace = fs::read_to_string(run.join("measured.log")).map_err(error)?;
            let result = metrics::evaluate(&trace, 10.0, 60.0, if plain { 57.0 } else { 45.0 })?;
            write_json(&run.join("pacing.json"), &result)?;
            require(result["passed"] == true, format!("Release frame pacing failed: {}", run.display()))?;
            measurements.push(json!({"plain":plain,"repetition":repetition,"run":run,"metrics":result}));
        }
    }
    write_json(&directory.join("pacing.json"), &measurements)?;
    Ok(Some("Release video/pacing/lifecycle measurements completed; reuse scenario 10 for the two 30-minute resource windows".into()))
}
