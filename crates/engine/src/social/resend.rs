use super::*;
use nucleus::social::requests::*;

fn eligible(
    participant: &ConversationParticipant,
    content: &PrivateContent,
    metadata: &Value,
    delivery: &Value,
    now: i64,
) -> bool {
    participant.state == ConversationState::Accepted
        && content.kind.purpose() == EnvelopePurpose::Content
        && content.author_owner == participant.local_owner
        && content.conversation == participant.token
        && conversation::validate_content(&content).is_ok()
        && document_hash("private-content", &content).ok().as_deref()
            == metadata["content_hash"].as_str()
        && delivery["expires_at"]
            .as_i64()
            .is_some_and(|expiry| expiry <= now)
        && !matches!(
            delivery["stage"].as_str(),
            Some("recipient-durable" | "recipient-refused")
        )
}

async fn terminal_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    message: &str,
) -> Result<bool, EngineError> {
    Ok(store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_private_outbox WHERE record_uid=? AND state='ready') OR EXISTS(SELECT 1 FROM social_private_destination d JOIN social_private_outbox o ON o.id=d.envelope WHERE o.record_uid=? AND json_extract(d.receipt,'$.stage') IN ('recipient-durable','recipient-refused'))")
        .bind(message).bind(message).fetch_one(&mut **tx).await?)
}

impl Engine {
    pub(super) async fn social_private_delivery_status(
        &self,
        message: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_own_record(message, actor).await?;
        let root: Option<String> = store::sqlx::query_scalar(
            "SELECT replica_root FROM record WHERE uid=? AND kind='message' AND deleted_at IS NULL",
        )
        .bind(message)
        .fetch_optional(&self.store.pool)
        .await?
        .flatten();
        let root = root.ok_or_else(|| invalid("No retained private Message"))?;
        self.social_own_record(&root, actor).await?;
        let participant = self.social_participant(&root).await?;
        let metadata = store::records::get_extension(&self.store.pool, message, MESSAGE_NAMESPACE)
            .await?
            .unwrap_or(Value::Null);
        let delivery = store::records::get_extension(&self.store.pool, message, DELIVERY_NAMESPACE)
            .await?
            .unwrap_or(Value::Null);
        let now = nucleus::execution::now().timestamp();
        let can_edit = self.social_require_local_write().await.is_ok()
            && self
                .require_permission(actor, "record:update")
                .await
                .is_ok();
        let mut tx = self.store.pool.begin().await?;
        let blocked = admission::map_on(&mut tx, &participant.context).await?
            [&participant.peer_owner]["blocked"]
            == true;
        let terminal = terminal_on(&mut tx, message).await?;
        tx.commit().await?;
        let content = conversation::load_content(&self.store.pool, message, &metadata)
            .await
            .ok();
        let can_resend = content
            .as_ref()
            .is_some_and(|content| eligible(&participant, content, &metadata, &delivery, now))
            && !blocked
            && !terminal;
        let destinations: Vec<(String, i64, i64, Option<String>)> = store::sqlx::query_as("SELECT d.service,SUM(d.state='stored'),SUM(d.state='pending'),MAX(d.error) FROM social_private_destination d JOIN social_private_outbox o ON o.id=d.envelope WHERE o.record_uid=? AND o.state IN ('pending','stored') GROUP BY d.service ORDER BY d.service LIMIT 8")
            .bind(message).fetch_all(&self.store.pool).await?;
        let destinations: Vec<Value> = destinations.into_iter().map(|(service, stored, pending, error)| json!({"service":service,"stored":stored,"pending":pending,"error":error})).collect();
        Ok(
            json!({"private_delivery":{"message":message,"conversation":root,"metadata":metadata,"delivery":delivery,"destinations":destinations,"can_resend":can_resend,"observed_at":now,"new_expires_at":now+30*86400},"can_edit":can_edit,"status":"An expired outgoing message can be deliberately resent only in an accepted, open conversation. Its Message, attachments and original creation time stay the same; new copies receive a new delivery deadline"}),
        )
    }

