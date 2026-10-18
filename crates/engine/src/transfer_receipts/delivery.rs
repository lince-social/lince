use super::*;
use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use nucleus::{
    Cause, NewFact, RecordKind,
    transfer::{
        receipt::Command as ReceiptCommand,
        receipt_delivery::{self, Artifact, Command, Delivery, Request, Source},
    },
};
use serde_json::{Value, json};
use std::collections::HashSet;

fn result(message: &str, delivery: &Delivery, status: Value) -> Result<ActionOutcome, EngineError> {
    Ok(ActionOutcome {
        data: Some(
            json!({"receipt":{"state":"artifact_delivery","document":{"kind":"delivery","value":{"message":message,"artifact":delivery,"hash":delivery.hash().map_err(invalid)?,"status":status}}}}),
        ),
        ..Default::default()
    })
}

impl Engine {
    async fn delivery_recipients(
        &self,
        thread: &str,
        actor: Option<&str>,
    ) -> Result<(String, Vec<String>), EngineError> {
        self.refuse_unreadable(actor, &[thread.to_owned()]).await?;
        let row = store::records::get(&self.store.pool, thread)
            .await?
            .ok_or_else(|| invalid("Choose an available conversation Thread"))?;
        if row.kind != RecordKind::Thread.as_str() || !row.quantity.is_positive() {
            return Err(invalid("Choose a conversation Thread"));
        }
        let root = store::replica::root_of(&self.store.pool, thread)
            .await?
            .ok_or_else(|| invalid("This Thread has no Organ conversation"))?;
        self.refuse_unreadable(actor, std::slice::from_ref(&root))
            .await?;
        let root_record = store::records::get(&self.store.pool, &root)
            .await?
            .ok_or_else(|| invalid("The conversation is unavailable"))?;
        if root_record.kind != RecordKind::Conversation.as_str()
            || !root_record.quantity.is_positive()
            || store::records::get_extension(
                &self.store.pool,
                &root,
                nucleus::social::requests::PARTICIPANTS_NAMESPACE,
            )
            .await?
            .is_some()
        {
            return Err(invalid(
                "Choose a conversation with explicitly known Organ recipients",
            ));
        }
        let recipients:Vec<String>=store::sqlx::query_scalar("SELECT contact_organ FROM replica_grant WHERE root_record=? AND state='accepted' ORDER BY contact_organ LIMIT 65").bind(&root).fetch_all(&self.store.pool).await?;
        if recipients.is_empty() || recipients.len() > 64 {
            return Err(invalid(
                "Choose a conversation with 1–64 accepted Organ recipients",
            ));
        }
        for recipient in &recipients {
            let contact = store::organs::contact(&self.store.pool, recipient)
                .await?
                .ok_or_else(|| invalid("A conversation recipient is no longer known"))?;
            if contact.trust != "known" {
                return Err(EngineError::Forbidden(
                    "A conversation recipient is blocked or its identity is not accepted".into(),
                ));
            }
            if store::visibility::hidden_from_organ(&self.store.pool, recipient)
                .await?
                .contains(thread)
            {
                return Err(EngineError::Forbidden(
                    "The Thread is hidden from a recipient".into(),
                ));
            }
        }
        Ok((root, recipients))
    }

