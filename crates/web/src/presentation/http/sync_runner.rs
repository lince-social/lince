//! The sync runner (Ontology §11): one background task driving both
//! convergence mechanisms against every synced contact, over iroh.
//!
//! - **Reactive deltas**: any committed fact wakes the runner immediately, so
//!   the bounded outbox drains as writes happen (the fact bus is the "one
//!   central change-handling function" fanout — sync is just one subscriber).
//! - **Catch-up reconciliation**: per contact with `sync_in`, pull the ops past
//!   our checkpoint and advance `last_synced_seq` only after a successful
//!   import. `0` disables the cycle for that contact; reactive pushes still
//!   flow.
//!
//! Transport is `engine::wire` (Ontology §11 "Transport: iroh"), which REPLACED
//! the signed-HTTP peer path here on 2026-08-03. Everything the old path did by
//! hand — signing each request, verifying each response, a 120s replay window,
//! and a list of address candidates to try in turn — is gone, because an iroh
//! connection is mutually authenticated at the QUIC/TLS handshake and a NodeId
//! resolves itself. Op-batch payload signing is untouched: that answers "who
//! wrote this op", which must still hold a year later, from a backup, with no
//! connection in sight.
//!
//! The offline send queue is not a separate mechanism. The outbox is durable,
//! so ops for a peer with a closed laptop stay queued and flush on the next
//! pass — which is exactly the "meet at the beach, they go home, the message
//! arrives later" case.

use std::time::Duration;

use crate::CellApiState;

pub(crate) fn spawn_runner(state: CellApiState) {
    tokio::spawn(async move {
        let mut bus = state.engine.subscribe();
        loop {
            // Re-read every pass: a discovery change REBINDS the endpoint
            // (see `wire_supervisor`), so a handle captured once would keep
            // using a socket that has been closed.
            let wire = state.wire.read().await.clone();
            match match &wire {
                Some(wire) => wire.sync_once().await,
                None => Ok(0),
            } {
                Ok(moved) if moved > 0 => tracing::debug!(moved, "sync pass moved batches"),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "sync pass failed"),
            }
            let sleep_secs = shortest_interval(&state).await.unwrap_or(30);
            // Wake early on any committed fact (a reactive delta just queued);
            // drain the burst before looping so one wake serves many writes.
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

/// The soonest catch-up any contact wants, clamped to [5, 300]s; 30 when no
/// contact pulls.
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
