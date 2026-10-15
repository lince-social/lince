use std::collections::{BTreeMap, BTreeSet};

use nucleus::simulation::{Cause, Event, Observation, RecordedSchedule};
use serde::{Deserialize, Serialize};

use crate::{Result, proposal::Occurrence, scenario::Scenario, world::World};

pub const MAX_TARGETS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub cell: String,
    pub record: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub targets: Vec<Target>,
    pub truncated: bool,
    pub source_history: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub selection: Selection,
    pub budget_exhausted: bool,
}

impl Selection {
    pub fn collect(worlds: &[&World]) -> Self {
        let mut targets = BTreeSet::new();
        let mut truncated = false;
        for target in worlds
            .iter()
            .flat_map(|world| self::targets(&world.scenario, &world.trace))
        {
            targets.insert(target);
            if targets.len() > MAX_TARGETS {
                targets.pop_last();
                truncated = true;
            }
        }
        Self {
            targets: targets.into_iter().take(MAX_TARGETS).collect(),
            truncated,
            source_history: worlds
                .iter()
                .flat_map(|world| world.source_history.iter().cloned())
                .collect(),
        }
    }

    pub fn validate(&self, scenario: &Scenario) -> Result<()> {
        if self.targets.len() > MAX_TARGETS
            || self.targets.windows(2).any(|pair| pair[0] >= pair[1])
            || self.targets.iter().any(|target| {
                !nucleus::valid_uid(&target.record, "r")
                    || !scenario.cells.iter().any(|cell| cell.name == target.cell)
            })
        {
            return Err("Invalid Record evidence selection".into());
        }
        if self.source_history.iter().any(|name| {
            !scenario
                .cells
                .iter()
                .any(|cell| &cell.name == name && cell.database.is_some())
        }) {
            return Err("Historical coverage has no retained source Cell".into());
        }
        Ok(())
    }
}

pub fn validate_bundle(
    selection: &Selection,
    records: &[crate::proposal::Outcome],
    scenario: &Scenario,
    events: &[Event],
    stop: &nucleus::simulation::Stop,
    budget_exhausted: bool,
) -> Result<()> {
    use crate::proposal::{CaptureStatus, OccurrenceGap};
    selection.validate(scenario)?;
    let selected: BTreeSet<_> = selection.targets.iter().cloned().collect();
    if !selection.truncated && !targets(scenario, events).is_subset(&selected) {
        return Err("Record evidence omits required targets".into());
    }
    if selection.truncated && !budget_exhausted {
        return Err("Truncated Record selection lacks its capture-budget explanation".into());
    }
    if budget_exhausted && *stop == (nucleus::simulation::Stop::HorizonReached {}) {
        return Err("Incomplete Record capture claims a completed run".into());
    }
    let expected = occurrence_evidence(events, scenario.limits.evidence_bytes)?;
    let mut applied = BTreeMap::new();
    for event in events {
        if let Observation::RuleApplication {
            occurrence,
            status: nucleus::simulation::ApplicationStatus::Applied,
            ..
        } = &event.observation
        {
            applied.insert(
                (
                    &event.cell,
                    &occurrence.rule_uid,
                    occurrence.revision,
                    &occurrence.event_id,
                ),
                occurrence,
            );
        }
    }
    let mut captured = BTreeSet::new();
    for outcome in records {
        let target = Target {
            cell: outcome.cell.clone(),
            record: outcome.record.clone(),
        };
        if !selected.contains(&target) || !captured.insert(target.clone()) {
            return Err("Duplicate or unselected Record outcome".into());
        }
        validate_outcome(
            outcome,
            scenario,
            expected.get(&target).map_or(&[], Vec::as_slice),
            stop,
        )?;
        if outcome.status == CaptureStatus::Captured
            && outcome
                .occurrence_gaps
                .contains(&OccurrenceGap::SourceHistory)
                != selection.source_history.contains(&outcome.cell)
        {
            return Err("Historical occurrence completeness is inconsistent".into());
        }
    }
    if captured != selected && !budget_exhausted {
        return Err("Missing outcomes have no evidence-budget explanation".into());
    }
    let mut schedules = BTreeSet::new();
    for event in events {
        if let Observation::RuleSchedule { schedule } = &event.observation {
            if schedule.cell != event.cell {
                return Err("Scheduled application belongs to a different Cell".into());
            }
            validate_schedule(schedule)?;
            let identity = (
                &event.cell,
                &schedule.occurrence.rule_uid,
                schedule.occurrence.revision,
                &schedule.occurrence.event_id,
            );
            if !schedules.insert(identity) {
                return Err("Duplicate scheduled application identity".into());
            }
            if applied.get(&identity).copied() != Some(&schedule.occurrence) {
                return Err("Schedule evidence has no committed application".into());
            }
        }
    }
    Ok(())
}

