use std::path::PathBuf;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() == 2 && arguments[0] == "--install" {
        let executable = format!("lince-plugin-activitypub{}", std::env::consts::EXE_SUFFIX);
        let package = plugins::install_current(
            &PathBuf::from(&arguments[1]),
            lince_plugin_activitypub::manifest(executable),
        )?;
        println!(
            "Installed {}. Reload packages in Sync, then review and enable it.",
            package.display()
        );
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("Usage: lince-plugin-activitypub [--install <Lince-data-directory>]".into());
    }
    plugins::serve(|request| async move {
        lince_plugin_activitypub::handle(&request.contribution, &request.operation, request.input)
            .await
    })
    .await
}
