use anyhow::{Context, ensure};
use std::time::{Duration, Instant};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        arguments.len() == 2,
        "Usage: device_check <ESP-EndpointId> <direct|relay>"
    );
    let peer: iroh::EndpointId = arguments[0].parse().context("Invalid ESP EndpointId")?;
    let relay = match arguments[1].as_str() {
        "direct" => false,
        "relay" => true,
        _ => anyhow::bail!("Choose direct or relay qualification"),
    };
    let mut builder = lince_ailuros_esp_iroh::endpoint_builder(iroh::SecretKey::generate());
    if relay {
        builder = builder.clear_ip_transports();
    }
    let endpoint = builder.bind().await?;
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        endpoint.online().await;
        for sequence in 0..3 {
            let started = Instant::now();
            let connection = endpoint.connect(iroh::EndpointAddr::new(peer), b"lince/ailuros-qualification/1").await?;
            ensure!(connection.remote_id() == peer, "Unexpected device identity");
            let deadline = Instant::now() + Duration::from_secs(5);
            let path = loop {
                let selected = connection.paths().iter().find(|path| path.is_selected()).map(|path| path.is_relay());
                if selected == Some(relay) {
                    break if relay { "relay" } else { "direct" };
                }
                ensure!(Instant::now() < deadline, "Requested path did not become selected");
                tokio::time::sleep(Duration::from_millis(50)).await;
            };
            let payload = format!("ailuros-device-qualification-{sequence}");
            let (mut send, mut receive) = connection.open_bi().await?;
            send.write_all(payload.as_bytes()).await?;
            send.finish()?;
            ensure!(receive.read_to_end(256).await? == payload.as_bytes(), "Device response differs from the sent payload");
            println!("{}", serde_json::json!({"type":"ailuros-device-qualification","endpoint":peer.to_string(),"sequence":sequence,"path":path,"elapsed_ms":started.elapsed().as_millis(),"ok":true}));
            connection.close(0u32.into(), b"qualified");
        }
        Ok::<_, anyhow::Error>(())
    }).await;
    endpoint.close().await;
    result.context("Device qualification exceeded its deadline")?
}
