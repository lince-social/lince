// non-lean-yet
// Scope: fn main.
// BeautifulRust (baseline lines):
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 10.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 10.

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
