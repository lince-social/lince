use super::*;
use nucleus::social::gossip::{self as contract, ContactConsent, Offer, Payload, Settings};

pub(super) fn payload_hash(payload: &Payload) -> Result<String, EngineError> {
    document_hash("gossip-payload", payload)
}

pub(super) fn payload_identity(payload: &Payload) -> Result<(&'static str, String), EngineError> {
    match payload {
        Payload::Snippet { document } => Ok(("snippet", document_hash("snippet", document)?)),
        Payload::ProfileAuthority { authority, .. } => Ok((
            "profile-authority",
            document_hash("profile-authority", authority)?,
        )),
        Payload::PostingAuthority { authority, .. } => Ok((
            "posting-authority",
            document_hash("posting-authority", authority)?,
        )),
    }
}

pub(super) fn validate_payload(payload: &Payload, now: i64) -> Result<(), EngineError> {
    let proof = payload.proof();
    if serde_json::to_vec(payload)?.len() > contract::MAX_PAYLOAD_BYTES
        || !proof.redistribute
        || proof.destinations.is_empty()
        || proof.issued_at > now + 300
    {
        return Err(invalid(
            "Gossip accepts only bounded signed announcements that allow redistribution",
        ));
    }
    match payload {
        Payload::Snippet { document } => validate_snippet(document, now)?,
        Payload::ProfileAuthority { proof, authority } => {
            validate_snippet(proof, proof.issued_at)?;
            profile::validate_delegation(authority, now)?;
            let original = proof
                .profile
                .as_ref()
                .ok_or_else(|| invalid("Unrelated public profile control"))?;
            if authority.organ != original.organ
                || authority.generation.parse::<i64>().unwrap_or(0)
                    < original.generation.parse::<i64>().unwrap_or(0)
                || authority.root_key != original.root_key
                    && !authority
                        .successions
                        .iter()
                        .any(|edge| edge.old_key == original.root_key)
            {
                return Err(invalid(
                    "Public profile control differs from its redistribution evidence",
                ));
            }
        }
        Payload::PostingAuthority { proof, authority } => {
            validate_snippet(proof, proof.issued_at)?;
            posting::validate_authority(authority, now)?;
            let original = proof
                .anonymous
                .as_ref()
                .ok_or_else(|| invalid("Unrelated anonymous posting control"))?;
            if authority.owner_key != original.owner_key
                || authority.generation.parse::<i64>().unwrap_or(0)
                    < original.generation.parse::<i64>().unwrap_or(0)
            {
                return Err(invalid(
                    "Anonymous control differs from its redistribution evidence",
                ));
            }
        }
    }
    Ok(())
}

