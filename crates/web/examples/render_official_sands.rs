use std::path::PathBuf;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let target = match args.next().as_deref() {
        Some("--target") => args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| "--target requires a directory".to_string())?,
        Some(argument) => return Err(format!("unknown argument: {argument}")),
        None => return Err("--target <directory> is required".to_string()),
    };
    web::sand::render_official_widgets(&target)?;
    println!("Rendered official sands into {}", target.display());
    Ok(())
}
