use engine::wire::{PeerId, Wire};
use iroh::endpoint::TransportAddrUsage;
use std::{io, path::PathBuf, sync::Arc, time::Duration};

const USAGE: &str = "Usage: ailuros_camera_check <cell-directory> <signal-uid> [https-origin | --direct <device-endpoint-id>] [--frames 300..10000]; omit https-origin for the Cell's iroh receiver";

fn verification_timeout(samples: usize) -> Duration {
    Duration::from_secs(180 + samples.saturating_sub(300) as u64 / 4)
}

async fn direct_paths(wire: &Wire, peer: PeerId) -> (usize, usize) {
    let Some(info) = wire.endpoint().remote_info(peer).await else {
        return (0, 0);
    };
    info.addrs()
        .filter(|address| matches!(address.usage(), TransportAddrUsage::Active))
        .fold((0, 0), |(direct, other), address| {
            if address.addr().is_ip() {
                (direct + 1, other)
            } else {
                (direct, other + 1)
            }
        })
}

async fn prepare_direct(wire: &Wire, peer: PeerId) -> Result<(), io::Error> {
    let ready = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let (direct, other) = direct_paths(wire, peer).await;
            if direct > 0 && other == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await;
    if ready.is_err() {
        eprintln!(
            "{}",
            serde_json::json!({
                "type":"direct-path-failure",
                "local_addresses":wire.endpoint().addr().ip_addrs().cloned().collect::<Vec<_>>(),
                "remote":format!("{:?}",wire.endpoint().remote_info(peer).await),
            })
        );
        return Err(io::Error::other(
            "No exclusive direct IP path to the camera",
        ));
    }
    eprintln!("Camera check: direct IP path active; receiver has no relay transports");
    Ok(())
}

async fn frames(
    subscription: &mut engine::signal_streams::Subscription,
    count: usize,
    phase: &str,
    direct: Option<(&Wire, PeerId)>,
) -> Result<serde_json::Value, io::Error> {
    let started = std::time::Instant::now();
    let mut first_received = None;
    let mut last_received = None;
    let mut received = 0;
    let mut bytes = 0;
    let mut maximum_age_ms = 0;
    let mut skipped_frames = 0_u64;
    let mut epoch_changes = 0_u64;
    let mut maximum_observer_delay_ms = 0;
    let mut maximum_decode_ms = 0;
    let mut dimensions = (0, 0);
    let mut statuses = std::collections::HashSet::new();
    eprintln!("Camera check: waiting for {count} frames ({phase})");
    let mut previous = subscription
        .updates
        .borrow_and_update()
        .value
        .as_ref()
        .map(|value| (value.metadata.epoch.clone(), value.metadata.sequence));
    while received < count {
        tokio::time::timeout(Duration::from_secs(90), subscription.updates.changed())
            .await
            .map_err(|_| io::Error::other(format!("{phase}: no camera frame within 90 seconds")))?
            .map_err(io::Error::other)?;
        let snapshot = subscription.updates.borrow_and_update().clone();
        if statuses.insert(snapshot.status.clone()) {
            eprintln!("Camera check: {phase}: {}", snapshot.status);
        }
        if let Some(value) = snapshot.value {
            let key = (value.metadata.epoch.clone(), value.metadata.sequence);
            if previous.as_ref() == Some(&key) {
                continue;
            }
            if let Some((epoch, sequence)) = &previous {
                if epoch == &key.0 {
                    if key.1 <= *sequence {
                        return Err(io::Error::other("Camera sequence moved backwards"));
                    }
                    skipped_frames += key.1 - sequence - 1;
                } else {
                    epoch_changes += 1;
                }
            }
            previous = Some(key);
            if let Some((wire, peer)) = direct {
                let (ip, other) = direct_paths(wire, peer).await;
                if ip == 0 || other != 0 {
                    return Err(io::Error::other(
                        "Direct camera qualification lost its exclusive IP path",
                    ));
                }
            }
            first_received.get_or_insert(value.received);
            last_received = Some(value.received);
            let observed = std::time::Instant::now();
            let observer_delay_ms = observed
                .saturating_duration_since(value.received)
                .as_millis();
            let (width, height, _) =
                engine::signal_streams::decode_jpeg(&value.bytes).map_err(io::Error::other)?;
            dimensions = (width, height);
            bytes += value.bytes.len();
            let decode_ms = observed.elapsed().as_millis();
            let age_ms = chrono::Utc::now().timestamp_millis() - value.metadata.captured_at;
            maximum_age_ms = maximum_age_ms.max(age_ms);
            maximum_observer_delay_ms = maximum_observer_delay_ms.max(observer_delay_ms);
            maximum_decode_ms = maximum_decode_ms.max(decode_ms);
            if !(-1000..=5000).contains(&age_ms) {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "type":"ailuros-camera-failure", "phase":phase, "frames":received,
                        "age_ms":age_ms, "observer_delay_ms":observer_delay_ms, "decode_ms":decode_ms,
                        "sequence":value.metadata.sequence, "skipped_frames":skipped_frames,
                        "epoch_changes":epoch_changes,
                    })
                );
                return Err(io::Error::other(format!(
                    "{phase}: camera frame exceeds the capture-age bounds"
                )));
            }
            received += 1;
            if received % 50 == 0 {
                eprintln!("{}", serde_json::json!({"phase":phase,"frames":received}));
            }
        }
    }
    let first_received =
        first_received.ok_or_else(|| io::Error::other("No camera frames received"))?;
    let stream_seconds = last_received
        .ok_or_else(|| io::Error::other("No camera frames received"))?
        .saturating_duration_since(first_received)
        .as_secs_f64();
    let result = serde_json::json!({
        "frames": received,
        "width": dimensions.0,
        "height": dimensions.1,
        "bytes": bytes,
        "seconds": started.elapsed().as_secs_f64(),
        "startup_ms": first_received.saturating_duration_since(started).as_millis(),
        "stream_seconds": stream_seconds,
        "fps": received.saturating_sub(1) as f64 / stream_seconds,
        "maximum_age_ms": maximum_age_ms,
        "skipped_frames": skipped_frames,
        "epoch_changes": epoch_changes,
        "maximum_observer_delay_ms": maximum_observer_delay_ms,
        "maximum_decode_ms": maximum_decode_ms,
        "direct_checked_frames": if direct.is_some() { received } else { 0 },
    });
    eprintln!("{}", serde_json::json!({"phase":phase,"result":&result}));
    Ok(result)
}

