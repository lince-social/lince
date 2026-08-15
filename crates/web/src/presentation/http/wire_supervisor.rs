//! Keeps the iroh endpoint in step with `lince.discovery` (Ontology §11).
//!
//! Discovery is an Endpoint BUILDER option, fixed at construction — there is no
//! way to turn internet reachability on or off on a live endpoint. So changing
//! it means REBINDING, and the choice is between demanding a reboot and doing
//! what File Sync already does for watchers: watch the fact bus, and rebuild
//! the thing when its config Fact changes.
//!
//! The node key is loaded from the same file every time, so the NodeId survives
//! a rebind. That matters more than it looks: a Cell whose NodeId changed when
//! a setting was toggled would strand every contact who had saved it.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::CellApiState;

/// The live endpoint handle. `None` while rebinding, or when binding failed —
/// callers treat that as "peers unreachable", never as an error.
pub(crate) type WireSlot = Arc<RwLock<Option<Arc<engine::wire::Wire>>>>;

pub(crate) fn spawn(state: CellApiState, key_dir: std::path::PathBuf) {
    tokio::spawn(async move {
        let mut bus = state.engine.subscribe();
        // Cell config is written raw and drops no Fact, so the bus alone would
        // never hear a discovery change — see `Engine::watch_config`.
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
                    // Only the local Organ's own extension matters here.
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
                    // Missed facts: re-read rather than assume nothing changed.
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

    // Take the old endpoint out FIRST and close it, so the new one can bind
    // and so no caller keeps using a socket that is about to go away.
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
            // Live sessions: a contact with a login granted drives a real
            // session on this Cell over `lince/live/1`. Installed on every
            // rebind, because the handler belongs to the endpoint and a
            // rebind makes a new one.
            wire.set_live_handler(transport::live::LiveHost::new(
                state.engine.clone(),
                state.lanes.clone(),
            ));
            // Same reasoning for Transfer: the delivery worker dials through
            // whatever endpoint is current, and a rebound endpoint with no
            // handler would answer every peer's envelope with "this Cell does
            // not deliver Transfers".
            wire.set_transfer_handler(std::sync::Arc::new(
                super::transfer_delivery::TransferPeerHandler::new(state.clone()),
            ));
            *state.wire.write().await = Some(wire.clone());
            tokio::spawn(async move { wire.serve().await });
            tracing::info!(?discovery, "iroh endpoint rebound for a discovery change");
        }
        // Left as `None`: the Cell serves its own board, peers are simply
        // unreachable until the setting is changed back or the Cell restarts.
        Err(error) => tracing::warn!(%error, "rebinding the iroh endpoint failed"),
    }
}
