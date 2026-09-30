use std::path::{Path, PathBuf};

use simulation::{
    Result,
    artifacts::{self, Bundle, Session},
};
use tokio::sync::{mpsc, watch};

use super::SimulationCastle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Control {
    Run,
    Pause,
    Step,
    Through(i64),
    Stop,
}

#[derive(Clone, Copy)]
pub(super) enum Job {
    SaveChecks,
    LoadChecks,
    ListChecks,
    Scenario(Control),
    Compare,
    Replay,
    Inspect,
    InspectOther,
    Reduce,
    Search,
    SearchSelected,
    Cases,
}

#[derive(Clone, Default)]
pub(super) struct Progress {
    pub status: String,
    pub evidence: String,
    pub execution: Option<nucleus::execution::control::Control>,
}

pub(super) struct Outcome {
    pub check_set: Option<nucleus::simulation::CheckSet>,
    pub path: Option<String>,
    pub status: String,
    pub bundle: Option<Bundle>,
    pub evidence: String,
}

pub(super) async fn run(
    model: SimulationCastle,
    runtime: Option<cell::CellRuntime>,
    job: Job,
    controls: mpsc::UnboundedReceiver<Control>,
    progress: watch::Sender<Progress>,
) -> Result<Outcome> {
    let output = Path::new(&model.output_directory);
    match job {
        Job::SaveChecks | Job::LoadChecks | Job::ListChecks => {
            let runtime = runtime.ok_or("Current Cell unavailable")?;
            let sets = simulation::saved_checks::list(&runtime.store.pool).await?;
            let mut scenario: simulation::scenario::Scenario =
                serde_json::from_str(&model.scenario)?;
            let selected = if matches!(job, Job::SaveChecks) {
                scenario.validate()?;
                let saved = model
                    .check_form
                    .saved
                    .as_ref()
                    .filter(|set| set.name == model.check_form.set_name.trim());
                let uid = saved.map(|set| set.uid.clone()).unwrap_or_else(|| {
                    format!(
                        "checks-{}",
                        nucleus::fact::sha256_hex(model.check_form.set_name.trim().as_bytes())
                    )
                });
                Some(
                    simulation::saved_checks::save(
                        &runtime.store.pool,
                        &uid,
                        &model.check_form.set_name,
                        saved.map_or(0, |set| set.revision),
                        &scenario.checks,
                    )
                    .await?,
                )
            } else if matches!(job, Job::LoadChecks) {
                let set = sets
                    .iter()
                    .find(|set| set.name == model.check_form.set_name.trim())
                    .ok_or("Saved set not found; use List sets to see its name")?
                    .clone();
                scenario.checks = set.checks.clone();
                scenario.validate()?;
                Some(set)
            } else {
                None
            };
            let status = selected.as_ref().map_or_else(
                || "Saved check sets".into(),
                |set| format!("{} · revision {}", set.name, set.revision),
            );
            Ok(Outcome {
                check_set: selected,
                path: None,
                status,
                bundle: None,
                evidence: serde_json::to_string_pretty(&sets)?,
            })
        }

        Job::Scenario(mode) => {
            let destination = artifacts::next_directory(output)?;
            let (scenario, sources, _temporary) = prepare(&model, runtime.clone()).await?;
            let execution = nucleus::execution::control::Control::new(scenario.limits.rule_evaluations, scenario.limits.wall_time_ms, None);
            progress.send_replace(Progress {status:"Preparing simulation".into(),evidence:String::new(),execution:Some(execution.clone())});
            let session = Session::open_interruptible(scenario, &destination, &sources, execution).await?;
            drive(session, mode, controls, progress).await?;
            inspect_source(&destination, &model.selected_cell, runtime.as_ref()).await
        }
        Job::Compare => {
            let destination = artifacts::next_directory(output)?;
            let (scenario, sources, _temporary) = prepare(&model, runtime).await?;
            let (proposed, mut baseline) =
                simulation::comparison::open(scenario, &destination, &sources).await?;
            let mut controls = controls;
            let proposed =
                drive_with_controls(proposed, Control::Run, &mut controls, progress.clone())
                    .await?;
            let baseline = if proposed.result.stop == (nucleus::simulation::Stop::Cancelled {}) {
                baseline.cancel();
                baseline.finish().await?
            } else {
                drive_with_controls(baseline, Control::Run, &mut controls, progress).await?
            };
            let comparison = simulation::comparison::save(&destination, proposed, baseline)?;
            Ok(Outcome {
                check_set: None,
                path: Some(comparison.with_transfers.clone()),
                bundle: None,
                status: format!(
                    "With Transfer assumptions: {:?} · Without: {:?}",
                    comparison.proposed.verdict, comparison.baseline.verdict
                ),
                evidence: serde_json::to_string_pretty(&comparison)?,
            })
        }
        Job::Inspect => {
            inspect_source(
                Path::new(&model.run_directory),
                &model.selected_cell,
                runtime.as_ref(),
            )
            .await
        }
        Job::InspectOther => {
            let current = Path::new(&model.run_directory).canonicalize()?;
            let parent = current.parent().ok_or("Comparison directory unavailable")?;
            let comparison: simulation::comparison::Comparison =
                serde_json::from_slice(&std::fs::read(parent.join("comparison.json"))?)?;
            let other = if current == Path::new(&comparison.with_transfers) {
                &comparison.without_transfers
            } else if current == Path::new(&comparison.without_transfers) {
                &comparison.with_transfers
            } else {
                return Err("Selected run does not belong to this comparison".into());
            };
            inspect_source(Path::new(other), &model.selected_cell, runtime.as_ref()).await
        }
        Job::Replay => {
            let destination = artifacts::next_directory(output)?;
            let result = artifacts::replay(Path::new(&model.run_directory), &destination).await?;
            Ok(Outcome {
                check_set: None,
                path: None,
                status: format!("Replay: {result:?}"),
                bundle: None,
                evidence: serde_json::to_string_pretty(&result)?,
            })
        }
        Job::Reduce => {
            let destination = artifacts::next_directory(output)?;
            let reduction =
                simulation::campaign::minimize(Path::new(&model.run_directory), &destination, 32)
                    .await?;
            let mut outcome = inspect(&destination.join("run"))?;
            outcome.status = format!(
                "Reduced {} inputs to {}. Reproduction verified: {}",
                reduction.original_inputs, reduction.remaining_inputs, reduction.verified
            );
            Ok(outcome)
        }
        Job::Search | Job::SearchSelected => {
            let count: u64 = model.search_count.trim().parse()?;
            if count == 0 || count > 100_000 {
                return Err("Choose between 1 and 100000 search cases".into());
            }
            let selection = if matches!(job, Job::SearchSelected) {
                let scenario: simulation::scenario::Scenario =
                    serde_json::from_str(&model.scenario)?;
                Some(simulation::campaign::CheckSelection {
                    checks: scenario.checks,
                    checking: scenario.checking,
                })
            } else {
                None
            };
            let status =
                simulation::campaign::run_with_checks(output, Some(count), selection.as_ref())
                    .await?;
            let directory = simulation::campaign::directory(output, selection.as_ref())?;
            let cursor: simulation::campaign::Cursor =
                serde_json::from_slice(&std::fs::read(directory.join("cursor.json"))?)?;
            let latest: Option<simulation::campaign::Summary> = if cursor.next_case == 0 {
                None
            } else {
                Some(serde_json::from_slice(&std::fs::read(
                    directory
                        .join("summaries")
                        .join(format!("{:08}.json", cursor.next_case - 1)),
                )?)?)
            };
            Ok(Outcome {
                check_set: None,
                path: None,
                status: format!(
                    "Search finished with status {status}. Progress and results saved in {}",
                    output.display()
                ),
                bundle: None,
                evidence: serde_json::to_string_pretty(
                    &serde_json::json!({"cursor":cursor,"latest":latest}),
                )?,
            })
        }
        Job::Cases => {
            let results =
                simulation::cli::case_results(Path::new(&model.source_directory), output).await?;
            let passed = results
                .iter()
                .filter(|run| run.result.verdict == nucleus::simulation::Verdict::Passed)
                .count();
            Ok(Outcome {
                check_set: None,
                path: None,
                status: format!(
                    "{} cases finished, {passed} passed. Runs saved in {}",
                    results.len(),
                    output.display()
                ),
                bundle: None,
                evidence: serde_json::to_string_pretty(&results)?,
            })
        }
    }
}