pub fn targets(scenario: &Scenario, events: &[Event]) -> BTreeSet<Target> {
    let mut selected = BTreeSet::new();
    if let Some(proposal) = &scenario.proposal {
        selected.extend(proposal.records.iter().map(|record| Target {
            cell: proposal.cell.clone(),
            record: record.uid.clone(),
        }));
    }
    for event in events {
        if matches!(event.caused_by, Cause::Seed {}) {
            continue;
        }
        let record = match &event.observation {
            Observation::CommittedQuantity { record, .. }
            | Observation::LoanQuantity { record, .. } => Some(record.as_str()),
            Observation::ActionAccepted {
                created: Some(record),
                ..
            } if nucleus::valid_uid(record, "r") => Some(record.as_str()),
            Observation::RuleSchedule { schedule } => Some(schedule.record.as_str()),
            _ => None,
        };
        if let Some(record) = record {
            selected.insert(Target {
                cell: event.cell.clone(),
                record: record.into(),
            });
            if selected.len() > MAX_TARGETS + 1 {
                selected.pop_last();
            }
        }
    }
    selected
}

pub fn occurrence_evidence(
    events: &[Event],
    limit: u64,
) -> Result<BTreeMap<Target, Vec<Occurrence>>> {
    let mut output = BTreeMap::<Target, Vec<Occurrence>>::new();
    let mut retained = 0u64;
    let mut count = 0u64;
    for event in events {
        if let Observation::RuleSchedule { schedule } = &event.observation {
            #[derive(Serialize)]
            struct Borrowed<'a> {
                rule: &'a str,
                revision: u64,
                event: &'a str,
                intended_at_ms: i64,
                observed_at_ms: i64,
                time: Option<&'a nucleus::schedule::TimeRange>,
                unavailable: bool,
            }
            let item = Borrowed {
                rule: &schedule.occurrence.rule_uid,
                revision: schedule.occurrence.revision,
                event: &schedule.occurrence.event_id,
                intended_at_ms: schedule
                    .occurrence
                    .intended_at_ms
                    .unwrap_or(schedule.observed_at_ms),
                observed_at_ms: schedule.observed_at_ms,
                time: schedule.time.as_ref(),
                unavailable: schedule.unavailable,
            };
            retained = retained.saturating_add(
                crate::limits::size(&item, limit.saturating_sub(retained), "occurrence capture")?
                    + 1,
            );
            count += 1;
            if retained > limit || count > crate::limits::rows::<Occurrence>(limit) {
                return Err(Box::new(crate::limits::Exceeded {
                    resource: "occurrence capture",
                    limit,
                }));
            }
            output
                .entry(Target {
                    cell: event.cell.clone(),
                    record: schedule.record.as_str().into(),
                })
                .or_default()
                .push(occurrence(schedule));
        }
    }
    for occurrences in output.values_mut() {
        sort_occurrences(occurrences);
    }
    Ok(output)
}

fn occurrence(schedule: &RecordedSchedule) -> Occurrence {
    Occurrence {
        rule: schedule.occurrence.rule_uid.clone(),
        revision: schedule.occurrence.revision,
        event: schedule.occurrence.event_id.clone(),
        intended_at_ms: schedule
            .occurrence
            .intended_at_ms
            .unwrap_or(schedule.observed_at_ms),
        observed_at_ms: schedule.observed_at_ms,
        time: schedule.time.clone(),
        unavailable: schedule.unavailable,
    }
}

fn sort_occurrences(output: &mut [Occurrence]) {
    output.sort_by(|left, right| {
        (left.intended_at_ms, &left.rule, left.revision, &left.event).cmp(&(
            right.intended_at_ms,
            &right.rule,
            right.revision,
            &right.event,
        ))
    });
}

