// non-lean-yet
// Scope: fn main, fn main.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

#[cfg(target_os = "linux")]
fn main() -> std::io::Result<()> {
    if std::env::args().any(|argument| argument == "--check-external") {
        return lince_park::clipboard::check_external_client();
    }
    if !lince_park::environment::environment_ready() {
        return Err(lince_park::environment::reexec_environment());
    }
    lince_park::clipboard::probe()
}

#[cfg(not(target_os = "linux"))]
fn main() -> std::io::Result<()> {
    Err(std::io::Error::other("Park requires Linux"))
}
