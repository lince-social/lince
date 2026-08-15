//! The sync runner (Ontology §11): one background task driving both
//! convergence mechanisms against every synced contact, over iroh.
//!
//! - **Reactive deltas**: any committed fact wakes the runner immediately, so
//!   the bounded outbox drains as writes happen (the fact bus is the "one
//!   central change-handling function" fanout — sync is just one subscriber).
//! - **Catch-up reconciliation**: per contact with `sync_in`, pull the ops past
//!   our position and advance `last_synced_seq` (a diagnostic now — catch-up
//!   runs on a version vector) only after a successful
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

/// How often the public directory record is re-broadcast.
///
/// DHT entries expire in hours, so something has to republish or the identity
/// key stops resolving — and an Organ whose key resolves to nothing is exactly
/// the stranded state publishing exists to prevent. Hourly is well inside the
/// expiry with room for a Cell that misses a tick, and it costs one small HTTP
/// PUT. It needs no key: the signed bytes are stored and re-sent verbatim.
const REPUBLISH_INTERVAL: Duration = Duration::from_secs(3600);

pub(crate) fn spawn_runner(state: CellApiState) {
    tokio::spawn(async move {
        let mut bus = state.engine.subscribe();
        // Boot already published once (`publish_local_roster`), so the first
        // tick here is a full interval away rather than immediate.
        let mut next_republish = tokio::time::Instant::now() + REPUBLISH_INTERVAL;
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
            if tokio::time::Instant::now() >= next_republish {
                next_republish = tokio::time::Instant::now() + REPUBLISH_INTERVAL;
                republish(&state, wire.as_deref()).await;
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

/// Re-broadcast this Organ's public directory record.
///
/// Gated on the ENDPOINT's reach, not on config: a Cell bound `Local`
/// publishes no addresses and must not publish a directory record either. A
/// Cell with no stored packet — every Organ with no front door — does nothing
/// and touches no network, which is what keeps this out of the way of tests.
async fn republish(state: &CellApiState, wire: Option<&engine::wire::Wire>) {
    // Relay-only publishes too — that is the point of it. Only a Cell bound
    // LOCAL publishes nothing.
    if wire.map(engine::wire::Wire::reach) == Some(engine::wire::Reach::Local) || wire.is_none() {
        return;
    }
    let Ok(Some(organ)) = store::organs::local(&state.store.pool).await else {
        return;
    };
    if let Err(error) = state.engine.republish_public_record(&organ.uid).await {
        // A relay being unreachable says nothing about whether this Cell
        // should keep running; the next tick tries again.
        tracing::warn!(%error, "could not republish the directory record");
    }
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
