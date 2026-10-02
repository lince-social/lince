use super::*;
use nucleus::social::gossip::{Offer, Payload};
use store::sqlx::Row;

impl Engine {
    pub async fn social_gossip_once(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let consent = self.social_gossip_settings().await?;
        if !consent.enabled {
            return Ok(0);
        }
        let now = nucleus::execution::now().timestamp();
        let settings = self.social_settings().await?;
        let mut peers = Vec::new();
        for contact in store::organs::contacts(&self.store.pool).await? {
            if contact.trust == "known"
                && contact.unreachable_since.is_none()
                && consent
                    .peers
                    .iter()
                    .any(|peer| peer.organ == contact.record_uid && peer.send)
                && let Some(endpoint) = contact
                    .node_id
                    .filter(|id| id.parse::<iroh::EndpointId>().is_ok())
            {
                peers.push((contact.record_uid, endpoint));
            }
        }
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_gossip_item WHERE expires_at<=?")
            .bind(now - 300)
            .execute(&mut *tx)
            .await?;
        store::sqlx::query("DELETE FROM social_gossip_seen WHERE expires_at<=?")
            .bind(now - 300)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        let after: String =
            store::sqlx::query_scalar("SELECT after FROM social_gossip_scan WHERE id=1")
                .fetch_one(&self.store.pool)
                .await?;
        let mut rows=store::sqlx::query("SELECT id,body,hash,state FROM social_document d WHERE kind='snippet' AND state='withdrawn' AND expires_at>? AND json_extract(body,'$.redistribute')=1 AND NOT EXISTS(SELECT 1 FROM social_gossip_item g WHERE g.kind='snippet' AND g.document_hash=d.hash) ORDER BY id LIMIT 8").bind(now).fetch_all(&self.store.pool).await?;
        let page=store::sqlx::query("SELECT id,body,hash,state FROM social_document WHERE kind='snippet' AND expires_at>? AND json_extract(body,'$.redistribute')=1 AND id>? ORDER BY id LIMIT 32").bind(now).bind(after).fetch_all(&self.store.pool).await?;
        let cursor = if page.len() == 32 {
            page.last()
                .map(|row| row.get::<String, _>("id"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        store::sqlx::query("UPDATE social_gossip_scan SET after=? WHERE id=1")
            .bind(cursor)
            .execute(&self.store.pool)
            .await?;
        rows.extend(page);
        if !peers.is_empty() {
            let waiting:Vec<(String,String,String)>=store::sqlx::query_as("SELECT kind,document_hash,body FROM social_gossip_item WHERE assigned=0 AND expires_at>? ORDER BY control DESC,hash LIMIT 8").bind(now).fetch_all(&self.store.pool).await?;
            for (kind, identity, body) in waiting {
                let payload: Payload = serde_json::from_str(&body)?;
                if gossip::validate_payload(&payload, now).is_ok() {
                    self.social_queue_gossip(&payload, &kind, &identity, &peers, &settings)
                        .await?;
                }
            }
            for row in rows {
                let document: Snippet = serde_json::from_str(&row.get::<String, _>("body"))?;
                let mut payloads = Vec::new();
                let removed: bool = store::sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM social_listing_removal WHERE post=?)",
                )
                .bind(&document.id)
                .fetch_one(&self.store.pool)
                .await?;
                if !matches!(
                    row.get::<String, _>("state").as_str(),
                    "revoked" | "conflict"
                ) && (!removed || document.state == PostState::Withdrawn)
                {
                    payloads.push((
                        Payload::Snippet {
                            document: Box::new(document.clone()),
                        },
                        "snippet",
                        row.get::<String, _>("hash"),
                    ));
                }
                if let Some(authority) = &document.profile
                    && let Some(body) = store::sqlx::query_scalar::<_, String>(
                        "SELECT body FROM social_profile_authority WHERE organ=?",
                    )
                    .bind(&authority.organ)
                    .fetch_optional(&self.store.pool)
                    .await?
                {
                    let control: Delegation = serde_json::from_str(&body)?;
                    let identity = document_hash("profile-authority", &control)?;
                    payloads.push((
                        Payload::ProfileAuthority {
                            proof: Box::new(document.clone()),
                            authority: Box::new(control),
                        },
                        "profile-authority",
                        identity,
                    ));
                }
                if let Some(authority) = &document.anonymous
                    && let Some(body) = store::sqlx::query_scalar::<_, String>(
                        "SELECT body FROM social_posting_authority WHERE owner=?",
                    )
                    .bind(&authority.owner_key)
                    .fetch_optional(&self.store.pool)
                    .await?
                {
                    let control: PostingAuthority = serde_json::from_str(&body)?;
                    let identity = document_hash("posting-authority", &control)?;
                    payloads.push((
                        Payload::PostingAuthority {
                            proof: Box::new(document.clone()),
                            authority: Box::new(control),
                        },
                        "posting-authority",
                        identity,
                    ));
                }
                payloads.sort_by_key(|(payload, _, _)| !payload.control());
                for (payload, kind, identity) in payloads {
                    if gossip::validate_payload(&payload, now).is_err() {
                        continue;
                    }
                    if let Err(error) = self
                        .social_queue_gossip(&payload, kind, &identity, &peers, &settings)
                        .await
                    {
                        store::sqlx::query("UPDATE social_gossip_scan SET error=? WHERE id=1")
                            .bind(error.to_string().chars().take(500).collect::<String>())
                            .execute(&self.store.pool)
                            .await?;
                        if payload.control() {
                            return Err(error);
                        }
                    }
                }
            }
        }
        let network = self.social_network()?;
        let jobs=store::sqlx::query("SELECT f.hash,f.peer,f.contact,f.attempts,i.kind,i.document_hash,i.body,i.expires_at FROM social_gossip_forward f JOIN social_gossip_item i ON i.hash=f.hash WHERE f.state='pending' AND f.next_attempt<=? AND i.expires_at>? ORDER BY i.control DESC,f.next_attempt,f.hash,f.peer LIMIT 3").bind(now).bind(now).fetch_all(&self.store.pool).await?;
        let mut forwarded = 0;
        for row in jobs {
            let hash: String = row.get("hash");
            let peer: String = row.get("peer");
            let contact: String = row.get("contact");
            let payload: Payload = serde_json::from_str(&row.get::<String, _>("body"))?;
            let current = self.social_gossip_peer(&peer, true).await;
            if !current.as_ref().is_ok_and(|(_, uid)| uid == &contact) {
                store::sqlx::query("UPDATE social_gossip_forward SET next_attempt=?,error=? WHERE hash=? AND peer=? AND state='pending'")
                    .bind(now+30).bind("Waiting for current contact consent, pinned endpoint and reachability").bind(&hash).bind(&peer).execute(&self.store.pool).await?;
                continue;
            }
            if !self
                .social_gossip_payload_current(&payload, &row.get::<String, _>("document_hash"))
                .await?
            {
                store::sqlx::query("UPDATE social_gossip_forward SET state='cancelled',error=? WHERE hash=? AND peer=?")
                    .bind("A newer announcement or authority superseded this queued copy").bind(&hash).bind(&peer).execute(&self.store.pool).await?;
                continue;
            }
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.social_forward_gossip(
                    &network,
                    &peer,
                    &contact,
                    &hash,
                    &row.get::<String, _>("document_hash"),
                    &payload,
                    &settings,
                    now,
                ),
            )
            .await;
            match result {
                Ok(Ok(())) => {
                    store::sqlx::query("UPDATE social_gossip_forward SET state='accepted',error=NULL WHERE hash=? AND peer=? AND state='pending'")
                        .bind(&hash).bind(&peer).execute(&self.store.pool).await?;
                    forwarded += 1;
                }
                result => {
                    let error = match result {
                        Ok(Err(error)) => error.to_string(),
                        _ => "The contact did not complete this public exchange in time".into(),
                    };
                    let attempts: i64 = row.get("attempts");
                    let delay = 5i64.saturating_mul(1i64 << attempts.min(9)).min(3600);
                    store::sqlx::query("UPDATE social_gossip_forward SET attempts=attempts+1,next_attempt=?,error=? WHERE hash=? AND peer=? AND state='pending'")
                        .bind(now+delay).bind(error.chars().take(500).collect::<String>()).bind(&hash).bind(&peer).execute(&self.store.pool).await?;
                }
            }
        }
        if forwarded > 0 {
            self.notify_query_changed();
        }
        Ok(forwarded)
    }

