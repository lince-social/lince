#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{ffi::OsStr, path::PathBuf, time::Duration};
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--probe-client")
    {
        let _ = lince_park::runtime::demo_client();
        return Ok(());
    }
    let mut directory = None;
    let mut duration = None;
    let mut applications = Vec::new();
    let mut smoke = false;
    let mut outputs = 1;
    let mut ready_hook = false;
    let mut showoff = false;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].to_str() {
            Some("--ready-hook") => ready_hook = true,
            Some("--showoff") => showoff = true,
            Some("--outputs") => {
                index += 1;
                outputs = arguments
                    .get(index)
                    .and_then(|argument| argument.to_str())
                    .ok_or("--outputs needs a count")?
                    .parse::<usize>()?;
                if !(1..=16).contains(&outputs) {
                    return Err("--outputs must be between one and sixteen".into());
                }
            }
            Some("--directory") => {
                index += 1;
                directory = Some(PathBuf::from(
                    arguments.get(index).ok_or("--directory needs a path")?,
                ));
            }
            Some("--duration") => {
                index += 1;
                let seconds: f64 = arguments
                    .get(index)
                    .and_then(|argument| argument.to_str())
                    .ok_or("--duration needs seconds")?
                    .parse()?;
                duration = Some(Duration::try_from_secs_f64(seconds)?);
            }
            Some("--smoke") => {
                smoke = true;
                duration = Some(Duration::from_secs(30));
            }
            Some("--") => {
                index += 1;
                let mut application = lince_park::launch::Application::new(
                    arguments
                        .get(index)
                        .ok_or("-- needs an executable")?
                        .clone(),
                )?;
                for argument in &arguments[index + 1..] {
                    application = application.arg(argument.clone());
                }
                applications.push(application);
                break;
            }
            Some("--help" | "-h") => {
                println!(
                    "cargo xtask park nested --directory PATH [--duration SECONDS] [--outputs COUNT] [--ready-hook] [--showoff] [-- PROGRAM ARGUMENTS]\ncargo xtask park nested --smoke"
                );
                return Ok(());
            }
            _ => {
                return Err(
                    format!("unknown argument: {}", arguments[index].to_string_lossy()).into(),
                );
            }
        }
        index += 1;
    }
    let temporary = smoke.then(tempfile::tempdir).transpose()?;
    let directory = directory
        .or_else(|| temporary.as_ref().map(|directory| directory.path().into()))
        .ok_or("nested development needs --directory or --smoke")?;
    std::fs::create_dir_all(&directory)?;
    let _owner = lince_interface::activation::claim_headless(&directory)?;
    if smoke {
        eprintln!("Checking Park native output views and GPU export.");
        lince_park::presentation::probe()?;
        eprintln!("Checking Park client state requests and canvas worker.");
        lince_park::runtime::probe()?;
        applications.push(
            lince_park::launch::Application::new(std::env::current_exe()?)?
                .arg(OsStr::new("--probe-client")),
        );
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()?;
    let report = lince_park::nested::run(
        lince_park::runtime::Configuration {
            showoff,
            size: [960, 600],
            directory,
            runtime: runtime.handle().clone(),
            cell: None,
        },
        applications,
        lince_park::nested::Options {
            ready_hook,
            activation: None,
            duration,
            smoke,
            outputs,
        },
    )?;
    if smoke
        && (report.applications == 0 || report.frames == 0 || report.layers != 2 || !report.checked)
    {
        return Err("the nested smoke check did not render an application and both bars".into());
    }
    println!(
        "Park nested: {} presented frames, {} application(s), {} layer surface(s), {:.2}s.",
        report.frames,
        report.applications,
        report.layers,
        report.elapsed.as_secs_f64()
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("Park requires Linux".into())
}
