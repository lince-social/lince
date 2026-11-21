// non-lean-yet

//! Connection admission, session lifetime, and inbound frames.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use iroh::endpoint::Connection;

use super::{
    ALPN_AILUROS, ALPN_HELLO, ALPN_LIVE, ALPN_LOCATION, ALPN_MAILBOX, ALPN_SOCIAL, ALPN_SYNC,
    ALPN_THREAD, DIAL_TIMEOUT, MAX_CONNECTIONS_PER_PEER, MAX_FRAME_BYTES,
    MAX_FRAMES_PER_CONNECTION, PrivateWireConfig, WIRE_EPOCH, Wire, WireRequest, WireResponse,
};
use crate::{error::EngineError, sync::Introduction};

struct PeerPermit {
    peers: Arc<Mutex<HashMap<String, usize>>>,
    peer: String,
}

impl Drop for PeerPermit {
    fn drop(&mut self) {
        let mut peers = self.peers.lock().expect("open per peer");
        if let Some(count) = peers.get_mut(&self.peer) {
            *count -= 1;
            if *count == 0 {
                peers.remove(&self.peer);
            }
        }
    }
}

struct ClosingConnection(Connection);

impl Drop for ClosingConnection {
    fn drop(&mut self) {
        self.0.close(0u32.into(), b"private session ended");
    }
}

impl Wire {
    pub async fn serve(&self) {
        if let Some(config) = self.private {
            self.serve_private(config).await;
            return;
        }
        while let Some(incoming) = self.endpoint.accept().await {
            let Ok(slot) = self.connection_slots.clone().try_acquire_owned() else {
                incoming.refuse();
                continue;
            };
            let wire = self.clone();
            tokio::spawn(async move {
                let _slot = slot;
                let connection = match tokio::time::timeout(DIAL_TIMEOUT, incoming).await {
                    Ok(Ok(connection)) => connection,
                    _ => {
                        tracing::debug!("iroh handshake failed or timed out");
                        return;
                    }
                };
                let peer = connection.remote_id().to_string();
                if !wire.admit(&peer) {
                    tracing::warn!(
                        %peer,
                        limit = MAX_CONNECTIONS_PER_PEER,
                        "refusing a connection: this peer already holds the maximum"
                    );
                    connection.close(0u32.into(), b"too many connections");
                    return;
                }
                if let Err(error) = wire.serve_connection(connection).await {
                    tracing::debug!(%error, "iroh connection ended");
                }
                wire.release(&peer);
            });
        }
    }

    async fn serve_private(&self, config: PrivateWireConfig) {
        let mut tasks = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                incoming = self.endpoint.accept() => {
                    let Some(incoming) = incoming else { break };
                    while tasks.try_join_next().is_some() {}
                    if tasks.len() >= config.max_connections {
                        incoming.refuse();
                        continue;
                    }
                    let Ok(slot) = self.connection_slots.clone().try_acquire_owned() else {
                        incoming.refuse();
                        continue;
                    };
                    let wire = self.clone();
                    tasks.spawn(async move {
                        let _slot = slot;
                        let Ok(Ok(connection)) = tokio::time::timeout(
                            config.handshake_timeout, incoming,
                        ).await else { return };
                        let connection = ClosingConnection(connection);
                        let peer = connection.0.remote_id().to_string();
                        let Some(_peer) = wire.admit_private(peer, config.max_connections_per_peer) else {
                            return;
                        };
                        let _ = tokio::time::timeout(
                            config.session_timeout, wire.serve_connection(connection.0.clone()),
                        ).await;
                    });
                }
                _ = tasks.join_next(), if !tasks.is_empty() => {}
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }

    fn admit_private(&self, peer: String, maximum: usize) -> Option<PeerPermit> {
        let mut peers = self.open_per_peer.lock().expect("open per peer");
        let count = peers.entry(peer.clone()).or_insert(0);
        if *count >= maximum {
            return None;
        }
        *count += 1;
        Some(PeerPermit {
            peers: self.open_per_peer.clone(),
            peer,
        })
    }

