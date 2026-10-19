use engine::{
    Engine,
    actions::Action,
    ailuros::receiver,
    signal_endpoints::Request,
    signal_streams::Subscription,
    wire::{Reach, Wire},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha384};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

async fn enabled(
    engine: &Engine,
    grant: &mut store::signal_endpoints::Endpoint,
    enabled: bool,
) -> Result<()> {
    let result = engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint: grant.uid.clone(),
                    expected_revision: grant.revision,
                    enabled,
                },
            },
            None,
        )
        .await?
        .data
        .ok_or("Missing grant confirmation")?;
    *grant = serde_json::from_value(result["endpoints"][0].clone())?;
    assert_eq!(grant.enabled, enabled);
    Ok(())
}

async fn frames(
    viewer: &mut Subscription,
    count: usize,
    stage: &str,
    seen: &mut BTreeSet<(String, u64)>,
    hashes: &mut BTreeSet<Vec<u8>>,
) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(180), async {
        let started = Instant::now();
        let mut first: Option<Instant> = None;
        let mut last: Option<Instant> = None;
        let mut maximum_age = 0i64;
        let mut bytes = 0usize;
        let mut received = 0usize;
        while received < count {
            viewer.updates.changed().await?;
            let snapshot = viewer.updates.borrow_and_update().clone();
            let Some(value) = snapshot.value else { continue };
            if !seen.insert((value.metadata.epoch.clone(), value.metadata.sequence)) { continue; }
            assert_eq!(snapshot.epoch, value.metadata.epoch);
            assert_eq!(value.metadata.content_type, "image/jpeg");
            let observed = Instant::now();
            let observer_delay = observed.saturating_duration_since(value.received).as_millis();
            let image = image::load_from_memory(&value.bytes)?;
            assert_eq!((image.width(), image.height()), (320, 240));
            let age = nucleus::execution::now().timestamp_millis() - value.metadata.captured_at;
            assert!((-1000..=5000).contains(&age), "{}", json!({
                "error":"unexpected-capture-age","stage":stage,"age_ms":age,
                "observer_delay_ms":observer_delay,"decode_ms":observed.elapsed().as_millis(),
                "captured_at":value.metadata.captured_at,"sequence":value.metadata.sequence,
                "frame_bytes":value.bytes.len(),
            }));
            maximum_age = maximum_age.max(age);
            bytes += value.bytes.len();
            hashes.insert(Sha384::digest(&value.bytes).to_vec());
            first.get_or_insert(value.received);
            last = Some(value.received);
            received += 1;
            if received % 20 == 0 {
                println!("{}", json!({"type":"ailuros-hardware-progress","stage":stage,"frames":received}));
            }
        }
        let elapsed = last.unwrap().saturating_duration_since(first.unwrap()).as_secs_f64();
        let fps = (count - 1) as f64 / elapsed;
        let report = json!({"stage":stage,"frames":count,"fps":fps,"bytes":bytes,"maximum_age_ms":maximum_age,
            "first_frame_delay_ms":first.unwrap().saturating_duration_since(started).as_millis()});
        println!("{}", json!({"type":"ailuros-hardware-stage","result":report}));
        Ok::<_, Box<dyn std::error::Error>>(report)
    }).await?
}

