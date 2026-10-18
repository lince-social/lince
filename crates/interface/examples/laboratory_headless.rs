use std::{ffi::OsStr, fs::OpenOptions, io::Error};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let output = arguments
        .next()
        .ok_or_else(|| Error::other("Choose a new JSON report path"))?;
    let config = match arguments.next().as_deref() {
        None => lince_interface::native::laboratory::StressConfig {
            max_sands: 128,
            batch: 32,
            warmup_frames: 3,
            sample_frames: 8,
            budget_ms: 20.0,
        },
        Some(flag) if flag == OsStr::new("--full-stress") => Default::default(),
        Some(_) => {
            return Err(Error::other("Use a new report path and optionally --full-stress").into());
        }
    };
    if arguments.next().is_some() {
        return Err(Error::other("Unexpected Laboratory argument").into());
    }
    eprintln!(
        "Laboratory CPU probe: maximum {} Sands, {} warmup frames, {} measured frames",
        config.max_sands, config.warmup_frames, config.sample_frames
    );
    let report = lince_interface::native::laboratory::run_headless(config)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    serde_json::to_writer_pretty(file, &report)?;
    if report.behavior.iter().any(|result| result.error.is_some()) {
        return Err(
            Error::other("Laboratory behavior checks failed; inspect the JSON report").into(),
        );
    }
    Ok(())
}
