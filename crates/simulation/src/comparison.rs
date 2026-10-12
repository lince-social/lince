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
    pub proposed: Option<Outcome>,
    pub baseline: Option<Outcome>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub proposed_run: String,
    pub baseline_run: String,
    pub proposed: nucleus::simulation::Result,
    pub baseline: nucleus::simulation::Result,
    pub proposed_failure: Option<nucleus::simulation::Finding>,
    pub baseline_failure: Option<nucleus::simulation::Finding>,
    pub changes: Vec<Change>,
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
    let proposed = proposed.finish().await?;
    while baseline.step().await? {}
    save(directory, proposed, baseline.finish().await?)
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
    let keys: std::collections::BTreeSet<_> = left.keys().chain(right.keys()).cloned().collect();
    let mut changes = Vec::new();
    for (cell, record) in keys {
        let key = (cell.clone(), record.clone());
        let proposed = left.get(&key);
        let baseline = right.get(&key);
        if proposed.map(Outcome::content).transpose()?
            != baseline.map(Outcome::content).transpose()?
        {
            changes.push(Change {
                cell,
                record,
                proposed: proposed.cloned(),
                baseline: baseline.cloned(),
            });
        }
    }
    let comparison = Comparison {
        proposed_run: proposed.directory.canonicalize()?.to_string_lossy().into(),
        baseline_run: baseline.directory.canonicalize()?.to_string_lossy().into(),
        proposed: proposed.result,
        baseline: baseline.result,
        proposed_failure: proposed.findings.into_iter().next(),
        baseline_failure: baseline.findings.into_iter().next(),
        changes,
    };
    crate::artifacts::atomic(&directory.join("comparison.json"), &comparison)?;
    Ok(comparison)
}
