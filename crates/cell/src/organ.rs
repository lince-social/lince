use crate::CellRuntime;
use std::{io, sync::Arc};

impl CellRuntime {
    async fn nearby_wire(&self, node_id: &str, name: &str) -> io::Result<Arc<engine::wire::Wire>> {
        if name.trim().is_empty() || name.chars().count() > 160 {
            return Err(io::Error::other("Use a name or title of 1–160 characters"));
        }
        if !self
            .engine
            .nearby_peers()
            .iter()
            .any(|peer| peer.node_id == node_id)
        {
            return Err(io::Error::other(
                "This Cell is no longer nearby. Wait for discovery or use a pairing code.",
            ));
        }
        if store::organs::contact_by_node_id(&self.store.pool, node_id)
            .await
            .map_err(io::Error::other)?
            .is_some_and(|contact| contact.trust == "blocked")
        {
            return Err(io::Error::other(
                "This Organ is blocked. Change its trust before connecting.",
            ));
        }
        self.wire
            .read()
            .await
            .clone()
            .ok_or_else(|| io::Error::other("Network is offline"))
    }

    pub async fn pair_nearby(&self, node_id: &str, name: &str) -> io::Result<String> {
        let wire = self.nearby_wire(node_id, name).await?;
        let invite = engine::pairing::PairingInvite {
            node_id: node_id.into(),
            root_key: None,
            label: None,
            addrs: vec![],
        };
        wire.pair_with(&invite, name.trim())
            .await
            .map_err(io::Error::other)
    }

    pub async fn chat_nearby(&self, node_id: &str, title: &str) -> io::Result<(String, String)> {
        self.nearby_wire(node_id, title)
            .await?
            .offer_conversation_to_node(node_id, title.trim())
            .await
            .map_err(io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nearby_controls_refuse_missing_peers_and_never_unblock_contacts() {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let runtime = CellRuntime {
            commands: Default::default(),
            store: engine.store.clone(),
            engine: engine.clone(),
            lanes: Arc::new(crate::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            information: None,
        };
        assert!(
            runtime
                .pair_nearby("missing", "Friend")
                .await
                .unwrap_err()
                .to_string()
                .contains("no longer nearby")
        );
        assert!(
            runtime
                .chat_nearby("missing", " ")
                .await
                .unwrap_err()
                .to_string()
                .contains("1–160")
        );
        let nearby = engine::wire::Nearby::default();
        nearby.observe("node".into(), "fingerprint".into(), "Claim".into());
        engine.attach_nearby(nearby);
        store::organs::add_contact(&runtime.store.pool, "peer", None, "Blocked", "", 1)
            .await
            .unwrap();
        store::organs::set_node_id(&runtime.store.pool, "peer", Some("node"))
            .await
            .unwrap();
        store::organs::set_trust(&runtime.store.pool, "peer", "blocked")
            .await
            .unwrap();
        for error in [
            runtime.pair_nearby("node", "Friend").await.unwrap_err(),
            runtime.chat_nearby("node", "Hello").await.unwrap_err(),
        ] {
            assert!(error.to_string().contains("blocked"));
        }
        assert_eq!(
            store::organs::contact(&runtime.store.pool, "peer")
                .await
                .unwrap()
                .unwrap()
                .trust,
            "blocked"
        );
    }
}