impl Engine {
    pub(super) async fn social_gossip_settings(&self) -> Result<Settings, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let settings: Settings =
            store::records::get_extension(&self.store.pool, &cell.uid, contract::NAMESPACE)
                .await?
                .map(serde_json::from_value)
                .transpose()?
                .unwrap_or_default();
        if settings.peers.len() > contract::MAX_PEERS {
            return Err(invalid("The contact gossip settings exceed their bound"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for peer in &settings.peers {
            if !nucleus::valid_uid(&peer.organ, "r") || !seen.insert(&peer.organ) {
                return Err(invalid("Choose each valid known contact once"));
            }
        }
        Ok(settings)
    }

    pub(super) async fn social_configure_gossip(
        &self,
        enabled: Option<bool>,
        choice: Option<ContactConsent>,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let mut tx = self.social_write_tx().await?;
        let body: Option<String> = store::sqlx::query_scalar(
            "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
        )
        .bind(&cell.uid)
        .bind(contract::NAMESPACE)
        .fetch_optional(&mut *tx)
        .await?;
        let mut settings: Settings = body
            .map(|body| serde_json::from_str(&body))
            .transpose()?
            .unwrap_or_default();
        if let Some(enabled) = enabled {
            settings.enabled = enabled;
        }
        if let Some(choice) = choice {
            if !nucleus::valid_uid(&choice.organ, "r") {
                return Err(invalid("Choose a valid known Organ contact"));
            }
            let known:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organ_contact c JOIN record r ON r.uid=c.record_uid WHERE c.record_uid=? AND c.trust='known' AND r.deleted_at IS NULL)")
                .bind(&choice.organ).fetch_one(&mut *tx).await?;
            if (choice.send || choice.receive) && !known {
                return Err(invalid(
                    "Gossip requires a known contact and separate consent",
                ));
            }
            settings.peers.retain(|peer| peer.organ != choice.organ);
            if choice.send || choice.receive {
                settings.peers.push(choice);
            }
        }
        if settings.peers.len() > contract::MAX_PEERS {
            return Err(invalid("Choose at most thirty-two gossip contacts"));
        }
        settings.peers.sort_by(|a, b| a.organ.cmp(&b.organ));
        store::records::set_extension_on(
            &mut tx,
            &cell.uid,
            contract::NAMESPACE,
            &serde_json::to_value(&settings)?,
        )
        .await?;
        tx.commit().await?;
        self.notify_query_changed();
        self.social_gossip_view(actor).await
    }

    pub(super) async fn social_gossip_view(
        &self,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let settings = self.social_gossip_settings().await?;
        let can_manage = self.require_permission(actor, "organ:update").await.is_ok()
            && self.social_require_local_write().await.is_ok();
        let mut contacts = Vec::new();
        let mut endpoints = std::collections::BTreeSet::new();
        let mut known = store::organs::contacts(&self.store.pool).await?;
        known.sort_by_key(|contact| {
            (
                !settings
                    .peers
                    .iter()
                    .any(|peer| peer.organ == contact.record_uid),
                contact.record_uid.clone(),
            )
        });
        for contact in known {
            let chosen = settings
                .peers
                .iter()
                .find(|peer| peer.organ == contact.record_uid);
            if !self.may_read_record(actor, &contact.record_uid).await?
                || contact.trust != "known" && (chosen.is_none() || !can_manage)
            {
                continue;
            }
            let choice = chosen.cloned().unwrap_or(ContactConsent {
                organ: contact.record_uid.clone(),
                ..Default::default()
            });
            if let Some(endpoint) = &contact.node_id {
                endpoints.insert(endpoint.clone());
            }
            contacts.push(json!({"choice":choice,"name":contact.head,"endpoint":contact.node_id,"unreachable":contact.unreachable_since.is_some(),"retired":contact.trust != "known"}));
            if contacts.len() == contract::MAX_PEERS {
                break;
            }
        }
        if can_manage {
            for choice in &settings.peers {
                if !contacts
                    .iter()
                    .any(|row| row["choice"]["organ"] == choice.organ)
                {
                    contacts.push(json!({"choice":choice,"name":"Retained consent for removed contact","endpoint":null,"retired":true}));
                }
            }
        }
        let queued: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM social_gossip_forward WHERE state='pending'",
        )
        .fetch_one(&self.store.pool)
        .await?;
        let admission_error: Option<String> =
            store::sqlx::query_scalar("SELECT error FROM social_gossip_scan WHERE id=1")
                .fetch_one(&self.store.pool)
                .await?;
        let errors:Vec<(String,String)>=store::sqlx::query_as("SELECT peer,MAX(error) FROM social_gossip_forward WHERE state='pending' AND error IS NOT NULL GROUP BY peer ORDER BY peer LIMIT 32").fetch_all(&self.store.pool).await?;
        let errors: Vec<_> = errors
            .into_iter()
            .filter(|(peer, _)| endpoints.contains(peer))
            .collect();
        Ok(
            json!({"gossip":{"enabled":settings.enabled,"contacts":contacts,"queued":queued,"errors":errors,"admission_error":admission_error},"can_manage_services":can_manage,"status":"Gossip shares only permitted signed public announcements with separately consenting contacts. It carries neither conversation history nor profile images"}),
        )
    }

    pub(super) async fn social_gossip_peer(
        &self,
        source: &str,
        send: bool,
    ) -> Result<(Settings, String), EngineError> {
        let settings = self.social_gossip_settings().await?;
        let contact = store::organs::contact_by_node_id(&self.store.pool, source)
            .await?
            .ok_or_else(|| invalid("Gossip requires a pinned known contact endpoint"))?;
        if store::records::get(&self.store.pool, &contact.record_uid)
            .await?
            .is_none()
        {
            return Err(invalid("This gossip contact was removed"));
        }
        if !settings.enabled
            || contact.trust != "known"
            || contact.unreachable_since.is_some()
            || !settings.peers.iter().any(|peer| {
                peer.organ == contact.record_uid && if send { peer.send } else { peer.receive }
            })
        {
            return Err(invalid(
                "This device or contact has not consented to this gossip direction",
            ));
        }
        Ok((settings, contact.record_uid))
    }

