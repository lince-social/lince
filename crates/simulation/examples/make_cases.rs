fn main() -> simulation::Result<()> {
    let directory = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("provide an output directory")?,
    );
    std::fs::create_dir_all(&directory)?;
    for case in [
        simulation::fixtures::daily(),
        simulation::fixtures::network(false),
        simulation::fixtures::network(true),
        simulation::fixtures::transfer::sale(),
    ] {
        let path = directory.join(format!("{}.json", case.name));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        serde_json::to_writer_pretty(file, &case)?;
    }
    Ok(())
}
