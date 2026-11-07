use std::{
    ffi::OsString,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{Result, cargo_command, checked};

pub fn run(arguments: &[OsString]) -> Result<()> {
    if arguments.len() == 1 && matches!(arguments[0].to_str(), Some("--help" | "help" | "-h")) {
        println!("cargo xtask park swap-probe [--report PATH]");
        println!(
            "Runs an isolated production Park protocol-server experiment with two synthetic windows and a socket-preserving exec. Writes a JSON result. A failed window-preservation result exits nonzero. Never targets the live desktop."
        );
        return Ok(());
    }
    let report = match arguments {
        [] => PathBuf::from(format!(
            "target/park-validation/swap-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        )),
        [flag, path] if flag == "--report" => PathBuf::from(path),
        _ => return Err("usage: cargo xtask park swap-probe [--report PATH]".into()),
    };
    let report = std::path::absolute(report).map_err(|error| error.to_string())?;
    fs::create_dir_all(report.parent().ok_or("Report needs a parent directory")?)
        .map_err(|error| error.to_string())?;
    fs::write(&report, br#"{"outcome":"running","swap_complete":false}"#)
        .map_err(|error| error.to_string())?;
    eprintln!(
        "Isolated Park exec experiment; report: {}",
        report.display()
    );
    let result = checked(
        cargo_command()
            .args([
                "test",
                "--locked",
                "-p",
                "lince-park",
                "--features",
                "server-tests",
                "--lib",
                "server::tests::swap_tests::socket_preserving_exec_probe",
                "--",
                "--exact",
                "--nocapture",
            ])
            .env("LINCE_PARK_SWAP_REPORT", &report),
    );
    if let Err(error) = result {
        fs::write(&report, serde_json::to_vec_pretty(&serde_json::json!({"outcome":"experiment_error", "swap_complete":false, "error":error})).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
        return Err(error);
    }
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&report).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    if receipt["outcome"] == "unsupported" && receipt["swap_complete"] == false {
        return Err(format!(
            "Window-preserving exec is unsupported: existing windows lose their protocol state. No live swap was attempted. Read {}",
            report.display()
        ));
    }
    Err(format!(
        "The experiment did not establish a supported live upgrade; read {}",
        report.display()
    ))
}
