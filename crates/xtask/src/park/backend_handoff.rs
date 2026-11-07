use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{Result, cargo_command, checked, file_sha256};

const CASE: &str = "server::tests::backend_handoff_tests::backend_connection_state_survives_exec";

fn capture(command: &mut Command) -> Result<String> {
    let result = command.output().map_err(|error| error.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "{:?} exited with {}: {}",
            command.get_program(),
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    String::from_utf8(result.stdout).map_err(|error| error.to_string())
}

fn compile(directory: &Path, generation: &str) -> Result<PathBuf> {
    eprintln!("Compiling the private backend proof helper: {generation}");
    let result = cargo_command()
        .args([
            "test",
            "--locked",
            "-p",
            "lince-park",
            "--features",
            "server-tests",
            "--lib",
            "--no-run",
            "--message-format=json",
        ])
        .env("LINCE_REFRESH_PROBE_GENERATION", generation)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| error.to_string())?;
    let mut executables = Vec::new();
    for line in result.stdout.split(|byte| *byte == b'\n') {
        let Ok(message) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if message["reason"] == "compiler-message" {
            if let Some(rendered) = message["message"]["rendered"].as_str() {
                eprint!("{rendered}");
            }
        }
        if message["reason"] == "compiler-artifact"
            && message["target"]["name"] == "lince_park"
            && message["profile"]["test"] == true
        {
            if let Some(executable) = message["executable"].as_str() {
                executables.push(PathBuf::from(executable));
            }
        }
    }
    if !result.status.success() {
        return Err(format!(
            "Backend helper compilation failed: {}",
            result.status
        ));
    }
    if executables.len() != 1 {
        return Err(format!(
            "Expected exactly one Park proof helper executable, found {}",
            executables.len()
        ));
    }
    let frozen = directory.join(generation);
    fs::copy(&executables[0], &frozen).map_err(|error| error.to_string())?;
    Ok(frozen)
}

fn execute(directory: &Path, report: &Path) -> Result<()> {
    let head = capture(Command::new("git").args(["rev-parse", "HEAD"]))?;
    if !capture(Command::new("git").args(["status", "--porcelain", "--untracked-files=normal"]))?
        .trim()
        .is_empty()
    {
        return Err("Publish the source before running the backend handoff proof; this experiment requires a clean identified checkout".into());
    }
    let candidate = compile(directory, "next")?;
    let initial = compile(directory, "initial")?;
    if capture(Command::new("git").args(["rev-parse", "HEAD"]))? != head
        || !capture(Command::new("git").args([
            "status",
            "--porcelain",
            "--untracked-files=normal",
        ]))?
        .trim()
        .is_empty()
    {
        return Err("The source changed while preparing the two proof executables".into());
    }
    let initial_hash = file_sha256(&initial)?;
    let candidate_hash = file_sha256(&candidate)?;
    if initial_hash == candidate_hash {
        return Err("The proof requires distinct compiled executable images".into());
    }
    checked(
        Command::new(&initial)
            .args([CASE, "--exact", "--nocapture", "--test-threads=1"])
            .env("LINCE_BACKEND_EXEC_CANDIDATE", &candidate)
            .env("LINCE_BACKEND_HANDOFF_REPORT", report),
    )?;
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(report).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    if receipt["experiment"] != "backend_connection_exec"
        || receipt["outcome"] != "backend_handoff_verified"
        || receipt["backend_connections_preserved"] != true
        || receipt["distinct_builds"] != true
        || receipt["swap_complete"] != false
        || receipt["initial_sha256"].as_str() != Some(initial_hash.as_str())
        || receipt["candidate_sha256"].as_str() != Some(candidate_hash.as_str())
        || receipt["before"]["generation"] != "initial"
        || receipt["after"]["generation"] != "next"
    {
        return Err(
            "The test did not establish the requested distinct-build backend handoff".into(),
        );
    }
    receipt["source_revision"] = head.trim().into();
    receipt["initial_executable"] = initial.to_string_lossy().as_ref().into();
    receipt["candidate_executable"] = candidate.to_string_lossy().as_ref().into();
    fs::write(
        report,
        serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    eprintln!(
        "Backend connection handoff verified; full Park refresh is still unimplemented. Receipt: {}",
        report.display()
    );
    Ok(())
}

pub fn run(arguments: &[OsString]) -> Result<()> {
    if arguments.len() == 1 && matches!(arguments[0].to_str(), Some("help" | "--help" | "-h")) {
        println!("cargo xtask park backend-handoff-probe [--report PATH]");
        println!(
            "Compiles two distinct private protocol helpers and verifies connection-state restoration across exec. No live session is targeted. This backend proof does not certify production Park, native UI, GPU or complete session refresh."
        );
        return Ok(());
    }
    let requested = match arguments {
        [] => None,
        [flag, path] if flag == "--report" => Some(PathBuf::from(path)),
        _ => return Err("usage: cargo xtask park backend-handoff-probe [--report PATH]".into()),
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let directory = std::path::absolute(format!(
        "target/park-validation/backend-handoff-{}-{stamp}",
        std::process::id()
    ))
    .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let report = std::path::absolute(requested.unwrap_or_else(|| directory.join("receipt.json")))
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(report.parent().ok_or("Missing report parent")?)
        .map_err(|error| error.to_string())?;
    fs::write(&report, br#"{"outcome":"preparing","swap_complete":false}"#)
        .map_err(|error| error.to_string())?;
    let result = execute(&directory, &report);
    if let Err(error) = &result {
        fs::write(&report, serde_json::to_vec_pretty(&serde_json::json!({"outcome":"experiment_error", "swap_complete":false, "error":error})).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
    }
    result
}
