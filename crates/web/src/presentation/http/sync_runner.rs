//! The sync runner (Ontology §11): one background task driving both
//! convergence mechanisms against every synced contact.
//!
//! - **Reactive deltas**: any committed fact wakes the runner immediately, so
//!   the bounded outbox drains as writes happen (the fact bus is the "one
//!   central change-handling function" fanout — sync is just one subscriber).
//! - **Catch-up reconciliation**: per contact with `sync_in`, pull
//!   `GET /organ/ops?after=<checkpoint>` on its configured interval and
//!   advance `last_synced_seq` only after a successful import. `0` disables
//!   the cycle for that contact; reactive pushes still flow.
//!
//! Every request is SIGNED with the local Organ key and every response is
//! VERIFIED against the contact's stored keys before a byte is imported — a
//! peer at a known address that cannot prove the key is a stranger
//! (Ontology §11 "Peers"). Addresses are candidates, never identity: the
//! last address a verified exchange succeeded from is cached as a hint, and
//! a LAN-discovery sighting adds one more candidate, verified before use.

use std::time::Duration;

use crate::CellApiState;

pub(crate) fn spawn_runner(state: CellApiState) {
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                tracing::error!(%error, "cannot start sync runner");
                return;
            }
        };
        let mut bus = state.engine.subscribe();
        loop {
            if let Err(error) = push_deltas(&state, &client).await {
                tracing::warn!(%error, "sync push failed");
            }
            if let Err(error) = pull_catch_up(&state, &client).await {
                tracing::warn!(%error, "sync catch-up failed");
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

/// Base-url candidates for a contact, most-recently-proven first: the last
/// verified address, the introduction's base_url, then a LAN-discovery
/// sighting. All are hints; only a verified signed exchange promotes one.
fn candidate_urls(state: &CellApiState, contact: &store::organs::Contact) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut push = |url: Option<String>| {
        if let Some(url) = url {
            let url = url.trim_end_matches('/').to_string();
            if !url.is_empty() && !candidates.contains(&url) {
                candidates.push(url);
            }
        }
    };
    push(contact.last_seen_addr.clone());
    push(Some(contact.base_url.clone()));
    push(state.nearby.candidate_url_for(&contact.record_uid));
    candidates
}

/// A verified exchange from `url` succeeded — remember the address hint.
async fn remember_address(state: &CellApiState, contact_uid: &str, url: &str, previous: Option<&str>) {
    if previous == Some(url) {
        return;
    }
    if let Err(error) =
        store::organs::set_last_seen_addr(&state.store.pool, contact_uid, Some(url)).await
    {
        tracing::warn!(%error, "cannot cache verified peer address");
    }
}

/// Send one signed request and return the VERIFIED body bytes: response
/// headers must carry the contact's signature over the body, fresh, matching
/// the organ we called. Anything less is a stranger's answer — dropped.
async fn signed_exchange(
    state: &CellApiState,
    client: &reqwest::Client,
    contact_uid: &str,
    base_url: &str,
    method: &str,
    path_and_query: &str,
    body: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let (organ, ts, sig) = state
        .engine
        .sign_peer_request(method, path_and_query, body.unwrap_or_default())
        .await
        .ok_or_else(|| "no organ signer installed".to_string())?;
    let url = format!("{base_url}{path_and_query}");
    let request = match method {
        "POST" => client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.unwrap_or_default().to_vec()),
        _ => client.get(&url),
    };
    let response = request
        .header("x-lince-organ", organ)
        .header("x-lince-ts", ts)
        .header("x-lince-sig", sig)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("{} from {url}", response.status()));
    }
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    let (claimed, resp_ts, resp_sig) = match (
        header("x-lince-organ"),
        header("x-lince-ts"),
        header("x-lince-sig"),
    ) {
        (Some(organ), Some(ts), Some(sig)) => (organ, ts, sig),
        _ => return Err(format!("unsigned response from {url}")),
    };
    let bytes = response
        .bytes()
        .await
        .map_err(|error| error.to_string())?
        .to_vec();
    let verified = state
        .engine
        .verify_peer_response(contact_uid, &claimed, &resp_ts, &resp_sig, &bytes)
        .await
        .map_err(|error| error.to_string())?;
    if !verified {
        return Err(format!(
            "peer at {base_url} failed the challenge — stranger at a known address"
        ));
    }
    Ok(bytes)
}