    async fn social_queue_gossip(
        &self,
        payload: &Payload,
        kind: &str,
        identity: &str,
        peers: &[(String, String)],
        settings: &ServiceSettings,
    ) -> Result<(), EngineError> {
        let mut tx = self.social_write_tx().await?;
        gossip_store::queue_on(&mut tx, payload, kind, identity, peers, settings).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn social_gossip_payload_current(
        &self,
        payload: &Payload,
        identity: &str,
    ) -> Result<bool, EngineError> {
        let current = match payload {
            Payload::Snippet { document } => {
                let hash:Option<String>=store::sqlx::query_scalar("SELECT hash FROM social_document WHERE kind='snippet' AND id=? AND state NOT IN ('revoked','conflict') AND (state='withdrawn' OR NOT EXISTS(SELECT 1 FROM social_listing_removal WHERE post=social_document.id)) AND json_extract(body,'$.redistribute')=1")
                    .bind(&document.id).fetch_optional(&self.store.pool).await?;
                hash
            }
            Payload::ProfileAuthority { authority, .. } => {
                let body: Option<String> = store::sqlx::query_scalar(
                    "SELECT body FROM social_profile_authority WHERE organ=?",
                )
                .bind(&authority.organ)
                .fetch_optional(&self.store.pool)
                .await?;
                body.map(|body| {
                    serde_json::from_str::<Delegation>(&body)
                        .map_err(EngineError::from)
                        .and_then(|doc| document_hash("profile-authority", &doc))
                })
                .transpose()?
            }
            Payload::PostingAuthority { authority, .. } => {
                let body: Option<String> = store::sqlx::query_scalar(
                    "SELECT body FROM social_posting_authority WHERE owner=?",
                )
                .bind(&authority.owner_key)
                .fetch_optional(&self.store.pool)
                .await?;
                body.map(|body| {
                    serde_json::from_str::<PostingAuthority>(&body)
                        .map_err(EngineError::from)
                        .and_then(|doc| document_hash("posting-authority", &doc))
                })
                .transpose()?
            }
        };
        Ok(current.as_deref() == Some(identity))
    }

    async fn social_forward_gossip(
        &self,
        network: &std::sync::Arc<dyn Network>,
        peer: &str,
        contact: &str,
        hash: &str,
        identity: &str,
        payload: &Payload,
        settings: &ServiceSettings,
        now: i64,
    ) -> Result<(), EngineError> {
        gossip::validate_payload(payload, now)?;
        let offer = PublicRequest::GossipOffer {
            offer: Offer {
                post: payload.proof().id.clone(),
                hash: hash.into(),
                expires_at: payload.expires_at(),
            },
        };
        self.social_gossip_outgoing(peer, contact, &offer, settings, now)
            .await?;
        let response = network.request(peer, offer).await?;
        store::social::spend(
            &self.store.pool,
            peer,
            "in",
            serde_json::to_vec(&response)?.len(),
            settings.incoming_bytes_per_minute,
            now,
        )
        .await?;
        if serde_json::to_vec(&response)?.len() > 1024
            || response["service"] != peer
            || response["hash"] != hash
            || !response["missing"].is_boolean()
        {
            return Err(invalid("Invalid contact gossip inventory response"));
        }
        if response["missing"] == false {
            return Ok(());
        }
        if self.social_gossip_peer(peer, true).await?.1 != contact {
            return Err(invalid("Contact consent changed before public forwarding"));
        }
        if !self
            .social_gossip_payload_current(payload, identity)
            .await?
        {
            return Err(invalid(
                "A newer announcement or authority superseded this inventory offer",
            ));
        }
        let deliver = PublicRequest::GossipDeliver {
            payload: Box::new(payload.clone()),
        };
        self.social_gossip_outgoing(peer, contact, &deliver, settings, now)
            .await?;
        let receipt = network.request(peer, deliver).await?;
        store::social::spend(
            &self.store.pool,
            peer,
            "in",
            serde_json::to_vec(&receipt)?.len(),
            settings.incoming_bytes_per_minute,
            now,
        )
        .await?;
        if serde_json::to_vec(&receipt)?.len() > 1024
            || receipt["service"] != peer
            || receipt["hash"] != hash
            || receipt["accepted"] != true
            || receipt["expires_at"].as_i64() != Some(payload.expires_at())
        {
            return Err(invalid("Invalid public forwarding confirmation"));
        }
        Ok(())
    }

    async fn social_gossip_outgoing(
        &self,
        peer: &str,
        contact: &str,
        request: &PublicRequest,
        settings: &ServiceSettings,
        now: i64,
    ) -> Result<(), EngineError> {
        let mut tx = self.social_write_tx().await?;
        self.social_require_local_write_on(&mut tx).await?;
        self.social_gossip_peer_on(&mut tx, peer, contact, true)
            .await?;
        store::social::spend_on(
            &mut tx,
            peer,
            "out",
            serde_json::to_vec(request)?.len(),
            settings.outgoing_bytes_per_minute,
            now,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
