use std::{fs::OpenOptions, io::Error};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .ok_or_else(|| Error::other("Choose a new JSON report path"))?;
    let report = lince_interface::native::laboratory::run_headless(Default::default())?;
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