/// Drain the bounded outbox over HTTP: one signed op batch per contact, tried
/// against each address candidate until a VERIFIED exchange succeeds.
async fn push_deltas(state: &CellApiState, client: &reqwest::Client) -> Result<(), String> {
    state
        .engine
        .drain_outbox(|contact, batch| {
            let client = client.clone();
            let state = state.clone();
            async move {
                let body = serde_json::to_vec(&batch).map_err(|error| error.to_string())?;
                let mut last_error = format!("no address for {}", contact.record_uid);
                for base_url in candidate_urls(&state, &contact) {
                    match signed_exchange(
                        &state,
                        &client,
                        &contact.record_uid,
                        &base_url,
                        "POST",
                        "/organ/inbox",
                        Some(&body),
                    )
                    .await
                    {
                        Ok(_) => {
                            remember_address(
                                &state,
                                &contact.record_uid,
                                &base_url,
                                contact.last_seen_addr.as_deref(),
                            )
                            .await;
                            return Ok(());
                        }
                        Err(error) => last_error = error,
                    }
                }
                Err(last_error)
            }
        })
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[derive(serde::Deserialize)]
struct OpsFeed {
    from_organ: String,
    ops: Vec<engine::sync::WireOp>,
    head: i64,
}

/// Pull each syncing contact's feed past our checkpoint and advance it.
async fn pull_catch_up(state: &CellApiState, client: &reqwest::Client) -> Result<(), String> {
    let contacts = store::organs::contacts(&state.store.pool)
        .await
        .map_err(|error| error.to_string())?;
    for contact in contacts {
        if !contact.sync_in || contact.trust == "blocked" || contact.catchup_interval_secs <= 0 {
            continue;
        }
        let path_and_query = format!("/organ/ops?after={}", contact.last_synced_seq);
        let mut verified_body: Option<(String, Vec<u8>)> = None;
        for base_url in candidate_urls(state, &contact) {
            match signed_exchange(
                state,
                client,
                &contact.record_uid,
                &base_url,
                "GET",
                &path_and_query,
                None,
            )
            .await
            {
                Ok(bytes) => {
                    verified_body = Some((base_url, bytes));
                    break;
                }
                Err(error) => {
                    tracing::debug!(base_url, %error, "catch-up candidate failed");
                }
            }
        }
        let Some((base_url, bytes)) = verified_body else {
            continue; // unreachable or unproven — catch-up waits
        };
        let feed: OpsFeed = match serde_json::from_slice(&bytes) {
            Ok(feed) => feed,
            Err(error) => {
                tracing::warn!(%error, "malformed ops feed");
                continue;
            }
        };
        // Verified signature already proved WHO answered; this catches a
        // proxy serving someone else's feed verbatim.
        if feed.from_organ != contact.record_uid {
            tracing::warn!(
                claimed = feed.from_organ,
                "ops feed claims a different organ; ignored"
            );
            continue;
        }
        remember_address(
            state,
            &contact.record_uid,
            &base_url,
            contact.last_seen_addr.as_deref(),
        )
        .await;
        if feed.ops.is_empty() {
            continue; // converged — O(1) cycle
        }
        let batch = engine::sync::OpBatch {
            from_organ: feed.from_organ,
            ops: feed.ops,
        };
        match state.engine.import_op_batch(&batch).await {
            Ok(_) => {
                // Advance only after a successful import.
                if let Err(error) = store::organs::set_last_synced_seq(
                    &state.store.pool,
                    &contact.record_uid,
                    feed.head,
                )
                .await
                {
                    tracing::warn!(%error, "cannot advance sync checkpoint");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "catch-up import failed; checkpoint not advanced");
            }
        }
    }
    Ok(())
}
