#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    lince_park::graphics::synchronization_probe()?;
    println!("GPU completion and retained-resource synchronization passed.");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {}
