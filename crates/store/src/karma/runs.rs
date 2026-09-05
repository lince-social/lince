use std::{collections::BTreeMap, num::NonZeroU32};

use chrono::{DateTime, TimeZone, Utc};
use nucleus::karma::{
    CanonicalHash, ControlState, DatumState, EvaluationLimits, EvaluationResult,
    FrozenEvaluationContext, InputSource, KarmaCandidateProposal, KarmaOccurrenceSource, KarmaRun,
    KarmaRunOutcome, LiteralValue, LocalId, NodeOperation, OccurrenceProgramEpoch,
    PersistedProgramNodeState, ProgramEpochMember, ProgramNotApplicableReason, ProgramRunBlockCode,
    ProgramStateEvent, ProgramStateResetReason, ReferenceKind, StateMigrationPolicy,
    StatePersistence, StateResetPolicy, TriggerSource, TypedUid, canonical_json_bytes,
    capture_evaluation_replay, evaluate_program,
};
use serde::Serialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::occurrences::{self, KarmaOccurrenceRow};
use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrenceProgramEpochRow {
    pub epoch_hash: CanonicalHash,
    pub epoch: OccurrenceProgramEpoch,
    pub next_member_ordinal: u64,
    pub completed: bool,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KarmaRunRow {
    pub run_hash: CanonicalHash,
    pub run: KarmaRun,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramProcessingTurn {
    pub occurrence_hash: CanonicalHash,
    pub cell_sequence: u64,
    pub epoch_hash: CanonicalHash,
    pub runs: Vec<KarmaRunRow>,
    pub completed: bool,
}

struct PreparedRun {
    run: KarmaRun,
    state_mutations: Vec<PreparedStateMutation>,
}

struct PreparedStateMutation {
    node_id: LocalId,
    expected_event_hash: Option<CanonicalHash>,
    state_revision: u64,
    reset_reason: Option<ProgramStateResetReason>,
    state: Option<PersistedProgramNodeState>,
}

#[async_trait::async_trait]
pub trait ExternalInputResolver: Send + Sync {
    async fn saved_protein(&self, view_uid: &str) -> Result<Option<LiteralValue>, StoreError>;
}

pub async fn process_next_occurrence(
    pool: &SqlitePool,
    limits: EvaluationLimits,
    member_page_size: NonZeroU32,
    now: DateTime<Utc>,
    resolver: Option<&dyn ExternalInputResolver>,
) -> Result<Option<ProgramProcessingTurn>, StoreError> {
    let now = canonical_time(now)?;
    let Some((occurrence, mut epoch)) = freeze_next_epoch(pool, now).await? else {
        return Ok(None);
    };
    let mut runs = Vec::new();
    for _ in 0..member_page_size.get() {
        if epoch.completed {
            break;
        }
        let ordinal = usize::try_from(epoch.next_member_ordinal)
            .map_err(|_| protocol("Program epoch member ordinal overflowed"))?;
        let member = epoch
            .epoch
            .members
            .get(ordinal)
            .ok_or_else(|| protocol("Program epoch cursor is outside its frozen member set"))?;
        let prepared = evaluate_member(pool, &occurrence, &epoch, member, limits, resolver).await?;
        let (stored, advanced) = persist_run(pool, &epoch, prepared, now).await?;
        runs.push(stored);
        epoch = advanced;
    }
    Ok(Some(ProgramProcessingTurn {
        occurrence_hash: occurrence.occurrence_hash,
        cell_sequence: occurrence.cell_sequence,
        epoch_hash: epoch.epoch_hash.clone(),
        runs,
        completed: epoch.completed,
    }))
}

pub async fn has_pending_occurrences(pool: &SqlitePool) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM karma_occurrence occurrence
            JOIN karma_occurrence_processing_state state
              ON state.singleton = 1
             AND occurrence.cell_sequence = state.next_cell_sequence
         )",
    )
    .fetch_one(pool)
    .await
}

