use super::*;
use nucleus::social::requests::*;
use store::sqlx::Row;

impl Engine {
    pub async fn social_process_message_events_once(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        store::sqlx::query("DELETE FROM social_message_event WHERE EXISTS(SELECT 1 FROM record r WHERE r.uid=social_message_event.record_uid AND r.deleted_at IS NOT NULL)").execute(&self.store.pool).await?;
        let now = nucleus::execution::now();
        let rows=store::sqlx::query("SELECT event,record_uid,fact,issued_at FROM social_message_event WHERE next_attempt<=? ORDER BY next_attempt,event LIMIT 8").bind(now.timestamp()).fetch_all(&self.store.pool).await?;
        let mut processed = 0;
        for row in rows {
            let event: String = row.get("event");
            let record: String = row.get("record_uid");
            let fact: String = row.get("fact");
            let issued_at: i64 = row.get("issued_at");
            let result = async {
                let original = store::facts::get(&self.store.pool, &fact)
                    .await?
                    .ok_or_else(|| invalid("The retained Message event lost its import Fact"))?;
                let metadata =
                    store::records::get_extension(&self.store.pool, &record, MESSAGE_NAMESPACE)
                        .await?
                        .ok_or_else(|| {
                            invalid("The retained Message event lost its immutable content")
                        })?;
                let content: PrivateContent = serde_json::from_value(metadata["content"].clone())?;
                if conversation::scoped_uid("event", &(&record, "social-message-saved"))? != event
                    || content.issued_at != issued_at
                    || original.record_uid != record
                {
                    return Err(invalid("The retained Message event identity differs"));
                }
                self.observe_fact_state(&original, now).await?;
                self.publish_committed_fact(original);
                Box::pin(
                    self.react_to_event(
                        vec![record],
                        event.clone(),
                        DateTime::from_timestamp(issued_at, 0)
                            .ok_or_else(|| invalid("Invalid retained Message event time"))?,
                    ),
                )
                .await?;
                Ok::<(), EngineError>(())
            }
            .await;
            match result {
                Ok(()) => {
                    store::sqlx::query("DELETE FROM social_message_event WHERE event=?")
                        .bind(event)
                        .execute(&self.store.pool)
                        .await?;
                    processed += 1;
                }
                Err(error) => {
                    store::sqlx::query("UPDATE social_message_event SET error=?,next_attempt=?+MIN(3600,5*(1<<MIN(attempts,10))),attempts=MIN(attempts+1,1000000) WHERE event=?").bind(error.to_string().chars().take(500).collect::<String>()).bind(now.timestamp()).bind(event).execute(&self.store.pool).await?;
                }
            }
        }
        Ok(processed)
    }