pub fn validate_schedule(schedule: &RecordedSchedule) -> Result<()> {
    let Some(intended) = schedule.occurrence.intended_at_ms else {
        return Err("Scheduled application lacks its intended instant".into());
    };
    if !nucleus::valid_uid(&schedule.occurrence.rule_uid, "rec")
        || schedule.occurrence.revision == 0
        || schedule.occurrence.revision > i64::MAX as u64
        || schedule.occurrence.event_id.is_empty()
        || schedule.occurrence.event_id.len() > 1024
        || schedule.occurrence.event_id.chars().any(char::is_control)
        || schedule.record.kind() != nucleus::karma::ReferenceKind::Record
        || schedule
            .occurrence
            .frequency
            .as_ref()
            .is_some_and(|frequency| frequency.kind() != nucleus::karma::ReferenceKind::Frequency)
        || !(0..=nucleus::schedule::MAX_INSTANT_MS).contains(&intended)
        || !(0..=nucleus::schedule::MAX_INSTANT_MS).contains(&schedule.observed_at_ms)
        || schedule.unavailable != schedule.time.is_none()
    {
        return Err("Invalid scheduled application identity or completeness".into());
    }
    if let Some(time) = &schedule.time {
        if time.from_ms != intended
            || time.until_ms.is_some_and(|until| {
                until <= time.from_ms || until > nucleus::schedule::MAX_INSTANT_MS
            })
        {
            return Err("Invalid scheduled application range".into());
        }
    }
    Ok(())
}

fn validate_outcome(
    outcome: &crate::proposal::Outcome,
    scenario: &Scenario,
    expected: &[Occurrence],
    stop: &nucleus::simulation::Stop,
) -> Result<()> {
    use crate::proposal::{CaptureStatus, OccurrenceGap};
    if !scenario.cells.iter().any(|cell| cell.name == outcome.cell)
        || !nucleus::valid_uid(&outcome.record, "r")
    {
        return Err("Invalid Record outcome binding".into());
    }
    if outcome.status == CaptureStatus::Unavailable {
        if outcome.state.is_some()
            || !outcome.occurrences.is_empty()
            || !outcome.occurrence_gaps.is_empty()
        {
            return Err("Unavailable Record evidence contains captured values".into());
        }
        return Ok(());
    }
    if let Some(state) = &outcome.state {
        if state.uid != outcome.record
            || state.revision <= 0
            || nucleus::RecordKind::parse(&state.kind).is_none()
            || state
                .unit
                .as_ref()
                .is_some_and(|unit| !nucleus::valid_uid(unit, "c"))
        {
            return Err("Invalid captured Record identity or revision".into());
        }
        crate::proposal::validate_record(state)?;
    }
    if outcome.occurrence_gaps.len() > 3
        || outcome
            .occurrence_gaps
            .iter()
            .enumerate()
            .any(|(index, gap)| outcome.occurrence_gaps[..index].contains(gap))
    {
        return Err("Duplicate or invalid occurrence completeness reasons".into());
    }
    if outcome
        .occurrence_gaps
        .contains(&OccurrenceGap::SourceHistory)
        && !scenario
            .cells
            .iter()
            .any(|cell| cell.name == outcome.cell && cell.database.is_some())
    {
        return Err("Historical occurrence gap has no captured source".into());
    }
    let interrupted = *stop != nucleus::simulation::Stop::HorizonReached {};
    if outcome
        .occurrence_gaps
        .contains(&OccurrenceGap::Interrupted)
        != interrupted
        || outcome
            .occurrence_gaps
            .contains(&OccurrenceGap::UnavailableTemplate)
            != outcome
                .occurrences
                .iter()
                .any(|occurrence| occurrence.unavailable)
    {
        return Err("Occurrence completeness disagrees with the observed run".into());
    }
    if outcome.occurrences != expected {
        return Err("Record occurrences do not match immutable schedule evidence".into());
    }
    let mut identities = BTreeSet::new();
    for occurrence in &outcome.occurrences {
        if !identities.insert((&occurrence.rule, occurrence.revision, &occurrence.event)) {
            return Err("Duplicate scheduled application identity".into());
        }
    }
    Ok(())
}
