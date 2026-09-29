use crate::CellRuntime;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub fn spawn(runtime: CellRuntime) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut active = HashMap::new();
        let mut attempted = HashMap::new();
        let mut tasks = tokio::task::JoinSet::new();
        let mut timer = tokio::time::interval(Duration::from_secs(5));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let Ok(blobs) = runtime.engine.blob_sync() else {
            return;
        };
        let mut changes = blobs.watch();
        loop {
            tokio::select! {
                _ = timer.tick() => {}
                changed = changes.changed() => { if changed.is_err() { break; } }
                Some(result) = tasks.join_next(), if !tasks.is_empty() => {
                    match result {
                        Ok(id) => { active.remove(&id); }
                        Err(error) => { tracing::error!(%error, "Blob Sync worker stopped"); active.clear(); }
                    }
                    continue;
                }
            }
            let Some(wire) = runtime.wire.read().await.clone() else {
                continue;
            };
            let Ok(mut transfers) = runtime.engine.blob_transfers().await else {
                continue;
            };
            transfers.sort_by_key(|transfer| attempted.get(&transfer.id).copied());
            attempted.retain(|id, _| transfers.iter().any(|transfer| transfer.id == *id));
            for transfer in transfers {
                if active.contains_key(&transfer.id)
                    || (transfer.direction == "incoming" && transfer.state == "offered")
                    || transfer.settled
                {
                    continue;
                }
                if attempted
                    .get(&transfer.id)
                    .is_some_and(|at: &Instant| at.elapsed() < Duration::from_secs(5))
                {
                    continue;
                }
                let download = transfer.direction == "incoming" && transfer.state == "accepted";
                if active.values().filter(|kind| **kind == download).count()
                    >= if download { 2 } else { 4 }
                {
                    continue;
                }
                active.insert(transfer.id.clone(), download);
                attempted.insert(transfer.id.clone(), Instant::now());
                let wire = wire.clone();
                tasks.spawn(async move {
                    let _ = wire.blob_step(&transfer.id).await;
                    transfer.id
                });
            }
        }
    })
}
