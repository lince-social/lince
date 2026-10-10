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
        let mut identity = store::organs::local(&state.store.pool)
            .await
            .ok()
            .flatten()
            .map(|organ| organ.uid);
        let mut current = state.wire.read().await.as_ref().map(|wire| Discovery {
            reach: wire.reach(),
            local: wire.local_discovery(),
            peer_port: wire.configured_port(),
            relays: wire.configured_relays().iter().map(ToString::to_string).collect(),
        });
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
            let next_identity = store::organs::local(&state.store.pool)
                .await
                .ok()
                .flatten()
                .map(|organ| organ.uid);
            if current.as_ref().map(|discovery| discovery.reach) != Some(wanted.reach)
                || current.as_ref().map(|discovery| discovery.peer_port) != Some(wanted.peer_port)
                || current.as_ref().map(|discovery| &discovery.relays) != Some(&wanted.relays)
                || state.wire.read().await.is_none()
            {
                if rebind(&state, &key_dir, wanted).await {
                    current = state.wire.read().await.as_ref().map(|wire| Discovery {
                        reach: wire.reach(),
                        local: wire.local_discovery(),
                        peer_port: wire.configured_port(),
                        relays: wire.configured_relays().iter().map(ToString::to_string).collect(),
                    });
                    identity = next_identity;
                }
            } else if let Some(wire) = state.wire.read().await.clone() {
                if let Err(error) = wire.set_local_discovery(wanted.local) {
                    tracing::warn!(%error, "Could not update LAN discovery");
                    continue;
                }
                if identity != next_identity
                    && let Ok(Some(organ)) = store::organs::local(&state.store.pool).await
                {
                    wire.set_display_name(&organ.head);
                }
                current = Some(wanted);
                identity = next_identity;
            }
            if let Some(wire) = state.wire.read().await.clone()
                && let Ok(Some(organ)) = store::organs::local(&state.store.pool).await
                && let Err(error) =
                    crate::publish_pairing_invite(&state.engine, &organ.uid, &wire).await
            {
                tracing::warn!(%error, "Could not refresh the pairing code");
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
    match engine::wire::Wire::bind_on_port(
        state.engine.clone(),
        secret,
        discovery.reach,
        Some(&label),
        discovery.local,
        discovery.peer_port,
    )
    .await
    {
        Ok(wire) => {
            let wire = Arc::new(wire);
            state.engine.attach_social_network(wire.clone());
            state.engine.attach_location_network(wire.clone());
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
            if let Err(error) =
                crate::publish_pairing_invite(&state.engine, &organ.uid, &wire).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn saved_relay_changes_restart_wire_keep_node_identity_and_apply_selected_relay() {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let directory = tempfile::tempdir().unwrap();
            let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
            store::cells::set_config(&engine.store.pool, "lince.network", &json!({"peer_port":0})).await.unwrap();
            store::cells::set_config(&engine.store.pool, "lince.discovery", &json!({"internet":false})).await.unwrap();
            let secret = engine::wire::node_secret(&directory.path().join("keys/node-ed25519-v1.key")).unwrap();
            let original = Arc::new(engine::wire::Wire::bind_with_discovery(engine.clone(), secret, engine::wire::Reach::Local, None, false).await.unwrap());
            let state = CellRuntime {
                engine:engine.clone(),store:engine.store.clone(),lanes:Arc::new(crate::LaneHub::new()),
                wire:Arc::new(RwLock::new(Some(original.clone()))),speech:None,commands:Default::default(),information:None,fiote:None,
            };
            let supervisor = spawn(state.clone(), directory.path().to_path_buf());
            tokio::task::yield_now().await;
            engine.act(engine::actions::Action::SetCellConfig { namespace:"lince.discovery".into(),fds:json!({"internet":true,"direct":false,"relays":["https://127.0.0.1:9"]}) },None).await.unwrap();
            let current = loop {
                if let Some(wire) = state.wire.read().await.clone()
                    && wire.reach() == engine::wire::Reach::Relay {
                        break wire;
                    }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            };
            assert!(!Arc::ptr_eq(&original,&current));
            assert_eq!(current.node_id(),original.node_id());
            assert_eq!(current.configured_relays()[0].as_str(),"https://127.0.0.1:9/");
            assert!(current.endpoint().bound_sockets().is_empty());
            assert_eq!(current.network_status()["relay_selection"],"custom");
            assert!(current.endpoint().remove_relay(&current.configured_relays()[0]).await.is_some());
            assert!(engine.act(engine::actions::Action::SetCellConfig { namespace:"lince.discovery".into(),fds:json!({"relays":["http://127.0.0.1:9"]}) },None).await.is_err());
            assert!(Arc::ptr_eq(state.wire.read().await.as_ref().unwrap(), &current));
            supervisor.abort();
            let _ = supervisor.await;
            current.shutdown().await;
        }).await.unwrap();
    }
}
