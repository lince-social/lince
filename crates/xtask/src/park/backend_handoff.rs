use std::{
    ffi::OsString,
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{Result, cargo_command, checked, file_sha256};

fn write_receipt(path: &Path, bytes: &[u8]) -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temporary = path.with_extension(format!("tmp-{}-{stamp}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn safe_report(path: &Path) -> Result<PathBuf> {
    let root =
        PathBuf::from(capture(Command::new("git").args(["rev-parse", "--show-toplevel"]))?.trim());
    let common = PathBuf::from(
        capture(Command::new("git").args([
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ]))?
        .trim(),
    );
    let parent = path.parent().ok_or("Missing report parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let path = fs::canonicalize(parent)
        .map_err(|error| error.to_string())?
        .join(path.file_name().ok_or("Missing report file name")?);
    if !path.starts_with(common)
        && let Ok(relative) = path.strip_prefix(&root)
    {
        let ignored = Command::new("git")
            .current_dir(&root)
            .args(["check-ignore", "--quiet", "--"])
            .arg(relative)
            .status()
            .map_err(|error| error.to_string())?;
        if !ignored.success() {
            return Err("Write the proof receipt outside the source checkout or inside an ignored output directory".into());
        }
    }
    Ok(path)
}

#[derive(Clone, Copy)]
enum Probe {
    Backend,
    ProductionProtocol,
}

impl Probe {
    fn command(self) -> &'static str {
        match self {
            Self::Backend => "backend-handoff-probe",
            Self::ProductionProtocol => "protocol-handoff-probe",
        }
    }
    fn case(self) -> &'static str {
        match self {
            Self::Backend => {
                "server::tests::backend_handoff_tests::backend_connection_state_survives_exec"
            }
            Self::ProductionProtocol => {
                "server::tests::protocol_handoff_tests::exec::production_protocol_state_survives_exec"
            }
        }
    }
    fn stamp(self) -> &'static str {
        match self {
            Self::Backend => "LINCE_REFRESH_PROBE_GENERATION",
            Self::ProductionProtocol => "LINCE_PROTOCOL_HANDOFF_GENERATION",
        }
    }
    fn candidate(self) -> &'static str {
        match self {
            Self::Backend => "LINCE_BACKEND_EXEC_CANDIDATE",
            Self::ProductionProtocol => "LINCE_PROTOCOL_HANDOFF_CANDIDATE_BINARY",
        }
    }
    fn report(self) -> &'static str {
        match self {
            Self::Backend => "LINCE_BACKEND_HANDOFF_REPORT",
            Self::ProductionProtocol => "LINCE_PROTOCOL_HANDOFF_REPORT",
        }
    }
}

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

fn compile(directory: &Path, generation: &str, probe: Probe) -> Result<PathBuf> {
    eprintln!("Compiling the private handoff proof helper: {generation}");
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
        .env_remove("LINCE_REFRESH_PROBE_GENERATION")
        .env_remove("LINCE_PROTOCOL_HANDOFF_GENERATION")
        .env(probe.stamp(), generation)
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
            "Handoff helper compilation failed: {}",
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

fn execute(directory: &Path, report: &Path, probe: Probe) -> Result<()> {
    let head = capture(Command::new("git").args(["rev-parse", "HEAD"]))?;
    if !capture(Command::new("git").args(["status", "--porcelain", "--untracked-files=normal"]))?
        .trim()
        .is_empty()
    {
        return Err("Commit and freeze the source locally before running the handoff proof; no push is required for this experiment".into());
    }
    let candidate = compile(directory, "next", probe)?;
    let initial = compile(directory, "initial", probe)?;
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
            .args([probe.case(), "--exact", "--nocapture", "--test-threads=1"])
            .env(probe.candidate(), &candidate)
            .env(probe.report(), report)
            .env("LINCE_PROTOCOL_HANDOFF_SOURCE_REVISION", head.trim()),
    )?;
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(report).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let established = match probe {
        Probe::Backend => {
            receipt["experiment"] == "backend_connection_exec"
                && receipt["outcome"] == "backend_handoff_verified"
                && receipt["backend_connections_preserved"] == true
                && receipt["before"]["generation"] == "initial"
                && receipt["after"]["generation"] == "next"
        }
        Probe::ProductionProtocol => {
            receipt["experiment"] == "production_park_protocol_exec"
                && receipt["outcome"] == "protocol_handoff_verified"
                && receipt["mapped_windows_preserved"] == true
                && receipt["buffer_pixels_preserved"] == true
                && receipt["pending_configure_acknowledged"] == true
                && receipt["new_and_destroyed_window"] == true
                && receipt["before"]["revision_step"] == 1
                && receipt["after"]["revision_step"] == 10
        }
    };
    if !established
        || receipt["distinct_builds"] != true
        || receipt["swap_complete"] != false
        || receipt["initial_sha256"].as_str() != Some(initial_hash.as_str())
        || receipt["candidate_sha256"].as_str() != Some(candidate_hash.as_str())
    {
        return Err("The test did not establish the requested distinct-build handoff scope".into());
    }
    receipt["source_revision"] = head.trim().into();
    receipt["initial_executable"] = initial.to_string_lossy().as_ref().into();
    receipt["candidate_executable"] = candidate.to_string_lossy().as_ref().into();
    write_receipt(
        report,
        &serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
    )?;
    eprintln!(
        "The requested handoff proof scope passed; full Park refresh is still unimplemented. Receipt: {}",
        report.display()
    );
    Ok(())
}

pub fn run(arguments: &[OsString]) -> Result<()> {
    run_for(Probe::Backend, arguments)
}

pub fn run_protocol(arguments: &[OsString]) -> Result<()> {
    run_for(Probe::ProductionProtocol, arguments)
}

fn run_for(probe: Probe, arguments: &[OsString]) -> Result<()> {
    if arguments.len() == 1 && matches!(arguments[0].to_str(), Some("help" | "--help" | "-h")) {
        println!("cargo xtask park {} [--report PATH]", probe.command());
        println!(
            "Compiles two distinct private helpers and verifies the selected protocol restoration scope across exec. No live session is targeted. Neither proof certifies native UI, GPU, other protocol families or complete session refresh."
        );
        return Ok(());
    }
    let requested = match arguments {
        [] => None,
        [flag, path] if flag == "--report" => Some(PathBuf::from(path)),
        _ => {
            return Err(format!(
                "usage: cargo xtask park {} [--report PATH]",
                probe.command()
            ));
        }
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let directory = std::path::absolute(format!(
        "target/park-validation/{}-{}-{stamp}",
        probe.command(),
        std::process::id()
    ))
    .map_err(|error| error.to_string())?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .map_err(|error| error.to_string())?;
    let report = std::path::absolute(requested.unwrap_or_else(|| directory.join("receipt.json")))
        .map_err(|error| error.to_string())?;
    let report = safe_report(&report)?;
    write_receipt(&report, br#"{"outcome":"preparing","swap_complete":false}"#)?;
    let result = execute(&directory, &report, probe);
    if let Err(error) = &result {
        write_receipt(&report, &serde_json::to_vec_pretty(&serde_json::json!({"outcome":"experiment_error", "swap_complete":false, "error":error})).map_err(|error| error.to_string())?)?;
    }
    result
}
