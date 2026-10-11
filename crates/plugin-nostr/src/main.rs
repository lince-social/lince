use nucleus::plugins::{PROTOCOL, RpcRequest, RpcResponse};
use std::path::PathBuf;
use tokio::io::{AsyncWriteExt, BufReader};

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
        let package = lince_plugin_nostr::install(&PathBuf::from(&arguments[1]))?;
        println!(
            "Installed {}. Restart Lince and enable the package in Sync.",
            package.display()
        );
        return Ok(());
    }
    if !arguments.is_empty() {
        return Err("Usage: lince-plugin-nostr [--install <Lince-data-directory>]".into());
    }
    let state = PathBuf::from(
        std::env::var_os("LINCE_PLUGIN_STATE").ok_or("Plugin state directory is missing")?,
    );
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    loop {
        let frame = match plugins::read_frame(&mut reader).await {
            Ok(frame) => frame,
            Err(error) if error == "Plugin connection closed" => return Ok(()),
            Err(error) => return Err(error),
        };
        let request: RpcRequest =
            serde_json::from_slice(&frame).map_err(|error| error.to_string())?;
        let result = if request.protocol == PROTOCOL {
            lince_plugin_nostr::handle(
                &request.contribution,
                &request.operation,
                request.input,
                &state,
            )
            .await
        } else {
            Err("Unsupported protocol".into())
        };
        let response = match result {
            Ok(result) => RpcResponse {
                protocol: PROTOCOL.into(),
                id: request.id,
                result: Some(result),
                error: None,
            },
            Err(error) => RpcResponse {
                protocol: PROTOCOL.into(),
                id: request.id,
                result: None,
                error: Some(error),
            },
        };
        let mut bytes = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        output
            .write_all(&bytes)
            .await
            .map_err(|error| error.to_string())?;
        output.flush().await.map_err(|error| error.to_string())?;
    }
}
