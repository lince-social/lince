use super::*;
use nucleus::social::requests::*;
use store::sqlx::Row;

async fn hold_private_copy_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    id: &str,
    error: &str,
) -> Result<(), EngineError> {
    store::sqlx::query("UPDATE social_private_outbox SET state='held',error=? WHERE id=? AND state IN ('pending','stored')")
        .bind(error).bind(id).execute(&mut **tx).await?;
    store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error=? WHERE envelope=? AND state IN ('pending','stored')")
        .bind(error).bind(id).execute(&mut **tx).await?;
    Ok(())
}

impl Engine {
    pub async fn social_refresh_private_routes_once(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let network = self.social_network()?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        store::sqlx::query("INSERT INTO social_peer_work(conversation) SELECT r.uid FROM record r JOIN record_extension e ON e.record_uid=r.uid WHERE e.namespace=? AND r.organ_uid=? AND r.deleted_at IS NULL AND json_extract(e.fds,'$.state') IN ('pending','accepted') ON CONFLICT DO NOTHING")
            .bind(PARTICIPANTS_NAMESPACE).bind(&organ).execute(&self.store.pool).await?;
        let now = nucleus::execution::now().timestamp();
        let root:Option<String>=store::sqlx::query_scalar("SELECT w.conversation FROM social_peer_work w JOIN record r ON r.uid=w.conversation JOIN record_extension e ON e.record_uid=r.uid AND e.namespace=? WHERE r.deleted_at IS NULL AND json_extract(e.fds,'$.state') IN ('pending','accepted') AND w.next_attempt<=? ORDER BY w.next_attempt,w.conversation LIMIT 1")
            .bind(PARTICIPANTS_NAMESPACE).bind(now).fetch_optional(&self.store.pool).await?;
        let Some(root) = root else {
            return Ok(0);
        };
        let result = self.social_refresh_private_peer(&network, &root).await;
        let error = result
            .as_ref()
            .err()
            .map(|e| e.to_string().chars().take(500).collect::<String>());
        store::sqlx::query("UPDATE social_peer_work SET error=?,attempts=CASE WHEN ? IS NULL THEN 0 ELSE MIN(attempts+1,1000000) END,next_attempt=?+CASE WHEN ? IS NULL THEN 60 ELSE MIN(3600,5*(1<<MIN(attempts,10))) END WHERE conversation=?")
            .bind(&error).bind(&error).bind(now).bind(&error).bind(&root).execute(&self.store.pool).await?;
        result
    }

