use std::sync::Arc;

use tokio::sync::RwLock;

use crate::CellApiState;

pub(crate) type WireSlot = Arc<RwLock<Option<Arc<engine::wire::Wire>>>>;

pub(crate) fn spawn(state: CellApiState, key_dir: std::path::PathBuf) {
    tokio::spawn(async move {
        let mut bus = state.engine.subscribe();
        let mut config = state.engine.watch_config();
        let mut current = discovery_of(&state).await;
        loop {
            let event = tokio::select! {
                received = bus.recv() => received,
                changed = config.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let wanted = discovery_of(&state).await;
                    if wanted != current {
                        current = wanted;
                        rebind(&state, &key_dir, wanted).await;
                    }
                    continue;
                }
            };
            match event {
                Ok(fact) => {
                    let Ok(Some(organ)) = store::organs::local(&state.store.pool).await else {
                        continue;
                    };
                    if fact.record_uid != organ.uid {
                        continue;
                    }
                    let wanted = discovery_of(&state).await;
                    if wanted == current {
                        continue;
                    }
                    current = wanted;
                    rebind(&state, &key_dir, wanted).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let wanted = discovery_of(&state).await;
                    if wanted != current {
                        current = wanted;
                        rebind(&state, &key_dir, wanted).await;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Discovery {
    reach: engine::wire::Reach,
    local: bool,
}

async fn discovery_of(state: &CellApiState) -> Discovery {
    let Ok(Some(organ)) = store::organs::local(&state.store.pool).await else {
        return Discovery {
            reach: engine::wire::Reach::Relay,
            local: false,
        };
    };
    let reach = crate::discovery_reach(&state.store, &organ.uid).await;
    Discovery {
        reach,
        local: crate::discovery_is_local(&state.store, &organ.uid).await,
    }
}

async fn rebind(state: &CellApiState, key_dir: &std::path::Path, discovery: Discovery) {
    let secret = match engine::wire::node_secret(&key_dir.join("keys").join("node-ed25519-v1.key"))
    {
        Ok(secret) => secret,
        Err(error) => {
            tracing::warn!(%error, "cannot reload the node key; keeping the current endpoint");
            return;
        }
    };
    let label = store::organs::local(&state.store.pool)
        .await
        .ok()
        .flatten()
        .map(|organ| organ.head)
        .unwrap_or_default();

    let previous = state.wire.write().await.take();
    if let Some(previous) = previous {
        previous.shutdown().await;
    }

    match engine::wire::Wire::bind_with_discovery(
        state.engine.clone(),
        secret,
        discovery.reach,
        Some(&label),
        discovery.local,
    )
    .await
    {
        Ok(wire) => {
            let wire = Arc::new(wire);
            wire.set_live_handler(transport::live::LiveHost::new(
                state.engine.clone(),
                state.lanes.clone(),
            ));
            wire.set_transfer_handler(std::sync::Arc::new(
                super::transfer_delivery::TransferPeerHandler::new(state.clone()),
            ));
            *state.wire.write().await = Some(wire.clone());
            tokio::spawn(async move { wire.serve().await });
            tracing::info!(?discovery, "iroh endpoint rebound for a discovery change");
        }
        Err(error) => tracing::warn!(%error, "rebinding the iroh endpoint failed"),
    }
}
