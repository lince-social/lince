use lince_sandbox::laboratory::{StressConfig, run_headless};
use std::{fs::OpenOptions, io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/map-laboratory.json".into()),
    );
    let report = run_headless(StressConfig {
        max_sands: 32,
        batch: 16,
        warmup_frames: 5,
        sample_frames: 10,
        ..Default::default()
    })?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    output.write_all(&serde_json::to_vec_pretty(&report)?)?;
    let failed = report
        .behavior
        .iter()
        .filter(|result| result.error.is_some())
        .count();
    println!(
        "Laboratory: {} passed, {failed} failed; {} CPU stress measurements; {}",
        report.behavior.len() - failed,
        report.stress.measurements.len(),
        path.display()
    );
    if failed > 0 {
        return Err("Laboratory behavior checks failed".into());
    }
    Ok(())
}