    pub(super) async fn social_resend_expired_private(
        &self,
        message: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_own_record(message, actor).await?;
        self.social_require_local_write().await?;
        let root: Option<String> = store::sqlx::query_scalar(
            "SELECT replica_root FROM record WHERE uid=? AND kind='message' AND deleted_at IS NULL",
        )
        .bind(message)
        .fetch_optional(&self.store.pool)
        .await?
        .flatten();
        let root = root.ok_or_else(|| invalid("No retained private Message"))?;
        self.social_own_record(&root, actor).await?;
        let participant = self.social_participant(&root).await?;
        let metadata = store::records::get_extension(&self.store.pool, message, MESSAGE_NAMESPACE)
            .await?
            .ok_or_else(|| invalid("No immutable private content"))?;
        let mut delivery =
            store::records::get_extension(&self.store.pool, message, DELIVERY_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("This is not an outgoing Message"))?;
        let now = nucleus::execution::now().timestamp();
        let content = conversation::load_content(&self.store.pool, message, &metadata).await?;
        if !eligible(&participant, &content, &metadata, &delivery, now) {
            return Err(invalid(
                "Only expired outgoing messages in an accepted conversation can be resent. Introductions, controls, refused and recipient-durable messages cannot be renewed",
            ));
        }
        if conversation::scoped_uid("r", &(&root, &content.author_owner, &content.message))?
            != message
        {
            return Err(invalid(
                "The retained Message identity differs from its immutable content",
            ));
        }
        let services = store::records::get_extension(
            &self.store.pool,
            &participant.context,
            SESSION_AUTHORITY_NAMESPACE,
        )
        .await?
        .ok_or_else(|| invalid("Missing private reply context"))?["services"]
            .clone();
        self.social_prepare_reply_keys(&participant.context, serde_json::from_value(services)?)
            .await?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        let operational_key = self.operational_key_for(&organ).await?.public_key_b64();
        let mut tx = self.social_write_tx().await?;
        if let Some(actor) = actor {
            let principal = store::auth::principal_on(&mut *tx, actor).await?;
            if principal.is_none_or(|principal| !principal.permits("record:update")) {
                return Err(EngineError::Forbidden(
                    "Current Message editing permission is required".into(),
                ));
            }
        }
        let roster: Option<(String, String)> =
            store::sqlx::query_as("SELECT payload,signature FROM organ_roster WHERE organ_uid=?")
                .bind(&organ)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((payload, signature)) = roster {
            let signed = crate::roster::SignedRoster {
                roster: serde_json::from_str(&payload)?,
                signature,
            };
            if !crate::roster::roster_signature_is_valid(&signed)
                || DateTime::parse_from_rfc3339(&signed.roster.not_after)
                    .ok()
                    .is_none_or(|expiry| expiry <= nucleus::execution::now())
                || !signed.roster.cells.iter().any(|entry| {
                    entry.cell_uid == cell
                        && entry.operational_key == operational_key
                        && entry.may(crate::roster::CAP_WRITE)
                })
            {
                return Err(EngineError::Forbidden(
                    "Current own-device write membership is required".into(),
                ));
            }
        }
        let live: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM record WHERE uid IN (?,?) AND organ_uid=? AND deleted_at IS NULL",
        )
        .bind(message)
        .bind(&root)
        .bind(&organ)
        .fetch_one(&mut *tx)
        .await?;
        if live != 2
            || owner::extension_on(&mut tx, &root, PARTICIPANTS_NAMESPACE).await?
                != serde_json::to_value(&participant)?
            || owner::extension_on(&mut tx, message, MESSAGE_NAMESPACE).await? != metadata
            || owner::extension_on(&mut tx, message, DELIVERY_NAMESPACE).await? != delivery
            || admission::map_on(&mut tx, &participant.context).await?[&participant.peer_owner]["blocked"]
                == true
        {
            return Err(invalid(
                "The Message, conversation or permission changed before resend. Refresh its delivery controls",
            ));
        }
        if terminal_on(&mut tx, message).await? {
            return Err(invalid(
                "A retained recipient receipt already ends delivery of this logical Message",
            ));
        }
        let expiry = now + 30 * 86400;
        store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error='A fresh delivery attempt replaced this copy' WHERE envelope IN (SELECT id FROM social_private_outbox WHERE record_uid=?) AND state IN ('pending','stored')")
            .bind(message).execute(&mut *tx).await?;
        store::sqlx::query("UPDATE social_private_outbox SET state='cancelled',error='A fresh delivery attempt replaced this copy' WHERE record_uid=? AND state<>'ready'")
            .bind(message).execute(&mut *tx).await?;
        store::sqlx::query("DELETE FROM social_message_work WHERE record_uid=?")
            .bind(message)
            .execute(&mut *tx)
            .await?;
        delivery["origin_cell"] = json!(cell);
        delivery["stage"] = json!("waiting");
        delivery["expires_at"] = json!(expiry);
        delivery["error"] = Value::Null;
        delivery["receipt"] = Value::Null;
        store::records::set_extension_on(&mut tx, message, DELIVERY_NAMESPACE, &delivery).await?;
        outbound::work_on(&mut tx, message, &root, &participant.context, expiry).await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"message":message,"expires_at":expiry,"status":"The same retained Message now has a fresh thirty-day delivery window. Sending waits for current owner-authorized keys and selected mailboxes; earlier copies cannot be recalled"}),
        )
    }
}
