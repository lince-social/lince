// non-lean-yet
// Scope: fn main, fn main.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    lince_park::presentation::probe()?;
    println!(
        "Native canvas rendering, independent output views, shared captures, reconnects, and GPU export verified."
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Park requires Linux");
    std::process::exit(1);
}
