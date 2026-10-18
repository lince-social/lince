#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::thread::Builder::new()
        .name("park-policy-check".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(run)?
        .join()
        .map_err(|_| "Park policy checks panicked")?
        .map_err(Into::into)
}

#[cfg(target_os = "linux")]
fn run() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(32 * 1024 * 1024)
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let cases: Vec<_> = lince_interface::native::laboratory::catalogue()
        .into_iter()
        .filter(|case| {
            [
                "native::laboratory::tests::",
                "native::layout::desktop::tests::",
                "native::shortcuts::tests::",
                "native::information::tests::",
                "native::topology::holes::tests::",
            ]
            .iter()
            .any(|module| case.name.contains(module))
        })
        .collect();
    if cases.is_empty() {
        return Err("No native Park policy checks were registered".into());
    }
    let count = cases.len();
    for case in cases {
        let result = case.execute(&runtime);
        if let Some(error) = result.error {
            return Err(format!("{}: {error}", result.name));
        }
    }
    println!("Park native policy: {count} checks passed.");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Park requires Linux");
    std::process::exit(1);
}
