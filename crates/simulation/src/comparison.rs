use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::scenario::{Event, Scenario};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub with_transfers: String,
    pub without_transfers: String,
    pub proposed: nucleus::simulation::Result,
    pub baseline: nucleus::simulation::Result,
}

pub fn without_transfer_assumptions(scenario: &Scenario) -> Scenario {
    let mut baseline = scenario.clone();
    baseline.name = format!(
        "{}-baseline",
        scenario.name.chars().take(48).collect::<String>()
    );
    baseline
        .inputs
        .retain(|input| !matches!(input.event, Event::AssumeTransfer { .. } | Event::AssumeLoan { .. }));
    baseline
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
    if !scenario
        .inputs
        .iter()
        .any(|input| matches!(input.event, Event::AssumeTransfer { .. } | Event::AssumeLoan { .. }))
    {
        return Err("Choose a Transfer assumption before comparing".into());
    }
    let baseline = without_transfer_assumptions(&scenario);
    std::fs::create_dir(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    }
    let proposed_path = directory.join("with-transfers");
    let baseline_path = directory.join("without-transfers");
    let proposed = crate::artifacts::Session::open(scenario, &proposed_path, sources).await?;
    let baseline = crate::artifacts::Session::open(baseline, &baseline_path, sources).await?;
    Ok((proposed, baseline))
}

pub fn save(
    directory: &Path,
    proposed: crate::artifacts::Run,
    baseline: crate::artifacts::Run,
) -> crate::Result<Comparison> {
    let comparison = Comparison {
        with_transfers: proposed.directory.canonicalize()?.to_string_lossy().into(),
        without_transfers: baseline.directory.canonicalize()?.to_string_lossy().into(),
        proposed: proposed.result,
        baseline: baseline.result,
    };
    crate::artifacts::atomic(&directory.join("comparison.json"), &comparison)?;
    Ok(comparison)
}