pub(super) fn inspect(path: &Path) -> Result<Outcome> {
    let bundle = artifacts::load(path).map_err(|error| format!("Run unavailable: {error:?}"))?;
    Ok(Outcome {
        check_set: None,
        path: Some(path.canonicalize()?.to_string_lossy().into()),
        status: format!(
            "{:?} · {:?} · {} steps · {} changes · {} findings",
            bundle.result.verdict,
            bundle.result.stop,
            bundle.result.steps,
            bundle.result.events,
            bundle.result.findings
        ),
        bundle: Some(bundle),
        evidence: String::new(),
    })
}

async fn inspect_source(
    path: &Path,
    cell: &str,
    runtime: Option<&cell::CellRuntime>,
) -> Result<Outcome> {
    let mut outcome = inspect(path)?;
    if let Some(runtime) = runtime {
        match artifacts::source_status(path, cell, &runtime.store).await? {
            artifacts::SourceStatus::Matching => outcome.status.push_str(" · Source unchanged"),
            artifacts::SourceStatus::Changed => {
                outcome.status.push_str(" · Source changed since this run")
            }
            artifacts::SourceStatus::Generated => {}
        }
    }
    Ok(outcome)
}

async fn prepare(
    model: &SimulationCastle,
    runtime: Option<cell::CellRuntime>,
) -> Result<(simulation::scenario::Scenario, PathBuf, tempfile::TempDir)> {
    if model.transfer_form.pending || !model.pending_transfers.is_empty() {
        return Err("Choose your Record and add the Transfer assumption before running".into());
    }
    let mut scenario: simulation::scenario::Scenario = serde_json::from_str(&model.scenario)?;
    scenario.validate()?;
    let temporary = tempfile::tempdir()?;
    let mut sources = PathBuf::from(&model.source_directory);
    if model.current_database {
        if scenario.cells.len() != 1
            || scenario.cells[0].name != "current"
            || scenario.cells[0].database.is_some()
        {
            return Err(
                "Current database setup requires one Cell named current, without a database file"
                    .into(),
            );
        }
        let runtime = runtime.ok_or("Current Cell unavailable")?;
        let path = temporary.path().join("cell.sqlite");
        runtime.store.snapshot_into(&path).await?;
        let shift = nucleus::execution::now().timestamp_millis() - scenario.start_ms;
        let shifted = |time: i64| time.checked_add(shift).ok_or("time range overflow");
        scenario.start_ms = shifted(scenario.start_ms)?;
        scenario.end_ms = shifted(scenario.end_ms)?;
        for input in &mut scenario.inputs {
            input.at_ms = shifted(input.at_ms)?;
        }
        for check in &mut scenario.checks {
            if let nucleus::simulation::Evaluation::At { at_ms } = &mut check.options.evaluation {
                *at_ms = shifted(*at_ms)?;
            }
            check.options.window.from_ms = check.options.window.from_ms.map(shifted).transpose()?;
            check.options.window.until_ms =
                check.options.window.until_ms.map(shifted).transpose()?;
            match &mut check.predicate {
                nucleus::simulation::Predicate::QuantityEquals { at_ms, .. }
                | nucleus::simulation::Predicate::Converged { at_ms, .. } => {
                    *at_ms = shifted(*at_ms)?
                }
                _ => {}
            }
        }
        let original = sources.canonicalize()?;
        for source in &mut scenario.cells[0].lingua {
            let path = original.join(&source.file).canonicalize()?;
            if !path.starts_with(&original) {
                return Err("Lingua seed escapes its folder".into());
            }
            source.file = format!("lingua/{}", source.name);
            let destination = temporary.path().join(&source.file);
            std::fs::create_dir_all(destination.parent().ok_or("Lingua file has no parent")?)?;
            std::fs::copy(path, destination)?;
        }
        scenario.cells[0].database = Some(simulation::scenario::Database {
            file: "cell.sqlite".into(),
            hash: artifacts::file_hash(&path)?,
        });
        sources = temporary.path().into();
    }
    Ok((scenario, sources, temporary))
}