#[tokio::test]
#[ignore = "Requires an explicitly selected physical ESP and USB destination update; starts only an isolated temporary Wire Cell"]
async fn camera_publishes_170_fresh_jpegs_tracks_viewers_and_recovers_after_revocation()
-> Result<()> {
    let public_key = std::env::var("AILUROS_DEVICE_PUBLIC_KEY")?;
    let path = PathBuf::from(
        std::env::var_os("AILUROS_HARDWARE_OUTPUT")
            .ok_or("Select a new isolated output directory")?,
    );
    std::fs::create_dir(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    }
    let relay = match std::env::var("AILUROS_HARDWARE_PATH").as_deref() {
        Ok("relay") => true,
        Ok("internet") => false,
        _ => return Err("Choose AILUROS_HARDWARE_PATH=internet or relay".into()),
    };
    let engine = Arc::new(Engine::open_memory().await?);
    let organ = store::organs::ensure_local(&engine.store.pool, "http://hardware.test").await?;
    store::cells::ensure_local(
        &engine.store.pool,
        &organ.uid,
        "Isolated camera qualification",
    )
    .await?;
    engine
        .act(
            Action::SetCellConfig {
                namespace: receiver::NAMESPACE.into(),
                fds: json!({"enabled":true,"iroh":true,"https":false}),
            },
            None,
        )
        .await?;
    let result = engine
        .act(
            Action::SignalEndpoints {
                request: Request::Save {
                    endpoint: None,
                    expected_revision: None,
                    name: "Camera qualification".into(),
                    public_key,
                    signal_slug: "camera".into(),
                    enabled: false,
                    cooldown_seconds: 1,
                    minimum: "0".into(),
                    maximum: "100".into(),
                    expires_at: (nucleus::execution::now() + chrono::Duration::days(1))
                        .to_rfc3339(),
                },
            },
            None,
        )
        .await?
        .data
        .ok_or("Missing enrollment confirmation")?;
    let mut grant: store::signal_endpoints::Endpoint =
        serde_json::from_value(result["endpoints"][0].clone())?;
    enabled(&engine, &mut grant, true).await?;
    engine
        .configure_stream(&grant.signal_uid, Some("jpeg"), "", None, None)
        .await?;
    let wire = Wire::bind_with_discovery(
        engine.clone(),
        iroh::SecretKey::generate(),
        if relay { Reach::Relay } else { Reach::Internet },
        None,
        false,
    )
    .await?;
    tokio::time::timeout(Duration::from_secs(45), wire.endpoint().online()).await?;
    let setup = json!({"destination":{"transport":"iroh","endpoint_id":wire.node_id().to_string()},
        "setup":{"audience":result["audience"],"device":grant.uid,
            "signal":grant.signal_uid,"key_epoch":grant.credential_epoch}});
    std::fs::write(
        path.join("destination.json"),
        serde_json::to_vec_pretty(&setup)?,
    )?;
    let server = wire.clone();
    let serving = tokio::spawn(async move { server.serve().await });
    println!(
        "{}",
        json!({"type":"ailuros-hardware-ready","configuration":setup,
        "output":path,"network":wire.network_status()})
    );
    let channel = engine.stream_channel(&grant.signal_uid, None).await?;
    let mut first = channel.subscribe()?;
    let mut second = channel.subscribe()?;
    let mut seen = BTreeSet::new();
    let mut hashes = BTreeSet::new();
    let mut stages = Vec::new();
    stages.push(frames(&mut first, 80, "two-viewers", &mut seen, &mut hashes).await?);
    drop(first);
    assert_eq!(channel.snapshot().consumers, 1);
    stages.push(frames(&mut second, 40, "one-viewer", &mut seen, &mut hashes).await?);
    let previous_epoch = channel.snapshot().epoch;
    drop(second);
    assert_eq!(channel.snapshot().consumers, 0);
    assert!(channel.snapshot().value.is_none());
    assert_ne!(channel.snapshot().epoch, previous_epoch);
    println!("{}", json!({"type":"ailuros-hardware-no-viewers"}));
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(channel.snapshot().value.is_none());
    let mut reopened = channel.subscribe()?;
    assert_ne!(channel.snapshot().epoch, previous_epoch);
    stages.push(frames(&mut reopened, 30, "reopened", &mut seen, &mut hashes).await?);
    enabled(&engine, &mut grant, false).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(publisher) = channel.publisher() {
                drop(publisher);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    assert!(channel.snapshot().value.is_none());
    enabled(&engine, &mut grant, true).await?;
    stages.push(frames(&mut reopened, 20, "reauthenticated", &mut seen, &mut hashes).await?);
    assert_eq!(seen.len(), 170);
    assert!(
        hashes.len() > 1,
        "The camera did not produce changing JPEG content"
    );
    drop(reopened);
    let performance_ok = stages.iter().all(|stage| {
        stage["fps"]
            .as_f64()
            .is_some_and(|fps| fps.is_finite() && fps >= 4.5)
    });
    let report = json!({"ok":performance_ok,"performance_ok":performance_ok,
        "lifecycle_checked":true,"requested_path":if relay {"relay"} else {"internet"},
        "endpoint":wire.node_id().to_string(),"frames":seen.len(),"unique_jpegs":hashes.len(),
        "stages":stages,"cleared_after_viewers":channel.snapshot().value.is_none()});
    std::fs::write(
        path.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "{}",
        json!({"type":"ailuros-hardware-result","report":report})
    );
    serving.abort();
    wire.endpoint().close().await;
    engine.store.pool.close().await;
    assert!(
        performance_ok,
        "Camera stages fell below the 4.5 FPS minimum for the 5 FPS target: {}",
        report["stages"]
    );
    Ok(())
}
