use std::path::PathBuf;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let target = match args.next().as_deref() {
        Some("--target") => args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "--target requires a directory".to_string())?,
        Some(argument) => return Err(format!("unknown argument: {argument}")),
        None => dirs::config_dir()
            .map(|directory| directory.join("lince/web/sand"))
            .ok_or_else(|| "unable to resolve the Lince config directory".to_string())?,
    };
    web::sand::lynx_ui::render(&target)?;
    println!("Rendered LynxUI Gallery into {}", target.display());
    Ok(())
}