    pub(super) async fn social_note_work_error(
        &self,
        record: &str,
        error: &str,
    ) -> Result<(), EngineError> {
        let now = nucleus::execution::now().timestamp();
        let error: String = error.chars().take(500).collect();
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("UPDATE social_message_work SET error=?,next_attempt=?+MIN(3600,5*(1<<MIN(attempts,10))),attempts=MIN(attempts+1,1000000) WHERE record_uid=?")
            .bind(&error).bind(now).bind(record).execute(&mut *tx).await?;
        let mut status = owner::extension_on(&mut tx, record, DELIVERY_NAMESPACE).await?;
        if status.get("origin_cell").is_some() {
            status["error"] = json!(error);
            store::records::set_extension_on(&mut tx, record, DELIVERY_NAMESPACE, &status).await?;
        } else {
            let mut draft = owner::extension_on(&mut tx, record, REQUEST_DRAFT_NAMESPACE).await?;
            if draft.get("draft").is_some() {
                draft["error"] = json!(error);
                store::records::set_extension_on(&mut tx, record, REQUEST_DRAFT_NAMESPACE, &draft)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn social_decide_request(
        &self,
        root: &str,
        decision: RequestDecision,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_own_record(root, actor).await?;
        let participant = self.social_participant(root).await?;
        let kind = match decision {
            RequestDecision::Accept => ContentKind::Accept,
            RequestDecision::Decline => ContentKind::Decline,
            RequestDecision::Close | RequestDecision::Block => ContentKind::Close,
        };
        let content = PrivateContent {
            protocol: "lince.private-content.1".into(),
            conversation: participant.token.clone(),
            message: nucleus::new_uid("msg"),
            author_owner: participant.local_owner.clone(),
            issued_at: nucleus::execution::now().timestamp(),
            kind,
        };
        let result = self
            .social_save_content(
                root,
                participant,
                content,
                None,
                matches!(decision, RequestDecision::Block),
            )
            .await?;
        self.social_reconcile_private_admissions().await?;
        Ok(result)
    }

    pub async fn social_prepare_messages_once(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let now = nucleus::execution::now().timestamp();
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_message_work WHERE record_uid IN (SELECT uid FROM record WHERE deleted_at IS NOT NULL)").execute(&mut *tx).await?;
        let expired=store::sqlx::query("SELECT record_uid FROM social_message_work WHERE expires_at<=? ORDER BY record_uid LIMIT 8").bind(now).fetch_all(&mut *tx).await?;
        for row in expired {
            let record: String = row.get("record_uid");
            let mut status = owner::extension_on(&mut tx, &record, DELIVERY_NAMESPACE).await?;
            if status.get("origin_cell").is_some()
                && !matches!(
                    status["stage"].as_str(),
                    Some("recipient-durable" | "recipient-refused")
                )
            {
                status["stage"] = json!("expired");
                status["error"] = json!("The delivery window ended; retained history remains");
                store::records::set_extension_on(&mut tx, &record, DELIVERY_NAMESPACE, &status)
                    .await?;
            } else {
                let mut draft =
                    owner::extension_on(&mut tx, &record, REQUEST_DRAFT_NAMESPACE).await?;
                if draft.get("draft").is_some() {
                    draft["expired"] = json!(true);
                    draft["error"] = json!(
                        "This introduction expired. Compose a fresh one from an active announcement"
                    );
                    store::records::set_extension_on(
                        &mut tx,
                        &record,
                        REQUEST_DRAFT_NAMESPACE,
                        &draft,
                    )
                    .await?;
                }
            }
            store::sqlx::query("DELETE FROM social_message_work WHERE record_uid=?")
                .bind(record)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        let rows=store::sqlx::query("SELECT record_uid,conversation,context FROM social_message_work WHERE expires_at>? AND next_attempt<=? ORDER BY next_attempt,record_uid LIMIT 8")
            .bind(now).bind(now).fetch_all(&self.store.pool).await?;
        let mut prepared = 0;
        for row in rows {
            let record: String = row.get("record_uid");
            let context: String = row.get("context");
            let root: String = row.get("conversation");
            let result = if record == root && record == context {
                self.social_materialize_request(&record)
                    .await
                    .map(|()| true)
            } else {
                self.social_seal_message(&record, &root, &context).await
            };
            match result {
                Ok(true) => prepared += 1,
                Ok(false) => {}
                Err(error) => {
                    self.social_note_work_error(&record, &error.to_string())
                        .await?
                }
            }
        }
        Ok(prepared)
    }

    async fn social_seal_message(
        &self,
        record: &str,
        root: &str,
        context: &str,
    ) -> Result<bool, EngineError> {
        let now = nucleus::execution::now().timestamp();
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let p = self.social_participant(root).await?;
        let intent = store::records::get_extension(&self.store.pool, record, DELIVERY_NAMESPACE)
            .await?
            .unwrap_or(Value::Null);
        if intent["origin_cell"].as_str() != Some(cell.as_str())
            || matches!(
                intent["stage"].as_str(),
                Some("recipient-durable" | "recipient-refused")
            )
        {
            store::sqlx::query("DELETE FROM social_message_work WHERE record_uid=?")
                .bind(record)
                .execute(&self.store.pool)
                .await?;
            return Ok(false);
        }
        let authority =
            store::records::get_extension(&self.store.pool, context, SESSION_AUTHORITY_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("Waiting for private reply authority"))?;
        let binding: PrivateOwnerBinding = serde_json::from_value(authority["binding"].clone())?;
        self.social_validate_private_binding(context, &binding)
            .await?;
        let local: CertifiedRoute =
            serde_json::from_value(authority[format!("authorized_{cell}")].clone())
                .map_err(|_| invalid("Waiting for fresh owner-authorized messaging keys"))?;
        request_auth::validate_route(&local, now)?;
        if binding.owner_key != p.local_owner
            || local.control.owner_key != p.local_owner
            || serde_json::to_value(&local.control)? != authority["control"]
        {
            return Err(invalid("The private owner authorization is stale"));
        }
        let key = self.social_storage_key().await?;
        let mut tx = self.social_write_tx().await?;
        let retained:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND kind='message' AND deleted_at IS NULL)").bind(record).fetch_one(&mut *tx).await?;
        if !retained {
            return Err(invalid(
                "This outgoing Message was deleted; its pending delivery is cancelled",
            ));
        }
        if owner::extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await? != authority
            || owner::extension_on(&mut tx, root, PARTICIPANTS_NAMESPACE).await?
                != serde_json::to_value(&p)?
        {
            return Err(invalid(
                "Private routing changed while preparing ciphertext",
            ));
        }
        let status = owner::extension_on(&mut tx, record, DELIVERY_NAMESPACE).await?;
        if status["origin_cell"].as_str() != Some(cell.as_str()) {
            return Ok(false);
        }
        let expiry = status["expires_at"]
            .as_i64()
            .ok_or_else(|| invalid("Missing private message lifetime"))?;
        if expiry <= now {
            return Err(invalid(
                "This retained message's delivery window has expired",
            ));
        }
        let metadata = owner::extension_on(&mut tx, record, MESSAGE_NAMESPACE).await?;
        let content = conversation::load_content_on(&mut tx, record, &metadata).await?;
        conversation::validate_content(&content)?;
        if p.token != content.conversation
            || p.context != context
            || p.local_owner != content.author_owner
            || document_hash("private-content", &content)? != metadata["content_hash"]
        {
            return Err(invalid("The retained outgoing Message content differs"));
        }
        if !matches!(
            p.state,
            ConversationState::Pending | ConversationState::Accepted
        ) && content.kind.purpose() != EnvelopePurpose::Control
        {
            return Err(invalid("This conversation is closed"));
        }
        let account_id = format!("account:{context}");
        let body: String = store::sqlx::query_scalar(
            "SELECT body FROM social_device_state WHERE id=? AND kind='account'",
        )
        .bind(&account_id)
        .fetch_one(&mut *tx)
        .await?;
        let state: session::AccountState = session::open_local(&account_id, &body, &key)?;
        if state.route != local.route {
            return Err(invalid("The owner authorized different live device keys"));
        }
        let (count,bytes):(i64,i64)=store::sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM social_private_outbox").fetch_one(&mut *tx).await?;
        if count + p.routes.len() as i64 > 4096 {
            return Err(invalid("The device's encrypted outgoing queue is full"));
        }
        if p.routes.is_empty() || p.routes.len() > 64 {
            return Err(invalid(
                "The private recipient routes are unavailable or oversized",
            ));
        }
        let mut made = 0;
        let mut queued_bytes = bytes;
        for peer in &p.routes {
            request_auth::validate_route(peer, now)?;
            if peer.control.owner_key != p.peer_owner {
                return Err(invalid(
                    "The recipient route belongs to another participant",
                ));
            }
            mailbox::anchor_control(&mut tx, &peer.control).await?;
            let generation: i64 = peer
                .control
                .generation
                .parse()
                .map_err(|_| invalid("Invalid peer authority generation"))?;
            store::sqlx::query("UPDATE social_private_outbox SET state='held',error='Recipient or sender device keys changed; fresh ciphertext uses the retained Message' WHERE record_uid=? AND state IN ('pending','stored') AND (recipient_generation<? OR json_extract(body,'$.envelope.identity_key')<>?)")
                .bind(record).bind(generation).bind(&local.route.identity_key).execute(&mut *tx).await?;
            store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error='Messaging authority changed' WHERE envelope IN (SELECT id FROM social_private_outbox WHERE record_uid=? AND state='held') AND state IN ('pending','stored')")
                .bind(record).execute(&mut *tx).await?;
            let existing:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_private_outbox WHERE record_uid=? AND json_extract(body,'$.envelope.route')=? AND json_extract(body,'$.envelope.identity_key')=? AND state IN ('pending','stored','ready'))")
                .bind(record).bind(&peer.route.mailbox).bind(&local.route.identity_key).fetch_one(&mut *tx).await?;
            if existing {
                continue;
            }
            let held:Option<(String,String,i64)>=store::sqlx::query_as("SELECT s.id,s.body,s.version FROM social_session_peer p JOIN social_device_state s ON s.id=p.session_id WHERE p.context=? AND p.peer=?")
                .bind(context).bind(&peer.route.identity_key).fetch_optional(&mut *tx).await?;
            let mut live = if let Some((_, body, _)) = &held {
                session::open_session(body, &key)?
            } else {
                state
                    .account(&key)?
                    .create_outbound_session(
                        vodozemac::olm::SessionConfig::version_1(),
                        vodozemac::Curve25519PublicKey::from_base64(&peer.route.identity_key)
                            .map_err(|_| invalid("Invalid recipient session key"))?,
                        vodozemac::Curve25519PublicKey::from_base64(&peer.route.prekey)
                            .map_err(|_| invalid("Invalid recipient offline key"))?,
                    )
                    .map_err(|_| invalid("A fresh private session could not be established"))?
            };
            let encrypted = live
                .encrypt(serde_json::to_vec(&content)?)
                .map_err(|_| invalid("The private message could not be encrypted"))?;
            let (message_type, ciphertext) = encrypted.to_parts();
            let mut envelope = PrivateEnvelope {
                protocol: "lince.private-message.1".into(),
                id: String::new(),
                route: peer.route.mailbox.clone(),
                sender_owner: p.local_owner.clone(),
                sender_key: local.route.signing_key.clone(),
                identity_key: local.route.identity_key.clone(),
                control: local.control.clone(),
                certificate: local.certificate.clone(),
                session_id: live.session_id(),
                message: content.message.clone(),
                content_hash: document_hash("private-content", &content)?,
                created_at: now,
                expires_at: expiry.min(
                    now + if p.state == ConversationState::Accepted {
                        30 * 86400
                    } else {
                        AUTHORITY_LIFETIME
                    },
                ),
                message_type: message_type as u8,
                purpose: content.kind.purpose(),
                ciphertext: B64.encode(ciphertext),
                signature: String::new(),
            };
            envelope.id = request_auth::envelope_id(&envelope)?;
            envelope.signature = state
                .signing_key()?
                .sign_bytes(&signing_bytes("private-envelope", &envelope)?);
            let delivery = PrivateDelivery {
                envelope,
                authorization: FreshAuthorization {
                    control: local.control.clone(),
                    certificate: local.certificate.clone(),
                },
            };
            let hash = request_auth::validate_delivery(&delivery, now)?;
            let body = serde_json::to_string(&delivery)?;
            queued_bytes = queued_bytes.saturating_add(body.len() as i64);
            if queued_bytes > 64 * 1024 * 1024 {
                return Err(invalid("The device's encrypted outgoing queue is full"));
            }
            let session_id = conversation::scoped_uid(
                "session",
                &(
                    context,
                    &peer.route.identity_key,
                    &delivery.envelope.session_id,
                ),
            )?;
            store::social::put_device_state_on(
                &mut tx,
                &session_id,
                "session",
                context,
                &live.pickle().encrypt(&key),
                held.as_ref().map(|(_, _, v)| *v),
                now,
            )
            .await?;
            store::sqlx::query("INSERT INTO social_session_peer(context,peer,session_id) VALUES(?,?,?) ON CONFLICT(context,peer) DO UPDATE SET session_id=excluded.session_id")
                .bind(context).bind(&peer.route.identity_key).bind(&session_id).execute(&mut *tx).await?;
            store::sqlx::query("INSERT INTO social_private_outbox(id,context,body,hash,expires_at,record_uid,recipient_owner,recipient_generation) VALUES(?,?,?,?,?,?,?,?)")
                .bind(&delivery.envelope.id).bind(context).bind(body).bind(hash).bind(delivery.envelope.expires_at).bind(record).bind(&peer.control.owner_key).bind(generation).execute(&mut *tx).await?;
            for service in &peer.route.services {
                store::sqlx::query(
                    "INSERT INTO social_private_destination(envelope,service) VALUES(?,?)",
                )
                .bind(&delivery.envelope.id)
                .bind(service)
                .execute(&mut *tx)
                .await?;
            }
            made += 1;
        }
        let mut status = status;
        if !matches!(
            status["stage"].as_str(),
            Some("mailbox-stored" | "recipient-durable")
        ) {
            status["stage"] = json!("queued");
        }
        status["error"] = Value::Null;
        store::records::set_extension_on(&mut tx, record, DELIVERY_NAMESPACE, &status).await?;
        store::sqlx::query(
            "UPDATE social_message_work SET next_attempt=?,error=NULL WHERE record_uid=?",
        )
        .bind(now + 60)
        .bind(record)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(made > 0)
    }
}
