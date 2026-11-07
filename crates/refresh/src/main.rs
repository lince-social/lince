use std::{env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lince-refresh: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "linux")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let command = arguments
        .next()
        .ok_or("usage: lince-refresh <serve|status|shutdown> --runtime ABSOLUTE_DIRECTORY")?;
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--runtime")) {
        return Err("expected --runtime ABSOLUTE_DIRECTORY".into());
    }
    let runtime = PathBuf::from(arguments.next().ok_or("missing runtime directory")?);
    if arguments.next().is_some() {
        return Err("unexpected argument".into());
    }
    match command.to_str() {
        Some("serve") => lince_refresh::coordinator::serve(&runtime)?,
        Some("status") => println!(
            "{}",
            serde_json::to_string(&lince_refresh::coordinator::request(&runtime, false)?)?
        ),
        Some("shutdown") => println!(
            "{}",
            serde_json::to_string(&lince_refresh::coordinator::request(&runtime, true)?)?
        ),
        _ => return Err("unknown refresh command".into()),
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let _ = (env::args_os(), PathBuf::new());
    Err("Park refresh requires Linux".into())
}
