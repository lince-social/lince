#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    lince_park::presentation::client_probe()?;
    println!(
        "Native Castles and a live Wayland client: pixels, resize, input mapping, and close verified."
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Park requires Linux");
    std::process::exit(1);
}
