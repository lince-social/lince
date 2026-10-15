use std::path::{Path, PathBuf};

use nucleus::simulation::{ReplayStatus, Verdict};

use crate::{Result, artifacts};

pub async fn run(arguments: &[String]) -> Result<u8> {
    let start = arguments
        .iter()
        .position(|argument| argument == "--simulation")
        .ok_or("--simulation is required")?;
    let mut cases = None;
    let mut replay = None;
    let mut selected_checks = None;
    let mut count = None;
    let mut output = PathBuf::from("simulation-runs");
    let mut cursor = start + 1;
    while cursor < arguments.len() {
        match arguments[cursor].as_str() {
            "--cases" => {
                cursor += 1;
                count = Some(
                    arguments
                        .get(cursor)
                        .ok_or("--cases needs a positive count")?
                        .parse::<u64>()?,
                );
                if count == Some(0) {
                    return Err("--cases must be positive".into());
                }
            }
            "--replay" | "--simulation-output" | "--checks" => {
                let option = &arguments[cursor];
                cursor += 1;
                let value = arguments
                    .get(cursor)
                    .ok_or_else(|| format!("{option} needs a path"))?;
                if value.starts_with('-') {
                    return Err(format!("{option} needs a path").into());
                }
                if option == "--replay" {
                    replay = Some(PathBuf::from(value));
                } else if option == "--checks" {
                    selected_checks = Some(crate::limits::read_json::<
                        crate::campaign::CheckSelection,
                    >(
                        Path::new(value), crate::limits::CAPTURE_BYTES
                    )?);
                } else {
                    output = value.into();
                }
            }
            value if !value.starts_with('-') && cases.is_none() => {
                cases = Some(PathBuf::from(value))
            }
            value => return Err(format!("unknown simulation argument {value}").into()),
        }
        cursor += 1;
    }
    if selected_checks.is_some() && (replay.is_some() || cases.is_some()) {
        return Err(
            "--checks selects campaign checks; saved cases and replays keep their own definitions"
                .into(),
        );
    }
    if let Some(replay) = replay {
        if cases.is_some() {
            return Err("select cases or replay, not both".into());
        }
        let destination = artifacts::next_directory(&output)?;
        let status = artifacts::replay(&replay, &destination).await?;
        println!("{}", serde_json::to_string(&status)?);
        return Ok(match status {
            ReplayStatus::Verified { .. } => 0,
            ReplayStatus::Diverged { .. } => 2,
            _ => 3,
        });
    }
    match cases {
        Some(cases) => fixed_cases(&cases, &output).await,
        None => crate::campaign::run_with_checks(&output, count, selected_checks.as_ref()).await,
    }
}

pub async fn fixed_cases(cases: &Path, output: &Path) -> Result<u8> {
    let runs = case_results(cases, output).await?;
    let mut status = 0;
    for run in runs {
        println!("{}", serde_json::to_string(&run)?);
        status = status.max(match run.result.verdict {
            Verdict::Passed => 0,
            Verdict::Failed => 2,
            Verdict::Inconclusive | Verdict::Unverified => 3,
        });
    }
    Ok(status)
}

#[derive(serde::Serialize)]
pub struct CaseResult {
    pub run: PathBuf,
    pub result: nucleus::simulation::Result,
    pub cost: nucleus::simulation::RunCost,
}

pub async fn case_results(cases: &Path, output: &Path) -> Result<Vec<CaseResult>> {
    let mut files = Vec::new();
    let mut bytes = 0u64;
    for entry in std::fs::read_dir(cases)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
        {
            bytes = bytes
                .saturating_add(entry.metadata()?.len())
                .saturating_add(entry.path().as_os_str().len() as u64)
                .saturating_add(std::mem::size_of::<crate::scenario::Scenario>() as u64);
            if bytes > crate::limits::LINES_BYTES {
                return Err(Box::new(crate::limits::Exceeded {
                    resource: "CLI case batch",
                    limit: crate::limits::LINES_BYTES,
                }));
            }
            files.push(entry.path());
        }
    }
    files.sort();
    if files.is_empty() {
        return Err("the explicit scenario directory contains no JSON cases".into());
    }
    let mut parsed = Vec::new();
    for file in files {
        let scenario: crate::scenario::Scenario =
            crate::limits::read_json(&file, crate::limits::JSON_BYTES)
                .map_err(|error| format!("{}: {error}", file.display()))?;
        scenario.validate()?;
        parsed.push(scenario);
    }
    let mut results = Vec::new();
    for scenario in parsed {
        let directory = artifacts::next_directory(output)?;
        let run = artifacts::execute_with_sources(scenario, &directory, cases).await?;
        results.push(CaseResult {
            run: directory,
            result: run.result,
            cost: run.cost,
        });
    }
    Ok(results)
}
