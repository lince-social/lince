use std::time::Duration;

use crate::CellApiState;

const REPUBLISH_INTERVAL: Duration = Duration::from_secs(3600);

pub(crate) fn spawn_runner(state: CellApiState) {
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
            let sleep_secs = shortest_interval(&state).await.unwrap_or(30);
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(sleep_secs)) => {}
                received = bus.recv() => {
                    if received.is_ok() {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        while bus.try_recv().is_ok() {}
                    }
                }
            }
        }
    });
}

async fn republish(state: &CellApiState, wire: Option<&engine::wire::Wire>) {
    if wire.map(engine::wire::Wire::reach) == Some(engine::wire::Reach::Local) || wire.is_none() {
        return;
    }
    let Ok(Some(organ)) = store::organs::local(&state.store.pool).await else {
        return;
    };
    if let Err(error) = state.engine.republish_public_record(&organ.uid).await {
        tracing::warn!(%error, "could not republish the directory record");
    }
}

async fn shortest_interval(state: &CellApiState) -> Result<u64, String> {
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
