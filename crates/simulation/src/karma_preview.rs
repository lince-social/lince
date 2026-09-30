use std::sync::Arc;

use chrono::{DateTime, Utc};
use engine::{
    Engine, EngineError,
    actions::Action,
    karma_preview::{self as preview, FinalValue, Input, Report, Request},
};
use nucleus::simulation::{self as report, Observation, Predicate};

use crate::{
    artifacts::{self, Session},
    scenario,
};

struct Runner;

pub fn install(engine: &Engine) -> Result<(), EngineError> {
    engine.install_karma_preview_runner(Arc::new(Runner))
}

impl preview::Runner for Runner {
    fn run<'a>(
        &'a self,
        engine: &'a Engine,
        actor: Option<String>,
        request: Request,
        now: DateTime<Utc>,
    ) -> preview::PreviewFuture<'a> {
        Box::pin(async move {
            run(engine, actor.as_deref(), request, now)
                .await
                .map_err(|error| {
                    error
                        .downcast::<EngineError>()
                        .map(|error| *error)
                        .unwrap_or_else(preview::invalid)
                })
        })
    }
}

fn shifted(value: i64, delta: i64) -> crate::Result<i64> {
    value
        .checked_add(delta)
        .ok_or_else(|| "Check date is out of range".into())
}

pub fn shift_checks(
    checks: &mut [report::CheckDefinition],
    from: i64,
    to: i64,
) -> crate::Result<()> {
    let delta = to.checked_sub(from).ok_or("Check dates are out of range")?;
    for check in checks {
        if let report::Evaluation::At { at_ms } = &mut check.options.evaluation {
            *at_ms = shifted(*at_ms, delta)?;
        }
        check.options.window.from_ms = check
            .options
            .window
            .from_ms
            .map(|value| shifted(value, delta))
            .transpose()?;
        check.options.window.until_ms = check
            .options
            .window
            .until_ms
            .map(|value| shifted(value, delta))
            .transpose()?;
        match &mut check.predicate {
            Predicate::Quantity { cell, .. } | Predicate::Nonnegative { cell, .. } => {
                *cell = "current".into()
            }
            Predicate::QuantityEquals { cell, at_ms, .. } => {
                *cell = "current".into();
                *at_ms = shifted(*at_ms, delta)?;
            }
            Predicate::Converged { .. } | Predicate::ExpectedMessageRefusal { .. } => {
                return Err("A single-Cell Rule preview cannot evaluate peer checks".into());
            }
            _ => {}
        }
    }
    Ok(())
}

async fn readable_rule(
    live: &Engine,
    copied: &Engine,
    actor: Option<&str>,
    uid: &str,
) -> crate::Result<Vec<String>> {
    let rule = store::recurrence::get(&copied.store.pool, uid)
        .await?
        .ok_or("Rule evidence is unavailable")?;
    let mut records = vec![rule.consequences.iter().find_map(nucleus::karma::Consequence::transfer_target).map(str::to_owned).unwrap_or(rule.record_uid)];
    if let Some(condition) = rule.condition {
        records.extend(
            copied
                .karma_condition_records(condition.parsed()?, actor)
                .await?,
        );
    }
    live.check_karma_input_visibility(actor, &records).await?;
    Ok(records)
}

async fn readable_cycle(
    live: &Engine,
    copied: &Engine,
    actor: Option<&str>,
    cycle: &report::RuleCycle,
) -> crate::Result<Vec<String>> {
    let mut records = Vec::new();
    for rule in &cycle.rules {
        records.extend(readable_rule(live, copied, actor, rule).await?);
    }
    for step in &cycle.steps {
        live.refuse_unreadable(
            actor,
            &step
                .changes
                .iter()
                .map(|change| change.record.as_str().into())
                .collect::<Vec<_>>(),
        )
        .await?;
    }
    records.extend(cycle.steps.iter().flat_map(|step| {
        step.changes
            .iter()
            .map(|change| change.record.as_str().into())
    }));
    for change in cycle.steps.iter().flat_map(|step| &step.transfer_changes) {
        live.check_karma_input_visibility(actor, &[change.after.transfer.clone()]).await?;
        records.push(change.after.transfer.clone());
    }
    Ok(records)
}

async fn bind_record(
    engine: &Engine,
    actor: Option<&str>,
    reference: &str,
) -> crate::Result<String> {
    let record = store::records::resolve(&engine.store.pool, reference).await?;
    let Some(record) = record else { return Ok(engine.read_karma_transfer_state(reference, actor).await?.transfer); };
    if record.kind == "transfer" { return Ok(engine.read_karma_transfer_state(&record.uid, actor).await?.transfer); }
    engine
        .refuse_unreadable(actor, std::slice::from_ref(&record.uid))
        .await?;
    Ok(record.uid)
}

