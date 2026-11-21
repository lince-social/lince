//! Projection of the current nearby peers supplied by the caller.

use crate::{Protein, ProteinError};
use nucleus::{
    nearby::NearbyPeer,
    performance::{Span, add},
};
use serde_json::{Value, json};
use store::{Store, organs::presentation::peers_by_node};

pub(crate) async fn execute_nearby(
    store: &Store,
    protein: &Protein,
    peers: &[NearbyPeer],
) -> Result<Vec<Value>, ProteinError> {
    let preparation = Span::new("protein-nearby-preparation");
    let mut peers: Vec<_> = peers.iter().collect();
    peers.sort_by_key(|peer| peer.node_id.as_str());
    if let Some(limit) = protein.limit {
        peers.truncate(limit.max(1));
    }
    let nodes: Vec<_> = peers.iter().map(|peer| peer.node_id.as_str()).collect();
    add("nearby-presentation-selected-peers", peers.len() as u64);
    drop(preparation);
    let contacts = peers_by_node(&store.pool, &nodes).await?;
    let _publication = Span::new("protein-nearby-publish");
    Ok(peers
        .into_iter()
        .map(|peer| {
            let contact = contacts.get(&peer.node_id);
            json!({
                "node_id": peer.node_id,
                "fingerprint": peer.fingerprint,
                "claimed_name": peer.name,
                "known": contact.is_some(),
                "name": contact.map(|contact| contact.head.as_str()),
                "trust": contact.map(|contact| contact.trust.as_str()),
            })
        })
        .collect())
}
