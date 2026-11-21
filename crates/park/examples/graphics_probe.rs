// non-lean-yet
// Scope: fn main, fn main.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let report = lince_park::graphics::probe();
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.failure.is_some() {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Park requires Linux");
    std::process::exit(1);
}