    fn admit(&self, peer: &str) -> bool {
        let mut open = self.open_per_peer.lock().expect("open per peer");
        let count = open.entry(peer.to_string()).or_insert(0);
        if *count >= MAX_CONNECTIONS_PER_PEER {
            return false;
        }
        *count += 1;
        true
    }

    fn release(&self, peer: &str) {
        let mut open = self.open_per_peer.lock().expect("open per peer");
        if let Some(count) = open.get_mut(peer) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                open.remove(peer);
            }
        }
    }

    async fn hold_at_the_door(
        &self,
        node_id: &str,
        intro: &Introduction,
    ) -> Result<(), EngineError> {
        let body = serde_json::to_string(intro)
            .map_err(|error| EngineError::Consequence(error.to_string()))?;
        store::door::hold(&self.engine.store.pool, node_id, &intro.organ_uid, &body).await?;
        tracing::info!(
            %node_id,
            organ = %intro.organ_uid,
            "a stranger knocked at the front door; held for the owner"
        );
        Ok(())
    }

    async fn private_live_admission(
        &self,
        authenticated_node_id: &str,
    ) -> Result<(String, Option<String>), EngineError> {
        let mut transaction = self.engine.store.pool.begin().await?;
        let peer =
            store::session_access::peer_contact_on(&mut transaction, authenticated_node_id).await?;
        let admission = match peer {
            Some(peer) => {
                if peer.trust == store::session_access::ContactTrust::Blocked {
                    return Err(EngineError::Forbidden("blocked private peer".into()));
                }
                let person = store::session_access::granted_login_on(
                    &mut transaction,
                    &peer.organ_uid,
                    authenticated_node_id,
                )
                .await?
                .map(|authentication| authentication.person_uid().to_string());
                (peer.organ_uid, person)
            }
            None => (format!("node:{authenticated_node_id}"), None),
        };
        transaction.rollback().await?;
        Ok(admission)
    }

    async fn serve_connection(&self, connection: Connection) -> Result<(), EngineError> {
        let peer = connection.remote_id();
        let alpn = connection.alpn().to_vec();
        if let Some(config) = self.private.filter(|config| !config.full_protocols) {
            if alpn != ALPN_LIVE {
                connection.close(0u32.into(), b"unsupported private alpn");
                return Ok(());
            }
            let (organ, granted) = self.private_live_admission(&peer.to_string()).await?;
            if granted.is_none() && !config.accept_credentials {
                connection.close(0u32.into(), b"this Cell does not accept live logins");
                return Ok(());
            }
            let Some(handler) = self.live.lock().expect("live handler").clone() else {
                connection.close(0u32.into(), b"no live session for this organ");
                return Ok(());
            };
            handler.serve(organ, granted, connection).await;
            return Ok(());
        }
        let contact =
            store::organs::contact_by_node_id(&self.engine.store.pool, &peer.to_string()).await?;

        if contact.as_ref().is_some_and(|c| c.trust == "blocked") {
            connection.close(0u32.into(), b"blocked");
            return Ok(());
        }

        if alpn == ALPN_AILUROS {
            return self.serve_ailuros_connection(connection).await;
        }

        if alpn == ALPN_SOCIAL {
            return self.serve_social_connection(connection).await;
        }

        if alpn == crate::blob_sync::ALPN || alpn == crate::blob_sync::DATA_ALPN {
            return self.serve_blob_connection(connection).await;
        }

        let sibling = self.sibling_organ(&peer.to_string()).await;

        let known = sibling.is_some() || contact.as_ref().is_some_and(|c| c.trust == "known");
        let listed = self.roster_names_node(&peer.to_string()).await;
        let identified = sibling.is_some() || contact.is_some() || listed;

        match (alpn.as_slice(), known) {
            (ALPN_SYNC, true) => {}
            (ALPN_SYNC, false) => {
                connection.close(0u32.into(), b"unknown organ");
                return Ok(());
            }
            (ALPN_MAILBOX, _) => {}
            (ALPN_HELLO, _) => {
                if !identified {
                    connection.close(0u32.into(), b"not known");
                    return Ok(());
                }
                if let Ok(mut send) = connection.open_uni().await {
                    let body = serde_json::json!({ "epoch": WIRE_EPOCH }).to_string();
                    let _ = send.write_all(body.as_bytes()).await;
                    let _ = send.finish();
                    connection.closed().await;
                }
                return Ok(());
            }
            (ALPN_LIVE, known) => {
                let organ = contact
                    .as_ref()
                    .map(|c| c.record_uid.clone())
                    .unwrap_or_default();
                let granted = if known {
                    store::logins::person_for_organ(&self.engine.store.pool, &organ).await?
                } else {
                    None
                };
                let granted = match granted {
                    Some(person)
                        if store::people::is_active(&self.engine.store.pool, &person).await? =>
                    {
                        Some(person)
                    }
                    _ => None,
                };
                if granted.is_none() && !self.accept_logins().await {
                    connection.close(0u32.into(), b"this Cell does not accept live logins");
                    return Ok(());
                }
                let Some(handler) = self.live.lock().expect("live handler").clone() else {
                    connection.close(0u32.into(), b"no live session for this organ");
                    return Ok(());
                };
                let session_organ = if organ.is_empty() {
                    format!("node:{peer}")
                } else {
                    organ
                };
                let live_connection = connection.clone();
                tokio::spawn(async move {
                    handler
                        .serve(session_organ, granted, live_connection.clone())
                        .await;
                    live_connection.close(0u32.into(), b"live session ended");
                });
            }
            (ALPN_LOCATION, _) => {}
            (ALPN_THREAD, true) => {}
            (ALPN_THREAD, false) if !identified => {
                let enrolling = store::roster::enrolment_is_open(&self.engine.store.pool)
                    .await
                    .unwrap_or(false);
                if !enrolling && !self.accept_unknown().await {
                    connection.close(0u32.into(), b"not accepting unknown organs");
                    return Ok(());
                }
            }
            (ALPN_THREAD, false) => {}
            _ => {
                connection.close(0u32.into(), b"unsupported alpn");
                return Ok(());
            }
        }

        let was_sibling = sibling.is_some();
        let from_organ = sibling
            .or_else(|| contact.map(|contact| contact.record_uid))
            .unwrap_or_default();

        for _ in 0..MAX_FRAMES_PER_CONNECTION {
            let (mut send, mut recv) = match connection.accept_bi().await {
                Ok(streams) => streams,
                Err(_) => return Ok(()),
            };
            let raw = recv
                .read_to_end(if alpn.as_slice() == ALPN_LOCATION {
                    16 * 1024
                } else {
                    MAX_FRAME_BYTES
                })
                .await
                .map_err(|error| EngineError::Consequence(format!("peer frame: {error}")))?;
            if (was_sibling
                && self.sibling_organ(&peer.to_string()).await.as_deref()
                    != Some(from_organ.as_str()))
                || (listed && !self.roster_names_node(&peer.to_string()).await)
                || store::organs::contact_by_node_id(&self.engine.store.pool, &peer.to_string())
                    .await?
                    .is_some_and(|contact| contact.trust == "blocked")
            {
                connection.close(0u32.into(), b"peer authorization changed");
                return Ok(());
            }
            let response = match serde_json::from_slice::<WireRequest>(&raw) {
                Ok(request)
                    if alpn.as_slice() == ALPN_LOCATION
                        && !matches!(request, WireRequest::Location { .. }) =>
                {
                    WireResponse::Refused {
                        code: "wrong_door".into(),
                        message: "Only location requests are served on this connection".into(),
                    }
                }
                Ok(request)
                    if alpn.as_slice() == ALPN_LIVE
                        && !matches!(
                            request,
                            WireRequest::PushOps { .. } | WireRequest::PushGrantOps { .. }
                        ) =>
                {
                    WireResponse::Refused {
                        code: "wrong_door".into(),
                        message: "that verb is not served on the live connection".into(),
                    }
                }
                Ok(request)
                    if (alpn.as_slice() == ALPN_MAILBOX)
                        != matches!(
                            request,
                            WireRequest::MailboxDeposit { .. }
                                | WireRequest::MailboxWaiting { .. }
                                | WireRequest::MailboxCollect { .. }
                                | WireRequest::MailboxCollected { .. }
                                | WireRequest::MailboxAskToCarry { .. }
                                | WireRequest::MailboxRedeemInvite { .. }
                                | WireRequest::MailboxExpiries
                                | WireRequest::MailboxExpiriesHeard { .. }
                        ) =>
                {
                    WireResponse::Refused {
                        code: "wrong_door".into(),
                        message: "that verb is not served on this door".into(),
                    }
                }
                Ok(request)
                    if !known
                        && alpn.as_slice() != ALPN_MAILBOX
                        && alpn.as_slice() != ALPN_LOCATION
                        && !matches!(
                            request,
                            WireRequest::Introduction
                                | WireRequest::Presence { .. }
                                | WireRequest::Introduce { .. }
                                | WireRequest::Enrol { .. }
                                | WireRequest::OfferGrant { .. }
                                | WireRequest::GroupOffer { .. }
                                | WireRequest::GroupAccept { .. }
                                | WireRequest::Call { .. }
                                | WireRequest::AcceptGrant { .. }
                                | WireRequest::DeclineGrant { .. }
                                | WireRequest::PushGrantOps { .. }
                                | WireRequest::FetchGrantOpsSince { .. }
                                | WireRequest::FetchReference { .. }
                                | WireRequest::PublishSealingKey { .. }
                        ) =>
                {
                    WireResponse::Refused {
                        code: "not_known".into(),
                        message: "only introduction is served to an unknown Organ".into(),
                    }
                }
                Ok(request) => {
                    if let WireRequest::Introduce { intro }
                    | WireRequest::OfferGrant { intro, .. } = &request
                    {
                        if from_organ.is_empty() && !self.may_represent().await {
                            let response =
                                match self.hold_at_the_door(&peer.to_string(), intro).await {
                                    Ok(()) => WireResponse::Refused {
                                        code: "held_for_owner".into(),
                                        message: "this is a front door and cannot accept for its \
                                              owner. Your request is waiting for one of their \
                                              devices to see it."
                                            .into(),
                                    },
                                    Err(error) => WireResponse::Error {
                                        message: error.to_string(),
                                    },
                                };
                            let bytes = serde_json::to_vec(&response)
                                .map_err(|error| EngineError::Consequence(error.to_string()))?;
                            send.write_all(&bytes).await.map_err(|error| {
                                EngineError::Consequence(format!("peer write: {error}"))
                            })?;
                            send.finish().map_err(|error| {
                                EngineError::Consequence(format!("peer finish: {error}"))
                            })?;
                            continue;
                        }
                    }
                    let authenticated = match &request {
                        WireRequest::OfferGrant { intro, .. }
                        | WireRequest::Introduce { intro }
                            if from_organ.is_empty() =>
                        {
                            match self.bind_unknown_peer(&peer.to_string(), intro).await {
                                Ok(organ_uid) => organ_uid,
                                Err(error) => {
                                    let response = WireResponse::Refused {
                                        code: "identity_binding_refused".into(),
                                        message: error.to_string(),
                                    };
                                    let bytes = serde_json::to_vec(&response).map_err(|error| {
                                        EngineError::Consequence(error.to_string())
                                    })?;
                                    send.write_all(&bytes).await.map_err(|error| {
                                        EngineError::Consequence(format!("peer write: {error}"))
                                    })?;
                                    send.finish().map_err(|error| {
                                        EngineError::Consequence(format!("peer finish: {error}"))
                                    })?;
                                    continue;
                                }
                            }
                        }
                        _ => from_organ.clone(),
                    };
                    self.handle(&authenticated, &peer.to_string(), request)
                        .await
                }
                Err(error) => WireResponse::Error {
                    message: format!("unreadable request: {error}"),
                },
            };
            let bytes = serde_json::to_vec(&response)
                .map_err(|error| EngineError::Consequence(error.to_string()))?;
            send.write_all(&bytes)
                .await
                .map_err(|error| EngineError::Consequence(format!("peer write: {error}")))?;
            send.finish()
                .map_err(|error| EngineError::Consequence(format!("peer finish: {error}")))?;
        }
        connection.close(0u32.into(), b"frame budget spent");
        Ok(())
    }
}
