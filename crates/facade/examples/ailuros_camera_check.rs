use std::{io, path::PathBuf, time::Duration};

const USAGE: &str = "Usage: ailuros_camera_check <cell-directory> <signal-uid> <https-origin>";

async fn frames(
    subscription: &mut engine::signal_streams::Subscription,
    count: usize,
    phase: &str,
) -> Result<serde_json::Value, io::Error> {
    let started = std::time::Instant::now();
    let mut received = 0;
    let mut bytes = 0;
    let mut maximum_age_ms = 0;
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
            previous = Some(key);
            let (width, height, _) =
                engine::signal_streams::decode_jpeg(&value.bytes).map_err(io::Error::other)?;
            dimensions = (width, height);
            bytes += value.bytes.len();
            maximum_age_ms = maximum_age_ms
                .max(chrono::Utc::now().timestamp_millis() - value.metadata.captured_at);
            received += 1;
        }
    }
    let result = serde_json::json!({
        "frames": received,
        "width": dimensions.0,
        "height": dimensions.1,
        "bytes": bytes,
        "seconds": started.elapsed().as_secs_f64(),
        "maximum_age_ms": maximum_age_ms,
    });
    eprintln!("{}", serde_json::json!({"phase":phase,"result":&result}));
    Ok(result)
}

async fn verify(runtime: &cell::CellRuntime, signal: &str) -> Result<(), io::Error> {
    let channel = runtime
        .engine
        .stream_channel(signal, None)
        .await
        .map_err(io::Error::other)?;
    let mut first = channel.subscribe().map_err(io::Error::other)?;
    let mut second = channel.subscribe().map_err(io::Error::other)?;
    let initial = frames(&mut first, 150, "two viewers").await?;
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
    let remaining_viewer = frames(&mut second, 10, "one viewer").await?;
    drop(second);
    let idle = channel.snapshot();
    if idle.consumers != 0 || idle.value.is_some() || idle.epoch == epoch {
        return Err(io::Error::other(
            "Closing the final viewer did not release the current frame",
        ));
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    let mut reopened = channel.subscribe().map_err(io::Error::other)?;
    let resumed = frames(&mut reopened, 10, "resumed viewer").await?;
    println!(
        "{}",
        serde_json::json!({
            "type": "ailuros-camera-check",
            "signal": signal,
            "initial": initial,
            "remaining_viewer": remaining_viewer,
            "resumed": resumed,
            "final_viewer_idle": true,
        })
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.as_slice() == ["--help"] {
        println!("{USAGE}");
        return Ok(());
    }
    let [directory, signal, origin] = arguments.as_slice() else {
        return Err(io::Error::other(USAGE));
    };
    let _instance = lince_interface::activation::claim_headless(std::path::Path::new(directory))?;
    let cell = cell::Cell::open_mobile(cell::CellOptions {
        data_dir: Some(PathBuf::from(directory)),
        ..Default::default()
    })
    .await?;
    let result =
        match lince_facade::Facade::start_public(cell.runtime().clone(), "127.0.0.1:6174", origin)
            .await
        {
            Ok(facade) => {
                let result =
                    tokio::time::timeout(Duration::from_secs(180), verify(cell.runtime(), signal))
                        .await
                        .map_err(|_| io::Error::other("Camera verification exceeded three minutes"))
                        .and_then(|result| result);
                drop(facade);
                result
            }
            Err(error) => Err(error),
        };
    cell.shutdown().await;
    result
}