    pub(super) async fn social_gossip_offer(
        &self,
        source: &str,
        node: &str,
        offer: Offer,
        now: i64,
    ) -> Result<Value, EngineError> {
        self.social_gossip_peer(source, false).await?;
        if !nucleus::valid_uid(&offer.post, "post")
            || offer.hash.len() != 64
            || !offer.hash.bytes().all(|b| b.is_ascii_hexdigit())
            || offer.expires_at <= now
            || offer.expires_at > now + MAX_LIFETIME + 300
        {
            return Err(invalid("Invalid bounded gossip inventory"));
        }
        let seen: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM social_gossip_seen WHERE hash=? AND expires_at>?)",
        )
        .bind(&offer.hash)
        .bind(now - 300)
        .fetch_one(&self.store.pool)
        .await?;
        Ok(json!({"service":node,"hash":offer.hash,"missing":!seen}))
    }

    pub(super) async fn social_gossip_peer_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        endpoint: &str,
        original: &str,
        send: bool,
    ) -> Result<(), EngineError> {
        let cell: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug=? AND kind=? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(store::cells::LOCAL_CELL_SLUG)
        .bind(nucleus::RecordKind::Device.as_str())
        .fetch_one(&mut **tx)
        .await?;
        let held = owner::extension_on(tx, &cell, contract::NAMESPACE).await?;
        let settings: Settings = if held == json!({}) {
            Settings::default()
        } else {
            serde_json::from_value(held)?
        };
        let pinned: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM organ_contact c JOIN record r ON r.uid=c.record_uid WHERE c.record_uid=? AND c.node_id=? AND c.trust='known' AND c.unreachable_since IS NULL AND r.deleted_at IS NULL)",
        )
        .bind(original)
        .bind(endpoint)
        .fetch_one(&mut **tx)
        .await?;
        if !pinned
            || !settings.enabled
            || settings.peers.len() > contract::MAX_PEERS
            || !settings
                .peers
                .iter()
                .any(|peer| peer.organ == original && if send { peer.send } else { peer.receive })
        {
            return Err(invalid(
                "Gossip contact or consent changed; review the original contact before retrying",
            ));
        }
        Ok(())
    }

    pub(super) async fn social_gossip_deliver(
        &self,
        source: &str,
        node: &str,
        payload: &Payload,
        now: i64,
    ) -> Result<Value, EngineError> {
        let (consent, contact) = self.social_gossip_peer(source, false).await?;
        validate_payload(payload, now)?;
        let hash = payload_hash(payload)?;
        let settings = self.social_settings().await?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let mut tx = self.social_write_tx().await?;
        if owner::extension_on(&mut tx, &cell.uid, contract::NAMESPACE).await?
            != serde_json::to_value(&consent)?
        {
            return Err(invalid("Gossip consent changed during reception"));
        }
        self.social_gossip_peer_on(&mut tx, source, &contact, false)
            .await?;
        store::sqlx::query("DELETE FROM social_gossip_seen WHERE expires_at<=?")
            .bind(now - 300)
            .execute(&mut *tx)
            .await?;
        let known: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM social_gossip_seen WHERE hash=?)",
        )
        .bind(&hash)
        .fetch_one(&mut *tx)
        .await?;
        if !known {
            let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_gossip_seen")
                .fetch_one(&mut *tx)
                .await?;
            if count
                >= if payload.control() {
                    contract::MAX_ENTRIES
                } else {
                    contract::MAX_ENTRIES * 9 / 10
                }
            {
                return Err(invalid(
                    "Contact gossip reception is full; control capacity is reserved",
                ));
            }
        }
        match payload {
            Payload::Snippet { document } => {
                self.social_cache_snippet_on(
                    &mut tx,
                    document,
                    &document_hash("snippet", document)?,
                    &format!("contact gossip: {source}"),
                    &settings,
                    now,
                )
                .await?;
            }
            Payload::ProfileAuthority { proof, authority } => {
                let pinned: bool = store::sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM social_profile_authority WHERE organ=?)",
                )
                .bind(&authority.organ)
                .fetch_one(&mut *tx)
                .await?;
                if !pinned {
                    store::social::anchor_profile_authority_on(
                        &mut tx,
                        proof
                            .profile
                            .as_ref()
                            .ok_or_else(|| invalid("Missing profile evidence"))?,
                        false,
                    )
                    .await?;
                }
                self.social_capacity(
                    &mut tx,
                    settings.cache_entries,
                    settings.storage_bytes,
                    &authority.organ,
                    "profile",
                    0,
                )
                .await?;
                store::social::anchor_profile_authority_on(&mut tx, authority, false).await?;
            }
            Payload::PostingAuthority { authority, .. } => {
                self.social_capacity(
                    &mut tx,
                    settings.cache_entries,
                    settings.storage_bytes,
                    &authority.owner_key,
                    "posting",
                    0,
                )
                .await?;
                store::social::anchor_posting_authority_on(&mut tx, authority).await?;
            }
        }
        store::sqlx::query("INSERT INTO social_gossip_seen(hash,expires_at,control) VALUES(?,?,?) ON CONFLICT(hash) DO NOTHING")
            .bind(&hash).bind(payload.expires_at()).bind(payload.control()).execute(&mut *tx).await?;
        let (kind, identity) = payload_identity(payload)?;
        let quarantined = if let Payload::Snippet { document } = payload {
            store::sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM social_document WHERE kind='snippet' AND id=? AND state='conflict')").bind(&document.id).fetch_one(&mut *tx).await?
        } else {
            false
        };
        if !quarantined {
            gossip_store::queue_on(&mut tx, payload, kind, &identity, &[], &settings).await?;
        }
        tx.commit().await?;
        self.notify_query_changed();
        Ok(json!({"service":node,"hash":hash,"accepted":true,"expires_at":payload.expires_at()}))
    }
}
