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
    Scenario(Control),
    Replay,
    Inspect,
    Reduce,
    Search,
    Cases,
}

#[derive(Clone, Default)]
pub(super) struct Progress {
    pub status: String,
    pub evidence: String,
}

pub(super) struct Outcome {
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
        Job::Scenario(mode) => {
            let destination = artifacts::next_directory(output)?;
            let (scenario, sources, _temporary) = prepare(&model, runtime).await?;
            let session = Session::open(scenario, &destination, &sources).await?;
            drive(session, mode, controls, progress).await?;
            inspect(&destination)
        }
        Job::Inspect => inspect(Path::new(&model.run_directory)),
        Job::Replay => {
            let destination = artifacts::next_directory(output)?;
            let result = artifacts::replay(Path::new(&model.run_directory), &destination).await?;
            Ok(Outcome {
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
        Job::Search => {
            let count: u64 = model.search_count.trim().parse()?;
            if count == 0 || count > 100_000 {
                return Err("Choose between 1 and 100000 search cases".into());
            }
            let status = simulation::campaign::run(output, Some(count)).await?;
            let directory = output
                .join("campaigns")
                .join(simulation::BUILD_HASH.trim_start_matches("sha256:"));
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

async fn prepare(
    model: &SimulationCastle,
    runtime: Option<cell::CellRuntime>,
) -> Result<(simulation::scenario::Scenario, PathBuf, tempfile::TempDir)> {
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
    mut session: Session,
    mut mode: Control,
    mut controls: mpsc::UnboundedReceiver<Control>,
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
            mode = controls.recv().await.unwrap_or(Control::Stop);
            continue;
        }
        if !session.step().await? {
            break;
        }
        if mode == Control::Step {
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
