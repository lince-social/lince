// non-lean-yet
// Scope: fn main.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

fn main() -> Result<(), std::io::Error> {
    let directory = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            std::io::Error::other("Pass a separate data directory for the mobile preview")
        })?;
    lince_mobile::run(directory);
    Ok(())
}