    async fn delivery_artifact(
        &self,
        source: &Source,
        person: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Artifact, EngineError> {
        match source {
            Source::Document {
                record,
                expected_version,
            } => {
                self.refuse_unreadable(actor, std::slice::from_ref(record))
                    .await?;
                let saved = store::transfer_receipts::get(&self.store.pool, record).await?;
                if saved.owner != person || saved.version != *expected_version {
                    return Err(invalid("Review your own current receipt before sending"));
                }
                self.verify_transfer_receipt_document(&saved.document, now)
                    .await?;
                Ok(Artifact::Document(saved.document))
            }
            Source::Participation {
                transfer,
                expected_revision,
                party,
            } => {
                self.require_permission(actor, "transfer:read").await?;
                Ok(Artifact::Participation(
                    self.prepare_transfer_participation(
                        transfer,
                        *expected_revision,
                        party.as_deref(),
                        actor,
                    )
                    .await?,
                ))
            }
        }
    }

    async fn delivery_preview(
        &self,
        person: &str,
        request: &Request,
        issued_at: i64,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(String, Delivery, CanonicalHash), EngineError> {
        if !nucleus::valid_uid(&request.thread, "r") {
            return Err(invalid(
                "Select a stable conversation Thread identity before review",
            ));
        }
        let thread = self.resolve(&request.thread).await?;
        let (root, recipients) = self.delivery_recipients(&thread, actor).await?;
        if recipients != request.recipients {
            return Err(invalid(
                "Review every current conversation recipient before sending",
            ));
        }
        let delivery = Delivery {
            domain: receipt_delivery::DOMAIN.into(),
            sender: store::organs::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("No local Organ"))?
                .uid,
            recipients,
            issued_at,
            expires_at: request.expires_at,
            artifact: self
                .delivery_artifact(&request.source, person, actor, now)
                .await?,
        };
        delivery.validate(now.timestamp()).map_err(invalid)?;
        if delivery.expires_at <= now.timestamp() {
            return Err(invalid("Choose an unexpired delivery term"));
        }
        let hash = canonical_hash(
            "lince.transfer-receipt-delivery.review.v1",
            &(person, request, &root, &delivery),
        )
        .map_err(|error| invalid(error.to_string()))?;
        Ok((root, delivery, hash))
    }

    async fn inspect_delivered_artifact(
        &self,
        message: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Delivery, EngineError> {
        self.refuse_unreadable(actor, &[message.to_owned()]).await?;
        let row = store::records::get(&self.store.pool, message)
            .await?
            .ok_or_else(|| invalid("Message is unavailable"))?;
        if row.kind != RecordKind::Message.as_str() {
            return Err(invalid("Choose an artifact Message"));
        }
        let parts = store::message_content::load(&self.store.pool, message).await?;
        let [
            nucleus::message::MessagePart::Attachment {
                mime_type, data, ..
            },
        ] = parts.as_slice()
        else {
            return Err(invalid(
                "Message must contain one receipt artifact attachment",
            ));
        };
        if mime_type != receipt_delivery::MIME
            || data.len() > receipt_delivery::MAX_BYTES.div_ceil(3) * 4
        {
            return Err(invalid("Invalid receipt attachment type or size"));
        }
        let bytes = B64
            .decode(data)
            .map_err(|_| invalid("Invalid artifact encoding"))?;
        let delivery = Delivery::parse(&bytes, now.timestamp()).map_err(invalid)?;
        if row.organ_uid.as_deref() != Some(delivery.sender.as_str()) {
            return Err(invalid("Message origin differs from its delivery sender"));
        }
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        if delivery.sender != local && !delivery.recipients.contains(&local) {
            return Err(EngineError::Forbidden(
                "This artifact names another recipient".into(),
            ));
        }
        Ok(delivery)
    }

    pub(crate) async fn receipt_delivery_command(
        &self,
        person: String,
        id: String,
        command: Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        match command {
            Command::Inbox { before } => {
                let principal = serde_json::to_string(&(&person, actor))?;
                let before = match before {
                    Some(token) => Some(
                        self.decode_read_cursor::<store::artifact_deliveries::InboxPosition>(
                            "receipt-delivery-inbox",
                            &principal,
                            &token,
                            now.timestamp(),
                        )
                        .await?,
                    ),
                    None => None,
                };
                let candidates =
                    store::artifact_deliveries::inbox_page(&self.store.pool, before.as_ref())
                        .await?;
                let visible = if let Some(actor) = actor {
                    let query = protein::Protein {
                        source: protein::Source::Record,
                        filter: vec![protein::Predicate::Any(
                            candidates
                                .iter()
                                .take(256)
                                .map(|entry| protein::Predicate::UidEq(entry.message.clone()))
                                .collect(),
                        )],
                        fields: Some(vec!["uid".into()]),
                        include: Default::default(),
                        aggregate: None,
                        order: vec![],
                        limit: Some(256),
                    };
                    Some(
                        protein::execute_for(&self.store, &query, Some(actor))
                            .await?
                            .into_iter()
                            .filter_map(|row| row["uid"].as_str().map(str::to_owned))
                            .collect::<HashSet<_>>(),
                    )
                } else {
                    None
                };
                let mut more = candidates.len() > 256;
                let mut position = None;
                let mut messages = Vec::new();
                for entry in candidates.into_iter().take(256) {
                    if messages.len() == 64 {
                        more = true;
                        break;
                    }
                    position = Some(store::artifact_deliveries::InboxPosition {
                        created_at: entry.created_at.clone(),
                        message: entry.message.clone(),
                    });
                    if visible
                        .as_ref()
                        .is_some_and(|visible| !visible.contains(&entry.message))
                    {
                        continue;
                    }
                    messages.push(json!({"message":entry.message,"label":entry.label,"outgoing":entry.outgoing,"created_at":entry.created_at}));
                }
                let next_before = if more {
                    match position {
                        Some(position) => Some(
                            self.encode_read_cursor(
                                "receipt-delivery-inbox",
                                &principal,
                                &position,
                                now.timestamp(),
                            )
                            .await?,
                        ),
                        None => None,
                    }
                } else {
                    None
                };
                Ok(ActionOutcome {
                    data: Some(
                        json!({"receipt":{"state":"delivery_inbox","document":{"kind":"delivery_inbox","value":{"messages":messages,"partial":next_before.is_some(),"next_before":next_before}}}}),
                    ),
                    ..Default::default()
                })
            }
            Command::Options { source } => {
                let artifact = self.delivery_artifact(&source, &person, actor, now).await?;
                let threads:Vec<(String,String)>=store::sqlx::query_as("SELECT uid,head FROM record WHERE kind='thread' AND replica_root IS NOT NULL AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 65").fetch_all(&self.store.pool).await?;
                let partial = threads.len() > 64;
                let mut options = Vec::new();
                for (thread, label) in threads.into_iter().take(64) {
                    if let Ok((_, recipients)) = self.delivery_recipients(&thread, actor).await {
                        options
                            .push(json!({"thread":thread,"label":label,"recipients":recipients}));
                    }
                }
                Ok(ActionOutcome {
                    data: Some(
                        json!({"receipt":{"state":"delivery_options","document":{"kind":"delivery_options","value":{"source":source,"artifact":artifact,"threads":options,"partial":partial}}}}),
                    ),
                    ..Default::default()
                })
            }
            Command::Preview { request } => {
                let (_, delivery, hash) = self
                    .delivery_preview(&person, &request, now.timestamp(), actor, now)
                    .await?;
                Ok(ActionOutcome {
                    data: Some(
                        json!({"receipt":{"state":"delivery_preview","document":{"kind":"delivery_preview","value":{"request":request,"artifact":delivery,"reviewed_at":now.timestamp(),"expected_projection":hash}}}}),
                    ),
                    ..Default::default()
                })
            }
            Command::Send {
                request,
                reviewed_at,
                expected_projection,
            } => {
                let request_json =
                    serde_json::to_string(&(&person, &request, reviewed_at, &expected_projection))?;
                if request_json.len() > 65536
                    || id.is_empty()
                    || id.len() > 256
                    || id.chars().any(char::is_control)
                {
                    return Err(invalid("Delivery request exceeds its budget"));
                }
                if let Some(saved) =
                    store::artifact_deliveries::replay(&self.store.pool, &id, &request_json).await?
                {
                    self.refuse_unreadable(actor, std::slice::from_ref(&saved.message))
                        .await?;
                    return result(
                        &saved.message,
                        &saved.delivery,
                        json!({"replayed":true,"cancelled":saved.cancelled}),
                    );
                }
                if reviewed_at > now.timestamp()
                    || now.timestamp().saturating_sub(reviewed_at) > 300
                {
                    return Err(invalid("Preview the exact delivery again before sending"));
                }
                let (root, delivery, hash) = self
                    .delivery_preview(&person, &request, reviewed_at, actor, now)
                    .await?;
                if hash != expected_projection {
                    return Err(invalid("Artifact or audience changed after review"));
                }
                let signer = self.transfer_person_signer(&person, authorship).await?;
                let message = nucleus::new_uid("r");
                let audit_record = nucleus::new_uid("r");
                let thread = self.resolve(&request.thread).await?;
                let predicate =
                    store::vocabulary::feature_write(&self.store.pool, "message").await?;
                let payload = serde_json::to_string(&delivery)?;
                let attachment = nucleus::message::MessagePart::Attachment {
                    name: "transfer-receipt.json".into(),
                    mime_type: receipt_delivery::MIME.into(),
                    data: B64.encode(payload.as_bytes()),
                };
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:create")
                    .await?;
                if !store::people::is_active_on(&mut tx, &person).await? {
                    return Err(EngineError::Forbidden(
                        "Delivery requires an active Person".into(),
                    ));
                }
                let current:Vec<String>=store::sqlx::query_scalar("SELECT contact_organ FROM replica_grant WHERE root_record=? AND state='accepted' ORDER BY contact_organ LIMIT 65").bind(&root).fetch_all(&mut *tx).await?;
                if current != delivery.recipients {
                    return Err(invalid("Conversation recipients changed after review"));
                }
                if let Source::Document {
                    record,
                    expected_version,
                } = &request.source
                {
                    let version: i64 = store::sqlx::query_scalar(
                        "SELECT MAX(version) FROM transfer_receipt_version WHERE record_uid=?",
                    )
                    .bind(record)
                    .fetch_one(&mut *tx)
                    .await?;
                    if version as u64 != *expected_version {
                        return Err(invalid("Receipt changed after review"));
                    }
                }
                let active:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM record_artifact_delivery WHERE cancelled_at IS NULL AND expires_at>?").bind(now.timestamp()).fetch_one(&mut *tx).await?;
                if active >= 256 {
                    return Err(invalid(
                        "At most 256 active artifact deliveries can be queued",
                    ));
                }
                let checkpoint = self
                    .record_checkpoint_on(
                        &mut tx,
                        actor,
                        vec![message.clone(), audit_record.clone()],
                        protein::authority::Operation::Create,
                    )
                    .await?;
                store::records::create_with_uid_on(&mut tx,&message,store::records::NewRecord {slug:None,kind:RecordKind::Message,head:"Transfer receipt artifact",body:"Inspect the attached artifact before importing or approving it. Transport delivery does not confirm a physical contribution.",quantity:store::exact::one()},&delivery.sender,Some(&root)).await?;
                store::records::create_with_uid_on(&mut tx,&audit_record,store::records::NewRecord {slug:None,kind:RecordKind::Plain,head:"Private artifact delivery review",body:"Private signed review of a Message artifact and its approved recipients.",quantity:store::exact::zero()},&delivery.sender,None).await?;
                if let Some(policy) = store::data_visibility::ensure_private_for_person_on(
                    &mut tx,
                    &audit_record,
                    nucleus::visibility::Data::Record,
                    Some(&person),
                )
                .await?
                {
                    store::sync_ops::log_local_tx(
                        &mut tx,
                        "data_visibility",
                        &audit_record,
                        "record",
                        store::sync_ops::OpKind::Set,
                        Some(serde_json::to_string(&policy)?),
                    )
                    .await?;
                }
                store::message_content::save_on(&mut tx, &message, &[attachment]).await?;
                store::assertions::insert_tx(
                    &mut tx,
                    &nucleus::new_uid("a"),
                    store::assertions::NewAssertion {
                        subject_uid: &message,
                        predicate_uid: &predicate,
                        object_uid: Some(&thread),
                        role: store::assertions::AssertionRole::Ordinary,
                        quantity: None,
                        unit_uid: None,
                        asserted_by: Some(&delivery.sender),
                    },
                )
                .await?;
                let audience = nucleus::visibility::Policy {
                    include: vec![
                        nucleus::visibility::Condition {
                            organs: vec![delivery.sender.clone()],
                            persons: vec![person.clone()],
                            ..Default::default()
                        },
                        nucleus::visibility::Condition {
                            organs: delivery.recipients.clone(),
                            ..Default::default()
                        },
                    ],
                    exclude: vec![],
                };
                store::data_visibility::save_on(
                    &mut tx,
                    &message,
                    nucleus::visibility::Data::Record,
                    Some(&person),
                    &audience,
                    0,
                )
                .await?;
                store::sync_ops::log_local_tx(
                    &mut tx,
                    "data_visibility",
                    &message,
                    "record",
                    store::sync_ops::OpKind::Set,
                    Some(serde_json::to_string(
                        &json!({"controller_uid":person,"revision":1,"policy":audience}),
                    )?),
                )
                .await?;
                let mut fact = nucleus::fact::seal(
                    NewFact {
                        uid: None,
                        record_uid: audit_record,
                        delta: store::exact::zero(),
                        at: None,
                        actor_uid: Some(person.clone()),
                        cause: Cause::user_edit(),
                        payload: Some(serde_json::to_string(&delivery.private_audit(&message))?),
                    },
                    &store::facts::last_hash(&mut tx).await?,
                    now,
                );
                fact.signature = signer.as_ref().map(|signer| signer.sign_hash(&fact.hash));
                store::facts::insert(&mut tx, &fact).await?;
                if let Some(proof) = authorship {
                    store::action_intents::link_pending_fact(&mut tx, &proof.intent_uid, &fact)
                        .await?;
                }
                let (source_record, source_version) = match &request.source {
                    Source::Document {
                        record,
                        expected_version,
                    } => (Some(record.as_str()), Some(*expected_version as i64)),
                    _ => (None, None),
                };
                store::sqlx::query("INSERT INTO record_artifact_delivery(message_uid,owner_person_uid,request_id,request_json,artifact_json,artifact_hash,recipients_json,source_record_uid,source_version,fact_uid,expires_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(&message).bind(&person).bind(&id).bind(&request_json).bind(&payload).bind(delivery.hash().map_err(invalid)?.as_str()).bind(serde_json::to_string(&delivery.recipients)?).bind(source_record).bind(source_version).bind(&fact.uid).bind(delivery.expires_at).execute(&mut *tx).await?;
                if let Some(checkpoint) = checkpoint {
                    checkpoint
                        .bounded()
                        .finish(&mut tx, Default::default())
                        .await?;
                }
                tx.commit().await?;
                let mut outcome = result(&message, &delivery, json!({"queued":true}))?;
                outcome.created = Some(message);
                outcome.facts = self.publish_committed_fact(fact);
                self.notify_query_changed();
                Ok(outcome)
            }
            Command::Inspect { message } => {
                let message = self.resolve(&message).await?;
                let delivery = self
                    .inspect_delivered_artifact(&message, actor, now)
                    .await?;
                let verification = match &delivery.artifact {
                    Artifact::Document(document) => {
                        match self.verify_transfer_receipt_document(document, now).await {
                            Ok(()) => json!({"state":"verified"}),
                            Err(EngineError::Conflict { code, message }) => {
                                json!({"state":code,"message":message})
                            }
                            Err(error) => return Err(error),
                        }
                    }
                    Artifact::Participation(_) => json!({"state":"unsigned_participation_request"}),
                };
                let outgoing = store::artifact_deliveries::get(&self.store.pool, &message)
                    .await
                    .ok();
                let queued: i64 =
                    store::sqlx::query_scalar("SELECT COUNT(*) FROM sync_outbox WHERE uid=?")
                        .bind(&message)
                        .fetch_one(&self.store.pool)
                        .await?;
                result(
                    &message,
                    &delivery,
                    json!({"verification":verification,"expired":delivery.expires_at<=now.timestamp(),"outgoing":outgoing.as_ref().is_some_and(|saved|saved.owner==person),"cancelled":outgoing.as_ref().is_some_and(|saved|saved.cancelled),"queued_operations":queued,"acknowledgment":"Transport status is not physical receipt or settlement"}),
                )
            }
            Command::Import {
                message,
                expected_projection,
            } => {
                let message = self.resolve(&message).await?;
                let delivery = self
                    .inspect_delivered_artifact(&message, actor, now)
                    .await?;
                if delivery.hash().map_err(invalid)? != expected_projection
                    || delivery.expires_at <= now.timestamp()
                {
                    return Err(invalid(
                        "Review an unchanged unexpired artifact before importing",
                    ));
                }
                let Artifact::Document(document) = delivery.artifact else {
                    return Err(invalid(
                        "Review and explicitly authorize a participation request instead of importing it as a receipt",
                    ));
                };
                Box::pin(self.transfer_receipt_command(
                    Some(&person),
                    id,
                    ReceiptCommand::Import {
                        document: serde_json::to_string(&document)?,
                    },
                    actor,
                    now,
                    authorship,
                ))
                .await
            }
            Command::Cancel { message } => {
                let message = self.resolve(&message).await?;
                self.refuse_unreadable(actor, std::slice::from_ref(&message))
                    .await?;
                let saved = store::artifact_deliveries::get(&self.store.pool, &message).await?;
                if saved.owner != person {
                    return Err(EngineError::Forbidden(
                        "Only the sender can cancel a delivery".into(),
                    ));
                }
                if saved.cancelled {
                    return result(&message, &saved.delivery, json!({"cancelled":true}));
                }
                let signer = self.transfer_person_signer(&person, authorship).await?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:update")
                    .await?;
                let checkpoint = self
                    .record_checkpoint_on(
                        &mut tx,
                        actor,
                        vec![message.clone(), saved.fact.record_uid.clone()],
                        protein::authority::Operation::Update,
                    )
                    .await?;
                store::sqlx::query("UPDATE record_artifact_delivery SET cancelled_at=COALESCE(cancelled_at,?) WHERE message_uid=?").bind(now.to_rfc3339()).bind(&message).execute(&mut *tx).await?;
                let mut fact = nucleus::fact::seal(
                    NewFact {
                        uid: None,
                        record_uid: saved.fact.record_uid.clone(),
                        delta: store::exact::zero(),
                        at: None,
                        actor_uid: Some(person),
                        cause: Cause::user_edit(),
                        payload: Some(serde_json::to_string(
                            &json!({"artifact_delivery_cancelled":saved.delivery.hash().map_err(invalid)?}),
                        )?),
                    },
                    &store::facts::last_hash(&mut tx).await?,
                    now,
                );
                fact.signature = signer.as_ref().map(|signer| signer.sign_hash(&fact.hash));
                store::facts::insert(&mut tx, &fact).await?;
                if let Some(proof) = authorship {
                    store::action_intents::link_pending_fact(&mut tx, &proof.intent_uid, &fact)
                        .await?;
                }
                if let Some(checkpoint) = checkpoint {
                    checkpoint
                        .bounded()
                        .finish(&mut tx, Default::default())
                        .await?;
                }
                tx.commit().await?;
                let mut outcome = result(
                    &message,
                    &saved.delivery,
                    json!({"cancelled":true,"prior_disclosure":"Already received bytes cannot be recalled"}),
                )?;
                outcome.facts = self.publish_committed_fact(fact);
                Ok(outcome)
            }
            Command::Retry { message } => {
                let message = self.resolve(&message).await?;
                self.refuse_unreadable(actor, std::slice::from_ref(&message))
                    .await?;
                let saved = store::artifact_deliveries::get(&self.store.pool, &message).await?;
                if saved.owner != person || saved.cancelled {
                    return Err(EngineError::Forbidden(
                        "Only the sender can retry an uncancelled delivery".into(),
                    ));
                }
                for recipient in &saved.delivery.recipients {
                    if self
                        .outgoing_hidden_from_organ(recipient)
                        .await?
                        .contains(&message)
                    {
                        return Err(invalid(
                            "Current audience, authority or term withholds delivery",
                        ));
                    }
                }
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:update")
                    .await?;
                let checkpoint = self
                    .record_checkpoint_on(
                        &mut tx,
                        actor,
                        vec![message.clone()],
                        protein::authority::Operation::Update,
                    )
                    .await?;
                if store::artifact_deliveries::get_on(&mut tx, &message)
                    .await?
                    .cancelled
                {
                    return Err(invalid("This delivery was cancelled"));
                }
                store::sqlx::query("INSERT INTO sync_outbox(contact_organ,tbl,uid,field,kind,seq,queued_at) SELECT j.value,o.tbl,o.uid,o.field,o.kind,o.seq,? FROM sync_op o, json_each(?) j WHERE (o.tbl IN ('record','record_extension') AND o.uid=?) OR (o.tbl='fact' AND o.uid IN (SELECT uid FROM fact WHERE record_uid=?)) OR (o.tbl='record_assertion' AND o.uid IN (SELECT uid FROM record_assertion WHERE subject_uid=?)) ON CONFLICT(contact_organ,tbl,uid,field,kind) DO UPDATE SET seq=excluded.seq,queued_at=excluded.queued_at").bind(now.to_rfc3339()).bind(serde_json::to_string(&saved.delivery.recipients)?).bind(&message).bind(&message).bind(&message).execute(&mut *tx).await?;
                if let Some(checkpoint) = checkpoint {
                    checkpoint
                        .bounded()
                        .finish(&mut tx, Default::default())
                        .await?;
                }
                tx.commit().await?;
                result(
                    &message,
                    &saved.delivery,
                    json!({"queued":true,"retry":true}),
                )
            }
        }
    }

    pub(crate) async fn outgoing_hidden_from_organ(
        &self,
        organ: &str,
    ) -> Result<HashSet<String>, EngineError> {
        let mut hidden = store::visibility::hidden_from_organ(&self.store.pool, organ).await?;
        for (message, outgoing) in store::artifact_deliveries::messages(&self.store.pool).await? {
            let allowed = if outgoing {
                self.artifact_delivery_may_send(&message, organ).await
            } else {
                self.received_artifact_may_forward(&message, organ).await
            };
            if !matches!(allowed, Ok(true)) {
                hidden.insert(message);
            }
        }
        Ok(hidden)
    }

    async fn received_artifact_may_forward(
        &self,
        message: &str,
        organ: &str,
    ) -> Result<bool, EngineError> {
        let now = nucleus::execution::now();
        let delivery = self.inspect_delivered_artifact(message, None, now).await?;
        if delivery.expires_at <= now.timestamp()
            || !delivery.recipients.iter().any(|uid| uid == organ)
        {
            return Ok(false);
        }
        match &delivery.artifact {
            Artifact::Document(document) => {
                self.verify_transfer_receipt_document(document, now).await?
            }
            Artifact::Participation(_) => return Ok(false),
        }
        Ok(true)
    }

    async fn artifact_delivery_may_send(
        &self,
        message: &str,
        organ: &str,
    ) -> Result<bool, EngineError> {
        let saved = store::artifact_deliveries::get(&self.store.pool, message).await?;
        let now = nucleus::execution::now();
        if saved.cancelled
            || saved.delivery.expires_at <= now.timestamp()
            || !saved.delivery.recipients.iter().any(|uid| uid == organ)
            || !store::people::is_active(&self.store.pool, &saved.owner).await?
        {
            return Ok(false);
        }
        if !crate::trust::verify_fact(&self.store, &saved.fact).await? {
            return Ok(false);
        }
        if !self
            .may_read_record(Some(&saved.owner), &saved.fact.record_uid)
            .await?
        {
            return Ok(false);
        }
        let received = self.inspect_delivered_artifact(message, None, now).await?;
        if received != saved.delivery {
            return Ok(false);
        }
        let (_, request, _, _): (String, Request, i64, CanonicalHash) =
            serde_json::from_str(&saved.request_json)?;
        let thread = self.resolve(&request.thread).await?;
        let (root, recipients) = self
            .delivery_recipients(&thread, Some(&saved.owner))
            .await?;
        if store::replica::root_of(&self.store.pool, message)
            .await?
            .as_deref()
            != Some(root.as_str())
        {
            return Ok(false);
        }
        if !recipients.contains(&organ.to_owned()) {
            return Ok(false);
        }
        match &saved.delivery.artifact {
            Artifact::Document(document) => {
                if let Source::Document { record, .. } = &request.source {
                    if !self.may_read_record(Some(&saved.owner), record).await? {
                        return Ok(false);
                    }
                }
                self.verify_transfer_receipt_document(document, now).await?;
            }
            Artifact::Participation(terms) => {
                let Artifact::Participation(current) = self
                    .delivery_artifact(&request.source, &saved.owner, Some(&saved.owner), now)
                    .await?
                else {
                    return Ok(false);
                };
                if &current != terms {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