pub async fn run(
    engine: &Engine,
    actor: Option<&str>,
    request: Request,
    now: DateTime<Utc>,
) -> crate::Result<Report> {
    request.validate()?;
    let draft = request.fingerprint()?;
    let start_ms = now.timestamp_millis();
    let end_ms = start_ms
        .checked_add(i64::try_from(request.limits.horizon_ms)?)
        .ok_or("Preview horizon is out of range")?;
    let temporary = tempfile::tempdir()?;
    let snapshot = temporary.path().join("source.sqlite");
    engine
        .access_scope(false, async {
            engine
                .store
                .snapshot_into(&snapshot)
                .await
                .map_err(EngineError::Store)
        })
        .await?;
    let copied_source =
        store::Store::open_existing_durable(&format!("sqlite://{}", snapshot.display())).await?;
    let source = copied_source.state_hash().await?.as_str().to_owned();
    copied_source.pool.close().await;
    let mut checks = request.checks.clone();
    if let Some(uid) = &request.saved_checks {
        let set = store::simulation_checks::list(&engine.store.pool)
            .await?
            .into_iter()
            .find(|set| &set.uid == uid)
            .ok_or("Saved check set is unavailable")?;
        checks.extend(set.checks);
    }
    shift_checks(
        &mut checks,
        request.checks_start_ms.unwrap_or(start_ms),
        start_ms,
    )?;
    let mut records = Vec::new();
    for reference in &request.records {
        records.push(bind_record(engine, actor, reference).await?);
    }
    for check in &mut checks {
        if let Predicate::Quantity { record, .. }
        | Predicate::QuantityEquals { record, .. }
        | Predicate::Nonnegative { record, .. } = &mut check.predicate
        {
            *record = bind_record(engine, actor, record).await?;
        }
    }
    let seed = request
        .proposals
        .iter()
        .enumerate()
        .map(|(index, proposal)| scenario::Invocation {
            id: format!("proposal-{index}"),
            actor: actor.map(str::to_owned),
            action: proposal.action(format!("proposal-{}-{index}", &draft[draft.len() - 32..])),
        })
        .collect();
    let mut inputs = Vec::new();
    for (index, input) in request.inputs.iter().enumerate() {
        let at_ms = start_ms
            .checked_add(i64::try_from(input.after_ms())?)
            .ok_or("Input date is out of range")?;
        let action = match input {
            Input::Quantity { record, value, .. } => Action::SetQuantityExact {
                target: bind_record(engine, actor, record).await?,
                amount: value.to_string(),
            },
            Input::Extension {
                record,
                namespace,
                value,
                ..
            } => Action::SetExtension {
                target: bind_record(engine, actor, record).await?,
                namespace: namespace.clone(),
                fds: value.clone(),
            },
            Input::Occurrence { proposal, .. } => Action::ApplyRecurrenceOccurrence {
                recurrence: format!("$proposal-{proposal}"),
                due_at: DateTime::from_timestamp_millis(at_ms)
                    .ok_or("Input date is out of range")?
                    .to_rfc3339(),
                amount: None,
                note: None,
            },
        };
        if !matches!(input, Input::Occurrence { .. }) {
            engine.authorize_action(&action, actor).await?;
        }
        let id = format!("input-{index}");
        inputs.push(scenario::Input {
            id: id.clone(),
            at_ms,
            cell: "current".into(),
            event: scenario::Event::Action {
                invocation: scenario::Invocation {
                    id,
                    actor: actor.map(str::to_owned),
                    action,
                },
            },
        });
    }
    let scenario = scenario::Scenario {
        version: report::Version::V1,
        name: "karma-proposal".into(),
        seed: 1,
        start_ms,
        end_ms,
        limits: scenario::Limits {
            steps: request.limits.steps,
            rule_evaluations: request.limits.rule_evaluations,
            wall_time_ms: request.limits.wall_time_ms,
            evidence_bytes: request.limits.evidence_bytes,
            pending_messages: 1024,
        },
        cells: vec![scenario::Cell {
            name: "current".into(),
            database: Some(scenario::Database {
                file: "source.sqlite".into(),
                hash: artifacts::file_hash(&snapshot)?,
            }),
            lingua: Vec::new(),
            seed,
        }],
        inputs,
        checks,
        checking: request.checking.clone(),
    };
    let mut session =
        Session::open_karma_preview(scenario, &temporary.path().join("run"), temporary.path(), engine.clone_karma_preview_signer(actor).await?).await?;
    if session.world.trace.iter().any(|event| {
        matches!(&event.observation,
        Observation::ActionRefused { input, .. } if input.starts_with("proposal-"))
    }) {
        session.world.stop = Some(report::Stop::ExecutionError {
            cell: "current".into(),
            input: None,
            category: report::ExecutionError::Domain,
        });
    }
    while session.step().await? {
        tokio::task::yield_now().await;
    }
    if records.is_empty() {
        for index in 0..request.proposals.len() {
            if let Some(uid) = session.world.captured.get(&format!("proposal-{index}"))
                && let Some(rule) =
                    store::recurrence::get(&session.world.nodes["current"].engine().store.pool, uid)
                        .await?
            {
                let target = rule.consequences.iter().find_map(nucleus::karma::Consequence::transfer_target).map(str::to_owned).unwrap_or(rule.record_uid);
                if !records.contains(&target) {
                    records.push(target);
                }
            }
        }
    }
    let mut required_reads = records.clone();
    let mut final_values = Vec::new();
    for record in records {
        engine.check_karma_input_visibility(actor, std::slice::from_ref(&record)).await?;
        let transfer = if engine.read_karma_transfer_state(&record, actor).await.is_ok() {
            Some(session.world.nodes["current"].engine().read_karma_transfer_state(&record, actor).await?)
        } else { None };
        let quantity = if transfer.is_some() { None } else { session
            .world
            .quantity_with_basis("current", &record, request.quantity_basis)
            .await?
            .map(|(_, quantity)| quantity) };
        final_values.push(FinalValue { record, quantity, transfer });
    }
    let copied = session.world.nodes["current"].engine();
    let mut cycles = Vec::new();
    let mut hidden = false;
    for cycle in session.world.control.cycles() {
        match readable_cycle(engine, copied, actor, &cycle).await {
            Ok(reads) => {
                required_reads.extend(reads);
                cycles.push(cycle);
            }
            Err(_) => hidden = true,
        }
    }
    for event in &session.world.trace {
        if let Observation::RuleApplication { occurrence, .. } = &event.observation {
            let rule = store::recurrence::get(&copied.store.pool, &occurrence.rule_uid).await?;
            if rule.is_some_and(|rule| required_reads.contains(&rule.consequences.iter().find_map(nucleus::karma::Consequence::transfer_target).map(str::to_owned).unwrap_or(rule.record_uid))) {
                required_reads
                    .extend(readable_rule(engine, copied, actor, &occurrence.rule_uid).await?);
            }
        }
    }
    let first_failure = session.checks.findings.first().cloned();
    if let Some(finding) = &first_failure {
        match &finding.witness {
            report::Witness::RuleCycle { cycle } => {
                required_reads.extend(readable_cycle(engine, copied, actor, cycle).await?);
            }
            report::Witness::Quantity { record, .. } => {
                engine
                    .refuse_unreadable(actor, &[record.as_str().into()])
                    .await?;
                required_reads.push(record.as_str().into());
            }
            report::Witness::RefusedRule { occurrence, .. }
            | report::Witness::DuplicateApplication { occurrence, .. } => {
                required_reads
                    .extend(readable_rule(engine, copied, actor, &occurrence.rule_uid).await?);
            }
            _ if actor.is_some() => {
                return Err(preview::invalid(
                    "The first restriction needs evidence outside the permitted preview",
                )
                .into());
            }
            _ => {}
        }
    }
    let mut unsupported: Vec<_> = session
        .world
        .trace
        .iter()
        .filter_map(|event| match &event.observation {
            Observation::ActionRefused { .. } => {
                Some("An input or proposed Rule was refused".to_owned())
            }
            Observation::RuleApplication {
                status: report::ApplicationStatus::Failed,
                ..
            } => Some("A Rule was refused or its reading failed".to_owned()),
            Observation::DatabaseEffect { ok: false, .. } => {
                Some("A resulting action failed".to_owned())
            }
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let pending: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_transfer_command k JOIN transfer_remote_command c ON c.command_uid = k.command_uid WHERE k.cancelled = 0 AND c.direction = 'outgoing' AND c.status NOT IN ('accepted', 'rejected'))")
        .fetch_one(&copied.store.pool).await?;
    if pending { unsupported.push("A Transfer command is pending; this single-Cell preview has no origin delivery adapter".into()); }
    let run = session.finish().await?;
    let source_current = engine.store.state_hash().await?.as_str() == source;
    let incomplete_checks = run
        .result
        .coverage
        .iter()
        .any(|coverage| coverage.status != report::CheckStatus::Skipped && !coverage.complete);
    Ok(Report {
        required_reads,
        draft,
        source,
        source_current,
        start_ms,
        requested_until_ms: end_ms,
        stopped_at_ms: run.result.stopped_at_ms,
        stop: run.result.stop.clone(),
        evaluations: run.result.rule_evaluations,
        final_values,
        first_failure,
        cycles,
        coverage: run.result.coverage,
        incomplete: hidden
            || incomplete_checks
            || !unsupported.is_empty()
            || run.result.stop != (report::Stop::HorizonReached {}),
        unsupported,
        assumptions: request.inputs,
    })
}