    async fn social_refresh_private_peer(
        &self,
        network: &std::sync::Arc<dyn Network>,
        root: &str,
    ) -> Result<usize, EngineError> {
        let mut p = self.social_participant(root).await?;
        let before = serde_json::to_value(&p)?;
        let services: std::collections::BTreeSet<String> = p
            .routes
            .iter()
            .flat_map(|r| r.route.services.iter().cloned())
            .collect();
        if services.is_empty() || services.len() > 8 {
            return Err(invalid("Choose at most eight peer mailbox hosts"));
        }
        let services: Vec<String> = services.into_iter().collect();
        let host = &services[(nucleus::execution::now().timestamp() as usize / 5) % services.len()];
        let mut after = None;
        let mut routes = std::collections::BTreeMap::new();
        for _ in 0..8 {
            let response = network
                .request(
                    host,
                    PublicRequest::LookupReplyRoutes {
                        owner: p.peer_owner.clone(),
                        after: after.clone(),
                    },
                )
                .await?;
            if response["service"] != *host
                || response["owner"] != p.peer_owner
                || serde_json::to_vec(&response)?.len() > MAX_FRAME_BYTES
            {
                return Err(invalid(
                    "The selected host returned another participant's routes",
                ));
            }
            let page = response["routes"]
                .as_array()
                .ok_or_else(|| invalid("Incomplete private route page"))?;
            if page.len() > 8 {
                return Err(invalid("The private route page exceeds its bound"));
            }
            for value in page {
                let route: CertifiedRoute = serde_json::from_value(value.clone())?;
                request_auth::validate_route(&route, nucleus::execution::now().timestamp())?;
                if route.control.owner_key != p.peer_owner
                    || !route.route.services.contains(host)
                    || after.as_ref().is_some_and(|a| route.route.mailbox <= *a)
                    || routes.insert(route.route.mailbox.clone(), route).is_some()
                {
                    return Err(invalid(
                        "The private route page conflicts with the selected participant or cursor",
                    ));
                }
            }
            let next = response["next_after"].as_str().map(str::to_owned);
            if next.as_ref().is_some_and(|n| {
                !nucleus::valid_uid(n, "mail") || after.as_ref().is_some_and(|a| n <= a)
            }) {
                return Err(invalid("The private route page did not advance"));
            }
            after = next;
            if after.is_none() {
                break;
            }
        }
        if routes.is_empty() {
            return Err(invalid(
                "The peer's current authorized device routes are unavailable",
            ));
        }
        let generation = routes
            .values()
            .filter_map(|r| r.control.generation.parse::<i64>().ok())
            .max()
            .ok_or_else(|| invalid("No current peer authority"))?;
        routes.retain(|_, r| r.control.generation.parse::<i64>().ok() == Some(generation));
        p.routes = routes.into_values().collect();
        if serde_json::to_vec(&p)?.len() > 256 * 1024 {
            return Err(invalid("The peer's device routing state exceeds its bound"));
        }
        let mut tx = self.social_write_tx().await?;
        if owner::extension_on(&mut tx, root, PARTICIPANTS_NAMESPACE).await? != before {
            return Err(invalid(
                "The conversation changed while refreshing its routes",
            ));
        }
        self.social_require_local_write_on(&mut tx).await?;
        for route in &p.routes {
            mailbox::anchor_control(&mut tx, &route.control).await?;
        }
        store::records::set_extension_on(
            &mut tx,
            root,
            PARTICIPANTS_NAMESPACE,
            &serde_json::to_value(&p)?,
        )
        .await?;
        store::sqlx::query("UPDATE social_message_work SET next_attempt=0 WHERE conversation=?")
            .bind(root)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(p.routes.len())
    }

