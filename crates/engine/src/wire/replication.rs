// non-lean-yet

//! Synchronization, catch-up, and remote roster reconciliation.

use std::collections::HashMap;

use iroh::{EndpointAddr, EndpointId, endpoint::Connection};

use super::{ALPN_THREAD, AuditAgreement, DIAL_TIMEOUT, Wire, WireRequest, WireResponse};
use crate::{
    error::EngineError,
    sync::{Delivery, OpBatch},
};

impl Wire {
    pub async fn sync_once(&self) -> Result<usize, EngineError> {
        let _sync = self.sync_lock.lock().await;
        self.engine.sweep_calls().await?;
        if let Err(error) = self.sync_groups().await {
            tracing::debug!(%error, "group membership was not exchanged this pass");
        }
        if let Err(error) = self.reconcile_pending().await {
            tracing::debug!(%error, "pending introductions not reconciled this pass");
        }
        let moved = self.sync_record_moves().await?;
        let pushed = self.push_outbox().await? + moved;
        let mut pulled = self.pull_catch_up().await? + self.pull_siblings().await?;
        match self.collect_own_mail().await {
            Ok(count) => pulled += count,
            Err(error) => tracing::debug!(%error, "mail not collected this pass"),
        }
        if let Err(error) = self.publish_own_sealing_key().await {
            tracing::debug!(%error, "this Cell's mail key was not published this pass");
        }
        if let Err(error) = self.hear_about_expired_mail().await {
            tracing::debug!(%error, "carriers not asked about expired mail this pass");
        }
        if let Err(error) = self.engine.sweep_mailbox().await {
            tracing::debug!(%error, "the mailbox was not swept this pass");
        }
        if let Err(error) = self.engine.compact_stale_docs().await {
            tracing::debug!(%error, "collab compaction skipped this pass");
        }
        match self.engine.prune_op_log(false).await {
            Ok(report) if report.removed > 0 => {
                tracing::debug!(
                    removed = report.removed,
                    retained = report.retained,
                    floor = report.floor,
                    "pruned superseded ops"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(%error, "prune skipped this pass"),
        }
        Ok(pushed + pulled)
    }

    pub async fn reconcile_pending(&self) -> Result<usize, EngineError> {
        let pool = &self.engine.store.pool;
        let mut reconciled = 0;
        for contact in store::organs::pending_introductions(pool).await? {
            let Some(node_id) = contact.node_id.clone() else {
                continue;
            };
            let Ok(id) = node_id.parse::<EndpointId>() else {
                continue;
            };
            let response = match tokio::time::timeout(
                DIAL_TIMEOUT,
                self.request(
                    EndpointAddr::new(id),
                    ALPN_THREAD,
                    &WireRequest::Introduction,
                ),
            )
            .await
            {
                Ok(Ok(response)) => response,
                Ok(Err(_)) | Err(_) => continue,
            };
            let WireResponse::Introduction { intro } = response else {
                continue;
            };

            let pasted_root = crate::trust::keys_of(&self.engine.store, &contact.record_uid)
                .await?
                .into_iter()
                .find(|(key_id, _)| key_id == crate::roster::ROOT_KEY_ID)
                .map(|(_, key)| key);
            let offered_root = intro
                .keys
                .iter()
                .find(|(key_id, _)| key_id == crate::roster::ROOT_KEY_ID)
                .map(|(_, key)| key.clone());
            if let (Some(pasted), Some(offered)) = (&pasted_root, &offered_root) {
                if pasted != offered {
                    tracing::warn!(
                        %node_id,
                        "the Organ at this address presents a different root key than the \
                         code did; leaving the contact pending"
                    );
                    continue;
                }
            }

            let name = contact.head.clone();
            store::organs::forget_contact(pool, &contact.record_uid).await?;
            self.engine
                .adopt_introduction(&intro, contact.proximity)
                .await?;
            if !name.trim().is_empty() {
                store::records::set_text(pool, &intro.organ_uid, Some(name.trim()), None).await?;
            }
            if let Some(root) = pasted_root {
                crate::trust::adopt_key(
                    &self.engine.store,
                    &intro.organ_uid,
                    crate::roster::ROOT_KEY_ID,
                    &root,
                )
                .await?;
            }
            store::organs::set_node_id(pool, &intro.organ_uid, Some(&node_id)).await?;
            store::organs::set_trust(pool, &intro.organ_uid, "known").await?;
            store::organs::set_pending_introduction(pool, &intro.organ_uid, false).await?;
            tracing::info!(
                uid = %intro.organ_uid,
                "a contact added by code is now filed under the uid they declare"
            );
            reconciled += 1;
        }
        Ok(reconciled)
    }

    pub async fn push_outbox(&self) -> Result<usize, EngineError> {
        let connections: std::sync::Arc<tokio::sync::Mutex<HashMap<String, Option<Connection>>>> =
            Default::default();
        let wire = self.clone();
        self.engine
            .drain_outbox(move |contact, root, batch| {
                let wire = wire.clone();
                let connections = connections.clone();
                async move {
                    if contact.delivery() == store::organs::Delivery::Mailbox {
                        return wire.mail_now(&contact, root.as_deref(), &batch).await;
                    }
                    let request = match root {
                        Some(ref root) => WireRequest::PushGrantOps {
                            root: root.clone(),
                            batch: batch.clone(),
                        },
                        None => WireRequest::PushOps {
                            batch: batch.clone(),
                        },
                    };
                    let live = wire
                        .live_connections
                        .lock()
                        .expect("live connections")
                        .get(&contact.record_uid)
                        .cloned();
                    let conversation = wire.is_conversation_delivery(root.as_deref()).await;
                    if conversation {
                        return match wire
                            .try_direct_conversation(&contact, &request, live, &connections)
                            .await
                        {
                            Ok(Some(WireResponse::BatchSaved { complete: true, .. })) => {
                                Delivery::Sent
                            }
                            Ok(Some(other)) => Delivery::Failed(format!("{other:?}")),
                            Ok(None) | Err(_)
                                if contact.delivery() == store::organs::Delivery::Direct =>
                            {
                                Delivery::Failed(format!("{} unreachable", contact.record_uid))
                            }
                            Ok(None) | Err(_) => {
                                wire.mail_now(&contact, root.as_deref(), &batch).await
                            }
                        };
                    }
                    if let Some(live) = live {
                        match wire.exchange(&live, &request).await {
                            Ok(WireResponse::BatchSaved { complete: true, .. }) => {
                                return Delivery::Sent;
                            }
                            Ok(other) => return Delivery::Failed(format!("{other:?}")),
                            Err(error) => tracing::debug!(
                                contact = %contact.record_uid,
                                %error,
                                "live delta path unavailable; falling back to sync"
                            ),
                        }
                    }
                    let mut open = connections.lock().await;
                    let entry = match open.get(&contact.record_uid) {
                        Some(existing) => existing.clone(),
                        None => {
                            let dialed = wire.dial(&contact).await;
                            open.insert(contact.record_uid.clone(), dialed.clone());
                            dialed
                        }
                    };
                    let Some(connection) = entry else {
                        drop(open);
                        if contact.delivery() == store::organs::Delivery::Direct {
                            return Delivery::Failed(format!("{} unreachable", contact.record_uid));
                        }
                        return wire.mail_if_due(&contact, root.as_deref(), &batch).await;
                    };
                    match wire.exchange(&connection, &request).await {
                        Ok(WireResponse::BatchSaved { complete: true, .. }) => Delivery::Sent,
                        Ok(other) => Delivery::Failed(format!("{other:?}")),
                        Err(error) => Delivery::Failed(error.to_string()),
                    }
                }
            })
            .await
    }

    pub async fn audit_against(
        &self,
        contact_organ: &str,
    ) -> Result<Option<AuditAgreement>, EngineError> {
        let pool = &self.engine.store.pool;
        let Some(contact) = store::organs::contact(pool, contact_organ).await? else {
            return Ok(None);
        };
        let Some(connection) = self.dial(&contact).await else {
            return Ok(None);
        };
        let Some(local) = store::organs::local(pool).await? else {
            return Ok(None);
        };
        let theirs = match self
            .exchange(
                &connection,
                &WireRequest::FetchVector {
                    organ_uid: local.uid.clone(),
                },
            )
            .await?
        {
            WireResponse::Vector { vector } => vector,
            WireResponse::Refused { code, message } => {
                return Err(EngineError::Consequence(format!("{code}: {message}")));
            }
            other => {
                return Err(EngineError::Consequence(format!(
                    "unexpected answer to an audit: {other:?}"
                )));
            }
        };
        let ours = store::sync_ops::version_vector_for_organ(pool, &local.uid).await?;
        let they_lack =
            store::sync_ops::ops_missing_from_vector(pool, &local.uid, &theirs, i64::MAX)
                .await?
                .len();
        let unknown_cells = ours
            .iter()
            .filter(|entry| {
                !theirs
                    .iter()
                    .any(|held| held.actor_cell == entry.actor_cell)
            })
            .count();
        Ok(Some(AuditAgreement {
            contact_organ: contact_organ.to_string(),
            they_lack,
            unknown_cells,
        }))
    }

    pub(super) async fn collect_door_requests(
        &self,
        connection: &Connection,
    ) -> Result<(), EngineError> {
        if !self.may_represent().await {
            return Ok(());
        }
        let response = self
            .exchange(connection, &WireRequest::FetchDoorRequests { limit: 50 })
            .await?;
        let WireResponse::DoorRequests { requests } = response else {
            return Ok(());
        };
        let mut taken: Vec<String> = Vec::new();
        for held in requests {
            match self.bind_unknown_peer(&held.node_id, &held.intro).await {
                Ok(organ_uid) => {
                    tracing::info!(
                        organ = %organ_uid,
                        "took a request from the front door; it is waiting for a decision"
                    );
                    taken.push(held.uid);
                }
                Err(error) => {
                    if let Err(error) = store::organs::quarantine(
                        &self.engine.store.pool,
                        &held.intro.organ_uid,
                        "front door: introduction refused",
                        &serde_json::to_string(&held.intro).unwrap_or_default(),
                    )
                    .await
                    {
                        tracing::warn!(%error, "could not quarantine a refused door request");
                    }
                    tracing::warn!(%error, "a held request could not be bound; quarantined");
                    taken.push(held.uid);
                }
            }
        }
        if !taken.is_empty() {
            self.exchange(
                connection,
                &WireRequest::ReleaseDoorRequests { uids: taken },
            )
            .await?;
        }
        Ok(())
    }

    pub(super) async fn refresh_organ_access(&self, connection: &Connection, organ: &str) {
        if let Ok(WireResponse::OrganAccessStatus { status }) = self
            .exchange(connection, &WireRequest::OrganAccessStatus)
            .await
        {
            match store::organ_access::observe(&self.engine.store.pool, organ, &status).await {
                Ok(()) => self.engine.notify_query_changed(),
                Err(error) => tracing::debug!(%error, "Organ access refresh refused"),
            }
        }
    }

    pub(super) async fn refresh_roster(&self, connection: &Connection, organ_uid: &str) {
        if let Ok(WireResponse::Roster {
            roster: Some(roster),
        }) = self.exchange(connection, &WireRequest::FetchRoster).await
        {
            self.adopt_fetched_roster(&roster, organ_uid).await;
        }
    }

    pub(super) async fn adopt_fetched_roster(
        &self,
        roster: &crate::roster::SignedRoster,
        organ_uid: &str,
    ) {
        if roster.roster.organ_uid != organ_uid {
            return;
        }
        match self.engine.adopt_roster(roster).await {
            Ok(crate::roster::RosterOutcome::Refused) => {
                tracing::warn!(
                    organ = %organ_uid,
                    "REFUSED a roster that does not chain from a held key"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(%error, "roster refresh failed"),
        }
    }

    async fn pull_catch_up(&self) -> Result<usize, EngineError> {
        let pool = &self.engine.store.pool;
        let mut pulled = 0usize;
        for contact in store::organs::contacts(pool).await? {
            if contact.trust == "blocked" {
                continue;
            }
            let Some(connection) = self.dial(&contact).await else {
                continue;
            };
            self.refresh_roster(&connection, &contact.record_uid).await;
            if contact.trust == "known" {
                self.refresh_organ_access(&connection, &contact.record_uid)
                    .await;
            }
            if contact.trust == "known" && contact.sync_in {
                let vector =
                    store::sync_ops::version_vector_for_organ(pool, &contact.record_uid).await?;
                let request = WireRequest::FetchOpsSince { vector, limit: 500 };
                if let Ok(WireResponse::Ops { ops, head, .. }) =
                    self.exchange(&connection, &request).await
                {
                    let batch = OpBatch {
                        from_organ: contact.record_uid.clone(),
                        ops,
                    };
                    if !batch.ops.is_empty() && self.engine.import_op_batch(&batch).await.is_ok() {
                        store::organs::set_last_synced_seq(pool, &contact.record_uid, head).await?;
                        pulled += 1;
                    }
                }
            }
            for root in store::replica::roots_for_contact(pool, &contact.record_uid).await? {
                let vector = store::sync_ops::version_vector_for_root(pool, &root).await?;
                let request = WireRequest::FetchGrantOpsSince {
                    root: root.clone(),
                    vector,
                    limit: 500,
                };
                if let Ok(WireResponse::Ops { ops, .. }) =
                    self.exchange(&connection, &request).await
                {
                    let batch = OpBatch {
                        from_organ: contact.record_uid.clone(),
                        ops,
                    };
                    if !batch.ops.is_empty()
                        && self.engine.import_grant_batch(&root, &batch).await.is_ok()
                    {
                        pulled += 1;
                    }
                }
            }
            if contact.trust != "known" {
                continue;
            }
            if let Ok(WireResponse::Revocations { certs }) = self
                .exchange(&connection, &WireRequest::FetchRevocations)
                .await
            {
                for cert in certs {
                    if cert.organ_uid != contact.record_uid {
                        continue;
                    }
                    match self
                        .engine
                        .adopt_revocation(&cert.organ_uid, &cert.revoked_key, &cert.signature)
                        .await
                    {
                        Ok(true) => tracing::warn!(
                            organ = %cert.organ_uid,
                            "adopted a key revocation certificate"
                        ),
                        _ => {}
                    }
                }
            }
            if let Ok(WireResponse::Successions { certs }) = self
                .exchange(&connection, &WireRequest::FetchSuccessions)
                .await
            {
                for cert in certs {
                    if cert.organ_uid != contact.record_uid {
                        continue;
                    }
                    match self
                        .engine
                        .adopt_succession(
                            &cert.organ_uid,
                            &cert.old_key,
                            &cert.new_key,
                            &cert.created_at,
                            &cert.signature,
                        )
                        .await
                    {
                        Ok(true) => tracing::warn!(
                            organ = %cert.organ_uid,
                            "adopted a key succession: this Organ rotated its identity key"
                        ),
                        Ok(false) => tracing::warn!(
                            organ = %cert.organ_uid,
                            old = %cert.old_key,
                            "REFUSED a key succession that does not chain from a key we hold"
                        ),
                        Err(error) => tracing::warn!(%error, "succession could not be evaluated"),
                    }
                }
            }
            connection.close(0u32.into(), b"done");
        }
        Ok(pulled)
    }
}
