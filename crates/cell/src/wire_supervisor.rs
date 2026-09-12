use crate::{
    CellRuntime,
    discovery::{Discovery, of as discovery_of},
};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type WireSlot = Arc<RwLock<Option<Arc<engine::wire::Wire>>>>;

pub fn spawn(state: CellRuntime, key_dir: std::path::PathBuf) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut bus = state.engine.subscribe();
        let mut config = state.engine.watch_config();
        let mut current = if state.wire.read().await.is_some() {
            match discovery_of(&state.store).await {
                Ok(discovery) => Some(discovery),
                Err(error) => {
                    tracing::warn!(%error, "Cannot read discovery settings. Keeping the current peer connection");
                    None
                }
            }
        } else {
            None
        };
        let mut retry = tokio::time::interval(std::time::Duration::from_secs(30));
        retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                received = bus.recv() => match received {
                    Ok(fact) => {
                        let organ = match store::organs::local(&state.store.pool).await {
                            Ok(Some(organ)) => organ,
                            Ok(None) => { tracing::warn!("Cannot update peer connections: the local Organ is missing"); continue; }
                            Err(error) => { tracing::warn!(%error, "Cannot read the local Organ. Keeping the current peer connection"); continue; }
                        };
                        if fact.record_uid != organ.uid { continue }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                changed = config.changed() => { if changed.is_err() { break } },
                _ = retry.tick() => {},
            }
            let wanted = match discovery_of(&state.store).await {
                Ok(wanted) => wanted,
                Err(error) => {
                    tracing::warn!(%error, "Cannot read discovery settings. Keeping the current peer connection");
                    continue;
                }
            };
            if (current != Some(wanted) || state.wire.read().await.is_none())
                && rebind(&state, &key_dir, wanted).await
            {
                current = Some(wanted);
            }
        }
    })
}

async fn rebind(state: &CellRuntime, key_dir: &std::path::Path, discovery: Discovery) -> bool {
    let secret = match engine::wire::node_secret(&key_dir.join("keys").join("node-ed25519-v1.key"))
    {
        Ok(secret) => secret,
        Err(error) => {
            tracing::warn!(%error, "Cannot reload the node key. Keeping the current peer connection");
            return false;
        }
    };
    let label = match store::organs::local(&state.store.pool).await {
        Ok(Some(organ)) => organ.head,
        Ok(None) => {
            tracing::warn!("Cannot update peer connections: the local Organ is missing");
            return false;
        }
        Err(error) => {
            tracing::warn!(%error, "Cannot read the local Organ. Keeping the current peer connection");
            return false;
        }
    };
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
            wire.set_transfer_handler(Arc::new(crate::transfer::TransferPeerHandler::new(
                state.clone(),
            )));
            wire.serve_enrolment();
            *state.wire.write().await = Some(wire.clone());
            let serving = wire.clone();
            tokio::spawn(async move { serving.serve().await });
            let organ = match store::organs::local(&state.store.pool).await {
                Ok(Some(organ)) => organ,
                Ok(None) => {
                    tracing::warn!(
                        "Could not refresh the pairing invitation: the local Organ is missing"
                    );
                    return true;
                }
                Err(error) => {
                    tracing::warn!(%error, "Could not refresh the pairing invitation");
                    return true;
                }
            };
            if let Err(error) = crate::publish_pairing_invite(&state.store, &organ.uid, &wire).await
            {
                tracing::warn!(%error, "Could not refresh the pairing invitation");
            }
            tracing::info!(?discovery, "Peer connection updated");
            true
        }
        Err(error) => {
            tracing::warn!(%error, "Could not update peer connections. Peer connections are off; Lince will retry");
            false
        }
    }
}