pub async fn get_epoch(
    pool: &SqlitePool,
    occurrence_hash: &CanonicalHash,
) -> Result<Option<OccurrenceProgramEpochRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_occurrence_program_epoch WHERE occurrence_hash = ?")
        .bind(occurrence_hash.as_str())
        .fetch_optional(pool)
        .await?;
    row.map(map_epoch).transpose()
}

pub async fn list_epochs(pool: &SqlitePool) -> Result<Vec<OccurrenceProgramEpochRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM karma_occurrence_program_epoch
         ORDER BY cell_sequence, occurrence_hash",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_epoch)
    .collect()
}

pub async fn list_runs(pool: &SqlitePool) -> Result<Vec<KarmaRunRow>, StoreError> {
    sqlx::query("SELECT * FROM karma_run ORDER BY cell_sequence, member_ordinal, run_hash")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_run)
        .collect()
}

async fn freeze_next_epoch(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<Option<(KarmaOccurrenceRow, OccurrenceProgramEpochRow)>, StoreError> {
    let at = now.to_rfc3339();
    let mut tx = crate::write_tx(pool).await?;
    let next_sequence = rust_u64(
        sqlx::query_scalar::<_, i64>(
            "SELECT next_cell_sequence FROM karma_occurrence_processing_state
             WHERE singleton = 1",
        )
        .fetch_one(&mut *tx)
        .await?,
        "occurrence processing sequence",
    )?;
    let Some(occurrence) = occurrences::get_by_cell_sequence_tx(&mut tx, next_sequence).await?
    else {
        tx.rollback().await?;
        return Ok(None);
    };
    if let Some(epoch) = get_epoch_tx(&mut tx, &occurrence.occurrence_hash).await? {
        if epoch.completed {
            return Err(protocol(
                "completed Program epoch is still selected by the occurrence processing cursor",
            ));
        }
        tx.commit().await?;
        return Ok(Some((occurrence, epoch)));
    }

    let this_cell: Option<String> =
        sqlx::query_scalar("SELECT uid FROM record WHERE slug = ? AND kind = 'device' LIMIT 1")
            .bind(crate::cells::LOCAL_CELL_SLUG)
            .fetch_optional(&mut *tx)
            .await?;

    let rows = sqlx::query(
        "SELECT program.record_uid, program.active_revision_hash, program.handle_revision
         FROM karma_program program
         JOIN record ON record.uid = program.record_uid
         LEFT JOIN karma_program_execution execution
                ON execution.program_uid = program.record_uid
         LEFT JOIN record_extension executor
                ON executor.record_uid = program.record_uid
               AND executor.namespace = 'lince.schedule.executor'
         WHERE program.status = 'active'
           AND program.active_revision_hash IS NOT NULL
           AND record.deleted_at IS NULL
           AND execution.executes IS NOT 0
           AND (executor.fds IS NULL
                OR json_extract(executor.fds, '$.cell') IS NULL
                OR json_extract(executor.fds, '$.cell') = ?)
         ORDER BY program.record_uid, program.active_revision_hash",
    )
    .bind(this_cell.as_deref())
    .fetch_all(&mut *tx)
    .await?;
    let members = rows
        .into_iter()
        .map(|row| {
            ProgramEpochMember::new(
                row.get::<String, _>("record_uid"),
                parse_hash(row.get("active_revision_hash"))?,
                rust_u64(
                    row.get("handle_revision"),
                    "Program activation handle revision",
                )?,
            )
            .map_err(boundary)
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    let epoch = OccurrenceProgramEpoch::new(
        occurrence.occurrence_hash.clone(),
        occurrence.cell_sequence,
        members,
    )
    .map_err(boundary)?;
    let epoch_hash = epoch.epoch_hash().map_err(boundary)?;
    let epoch_json = canonical_string(&epoch)?;
    let member_count = u64::try_from(epoch.members.len())
        .map_err(|_| protocol("Program epoch member count overflowed"))?;
    let completed = member_count == 0;
    sqlx::query(
        "INSERT INTO karma_occurrence_program_epoch
            (occurrence_hash, cell_sequence, epoch_hash, epoch_json, member_count,
             next_member_ordinal, completed, created_at, updated_at, completed_at)
         VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?, ?)",
    )
    .bind(occurrence.occurrence_hash.as_str())
    .bind(sql_i64(occurrence.cell_sequence, "Cell sequence")?)
    .bind(epoch_hash.as_str())
    .bind(epoch_json)
    .bind(sql_i64(member_count, "Program epoch member count")?)
    .bind(completed)
    .bind(&at)
    .bind(&at)
    .bind(completed.then_some(at.as_str()))
    .execute(&mut *tx)
    .await?;
    if completed {
        advance_processing_sequence(&mut tx, occurrence.cell_sequence).await?;
    }
    let stored = get_epoch_tx(&mut tx, &occurrence.occurrence_hash)
        .await?
        .expect("inserted Program epoch exists");
    tx.commit().await?;
    Ok(Some((occurrence, stored)))
}

async fn evaluate_member(
    pool: &SqlitePool,
    occurrence: &KarmaOccurrenceRow,
    epoch: &OccurrenceProgramEpochRow,
    member: &ProgramEpochMember,
    limits: EvaluationLimits,
    resolver: Option<&dyn ExternalInputResolver>,
) -> Result<PreparedRun, StoreError> {
    let revision = super::programs::get_revision(pool, &member.revision_hash)
        .await?
        .ok_or_else(|| protocol("frozen Program epoch revision is missing"))?;
    if revision.program_uid != member.program_uid {
        return Err(protocol(
            "frozen Program epoch revision belongs to another Program",
        ));
    }
    let ordinal = epoch.next_member_ordinal;
    let frequency_uid = occurrence_frequency_uid(pool, occurrence).await?;
    let mut boundary_values = BTreeMap::new();
    let mut matched = false;
    for (node_id, node) in &revision.program.nodes {
        let NodeOperation::Trigger { source, .. } = &node.operation else {
            continue;
        };
        let node_matched = matches!(source, TriggerSource::Frequency { frequency }
            if frequency_uid.as_deref() == Some(frequency.target.as_str()));
        matched |= node_matched;
        boundary_values.insert(
            node_id.clone(),
            LiteralValue::Bool {
                value: node_matched,
            },
        );
    }

    let mut blocked_input = None;
    for (node_id, node) in &revision.program.nodes {
        let NodeOperation::Input { source, .. } = &node.operation else {
            continue;
        };
        match source {
            InputSource::Parameter { .. } => {}
            InputSource::RecordQuantity { record } => {
                match record_quantity_value(pool, record.target.as_str()).await? {
                    Ok(value) => {
                        boundary_values.insert(node_id.clone(), value);
                    }
                    Err(code) => blocked_input = Some(code),
                }
            }
            InputSource::SavedProtein { view } => match resolver {
                Some(resolver) => match resolver.saved_protein(view.target.as_str()).await? {
                    Some(value) => {
                        boundary_values.insert(node_id.clone(), value);
                    }
                    None => blocked_input = Some(ProgramRunBlockCode::InputSourceUnresolved),
                },
                None => blocked_input = Some(ProgramRunBlockCode::InputSourceUnresolved),
            },
            InputSource::Signal { .. }
            | InputSource::SecretMetadata { .. }
            | InputSource::CapturedFact { .. } => {
                blocked_input = Some(ProgramRunBlockCode::InputSourceUnresolved);
            }
        }
    }

    let mut state_mutations = Vec::new();
    let outcome = if !matched {
        KarmaRunOutcome::NotApplicable {
            reason: ProgramNotApplicableReason::NoMatchingTrigger,
        }
    } else if let Some(code) = blocked_input {
        KarmaRunOutcome::Blocked { code }
    } else {
        let mut context = FrozenEvaluationContext {
            boundary_values,
            logical_at: Some(occurrence.logical_at),
            ..FrozenEvaluationContext::default()
        };
        match freeze_program_state(pool, &revision.program, member, &mut context).await? {
            StatePreparation::Blocked(code) => KarmaRunOutcome::Blocked { code },
            StatePreparation::Ready(frozen) => {
                match evaluate_program(&revision.program, &context, limits) {
                    Ok(result) => {
                        state_mutations = collect_state_mutations(&frozen, &result)?;
                        KarmaRunOutcome::Succeeded {
                            replay: capture_evaluation_replay(&revision.program, &context, limits)
                                .map_err(|error| {
                                    protocol(format!(
                                        "successful Program evaluation could not produce replay capsule: {error}"
                                    ))
                                })?,
                        }
                    }
                    Err(error) => KarmaRunOutcome::EvaluationFailed {
                        context,
                        limits,
                        error,
                    },
                }
            }
        }
    };
    let run = KarmaRun::new(
        occurrence.occurrence_hash.clone(),
        occurrence.cell_sequence,
        occurrence.logical_at,
        epoch.epoch_hash.clone(),
        ordinal,
        member.program_uid.clone(),
        member.revision_hash.clone(),
        outcome,
    )
    .map_err(boundary)?;
    Ok(PreparedRun {
        run,
        state_mutations,
    })
}

async fn record_quantity_value(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Result<LiteralValue, ProgramRunBlockCode>, StoreError> {
    let Some(record) = crate::records::get(pool, record_uid).await? else {
        return Ok(Err(ProgramRunBlockCode::RecordQuantityUnavailable));
    };
    let amount = crate::facts::level(pool, record_uid).await?;
    match &record.unit_uid {
        Some(unit_uid) => match TypedUid::new(ReferenceKind::Unit, unit_uid.clone()) {
            Ok(unit) => Ok(Ok(LiteralValue::Quantity { amount, unit })),
            Err(_) => Ok(Err(ProgramRunBlockCode::RecordUnitUntypable)),
        },
        None => Ok(Ok(LiteralValue::Decimal { value: amount })),
    }
}

enum StatePreparation {
    Ready(Vec<FrozenStateNode>),
    Blocked(ProgramRunBlockCode),
}

struct FrozenStateNode {
    node_id: LocalId,
    current: Option<super::states::ProgramNodeStateRow>,
    reset_reason: Option<ProgramStateResetReason>,
    kind: FrozenStateKind,
}

#[derive(Clone, Copy)]
enum FrozenStateKind {
    Delay,
    Control,
}

async fn freeze_program_state(
    pool: &SqlitePool,
    program: &nucleus::karma::ProgramAst,
    member: &ProgramEpochMember,
    context: &mut FrozenEvaluationContext,
) -> Result<StatePreparation, StoreError> {
    let mut frozen = Vec::new();
    for (node_id, node) in &program.nodes {
        let Some(contract) = state_contract(&node.operation) else {
            continue;
        };
        if contract.persistence != StatePersistence::Program {
            return Ok(StatePreparation::Blocked(
                ProgramRunBlockCode::UnsupportedStatePersistence,
            ));
        }
        let current = super::states::get_node_state(pool, &member.program_uid, node_id).await?;
        let mut reset_reason = None;
        if let Some(current) = &current {
            let activation_changed =
                current.activation_handle_revision != member.activation_handle_revision;
            let revision_changed = current.definition_revision_hash != member.revision_hash;
            if activation_changed && contract.reset == StateResetPolicy::OnProgramActivation {
                reset_reason = Some(ProgramStateResetReason::ProgramActivation);
            } else if revision_changed && contract.reset == StateResetPolicy::OnRevisionChange {
                reset_reason = Some(ProgramStateResetReason::RevisionChange);
            } else if revision_changed {
                match contract.migration {
                    StateMigrationPolicy::Reset => {
                        reset_reason = Some(ProgramStateResetReason::MigrationReset)
                    }
                    StateMigrationPolicy::RequireExplicit => {
                        return Ok(StatePreparation::Blocked(
                            ProgramRunBlockCode::StateMigrationRequired,
                        ));
                    }
                    StateMigrationPolicy::CompatibleTypeOnly => {
                        if current
                            .state
                            .as_ref()
                            .is_some_and(|state| !state_compatible(node, state))
                        {
                            return Ok(StatePreparation::Blocked(
                                ProgramRunBlockCode::StateMigrationRequired,
                            ));
                        }
                    }
                }
            } else if current
                .state
                .as_ref()
                .is_some_and(|state| !state_compatible(node, state))
            {
                return Err(protocol(
                    "persisted Program node state is incompatible with its unchanged revision",
                ));
            }
            if reset_reason.is_none()
                && let Some(state) = &current.state
            {
                match state {
                    PersistedProgramNodeState::Delay { value } => {
                        context.delay_state.insert(node_id.clone(), value.clone());
                    }
                    PersistedProgramNodeState::Control { value } => {
                        context.control_state.insert(node_id.clone(), value.clone());
                    }
                }
            }
        }
        frozen.push(FrozenStateNode {
            node_id: node_id.clone(),
            current,
            reset_reason,
            kind: if matches!(node.operation, NodeOperation::Delay { .. }) {
                FrozenStateKind::Delay
            } else {
                FrozenStateKind::Control
            },
        });
    }
    Ok(StatePreparation::Ready(frozen))
}

fn collect_state_mutations(
    frozen: &[FrozenStateNode],
    result: &EvaluationResult,
) -> Result<Vec<PreparedStateMutation>, StoreError> {
    let mut mutations = Vec::new();
    for item in frozen {
        let state = match item.kind {
            FrozenStateKind::Delay => result
                .state_updates
                .get(&item.node_id)
                .cloned()
                .map(|value| PersistedProgramNodeState::Delay { value }),
            FrozenStateKind::Control => result
                .control_state_updates
                .get(&item.node_id)
                .cloned()
                .map(|value| PersistedProgramNodeState::Control { value }),
        };
        if state.is_none() && item.reset_reason.is_none() {
            continue;
        }
        let state_revision = item.current.as_ref().map_or(Ok(1), |current| {
            current
                .state_revision
                .checked_add(1)
                .ok_or_else(|| protocol("Program state revision overflowed"))
        })?;
        mutations.push(PreparedStateMutation {
            node_id: item.node_id.clone(),
            expected_event_hash: item
                .current
                .as_ref()
                .map(|current| current.current_event_hash.clone()),
            state_revision,
            reset_reason: item.reset_reason,
            state,
        });
    }
    Ok(mutations)
}

fn state_contract(operation: &NodeOperation) -> Option<&nucleus::karma::StateContract> {
    match operation {
        NodeOperation::Delay { state, .. }
        | NodeOperation::Threshold { state, .. }
        | NodeOperation::Debounce { state, .. }
        | NodeOperation::Cooldown { state, .. }
        | NodeOperation::RateLimit { state, .. } => Some(state),
        NodeOperation::Trigger { .. }
        | NodeOperation::Input { .. }
        | NodeOperation::Derive { .. }
        | NodeOperation::RouteCandidate { .. } => None,
    }
}

fn state_compatible(node: &nucleus::karma::NodeAst, state: &PersistedProgramNodeState) -> bool {
    match (&node.operation, state) {
        (NodeOperation::Delay { output, .. }, PersistedProgramNodeState::Delay { value }) => node
            .outputs
            .get(output)
            .is_some_and(|contract| value.value_type().ok() == Some(contract.value_type.clone())),
        (
            NodeOperation::Threshold { .. },
            PersistedProgramNodeState::Control {
                value: ControlState::Threshold { .. },
            },
        )
        | (
            NodeOperation::Debounce { .. },
            PersistedProgramNodeState::Control {
                value: ControlState::Debounce { .. },
            },
        )
        | (
            NodeOperation::Cooldown { .. },
            PersistedProgramNodeState::Control {
                value: ControlState::Cooldown { .. },
            },
        )
        | (
            NodeOperation::RateLimit { .. },
            PersistedProgramNodeState::Control {
                value: ControlState::RateLimit { .. },
            },
        ) => true,
        _ => false,
    }
}

fn candidate_proposals(
    run_hash: &CanonicalHash,
    run: &KarmaRun,
) -> Result<Vec<KarmaCandidateProposal>, StoreError> {
    let KarmaRunOutcome::Succeeded { replay } = &run.outcome else {
        return Ok(Vec::new());
    };
    let mut proposals = Vec::new();
    for trace in &replay.capsule.expected_result.trace {
        for (output, value) in &trace.outputs {
            let LiteralValue::Datum {
                state: DatumState::Value,
                value: Some(candidate),
                ..
            } = value
            else {
                continue;
            };
            let LiteralValue::Candidate {
                route,
                template,
                fields,
            } = candidate.as_ref()
            else {
                continue;
            };
            proposals.push(
                KarmaCandidateProposal::new(
                    run_hash.clone(),
                    run.occurrence_hash.clone(),
                    run.program_uid.clone(),
                    run.program_revision_hash.clone(),
                    trace.node.clone(),
                    output.clone(),
                    *route,
                    template.clone(),
                    fields.clone(),
                )
                .map_err(boundary)?,
            );
        }
    }
    Ok(proposals)
}

async fn occurrence_frequency_uid(
    pool: &SqlitePool,
    occurrence: &KarmaOccurrenceRow,
) -> Result<Option<String>, StoreError> {
    let activation_hash = match &occurrence.envelope.source {
        KarmaOccurrenceSource::ScheduleTick { tick, .. } => &tick.activation_hash,
        KarmaOccurrenceSource::ScheduleCoalesced { batch, .. } => &batch.activation_hash,
        KarmaOccurrenceSource::CalendarTick { tick, .. } => &tick.activation_hash,
        KarmaOccurrenceSource::CalendarCoalesced { batch, .. } => &batch.activation_hash,
    };
    Ok(super::frequencies::get_activation(pool, activation_hash)
        .await?
        .map(|activation| activation.epoch.frequency_uid().to_string()))
}

async fn persist_run(
    pool: &SqlitePool,
    expected_epoch: &OccurrenceProgramEpochRow,
    prepared: PreparedRun,
    now: DateTime<Utc>,
) -> Result<(KarmaRunRow, OccurrenceProgramEpochRow), StoreError> {
    let run = prepared.run;
    let run_hash = run.run_hash().map_err(boundary)?;
    let run_json = canonical_string(&run)?;
    let at = now.to_rfc3339();
    let mut tx = crate::write_tx(pool).await?;
    let current = get_epoch_tx(&mut tx, &run.occurrence_hash)
        .await?
        .ok_or_else(|| protocol("Karma run Program epoch is missing"))?;
    if current.epoch_hash != expected_epoch.epoch_hash
        || current.completed
        || current.next_member_ordinal != run.member_ordinal
    {
        return Err(protocol(
            "Karma run lost Program epoch cursor serialization",
        ));
    }
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO karma_run
            (run_hash, occurrence_hash, cell_sequence, program_epoch_hash,
             member_ordinal, program_uid, program_revision_hash, status,
             fuel_used, run_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(run_hash.as_str())
    .bind(run.occurrence_hash.as_str())
    .bind(sql_i64(run.cell_sequence, "Karma run Cell sequence")?)
    .bind(run.program_epoch_hash.as_str())
    .bind(sql_i64(run.member_ordinal, "Program member ordinal")?)
    .bind(&run.program_uid)
    .bind(run.program_revision_hash.as_str())
    .bind(run.outcome.status_name())
    .bind(sql_i64(run.outcome.fuel_used(), "Karma run fuel")?)
    .bind(run_json)
    .bind(&at)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored = get_run_by_member_tx(&mut tx, &run.occurrence_hash, run.member_ordinal)
        .await?
        .ok_or_else(|| protocol("inserted Karma run is missing"))?;
    if !inserted && (stored.run_hash != run_hash || stored.run != run) {
        return Err(protocol(
            "Karma occurrence and Program revision already have a different run",
        ));
    }
    if !inserted && !prepared.state_mutations.is_empty() {
        return Err(protocol(
            "existing Karma run cannot be paired with unapplied Program state",
        ));
    }
    let member_index = usize::try_from(run.member_ordinal)
        .map_err(|_| protocol("Program member ordinal overflowed"))?;
    let frozen_member = current
        .epoch
        .members
        .get(member_index)
        .ok_or_else(|| protocol("Karma run member is outside its frozen Program epoch"))?;
    if frozen_member.program_uid != run.program_uid
        || frozen_member.revision_hash != run.program_revision_hash
    {
        return Err(protocol(
            "Karma run disagrees with its frozen Program member",
        ));
    }
    for mutation in prepared.state_mutations {
        let event = ProgramStateEvent::new(
            run.program_uid.clone(),
            mutation.node_id,
            mutation.state_revision,
            mutation.expected_event_hash.clone(),
            run_hash.clone(),
            run.program_revision_hash.clone(),
            frozen_member.activation_handle_revision,
            mutation.reset_reason,
            mutation.state,
        )
        .map_err(boundary)?;
        super::states::apply_event_tx(&mut tx, &event, mutation.expected_event_hash.as_ref(), &at)
            .await?;
    }
    for proposal in candidate_proposals(&run_hash, &run)? {
        super::candidates::insert_tx(&mut tx, &proposal, &at).await?;
    }

    let next_ordinal = run
        .member_ordinal
        .checked_add(1)
        .ok_or_else(|| protocol("Program epoch member ordinal overflowed"))?;
    let member_count = u64::try_from(current.epoch.members.len())
        .map_err(|_| protocol("Program epoch member count overflowed"))?;
    let completed = next_ordinal == member_count;
    let updated = sqlx::query(
        "UPDATE karma_occurrence_program_epoch
         SET next_member_ordinal = ?, completed = ?, updated_at = ?, completed_at = ?
         WHERE occurrence_hash = ? AND next_member_ordinal = ? AND completed = 0",
    )
    .bind(sql_i64(next_ordinal, "Program epoch member ordinal")?)
    .bind(completed)
    .bind(&at)
    .bind(completed.then_some(at.as_str()))
    .bind(run.occurrence_hash.as_str())
    .bind(sql_i64(run.member_ordinal, "Program epoch member ordinal")?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(protocol(
            "Karma run lost Program epoch cursor serialization",
        ));
    }
    if completed {
        advance_processing_sequence(&mut tx, run.cell_sequence).await?;
    }
    let advanced = get_epoch_tx(&mut tx, &run.occurrence_hash)
        .await?
        .expect("advanced Program epoch exists");
    tx.commit().await?;
    Ok((stored, advanced))
}

async fn advance_processing_sequence(
    tx: &mut Transaction<'_, Sqlite>,
    completed_sequence: u64,
) -> Result<(), StoreError> {
    let next = completed_sequence
        .checked_add(1)
        .ok_or_else(|| protocol("occurrence processing sequence overflowed"))?;
    let updated = sqlx::query(
        "UPDATE karma_occurrence_processing_state SET next_cell_sequence = ?
         WHERE singleton = 1 AND next_cell_sequence = ?",
    )
    .bind(sql_i64(next, "occurrence processing sequence")?)
    .bind(sql_i64(
        completed_sequence,
        "occurrence processing sequence",
    )?)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(protocol(
            "occurrence processing cursor lost Cell sequence serialization",
        ));
    }
    Ok(())
}

async fn get_epoch_tx(
    tx: &mut Transaction<'_, Sqlite>,
    occurrence_hash: &CanonicalHash,
) -> Result<Option<OccurrenceProgramEpochRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_occurrence_program_epoch WHERE occurrence_hash = ?")
        .bind(occurrence_hash.as_str())
        .fetch_optional(&mut **tx)
        .await?;
    row.map(map_epoch).transpose()
}

async fn get_run_by_member_tx(
    tx: &mut Transaction<'_, Sqlite>,
    occurrence_hash: &CanonicalHash,
    member_ordinal: u64,
) -> Result<Option<KarmaRunRow>, StoreError> {
    let row =
        sqlx::query("SELECT * FROM karma_run WHERE occurrence_hash = ? AND member_ordinal = ?")
            .bind(occurrence_hash.as_str())
            .bind(sql_i64(member_ordinal, "Program member ordinal")?)
            .fetch_optional(&mut **tx)
            .await?;
    row.map(map_run).transpose()
}

fn map_epoch(row: sqlx::sqlite::SqliteRow) -> Result<OccurrenceProgramEpochRow, StoreError> {
    let occurrence_hash = parse_hash(row.get("occurrence_hash"))?;
    let epoch_hash = parse_hash(row.get("epoch_hash"))?;
    let epoch_json: String = row.get("epoch_json");
    let epoch: OccurrenceProgramEpoch = serde_json::from_str(&epoch_json).map_err(json_protocol)?;
    epoch.validate().map_err(boundary)?;
    let next_member_ordinal = rust_u64(
        row.get("next_member_ordinal"),
        "Program epoch member ordinal",
    )?;
    let member_count = rust_u64(row.get("member_count"), "Program epoch member count")?;
    let completed: bool = row.get("completed");
    let completed_at: Option<String> = row.get("completed_at");
    if canonical_string(&epoch)? != epoch_json
        || epoch.occurrence_hash != occurrence_hash
        || epoch.epoch_hash().map_err(boundary)? != epoch_hash
        || u64::try_from(epoch.members.len()).ok() != Some(member_count)
        || next_member_ordinal > member_count
        || completed != (next_member_ordinal == member_count)
        || completed != completed_at.is_some()
    {
        return Err(protocol("stored occurrence Program epoch is invalid"));
    }
    Ok(OccurrenceProgramEpochRow {
        epoch_hash,
        epoch,
        next_member_ordinal,
        completed,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        completed_at,
    })
}

fn map_run(row: sqlx::sqlite::SqliteRow) -> Result<KarmaRunRow, StoreError> {
    let run_hash = parse_hash(row.get("run_hash"))?;
    let run_json: String = row.get("run_json");
    let run: KarmaRun = serde_json::from_str(&run_json).map_err(json_protocol)?;
    run.validate().map_err(boundary)?;
    if canonical_string(&run)? != run_json
        || run.run_hash().map_err(boundary)? != run_hash
        || run.occurrence_hash.as_str() != row.get::<String, _>("occurrence_hash")
        || sql_i64(run.cell_sequence, "Karma run Cell sequence")?
            != row.get::<i64, _>("cell_sequence")
        || run.program_epoch_hash.as_str() != row.get::<String, _>("program_epoch_hash")
        || sql_i64(run.member_ordinal, "Program member ordinal")?
            != row.get::<i64, _>("member_ordinal")
        || run.program_uid != row.get::<String, _>("program_uid")
        || run.program_revision_hash.as_str() != row.get::<String, _>("program_revision_hash")
        || run.outcome.status_name() != row.get::<String, _>("status")
        || sql_i64(run.outcome.fuel_used(), "Karma run fuel")? != row.get::<i64, _>("fuel_used")
    {
        return Err(protocol("stored Karma run projections or hash are invalid"));
    }
    Ok(KarmaRunRow {
        run_hash,
        run,
        created_at: row.get("created_at"),
    })
}

fn canonical_time(value: DateTime<Utc>) -> Result<DateTime<Utc>, StoreError> {
    Utc.timestamp_millis_opt(value.timestamp_millis())
        .single()
        .ok_or_else(|| protocol("Karma run time is outside the supported range"))
}

fn canonical_string<T: Serialize>(value: &T) -> Result<String, StoreError> {
    String::from_utf8(canonical_json_bytes(value).map_err(boundary)?)
        .map_err(|error| protocol(error.to_string()))
}

fn parse_hash(value: String) -> Result<CanonicalHash, StoreError> {
    CanonicalHash::parse(value).map_err(boundary)
}

fn sql_i64(value: u64, name: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol(format!("{name} exceeds SQLite integer range")))
}

fn rust_u64(value: i64, name: &str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| protocol(format!("stored {name} is invalid")))
}

fn boundary(error: nucleus::karma::KarmaBoundaryError) -> StoreError {
    protocol(error.to_string())
}

fn json_protocol(error: serde_json::Error) -> StoreError {
    protocol(error.to_string())
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}