    pub async fn social_send_private_once(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let network = self.social_network()?;
        let now = nucleus::execution::now().timestamp();
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("UPDATE social_private_outbox SET state='cancelled',error='The recipient refused this Message' WHERE state IN ('pending','stored','held') AND EXISTS(SELECT 1 FROM record_extension e WHERE e.record_uid=social_private_outbox.record_uid AND e.namespace=? AND json_extract(e.fds,'$.stage')='recipient-refused')").bind(DELIVERY_NAMESPACE).execute(&mut *tx).await?;
        store::sqlx::query("UPDATE social_private_outbox SET state='cancelled',error='Sending was deliberately resumed on another device' WHERE state IN ('pending','stored','held') AND EXISTS(SELECT 1 FROM record_extension e WHERE e.record_uid=social_private_outbox.record_uid AND e.namespace=? AND json_extract(e.fds,'$.origin_cell')<>?)").bind(DELIVERY_NAMESPACE).bind(cell).execute(&mut *tx).await?;
        store::sqlx::query("UPDATE social_private_outbox SET state='cancelled',error='The retained Message was deleted on your devices' WHERE state IN ('pending','stored','held') AND EXISTS(SELECT 1 FROM record r WHERE r.uid=social_private_outbox.record_uid AND r.deleted_at IS NOT NULL)").execute(&mut *tx).await?;
        store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error=(SELECT error FROM social_private_outbox WHERE id=social_private_destination.envelope) WHERE envelope IN (SELECT id FROM social_private_outbox WHERE state='cancelled') AND state IN ('pending','stored')").execute(&mut *tx).await?;
        store::sqlx::query("UPDATE social_private_outbox SET state='expired',error='The encrypted delivery lifetime ended' WHERE expires_at<=? AND state IN ('pending','stored','held')").bind(now).execute(&mut *tx).await?;
        store::sqlx::query("UPDATE social_private_outbox SET state='held',error='Known recipient device revocation; wait for fresh routing' WHERE state IN ('pending','stored') AND EXISTS(SELECT 1 FROM social_owner_control c WHERE c.owner=social_private_outbox.recipient_owner AND c.generation>social_private_outbox.recipient_generation)").execute(&mut *tx).await?;
        store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error='Known recipient device revocation' WHERE envelope IN (SELECT id FROM social_private_outbox WHERE state='held') AND state IN ('pending','stored')").execute(&mut *tx).await?;
        store::sqlx::query("DELETE FROM social_private_outbox WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error='The encrypted delivery lifetime ended' WHERE envelope IN (SELECT id FROM social_private_outbox WHERE state='expired') AND state IN ('pending','stored')").execute(&mut *tx).await?;
        let rows=store::sqlx::query("SELECT o.id,o.context,o.body,o.hash,o.record_uid,d.service,d.state FROM social_private_destination d JOIN social_private_outbox o ON o.id=d.envelope WHERE d.state IN ('pending','stored') AND o.state IN ('pending','stored') AND o.expires_at>? AND d.next_attempt<=? ORDER BY d.next_attempt,o.rowid,d.service LIMIT 8")
            .bind(now).bind(now).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        let mut sent = 0;
        for row in rows {
            let id: String = row.get("id");
            let active: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_private_outbox o JOIN social_private_destination d ON d.envelope=o.id WHERE o.id=? AND d.service=? AND o.state IN ('pending','stored') AND d.state IN ('pending','stored'))").bind(&id).bind(row.get::<String,_>("service")).fetch_one(&self.store.pool).await?;
            if !active {
                continue;
            }
            let context: String = row.get("context");
            let service: String = row.get("service");
            let stored: Value = serde_json::from_str(&row.get::<String, _>("body"))?;
            let result = self
                .social_try_private_destination(
                    &network,
                    &context,
                    &service,
                    &stored,
                    &row.get::<String, _>("hash"),
                    row.get::<String, _>("state") == "stored",
                )
                .await;
            match result {
                Ok(value) => {
                    let stage = value["stage"].as_str().unwrap_or_default();
                    let ready = stage == "recipient-durable";
                    let refused = stage == "recipient-refused";
                    let terminal = ready || refused;
                    let mut tx = self.social_write_tx().await?;
                    if let Err(error) = self.social_require_local_write_on(&mut tx).await {
                        store::sqlx::query("UPDATE social_private_destination SET receipt=? WHERE envelope=? AND service=? AND state IN ('pending','stored')")
                            .bind(serde_json::to_string(&value)?).bind(&id).bind(&service).execute(&mut *tx).await?;
                        hold_private_copy_on(&mut tx, &id, &error.to_string()).await?;
                        tx.commit().await?;
                        continue;
                    }
                    let updated = store::sqlx::query("UPDATE social_private_destination SET state=?,error=?,receipt=?,attempts=MIN(attempts+1,1000000),next_attempt=?+30 WHERE envelope=? AND service=? AND state IN ('pending','stored')")
                        .bind(if refused { "failed" } else { "stored" }).bind(if refused { Some("The recipient deliberately discarded this ciphertext") } else { None }).bind(serde_json::to_string(&value)?).bind(now).bind(&id).bind(&service).execute(&mut *tx).await?;
                    if !terminal && updated.rows_affected() == 0 {
                        tx.commit().await?;
                        continue;
                    }
                    store::sqlx::query("UPDATE social_private_outbox SET state=?,error=NULL WHERE id=? AND state IN ('pending','stored')")
                        .bind(if ready { "ready" } else if refused { "cancelled" } else { "stored" }).bind(&id).execute(&mut *tx).await?;
                    if terminal {
                        store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error=NULL WHERE envelope=? AND service<>? AND state IN ('pending','stored')").bind(&id).bind(&service).execute(&mut *tx).await?;
                    }
                    if let Some(record) = row.get::<Option<String>, _>("record_uid") {
                        let mut status =
                            owner::extension_on(&mut tx, &record, DELIVERY_NAMESPACE).await?;
                        if status["stage"] != "recipient-durable" {
                            status["stage"] =
                                json!(if terminal { stage } else { "mailbox-stored" });
                            status["error"] = if refused {
                                json!(
                                    "The recipient deliberately discarded this ciphertext; compose a new message if needed"
                                )
                            } else {
                                Value::Null
                            };
                            if terminal {
                                status["receipt"] = value["receipt"].clone();
                            }
                            store::records::set_extension_on(
                                &mut tx,
                                &record,
                                DELIVERY_NAMESPACE,
                                &status,
                            )
                            .await?;
                        }
                        if terminal {
                            if refused {
                                store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error='The recipient refused this logical Message' WHERE envelope IN (SELECT id FROM social_private_outbox WHERE record_uid=?) AND state IN ('pending','stored')").bind(&record).execute(&mut *tx).await?;
                                store::sqlx::query("UPDATE social_private_outbox SET state='cancelled',error='The recipient refused this logical Message' WHERE record_uid=? AND state IN ('pending','stored','held')").bind(&record).execute(&mut *tx).await?;
                            }
                            store::sqlx::query(
                                "DELETE FROM social_message_work WHERE record_uid=?",
                            )
                            .bind(record)
                            .execute(&mut *tx)
                            .await?;
                        }
                    }
                    tx.commit().await?;
                    sent += 1;
                }
                Err(error) => {
                    let error: String = error.to_string().chars().take(500).collect();
                    let mut tx = self.social_write_tx().await?;
                    if let Err(authority) = self.social_require_local_write_on(&mut tx).await {
                        hold_private_copy_on(&mut tx, &id, &authority.to_string()).await?;
                    } else {
                        store::sqlx::query("UPDATE social_private_destination SET error=?,attempts=MIN(attempts+1,1000000),next_attempt=?+MIN(3600,5*(1<<MIN(attempts,10))) WHERE envelope=? AND service=? AND state IN ('pending','stored')")
                            .bind(&error).bind(now).bind(&id).bind(&service).execute(&mut *tx).await?;
                        store::sqlx::query("UPDATE social_private_outbox SET error=? WHERE id=? AND state IN ('pending','stored')").bind(error).bind(&id).execute(&mut *tx).await?;
                    }
                    tx.commit().await?;
                }
            }
        }
        if sent > 0 {
            self.notify_query_changed();
        }
        Ok(sent)
    }

    async fn social_try_private_destination(
        &self,
        network: &std::sync::Arc<dyn Network>,
        context: &str,
        service: &str,
        stored: &Value,
        hash: &str,
        inspect: bool,
    ) -> Result<Value, EngineError> {
        self.social_require_local_write().await?;
        let ready = self.social_reply_key_status(context).await?;
        let local: CertifiedRoute = serde_json::from_value(ready["route"].clone())
            .map_err(|_| invalid("Waiting for current messaging authority before delivery"))?;
        let mut document: PrivateDelivery = serde_json::from_value(stored.clone())?;
        if document.envelope.sender_owner != local.control.owner_key
            || document.envelope.sender_key != local.route.signing_key
            || document.envelope.identity_key != local.route.identity_key
        {
            return Err(invalid(
                "The saved ciphertext needs fresh session keys; resume its retained Message",
            ));
        }
        document.authorization = FreshAuthorization {
            control: local.control,
            certificate: local.certificate,
        };
        request_auth::validate_delivery(&document, nucleus::execution::now().timestamp())?;
        let request = if inspect {
            PublicRequest::InspectPrivate {
                document: document.clone(),
            }
        } else {
            PublicRequest::DeliverPrivate {
                document: document.clone(),
            }
        };
        let mut value = network.request(service, request).await?;
        if inspect && value["stage"] == "unknown" {
            self.social_require_local_write().await?;
            value = network
                .request(
                    service,
                    PublicRequest::DeliverPrivate {
                        document: document.clone(),
                    },
                )
                .await?;
        }
        if value["service"] != service
            || value["envelope"] != document.envelope.id
            || value["hash"] != hash
            || !matches!(
                value["stage"].as_str(),
                Some("stored" | "recipient-durable" | "recipient-refused")
            )
        {
            return Err(invalid(
                "The selected mailbox returned a conflicting delivery result",
            ));
        }
        if matches!(
            value["stage"].as_str(),
            Some("recipient-durable" | "recipient-refused")
        ) && value["receipt"].is_null()
        {
            let expected = value["stage"].clone();
            self.social_require_local_write().await?;
            value = network
                .request(
                    service,
                    PublicRequest::InspectPrivate {
                        document: document.clone(),
                    },
                )
                .await?;
            if value["service"] != service
                || value["envelope"] != document.envelope.id
                || value["hash"] != hash
                || value["stage"] != expected
            {
                return Err(invalid(
                    "The selected mailbox omitted its recipient receipt",
                ));
            }
        }
        if matches!(
            value["stage"].as_str(),
            Some("recipient-durable" | "recipient-refused")
        ) {
            let expected = if value["stage"] == "recipient-refused" {
                ReceiptStage::RecipientRefused
            } else {
                ReceiptStage::RecipientDurable
            };
            let receipt: RecipientReceipt = serde_json::from_value(value["receipt"].clone())?;
            if receipt.certificate.issued_at > receipt.at
                || receipt.certificate.expires_at <= receipt.at
                || !crate::roster::verify_with(
                    &receipt.certificate.owner_key,
                    &signing_bytes("reply-device", &receipt.certificate)?,
                    &receipt.certificate.signature,
                )
            {
                return Err(invalid(
                    "The recipient receipt lacks valid owner authorization",
                ));
            }
            if receipt.envelope != document.envelope.id
                || receipt.envelope_hash != hash
                || receipt.message != document.envelope.message
                || receipt.content_hash != document.envelope.content_hash
                || receipt.stage != expected
                || receipt.certificate.mailbox != document.envelope.route
                || receipt.at <= 0
                || receipt.at > nucleus::execution::now().timestamp() + 300
                || !crate::roster::verify_with(
                    &receipt.certificate.signing_key,
                    &signing_bytes("recipient-receipt", &receipt)?,
                    &receipt.signature,
                )
            {
                return Err(invalid(
                    "The claimed recipient import could not be authenticated",
                ));
            }
            let certified:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_private_outbox o JOIN record_extension p ON p.record_uid=(SELECT replica_root FROM record WHERE uid=o.record_uid) AND p.namespace=? JOIN json_each(p.fds,'$.routes') r WHERE o.id=? AND json_extract(r.value,'$.control.owner_key')=? AND json_extract(r.value,'$.certificate.mailbox')=? AND json_extract(r.value,'$.certificate.signing_key')=?)")
                .bind(PARTICIPANTS_NAMESPACE).bind(&document.envelope.id).bind(&receipt.certificate.owner_key).bind(&receipt.certificate.mailbox).bind(&receipt.certificate.signing_key).fetch_one(&self.store.pool).await?;
            if !certified {
                return Err(invalid(
                    "This receipt belongs to an unrecognized recipient device",
                ));
            }
        }
        Ok(value)
    }

    pub async fn social_collect_private_once(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let network = self.social_network()?;
        let now = nucleus::execution::now().timestamp();
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let contexts=store::sqlx::query("SELECT e.record_uid,e.fds FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND (r.deleted_at IS NULL OR EXISTS(SELECT 1 FROM record_extension p JOIN record c ON c.uid=p.record_uid WHERE p.namespace=? AND c.organ_uid=? AND c.deleted_at IS NULL AND json_extract(p.fds,'$.context')=r.uid AND json_extract(p.fds,'$.state') IN ('pending','accepted'))) ORDER BY e.record_uid LIMIT 256")
            .bind(SESSION_AUTHORITY_NAMESPACE).bind(&organ).bind(PARTICIPANTS_NAMESPACE).bind(&organ).fetch_all(&self.store.pool).await?;
        let mut active = std::collections::HashSet::new();
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_receive_failure WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        for row in contexts {
            let context: String = row.get("record_uid");
            let state: Value = serde_json::from_str(&row.get::<String, _>("fds"))?;
            if let Ok(route) = serde_json::from_value::<CertifiedRoute>(
                state[format!("authorized_{cell}")].clone(),
            ) && request_auth::validate_route(&route, now).is_ok()
                && serde_json::to_value(&route.control)? == state["control"]
            {
                for service in route.route.services {
                    active.insert((context.clone(), service.clone()));
                    store::sqlx::query("INSERT INTO social_pickup_work(context,service) VALUES(?,?) ON CONFLICT DO NOTHING").bind(&context).bind(service).execute(&mut *tx).await?;
                }
            }
        }
        let rows=store::sqlx::query("SELECT context,service FROM social_pickup_work WHERE next_attempt<=? ORDER BY next_attempt,context,service LIMIT 8").bind(now).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        let mut imported = 0;
        for row in rows {
            let context: String = row.get("context");
            let service: String = row.get("service");
            if !active.contains(&(context.clone(), service.clone())) {
                store::sqlx::query("DELETE FROM social_pickup_work WHERE context=? AND service=?")
                    .bind(context)
                    .bind(service)
                    .execute(&self.store.pool)
                    .await?;
                continue;
            }
            let result = self.social_pickup_from(&network, &context, &service).await;
            let error = match result {
                Ok(count) => {
                    imported += count;
                    None
                }
                Err(error) => Some(error.to_string().chars().take(500).collect::<String>()),
            };
            store::sqlx::query("UPDATE social_pickup_work SET error=?,attempts=CASE WHEN ? IS NULL THEN 0 ELSE MIN(attempts+1,1000000) END,next_attempt=?+CASE WHEN ? IS NULL THEN 5 ELSE MIN(3600,5*(1<<MIN(attempts,10))) END WHERE context=? AND service=?")
                .bind(&error).bind(&error).bind(now).bind(&error).bind(&context).bind(&service).execute(&self.store.pool).await?;
        }
        Ok(imported)
    }

    async fn social_pickup_from(
        &self,
        network: &std::sync::Arc<dyn Network>,
        context: &str,
        service: &str,
    ) -> Result<usize, EngineError> {
        self.social_discard_failed_from(network, context, service)
            .await?;
        let access = self.social_private_access(context, vec![]).await?;
        let mailbox = access.mailbox.clone();
        let value = network
            .request(service, PublicRequest::CollectPrivate { access })
            .await?;
        if value["service"] != service || value["mailbox"] != mailbox {
            return Err(invalid(
                "The selected host returned another mailbox's contents",
            ));
        }
        let envelopes = value["envelopes"]
            .as_array()
            .ok_or_else(|| invalid("Incomplete private mailbox response"))?;
        if envelopes.len() > 8 || serde_json::to_vec(&value)?.len() > MAX_PRIVATE_FRAME_BYTES {
            return Err(invalid(
                "The private mailbox response exceeds its collection bound",
            ));
        }
        let mut receipts = Vec::new();
        let mut imported = 0;
        let mut receive_error = None;
        for item in envelopes {
            let document: PrivateDelivery = serde_json::from_value(item["document"].clone())?;
            let discarded: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_receive_failure WHERE context=? AND service=? AND envelope=? AND discard=1)").bind(context).bind(service).bind(&document.envelope.id).fetch_one(&self.store.pool).await?;
            if discarded {
                continue;
            }
            let admitted = item["accepted_at"]
                .as_i64()
                .ok_or_else(|| invalid("The mailbox omitted its admission time"))?;
            match self
                .social_receive_private(context, service, &document, admitted)
                .await
            {
                Ok(result) => {
                    store::sqlx::query("DELETE FROM social_receive_failure WHERE context=? AND service=? AND envelope=?").bind(context).bind(service).bind(&document.envelope.id).execute(&self.store.pool).await?;
                    if result["duplicate"] != true {
                        imported += 1;
                    }
                    receipts.push(serde_json::from_value::<RecipientReceipt>(
                        result["receipt"].clone(),
                    )?);
                }
                Err(error) => {
                    self.social_retain_receive_failure(
                        context,
                        service,
                        &document,
                        admitted,
                        &error.to_string(),
                    )
                    .await?;
                    tracing::debug!(%error,"Private ciphertext remains queued until its history/session prerequisite arrives or its lifetime ends");
                    receive_error = Some(error);
                }
            }
        }
        if receipts.is_empty()
            && let Some(error) = receive_error
        {
            return Err(error);
        }
        if !receipts.is_empty() {
            let envelopes: Vec<String> = receipts.iter().map(|r| r.envelope.clone()).collect();
            let access = self
                .social_private_access(context, envelopes.clone())
                .await?;
            let ack = network
                .request(
                    service,
                    PublicRequest::AcknowledgePrivate { access, receipts },
                )
                .await?;
            if ack["service"] != service
                || ack["mailbox"] != mailbox
                || ack["stage"] != "recipient-durable"
                || ack["envelopes"] != json!(envelopes)
            {
                return Err(invalid(
                    "The selected host did not acknowledge the exact durable imports",
                ));
            }
        }
        Ok(imported)
    }
}
