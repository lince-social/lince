use std::path::{Path, PathBuf};

use nucleus::simulation::{ReplayStatus, Verdict, Version};
use serde::{Deserialize, Serialize};

use crate::{BUILD_HASH, Result, artifacts, fixtures, scenario::Scenario};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub version: Version,
    pub build: String,
    pub next_case: u64,
    pub regressions_checked: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regression {
    pub source: PathBuf,
    pub scenario: nucleus::karma::CanonicalHash,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub version: Version,
    pub index: u64,
    pub scenario: Scenario,
    pub result: nucleus::simulation::Result,
    pub cost: nucleus::simulation::RunCost,
    pub retained: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reduction {
    pub source: PathBuf,
    pub source_result: nucleus::karma::CanonicalHash,
    pub check: nucleus::simulation::Check,
    pub original_inputs: usize,
    pub remaining_inputs: usize,
    pub attempts: u64,
    pub verified: bool,
}

pub fn generate(index: u64) -> Scenario {
    let family = index % 23;
    let replicated = index / 23 % 2 == 0;
    let mut case = match family {
        0 => fixtures::daily(),
        1 => fixtures::network(true),
        2 => fixtures::network(false),
        3 => fixtures::transfer::sale(),
        4 => fixtures::transfer::independent_donation(replicated),
        5 => fixtures::transfer::donation_with_lost_acknowledgements(replicated),
        6 => fixtures::transfer::trade(replicated),
        7 => fixtures::transfer::private_trade(replicated),
        8 => fixtures::transfer::three_parties(),
        9 => fixtures::transfer::visibility(),
        10 => fixtures::transfer::partial_cancellation(replicated),
        11 => fixtures::transfer::nested_parents(replicated),
        12 => fixtures::transfer::observer_outcomes(replicated),
        13 => {
            fixtures::transfer::grouped_needs(replicated, index / 46 % 2 != 0, index / 92 % 2 == 0)
        }
        14 => fixtures::transfer::temporary_loan(replicated, false),
        15 => fixtures::transfer::extended_loan(replicated),
        16 => fixtures::transfer::counteroffer(replicated),
        17 => fixtures::transfer::open_offer(replicated),
        18 => fixtures::transfer::competing_reservations(replicated),
        19 => fixtures::transfer::donation_without_private_source(replicated),
        20 => fixtures::transfer::declined_invitation(replicated),
        21 => fixtures::transfer::private_correction_after_restart(replicated),
        _ => fixtures::transfer::grouped_correction_after_restart(replicated),
    };
    let mut entropy = index.wrapping_add(0x9e3779b97f4a7c15);
    let mut next = || {
        entropy ^= entropy << 13;
        entropy ^= entropy >> 7;
        entropy ^= entropy << 17;
        entropy
    };
    case.seed = next();
    case.name = format!("generated-{index}-{}", case.name);
    for input in &mut case.inputs {
        if let crate::scenario::Event::Sync {
            delay_ms,
            copies,
            duplicate_spacing_ms,
            drop,
            ..
        } = &mut input.event
        {
            *delay_ms = 1 + next() % 15;
            *copies = 1 + (next() % 3) as u8;
            *duplicate_spacing_ms = next() % 40;
            *drop = family < 4 && next() % 4 == 0;
        }
    }
    if family == 0 {
        let delta = (next() % 21) as i64 - 10;
        let id = "generated-change".to_string();
        case.inputs.push(crate::scenario::Input {
            id: id.clone(),
            cell: "a".into(),
            at_ms: case.start_ms + 2 * 86_400_000 + (next() % 1000) as i64,
            event: crate::scenario::Event::Action {
                invocation: crate::scenario::Invocation {
                    id,
                    actor: None,
                    action: engine::actions::Action::AddQuantity {
                        target: "$stock".into(),
                        delta: delta as f64,
                    },
                },
            },
        });
        if let nucleus::simulation::Predicate::QuantityEquals { expected, .. } =
            &mut case.checks[0].predicate
        {
            expected.value =
                nucleus::DecimalValue::parse_inferred(&(delta - 2).to_string()).unwrap();
        }
    }
    if matches!(family, 1 | 2) {
        use crate::scenario::{Event, Input};
        let mut add = |cell: &str, at: i64, event| {
            case.inputs.push(Input {
                id: format!("fault-{}", case.inputs.len()),
                cell: cell.into(),
                at_ms: case.start_ms + at,
                event,
            })
        };
        add("b", 15, Event::Online { online: false });
        add(
            "a",
            100,
            Event::Clock {
                offset_ms: (next() % 201) as i64 - 100,
            },
        );
        add("b", 500, Event::Online { online: true });
        add("a", 500, Event::Clock { offset_ms: 0 });
        let sync = |peer: &str| Event::Sync {
            peer: peer.into(),
            delay_ms: 1,
            copies: 1,
            duplicate_spacing_ms: 0,
            drop: false,
        };
        add("a", 750, sync("b"));
        add("a", 750, sync("c"));
        add("a", 800, sync("d"));
        if family == 1 {
            add("c", 850, sync("d"));
        }
    }
    case
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSelection {
    pub checks: Vec<nucleus::simulation::CheckDefinition>,
    #[serde(default)]
    pub checking: nucleus::simulation::Checking,
}

pub fn directory(output: &Path, selection: Option<&CheckSelection>) -> Result<PathBuf> {
    let base = output
        .join("campaigns")
        .join(BUILD_HASH.trim_start_matches("sha256:"));
    Ok(match selection {
        Some(selection) => base.join(format!(
            "checks-{}",
            crate::world::digest(selection)?
                .as_str()
                .trim_start_matches("sha256:")
        )),
        None => base,
    })
}

pub async fn run(output: &Path, count: Option<u64>) -> Result<u8> {
    run_with_checks(output, count, None).await
}

pub async fn run_with_checks(
    output: &Path,
    count: Option<u64>,
    selection: Option<&CheckSelection>,
) -> Result<u8> {
    let directory = directory(output, selection)?;
    std::fs::create_dir_all(directory.join("summaries"))?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("campaign.lock"))?;
    lock.try_lock()
        .map_err(|_| "this campaign is already running")?;
    if let Some(selection) = selection {
        artifacts::atomic(&directory.join("selection.json"), selection)?;
    }
    let cursor_path = directory.join("cursor.json");
    let mut cursor = if cursor_path.exists() {
        let cursor: Cursor = serde_json::from_slice(&std::fs::read(&cursor_path)?)?;
        if cursor.build != BUILD_HASH {
            return Err("campaign build identity differs".into());
        }
        cursor
    } else {
        Cursor {
            version: Version::V1,
            build: BUILD_HASH.into(),
            next_case: 0,
            regressions_checked: false,
        }
    };
    artifacts::atomic(&cursor_path, &cursor)?;
    if !cursor.regressions_checked {
        if regressions(output, &directory).await? != 0 {
            return Ok(2);
        }
        cursor.regressions_checked = true;
        artifacts::atomic(&cursor_path, &cursor)?;
    }
    let mut signal = std::pin::pin!(tokio::signal::ctrl_c());
    let mut completed = 0;
    let mut status = 0;
    while count.is_none_or(|count| completed < count) {
        if retained_bytes(&directory)? > 2 * 1024 * 1024 * 1024 {
            return Err(
                "campaign retention budget reached; preserved evidence requires review".into(),
            );
        }
        if cursor.next_case >= 100_000 {
            return Err("campaign summary retention limit reached".into());
        }
        let summary_path = directory
            .join("summaries")
            .join(format!("{:08}.json", cursor.next_case));
        if summary_path.exists() {
            let summary: Summary = serde_json::from_slice(&std::fs::read(&summary_path)?)?;
            if summary.index != cursor.next_case {
                return Err("campaign summary identity differs".into());
            }
            cursor.next_case += 1;
            artifacts::atomic(&cursor_path, &cursor)?;
            match summary.result.verdict {
                Verdict::Failed => return Ok(2),
                Verdict::Inconclusive => return Ok(3),
                Verdict::Unverified => status = 3,
                Verdict::Passed => {}
            }
            continue;
        }
        let mut scenario = generate(cursor.next_case);
        if let Some(selection) = selection {
            scenario.checks = selection.checks.clone();
            scenario.checking = selection.checking.clone();
        }
        scenario.validate()?;
        let path = artifacts::next_directory(&directory)?;
        let result = tokio::select! {
            result = artifacts::execute(scenario.clone(), &path) => result?,
            signal = &mut signal => {
                signal?;
                return Ok(3);
            }
        };
        let passed = result.result.verdict == Verdict::Passed;
        if result.result.verdict == Verdict::Failed {
            if selection.is_none() {
                std::fs::create_dir_all(output.join("regressions"))?;
                artifacts::atomic(
                    &output.join("regressions").join(format!(
                        "{}-{}.json",
                        BUILD_HASH.trim_start_matches("sha256:"),
                        cursor.next_case
                    )),
                    &Regression {
                        source: path.canonicalize()?,
                        scenario: crate::world::digest(&scenario)?,
                    },
                )?;
            }
            let repeated = artifacts::next_directory(&directory)?;
            let replay = artifacts::replay(&path, &repeated).await?;
            if matches!(replay, ReplayStatus::Verified { .. })
                && result.result.verdict == Verdict::Failed
            {
                minimize(&path, &path.join("repro"), 32).await?;
            }
        }
        let summary = Summary {
            version: Version::V1,
            index: cursor.next_case,
            scenario,
            result: result.result,
            cost: result.cost,
            retained: (!passed).then(|| path.clone()),
        };
        artifacts::atomic(&summary_path, &summary)?;
        if passed {
            std::fs::remove_dir_all(&path)?;
        }
        cursor.next_case += 1;
        artifacts::atomic(&cursor_path, &cursor)?;
        completed += 1;
        println!("{}", serde_json::to_string(&summary)?);
        match summary.result.verdict {
            Verdict::Failed => return Ok(2),
            Verdict::Inconclusive => return Ok(3),
            Verdict::Unverified => status = 3,
            Verdict::Passed => {}
        }
    }
    Ok(status)
}

pub async fn minimize(source: &Path, destination: &Path, budget: u64) -> Result<Reduction> {
    let bundle =
        artifacts::load(source).map_err(|error| format!("invalid reduction source: {error:?}"))?;
    if bundle.manifest.build.as_str() != BUILD_HASH {
        return Err("reduction requires the tested build".into());
    }
    let finding = bundle
        .findings
        .first()
        .ok_or("reduction needs a witnessed check failure")?;
    let same_failure = |observed: &nucleus::simulation::Finding,
                        original: &nucleus::simulation::Finding| {
        observed.check == original.check
            && std::mem::discriminant(&observed.witness)
                == std::mem::discriminant(&original.witness)
    };
    let reproduces = |result: &artifacts::Run| {
        result.result.stop == bundle.result.stop
            && result
                .findings
                .iter()
                .any(|observed| same_failure(observed, finding))
            && result.findings.iter().all(|observed| {
                bundle
                    .findings
                    .iter()
                    .any(|original| same_failure(observed, original))
            })
    };
    let mut scenario = bundle.scenario.clone();
    let original_inputs = scenario.inputs.len();
    std::fs::create_dir(destination)?;
    let mut attempts = 0;
    let mut index = 0;
    while index < scenario.inputs.len() && attempts < budget {
        let mut candidate = scenario.clone();
        candidate.inputs.remove(index);
        if candidate.validate().is_err() {
            index += 1;
            continue;
        }
        attempts += 1;
        let trial = tempfile::tempdir_in(destination)?;
        let attempt = trial.path().join("run");
        let result = artifacts::execute_with_sources(candidate.clone(), &attempt, source).await;
        let reproduced = result.as_ref().is_ok_and(&reproduces);
        if reproduced
            && matches!(
                artifacts::replay(&attempt, &trial.path().join("replay")).await?,
                ReplayStatus::Verified { .. }
            )
        {
            scenario = candidate;
        } else {
            index += 1;
        }
    }
    let minimal = destination.join("run");
    let final_run = artifacts::execute_with_sources(scenario.clone(), &minimal, source).await?;
    let verified = reproduces(&final_run)
        && matches!(
            artifacts::replay(&minimal, &destination.join("replay")).await?,
            ReplayStatus::Verified { .. }
        );
    let reduction = Reduction {
        source: source.canonicalize()?,
        source_result: artifacts::file_hash(&source.join("result.json"))?,
        check: finding.check.clone(),
        original_inputs,
        remaining_inputs: scenario.inputs.len(),
        attempts,
        verified,
    };
    artifacts::atomic(&destination.join("reduction.json"), &reduction)?;
    Ok(reduction)
}

async fn regressions(output: &Path, directory: &Path) -> Result<u8> {
    let root = output.join("regressions");
    if !root.exists() {
        return Ok(0);
    }
    let mut entries: Vec<_> = std::fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    if entries.len() > 1000 {
        return Err("regression retention budget reached".into());
    }
    for entry in entries {
        if !entry.file_type()?.is_file()
            || entry
                .path()
                .extension()
                .is_none_or(|extension| extension != "json")
        {
            continue;
        }
        let regression: Regression = serde_json::from_slice(&std::fs::read(entry.path())?)?;
        let source = artifacts::load(&regression.source)
            .map_err(|error| format!("regression evidence unavailable: {error:?}"))?;
        if crate::world::digest(&source.scenario)? != regression.scenario {
            return Err("regression scenario hash differs".into());
        }
        let destination = artifacts::next_directory(directory)?;
        let run =
            artifacts::execute_with_sources(source.scenario, &destination, &regression.source)
                .await?;
        artifacts::atomic(&destination.join("regression.json"), &regression)?;
        if run.result.verdict != Verdict::Passed {
            return Ok(2);
        }
        std::fs::remove_dir_all(destination)?;
    }
    Ok(0)
}

fn retained_bytes(directory: &Path) -> Result<u64> {
    let mut bytes = 0u64;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if entry.file_type()?.is_symlink() {
            return Err("campaign retention does not follow symlinks".into());
        }
        bytes = bytes.saturating_add(if metadata.is_dir() {
            retained_bytes(&entry.path())?
        } else {
            metadata.len()
        });
    }
    Ok(bytes)
}
