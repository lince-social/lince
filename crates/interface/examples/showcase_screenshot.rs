fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let output = arguments.next().ok_or("Choose a PNG output path")?;
    if arguments.next().is_some() {
        return Err("Use cargo xtask park screenshot".into());
    }
    let runtime = tokio::runtime::Runtime::new()?;
    let _entered = runtime.enter();
    lince_interface::native::laboratory::showoff::screenshot::capture(std::path::Path::new(
        &output,
    ))?;
    println!("Updated {}", std::path::Path::new(&output).display());
    Ok(())
}
