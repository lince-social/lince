use lince_plugin_nomad::{manifest, settings::Settings, sync};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const USAGE: &str = "Usage: lince-plugin-nomad\n  [no arguments]                         Lince plugin RPC over stdin/stdout\n  --manifest                             Print package manifest\n  --install <Lince-data-directory>        Explicitly install this package\n  --sync <settings.json> <directory>      Refresh connected Lingua Records once\n  --watch <settings.json> <directory> <seconds>\n                                         Refresh until Ctrl-C (minimum 5 seconds)";

fn settings(path: &Path) -> Result<Settings, String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 65536 {
        return Err("Settings file exceeds 64 KiB".into());
    }
    let settings: Settings =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Invalid settings: {e}"))?;
    settings.validate()?;
    Ok(settings)
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let executable = format!("lince-plugin-nomad{}", std::env::consts::EXE_SUFFIX);
    if args.is_empty() {
        return plugins::serve(|request| async move {
            lince_plugin_nomad::handle(&request.contribution, &request.operation, request.input)
                .await
        })
        .await;
    }
    if args.len() == 1 && args[0] == "--help" {
        println!("{USAGE}");
        return Ok(());
    }
    if args.len() == 1 && args[0] == "--manifest" {
        println!(
            "{}",
            serde_json::to_string_pretty(&manifest(executable)).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if args.len() == 2 && args[0] == "--install" {
        println!(
            "{}",
            plugins::install_current(&PathBuf::from(&args[1]), manifest(executable))?.display()
        );
        return Ok(());
    }
    if args.len() == 3 && args[0] == "--sync" {
        let report = sync(settings(Path::new(&args[1]))?, Path::new(&args[2])).await?;
        println!(
            "{}",
            serde_json::to_string(&report).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if args.len() == 4 && args[0] == "--watch" {
        let seconds: u64 = args[3]
            .to_str()
            .and_then(|s| s.parse().ok())
            .filter(|s| *s >= 5)
            .ok_or("Choose a refresh interval of at least 5 seconds")?;
        let interrupted = tokio::signal::ctrl_c();
        tokio::pin!(interrupted);
        loop {
            match settings(Path::new(&args[1])) {
                Ok(config) => match sync(config, Path::new(&args[2])).await {
                    Ok(report) => println!(
                        "{}",
                        serde_json::to_string(&report).map_err(|e| e.to_string())?
                    ),
                    Err(error) => {
                        eprintln!("NOMAD refresh failed; previous export preserved: {error}")
                    }
                },
                Err(error) => eprintln!("NOMAD settings: {error}"),
            }
            tokio::select! {
                result = &mut interrupted => return result.map_err(|e| e.to_string()),
                _ = tokio::time::sleep(Duration::from_secs(seconds)) => {}
            }
        }
    }
    Err(USAGE.into())
}
