fn main() -> simulation::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("provide an isolated worker data directory")?;
    std::thread::Builder::new()
        .name("lince-simulation-worker".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || simulation::worker::run(std::path::Path::new(&path)))?
        .join()
        .map_err(|_| "simulation worker thread panicked")?
}
