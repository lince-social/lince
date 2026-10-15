use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    proposal::Outcome,
    scenario::{Event, Scenario},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub cell: String,
    pub record: String,
    pub kind: DifferenceKind,
    pub proposed: Option<Outcome>,
    pub baseline: Option<Outcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferenceKind {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Incomplete {
    pub cell: String,
    pub record: String,
    pub proposed: Vec<EvidenceGap>,
    pub baseline: Vec<EvidenceGap>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceGap {
    EvidenceBudget,
    Unavailable,
    SourceHistory,
    Interrupted,
    UnavailableTemplate,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub sources: BTreeMap<String, crate::artifacts::SourceVersion>,
    pub source_status: BTreeMap<String, crate::artifacts::SourceStatus>,
    pub proposed_run: String,
    pub baseline_run: String,
    pub proposed: nucleus::simulation::Result,
    pub baseline: nucleus::simulation::Result,
    pub proposed_failure: Option<nucleus::simulation::Finding>,
    pub baseline_failure: Option<nucleus::simulation::Finding>,
    pub changes: Vec<Change>,
    pub complete: bool,
    pub incomplete: Vec<Incomplete>,
}

pub fn without_transfer_assumptions(scenario: &Scenario) -> Scenario {
    let mut baseline = scenario.clone();
    baseline.name = format!(
        "{}-baseline",
        scenario.name.chars().take(48).collect::<String>()
    );
    baseline.inputs.retain(|input| {
        !matches!(
            input.event,
            Event::AssumeTransfer { .. } | Event::AssumeLoan { .. }
        )
    });
    baseline
}

pub fn without_proposed_changes(scenario: &Scenario) -> Scenario {
    if let Some(proposal) = &scenario.proposal
        && !proposal.changes.is_empty()
    {
        let mut baseline = scenario.clone();
        baseline.name = format!(
            "{}-baseline",
            scenario.name.chars().take(48).collect::<String>()
        );
        baseline.proposal.as_mut().unwrap().changes.clear();
        baseline
    } else {
        without_transfer_assumptions(scenario)
    }
}

pub async fn run(
    scenario: Scenario,
    directory: &Path,
    sources: &Path,
) -> crate::Result<Comparison> {
    let (mut proposed, mut baseline) = open(scenario, directory, sources).await?;
    while proposed.step().await? {}
    while baseline.step().await? {}
    finish(directory, proposed, baseline).await
}

pub async fn finish(
    directory: &Path,
    proposed: crate::artifacts::Session,
    baseline: crate::artifacts::Session,
) -> crate::Result<Comparison> {
    let selection = crate::record_evidence::Selection::collect(&[&proposed.world, &baseline.world]);
    let proposed = proposed.finish_with_selection(selection.clone()).await?;
    let baseline = baseline.finish_with_selection(selection).await?;
    save(directory, proposed, baseline)
}

pub async fn open(
    scenario: Scenario,
    directory: &Path,
    sources: &Path,
) -> crate::Result<(crate::artifacts::Session, crate::artifacts::Session)> {
    if scenario
        .proposal
        .as_ref()
        .is_none_or(|proposal| proposal.changes.is_empty())
        && !scenario.inputs.iter().any(|input| {
            matches!(
                input.event,
                Event::AssumeTransfer { .. } | Event::AssumeLoan { .. }
            )
        })
    {
        return Err("Add a proposed change or Transfer assumption before comparing".into());
    }
    let baseline = without_proposed_changes(&scenario);
    open_pair(scenario, baseline, directory, sources).await
}

pub async fn open_pair(
    proposed: Scenario,
    baseline: Scenario,
    directory: &Path,
    sources: &Path,
) -> crate::Result<(crate::artifacts::Session, crate::artifacts::Session)> {
    proposed.validate()?;
    baseline.validate()?;
    let context = |scenario: &Scenario| {
        let inputs: Vec<_> = scenario
            .inputs
            .iter()
            .filter(|input| {
                scenario.proposal.is_some()
                    || !matches!(
                        input.event,
                        Event::AssumeTransfer { .. } | Event::AssumeLoan { .. }
                    )
            })
            .collect();
        serde_json::to_value((
            scenario.version,
            scenario.seed,
            scenario.start_ms,
            scenario.end_ms,
            &scenario.cells,
            &scenario.limits,
            &scenario.checks,
            &scenario.checking,
            inputs,
        ))
    };
    if context(&proposed)? != context(&baseline)? {
        return Err(
            "Comparisons require the same base, clock, limits, checks and background events".into(),
        );
    }
    std::fs::create_dir(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    }
    let proposed_path = directory.join("proposed");
    let baseline_path = directory.join("baseline");
    let proposed = crate::artifacts::Session::open(proposed, &proposed_path, sources).await?;
    let baseline = crate::artifacts::Session::open(baseline, &baseline_path, sources).await?;
    Ok((proposed, baseline))
}

pub fn save(
    directory: &Path,
    proposed: crate::artifacts::Run,
    baseline: crate::artifacts::Run,
) -> crate::Result<Comparison> {
    if proposed.sources != baseline.sources {
        return Err("Comparison source versions disagree".into());
    }
    if proposed.selection != baseline.selection {
        return Err("Comparisons require a shared Record evidence selection".into());
    }
    let horizon = proposed.result.stop == (nucleus::simulation::Stop::HorizonReached {})
        && baseline.result.stop == (nucleus::simulation::Stop::HorizonReached {});
    let left: BTreeMap<_, _> = proposed
        .records
        .into_iter()
        .map(|record| ((record.cell.clone(), record.record.clone()), record))
        .collect();
    let right: BTreeMap<_, _> = baseline
        .records
        .into_iter()
        .map(|record| ((record.cell.clone(), record.record.clone()), record))
        .collect();
    let mut changes = Vec::new();
    let mut incomplete = Vec::new();
    for target in &proposed.selection.targets {
        let (cell, record) = (target.cell.clone(), target.record.clone());
        let key = (cell.clone(), record.clone());
        let proposed = left.get(&key);
        let baseline = right.get(&key);
        let proposed_gaps = evidence_gaps(proposed);
        let baseline_gaps = evidence_gaps(baseline);
        if !proposed_gaps.is_empty() || !baseline_gaps.is_empty() {
            incomplete.push(Incomplete {
                cell: cell.clone(),
                record: record.clone(),
                proposed: proposed_gaps,
                baseline: baseline_gaps,
            });
        }
        if let Some(kind) = difference_kind(proposed, baseline, horizon)? {
            changes.push(Change {
                cell,
                record,
                kind,
                proposed: proposed.cloned(),
                baseline: baseline.cloned(),
            });
        }
    }
    let comparison = Comparison {
        sources: proposed.sources,
        source_status: BTreeMap::new(),
        proposed_run: proposed.directory.canonicalize()?.to_string_lossy().into(),
        baseline_run: baseline.directory.canonicalize()?.to_string_lossy().into(),
        proposed: proposed.result,
        baseline: baseline.result,
        proposed_failure: proposed.findings.into_iter().next(),
        baseline_failure: baseline.findings.into_iter().next(),
        changes,
        complete: horizon && !proposed.selection.truncated && incomplete.is_empty(),
        incomplete,
    };
    crate::artifacts::atomic(&directory.join("comparison.json"), &comparison)?;
    Ok(comparison)
}

fn evidence_gaps(outcome: Option<&Outcome>) -> Vec<EvidenceGap> {
    use crate::proposal::{CaptureStatus, OccurrenceGap};
    match outcome {
        None => vec![EvidenceGap::EvidenceBudget],
        Some(outcome) if outcome.status == CaptureStatus::Unavailable => {
            vec![EvidenceGap::Unavailable]
        }
        Some(outcome) => outcome
            .occurrence_gaps
            .iter()
            .map(|gap| match gap {
                OccurrenceGap::SourceHistory => EvidenceGap::SourceHistory,
                OccurrenceGap::Interrupted => EvidenceGap::Interrupted,
                OccurrenceGap::UnavailableTemplate => EvidenceGap::UnavailableTemplate,
            })
            .collect(),
    }
}

fn difference_kind(
    proposed: Option<&Outcome>,
    baseline: Option<&Outcome>,
    horizon: bool,
) -> crate::Result<Option<DifferenceKind>> {
    use crate::proposal::CaptureStatus;
    let (Some(proposed), Some(baseline)) = (proposed, baseline) else {
        return Ok(None);
    };
    if proposed.status != CaptureStatus::Captured || baseline.status != CaptureStatus::Captured {
        return Ok(None);
    }
    match (&proposed.state, &baseline.state) {
        (Some(_), None) => return Ok(horizon.then_some(DifferenceKind::Added)),
        (None, Some(_)) => return Ok(horizon.then_some(DifferenceKind::Removed)),
        _ => {}
    }
    let mut left = proposed.content()?;
    let mut right = baseline.content()?;
    for value in [&mut left, &mut right] {
        let value = value
            .as_object_mut()
            .ok_or("Invalid Record comparison content")?;
        value.remove("status");
        value.remove("occurrence_gaps");
        if !proposed.occurrence_gaps.is_empty() || !baseline.occurrence_gaps.is_empty() {
            value.remove("occurrences");
        }
    }
    Ok((left != right).then_some(DifferenceKind::Changed))
}
