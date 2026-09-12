use std::time::Duration;

use crate::CellRuntime;

const REPUBLISH_INTERVAL: Duration = Duration::from_secs(3600);

pub fn spawn_runner(state: CellRuntime) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut bus = state.engine.subscribe();
        let mut next_republish = tokio::time::Instant::now() + REPUBLISH_INTERVAL;
        loop {
            let wire = state.wire.read().await.clone();
            match match &wire {
                Some(wire) => wire.sync_once().await,
                None => Ok(0),
            } {
                Ok(moved) if moved > 0 => tracing::debug!(moved, "sync pass moved batches"),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "sync pass failed"),
            }
            if tokio::time::Instant::now() >= next_republish {
                next_republish = tokio::time::Instant::now() + REPUBLISH_INTERVAL;
                republish(&state, wire.as_deref()).await;
            }
            let sleep_secs = match shortest_interval(&state).await {
                Ok(seconds) => seconds,
                Err(error) => {
                    tracing::warn!(%error, "Cannot read sync settings. Lince will retry in 30 seconds");
                    30
                }
            };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(sleep_secs)) => {}
                received = bus.recv() => {
                    if matches!(received, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; }
                    if received.is_ok() {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        while bus.try_recv().is_ok() {}
                    }
                }
            }
        }
    })
}

async fn republish(state: &CellRuntime, wire: Option<&engine::wire::Wire>) {
    if wire.map(engine::wire::Wire::reach) == Some(engine::wire::Reach::Local) || wire.is_none() {
        return;
    }
    let organ = match store::organs::local(&state.store.pool).await {
        Ok(Some(organ)) => organ,
        Ok(None) => {
            tracing::warn!("Cannot publish the directory record: the local Organ is missing");
            return;
        }
        Err(error) => {
            tracing::warn!(%error, "Cannot read the local Organ for the directory record");
            return;
        }
    };
    if let Err(error) = state.engine.republish_public_record(&organ.uid).await {
        tracing::warn!(%error, "could not republish the directory record");
    }
}

async fn shortest_interval(state: &CellRuntime) -> Result<u64, String> {
    let contacts = store::organs::contacts(&state.store.pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(contacts
        .iter()
        .filter(|c| c.sync_in && c.catchup_interval_secs > 0 && c.trust != "blocked")
        .map(|c| c.catchup_interval_secs.clamp(5, 300) as u64)
        .min()
        .unwrap_or(30))
}