pub(super) async fn drive(
    session: Session,
    mode: Control,
    mut controls: mpsc::UnboundedReceiver<Control>,
    progress: watch::Sender<Progress>,
) -> Result<simulation::artifacts::Run> {
    drive_with_controls(session, mode, &mut controls, progress).await
}

async fn drive_with_controls(
    mut session: Session,
    mut mode: Control,
    controls: &mut mpsc::UnboundedReceiver<Control>,
    progress: watch::Sender<Progress>,
) -> Result<simulation::artifacts::Run> {
    loop {
        if session.world.stop.is_some() {
            break;
        }
        while let Ok(control) = controls.try_recv() {
            mode = control;
        }
        if controls.is_closed() {
            mode = Control::Stop;
        }
        let mut pause_reason = None;
        if let Control::Through(until) = mode {
            if !(session.world.scenario.start_ms..=session.world.scenario.end_ms).contains(&until) {
                mode = Control::Pause;
                pause_reason = Some("Paused: requested date is outside the scenario's time range");
            } else if session.world.next_ms().is_none_or(|next| next > until) {
                mode = Control::Pause;
            }
        }
        progress.send_replace(Progress {
            execution: Some(session.world.control.clone()),
            status: format!(
                "{} · {} · {} steps · {} changes · {} findings",
                if let Some(reason) = pause_reason {
                    reason
                } else if mode == Control::Pause {
                    "Paused"
                } else {
                    "Running"
                },
                chrono::DateTime::from_timestamp_millis(session.world.now_ms).map_or_else(
                    || session.world.now_ms.to_string(),
                    |time| time.to_rfc3339()
                ),
                session.world.steps,
                session.world.trace.len(),
                session.checks.findings.len()
            ),
            evidence: serde_json::to_string_pretty(
                &session.world.trace[session.world.trace.len().saturating_sub(20)..],
            )?,
        });
        if mode == Control::Stop {
            session.cancel();
            break;
        }
        if mode == Control::Pause {
            session.world.control.pause();
            mode = controls.recv().await.unwrap_or(Control::Stop);
            session.world.control.resume();
            continue;
        }
        let previous_steps = session.world.steps;
        if !session.step().await? {
            break;
        }
        if mode == Control::Step && session.world.steps > previous_steps {
            mode = Control::Pause;
        }
        tokio::task::yield_now().await;
    }
    session.finish().await
}

pub(super) fn through(value: &str) -> Result<i64> {
    if let Ok(time) = value.trim().parse::<i64>() {
        return Ok(time);
    }
    if let Ok(time) = chrono::DateTime::parse_from_rfc3339(value.trim()) {
        return Ok(time.timestamp_millis());
    }
    Ok(chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")?
        .and_hms_opt(0, 0, 0)
        .ok_or("invalid date")?
        .and_utc()
        .timestamp_millis())
}
