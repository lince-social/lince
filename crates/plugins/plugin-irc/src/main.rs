use std::{path::PathBuf, sync::Arc};

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
        let executable = format!("lince-plugin-irc{}", std::env::consts::EXE_SUFFIX);
        let package = plugins::install_current(
            &PathBuf::from(&arguments[1]),
            lince_plugin_irc::manifest(executable),
        )?;
        println!(
            "Installed {}. Reload packages in Sync, then review and enable it.",
            package.display()
        );
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("Usage: lince-plugin-irc [--install <Lince-data-directory>]".into());
    }
    let state = PathBuf::from(
        std::env::var_os("LINCE_PLUGIN_STATE").ok_or("Plugin state directory is missing")?,
    );
    let runtime = Arc::new(lince_plugin_irc::Runtime::open(state).await?);
    plugins::serve(move |request| {
        let runtime = runtime.clone();
        async move {
            runtime
                .handle(&request.contribution, &request.operation, request.input)
                .await
        }
    })
    .await
}