async fn verify(
    engine: &engine::Engine,
    signal: &str,
    direct: Option<(&Wire, PeerId)>,
    samples: usize,
) -> Result<(), io::Error> {
    let channel = engine
        .stream_channel(signal, None)
        .await
        .map_err(io::Error::other)?;
    let mut first = channel.subscribe().map_err(io::Error::other)?;
    let mut second = channel.subscribe().map_err(io::Error::other)?;
    let path_setup_started = std::time::Instant::now();
    if let Some((wire, peer)) = direct {
        prepare_direct(wire, peer).await?;
    }
    let path_setup_ms = path_setup_started.elapsed().as_millis();
    let initial = frames(&mut first, samples, "two viewers", direct).await?;
    if channel.snapshot().consumers != 2 {
        return Err(io::Error::other(
            "Both camera viewers must remain subscribed",
        ));
    }
    let epoch = channel.snapshot().epoch;
    drop(first);
    if channel.snapshot().consumers != 1 {
        return Err(io::Error::other(
            "Closing one viewer interrupted the other viewer",
        ));
    }
    let remaining_viewer = frames(&mut second, 10, "one viewer", direct).await?;
    drop(second);
    let idle = channel.snapshot();
    if idle.consumers != 0 || idle.value.is_some() || idle.epoch == epoch {
        return Err(io::Error::other(
            "Closing the final viewer did not release the current frame",
        ));
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    let mut reopened = channel.subscribe().map_err(io::Error::other)?;
    let resumed = frames(&mut reopened, 10, "resumed viewer", direct).await?;
    let performance_ok = initial["fps"].as_f64().is_some_and(|fps| fps >= 4.5);
    let stability_ok = initial["epoch_changes"] == 0;
    println!(
        "{}",
        serde_json::json!({
            "type": "ailuros-camera-check",
            "signal": signal,
            "initial": initial,
            "remaining_viewer": remaining_viewer,
            "resumed": resumed,
            "final_viewer_idle": true,
            "performance_ok": performance_ok,
            "stability_ok": stability_ok,
            "direct_only": direct.is_some(),
            "path_setup_ms": path_setup_ms,
            "requested_frames": samples,
        })
    );
    if !performance_ok {
        return Err(io::Error::other(
            "Camera throughput is below the 4.5 FPS minimum",
        ));
    }
    if !stability_ok {
        return Err(io::Error::other(
            "Camera publication restarted during the sustained stream",
        ));
    }
    Ok(())
}

async fn warm_direct_path(
    wire: &Arc<Wire>,
    engine: &engine::Engine,
    signal: &str,
    peer: PeerId,
    local_ip: std::net::IpAddr,
) -> Result<u16, io::Error> {
    let serving = wire.clone();
    let serving = tokio::spawn(async move { serving.serve().await });
    let result = async {
        let bound = wire
            .endpoint()
            .bound_sockets()
            .into_iter()
            .find(|address| address.is_ipv4())
            .ok_or_else(|| io::Error::other("Direct qualification requires an IPv4 socket"))?;
        wire.endpoint()
            .add_external_addr(std::net::SocketAddr::new(local_ip, bound.port()))
            .await;
        let channel = engine
            .stream_channel(signal, None)
            .await
            .map_err(io::Error::other)?;
        let _viewer = channel.subscribe().map_err(io::Error::other)?;
        tokio::time::timeout(Duration::from_secs(90), async {
            loop {
                if channel.snapshot().value.is_some() && direct_paths(wire, peer).await.0 > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .map_err(|_| {
            io::Error::other(
                "Camera did not establish an authenticated direct route during rendezvous",
            )
        })?;
        eprintln!(
            "Camera check: authenticated direct route found; warming before disabling relays"
        );
        tokio::time::sleep(Duration::from_secs(30)).await;
        Ok(bound.port())
    }
    .await;
    serving.abort();
    let _ = serving.await;
    result
}

async fn verify_direct(
    directory: &str,
    signal: &str,
    peer: PeerId,
    samples: usize,
) -> Result<(), io::Error> {
    let directory = PathBuf::from(directory);
    if !directory.join("lince.db").is_file() {
        return Err(io::Error::other(
            "Direct qualification requires an existing Cell database",
        ));
    }
    utils::config::set_lince_data_dir_override(directory.clone())?;
    let secret: [u8; 32] = std::fs::read(directory.join("keys/node-ed25519-v1.key"))?
        .try_into()
        .map_err(|_| io::Error::other("Existing Cell key must contain exactly 32 bytes"))?;
    let store = store::Store::open(&format!(
        "sqlite://{}",
        directory.join("lince.db").display()
    ))
    .await
    .map_err(io::Error::other)?;
    let engine = Arc::new(
        engine::Engine::new(store.clone())
            .await
            .map_err(io::Error::other)?,
    );
    let route = std::net::UdpSocket::bind("0.0.0.0:0")?;
    route.connect("192.0.2.1:9")?;
    let local_ip = route.local_addr()?.ip();
    let open = |builder| {
        Wire::bind_with_endpoint_builder(
            engine.clone(),
            iroh::SecretKey::from_bytes(&secret),
            engine::wire::Reach::Internet,
            builder,
        )
    };
    let result = async {
        let bootstrap = Arc::new(
            open(iroh::Endpoint::builder(iroh::endpoint::presets::N0))
                .await
                .map_err(io::Error::other)?,
        );
        let warmed = warm_direct_path(&bootstrap, &engine, signal, peer, local_ip).await;
        bootstrap.shutdown().await;
        drop(bootstrap);
        let port = warmed?;
        let builder = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
            .clear_relay_transports()
            .clear_ip_transports()
            .bind_addr(std::net::SocketAddr::new(
                std::net::Ipv4Addr::UNSPECIFIED.into(),
                port,
            ))
            .map_err(io::Error::other)?;
        let wire = Arc::new(open(builder).await.map_err(io::Error::other)?);
        wire.endpoint()
            .add_external_addr(std::net::SocketAddr::new(local_ip, port))
            .await;
        let serving = wire.clone();
        let serving = tokio::spawn(async move { serving.serve().await });
        let result = tokio::time::timeout(
            verification_timeout(samples),
            verify(&engine, signal, Some((&wire, peer)), samples),
        )
        .await
        .map_err(|_| io::Error::other("Direct camera verification exceeded its deadline"))
        .and_then(|result| result);
        wire.shutdown().await;
        serving.abort();
        let _ = serving.await;
        result
    }
    .await;
    store.pool.close().await;
    result
}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    let mut arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.as_slice() == ["--help"] {
        println!("{USAGE}");
        return Ok(());
    }
    let samples = if let Some(index) = arguments.iter().position(|argument| argument == "--frames")
    {
        let count = arguments
            .get(index + 1)
            .ok_or_else(|| io::Error::other(USAGE))?
            .parse::<usize>()
            .map_err(|_| io::Error::other("Choose 300..10000 qualification frames"))?;
        if !(300..=10000).contains(&count) {
            return Err(io::Error::other("Choose 300..10000 qualification frames"));
        }
        arguments.drain(index..=index + 1);
        count
    } else {
        300
    };
    if let [directory, signal, mode, peer] = arguments.as_slice()
        && mode == "--direct"
    {
        let peer = peer.parse::<PeerId>().map_err(io::Error::other)?;
        let _instance =
            lince_interface::activation::claim_headless(std::path::Path::new(directory))?;
        return verify_direct(directory, signal, peer, samples).await;
    }
    let (directory, signal, origin) = match arguments.as_slice() {
        [directory, signal] => (directory, signal, None),
        [directory, signal, origin] => (directory, signal, Some(origin)),
        _ => return Err(io::Error::other(USAGE)),
    };
    let _instance = lince_interface::activation::claim_headless(std::path::Path::new(directory))?;
    let cell = cell::Cell::open_mobile(cell::CellOptions {
        data_dir: Some(PathBuf::from(directory)),
        ..Default::default()
    })
    .await?;
    let result = async {
        let _facade = match origin {
            Some(origin) => Some(
                lince_facade::Facade::start_public(
                    cell.runtime().clone(),
                    "127.0.0.1:6174",
                    origin,
                )
                .await?,
            ),
            None => None,
        };
        if let Some(wire) = cell.runtime().wire.read().await.as_ref() {
            eprintln!("Camera check: Cell iroh reach {:?}", wire.reach());
        }
        tokio::time::timeout(
            verification_timeout(samples),
            verify(&cell.runtime().engine, signal, None, samples),
        )
        .await
        .map_err(|_| io::Error::other("Camera verification exceeded its deadline"))?
    }
    .await;
    cell.shutdown().await;
    result
}
