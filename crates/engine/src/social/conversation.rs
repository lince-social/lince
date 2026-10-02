use super::*;
use nucleus::social::requests::*;
use sha2::{Digest, Sha256};
use store::sqlx::{Row, Sqlite, Transaction};

pub fn scoped_uid(prefix: &str, scope: &impl serde::Serialize) -> Result<String, EngineError> {
    let digest = Sha256::digest(serde_json::to_vec(scope)?);
    let mut time = [0u8; 8];
    time[2..].copy_from_slice(&digest[..6]);
    Ok(format!(
        "{prefix}_{}",
        nucleus::id::ulid_from(
            u64::from_be_bytes(time),
            u128::from_be_bytes(digest[..16].try_into().unwrap())
        )
    ))
}

pub fn validate_content(content: &PrivateContent) -> Result<(), EngineError> {
    let limit = if matches!(content.kind, ContentKind::Message { .. }) {
        MAX_MESSAGE_CONTENT_BYTES
    } else {
        MAX_CONTENT_BYTES
    };
    if serde_json::to_vec(content)?.len() > limit
        || content.protocol != "lince.private-content.1"
        || !nucleus::valid_uid(&content.conversation, "talk")
        || !nucleus::valid_uid(&content.message, "msg")
        || content.issued_at <= 0
    {
        return Err(invalid("Invalid private conversation content or size"));
    }
    request_auth::ed_key(&content.author_owner)?;
    match &content.kind {
        ContentKind::Introduction {
            post,
            text,
            alias,
            reply,
        } => {
            if !nucleus::valid_uid(post, "post") {
                return Err(invalid("The introduction does not identify a public post"));
            }
            nucleus::social::text(text, 8000, false).map_err(invalid)?;
            if text.len() > MAX_INTRO_BYTES {
                return Err(invalid("Keep an introduction within 2 KiB of UTF-8 text"));
            }
            nucleus::social::text(alias, 80, true).map_err(invalid)?;
            request_auth::validate_route(reply, content.issued_at)?;
            if text.trim().is_empty() || reply.control.owner_key != content.author_owner {
                return Err(invalid(
                    "The introduction reply route belongs to another participant",
                ));
            }
        }
        ContentKind::Text { text } | ContentKind::Message { text, .. } => {
            nucleus::social::text(text, 8000, false).map_err(invalid)?;
            if text.len() > MAX_TEXT_BYTES {
                return Err(invalid(
                    "Keep a private message within 16 KiB of UTF-8 text",
                ));
            }
            nucleus::message::validate(content.kind.parts()).map_err(invalid)?;
            if content.kind.parts().iter().any(|part| {
                !matches!(
                    part,
                    nucleus::message::MessagePart::Text { .. }
                        | nucleus::message::MessagePart::Attachment { .. }
                )
            }) {
                return Err(invalid(
                    "Private messages support attached files and text. Record references, shared questions and shared step lists require separate consent",
                ));
            }
            if text.trim().is_empty() && content.kind.parts().is_empty() {
                return Err(invalid("Write a message before sending"));
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn message_metadata(content: &PrivateContent, hash: &str) -> Result<Value, EngineError> {
    let kind = match &content.kind {
        ContentKind::Message { text, .. } => json!({"kind":"message","text":text,"content":[]}),
        kind => serde_json::to_value(kind)?,
    };
    Ok(json!({"content_hash":hash,"content":{
        "protocol":content.protocol,"conversation":content.conversation,
        "message":content.message,"author_owner":content.author_owner,
        "issued_at":content.issued_at,"kind":kind
    }}))
}

pub(super) async fn load_content_on(
    connection: &mut store::sqlx::SqliteConnection,
    message: &str,
    metadata: &Value,
) -> Result<PrivateContent, EngineError> {
    let mut content: PrivateContent = serde_json::from_value(metadata["content"].clone())?;
    if let ContentKind::Message { content: parts, .. } = &mut content.kind {
        *parts = store::message_content::load_on(connection, message).await?;
    }
    validate_content(&content)?;
    if document_hash("private-content", &content)? != metadata["content_hash"] {
        return Err(invalid(
            "The retained Message attachment contents differ from their authenticated hash",
        ));
    }
    Ok(content)
}

pub(super) async fn load_content(
    pool: &store::sqlx::SqlitePool,
    message: &str,
    metadata: &Value,
) -> Result<PrivateContent, EngineError> {
    let mut connection = pool.acquire().await?;
    load_content_on(&mut connection, message, metadata).await
}

pub(super) async fn record_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    kind: nucleus::RecordKind,
    organ: &str,
    root: &str,
    head: &str,
    body: &str,
) -> Result<bool, EngineError> {
    let row =
        store::sqlx::query("SELECT kind,organ_uid,replica_root,deleted_at FROM record WHERE uid=?")
            .bind(uid)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some(row) = row {
        if row.get::<String, _>("kind") != kind.as_str()
            || row.get::<Option<String>, _>("organ_uid").as_deref() != Some(organ)
            || row.get::<Option<String>, _>("replica_root").as_deref() != Some(root)
            || row.get::<Option<String>, _>("deleted_at").is_some()
        {
            return Err(invalid(
                "The scoped conversation Record has a conflicting identity or is deleted",
            ));
        }
        return Ok(false);
    }
    store::records::create_with_uid_on(
        tx,
        uid,
        store::records::NewRecord {
            slug: None,
            kind,
            head,
            body,
            quantity: store::exact::zero(),
        },
        organ,
        Some(root),
    )
    .await?;
    if matches!(
        kind,
        nucleus::RecordKind::Conversation
            | nucleus::RecordKind::Thread
            | nucleus::RecordKind::Message
    ) {
        let peer: String =
            store::sqlx::query_scalar("SELECT uid FROM record WHERE slug=? AND kind=?")
                .bind(store::cells::LOCAL_CELL_SLUG)
                .bind(nucleus::RecordKind::Device.as_str())
                .fetch_one(&mut **tx)
                .await?;
        let register = crate::record_change::Register {
            clock: nucleus::hlc::next(),
            peer,
            change_uid: scoped_uid("change", &(uid, "social-presence"))?,
            value: json!({"offset":"1","assigned":"1"}),
        };
        crate::record_change::apply_register(tx, uid, "quantity", &register).await?;
        store::sync_ops::log_local_tx(
            tx,
            "record",
            uid,
            "property:quantity",
            store::sync_ops::OpKind::Set,
            Some(serde_json::to_string(&register)?),
        )
        .await?;
    }
    Ok(true)
}

pub(super) async fn link_on(
    tx: &mut Transaction<'_, Sqlite>,
    child: &str,
    parent: &str,
    predicate: &str,
) -> Result<(), EngineError> {
    let exists: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_assertion WHERE subject_uid=? AND predicate_uid=? AND object_uid=? AND retracted_at IS NULL)")
        .bind(child).bind(predicate).bind(parent).fetch_one(&mut **tx).await?;
    if !exists {
        let uid = scoped_uid("a", &(child, parent, predicate))?;
        store::assertions::insert_tx(
            tx,
            &uid,
            store::assertions::NewAssertion {
                subject_uid: child,
                predicate_uid: predicate,
                object_uid: Some(parent),
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn pending_budget_on(
    tx: &mut Transaction<'_, Sqlite>,
    organ: &str,
    root: &str,
    message: &str,
    bytes: usize,
    now: i64,
) -> Result<(), EngineError> {
    let existing: bool = store::sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND deleted_at IS NULL)",
    )
    .bind(message)
    .fetch_one(&mut **tx)
    .await?;
    if existing {
        return Ok(());
    }
    let (count,held):(i64,bool)=store::sqlx::query_as("SELECT COUNT(*),COALESCE(MAX(e.record_uid=?),0) FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND r.deleted_at IS NULL AND json_extract(e.fds,'$.state')='pending' AND CAST(json_extract(e.fds,'$.started_at') AS INTEGER)>?")
        .bind(root).bind(PARTICIPANTS_NAMESPACE).bind(organ).bind(now-AUTHORITY_LIFETIME).fetch_one(&mut **tx).await?;
    let used:i64=store::sqlx::query_scalar("SELECT COALESCE(SUM(length(CAST(r.body AS BLOB)))+SUM((SELECT COALESCE(SUM(length(CAST(x.fds AS BLOB))),0) FROM record_extension x WHERE x.record_uid=r.uid)),0) FROM record r WHERE r.deleted_at IS NULL AND r.organ_uid=? AND r.replica_root IN (SELECT e.record_uid FROM record_extension e JOIN record p ON p.uid=e.record_uid WHERE e.namespace=? AND p.organ_uid=? AND p.deleted_at IS NULL AND json_extract(e.fds,'$.state')='pending' AND CAST(json_extract(e.fds,'$.started_at') AS INTEGER)>?)")
        .bind(organ).bind(PARTICIPANTS_NAMESPACE).bind(organ).bind(now-AUTHORITY_LIFETIME).fetch_one(&mut **tx).await?;
    if !held && count >= 32 || used.saturating_add(bytes as i64) > 1024 * 1024 {
        return Err(invalid(
            "Your retained pending-request partition is full. Accept, decline or archive older requests",
        ));
    }
    Ok(())
}

pub(super) async fn saved_message_on(
    tx: &mut Transaction<'_, Sqlite>,
    message: &str,
    root: &str,
    cell: &str,
    content: &PrivateContent,
    now: DateTime<Utc>,
    signer: Option<&Signer>,
) -> Result<Option<nucleus::Fact>, EngineError> {
    let event = scoped_uid("event", &(message, "social-message-saved"))?;
    let mut fact = nucleus::NewFact::quantity(
        message.to_owned(),
        store::exact::zero(),
        nucleus::Cause {
            kind: nucleus::CauseKind::Action,
            uid: Some(root.to_owned()),
        },
    );
    fact.uid = Some(scoped_uid(
        "f",
        &(message, cell, "social-message-imported"),
    )?);
    fact.payload=Some(json!({"message_content_hash":document_hash("private-content",content)?,"logical_message_event":event}).to_string());
    let fact = crate::append::append_one_in_transaction(tx, fact, now, signer).await?;
    if let Some(fact) = &fact {
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_message_event")
            .fetch_one(&mut **tx)
            .await?;
        if count >= 16384 {
            return Err(invalid("The retained Message event queue is full"));
        }
        store::sqlx::query("INSERT INTO social_message_event(event,record_uid,fact,issued_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING").bind(event).bind(message).bind(&fact.uid).bind(content.issued_at).execute(&mut **tx).await?;
    }
    Ok(fact)
}

impl Engine {
    pub async fn social_receive_private(
        &self,
        context: &str,
        service: &str,
        document: &PrivateDelivery,
        accepted_at: i64,
    ) -> Result<Value, EngineError> {
        self.social_require_local_write().await?;
        let now = nucleus::execution::now();
        let hash =
            request_auth::validate_collected_delivery(document, accepted_at, now.timestamp())?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let authority =
            store::records::get_extension(&self.store.pool, context, SESSION_AUTHORITY_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("Prepare this device's private reply account first"))?;
        let binding: PrivateOwnerBinding = serde_json::from_value(authority["binding"].clone())?;
        self.social_validate_private_binding(context, &binding)
            .await?;
        let route: CertifiedRoute =
            serde_json::from_value(authority[format!("authorized_{cell}")].clone())?;
        request_auth::validate_route(&route, now.timestamp())?;
        if route.control.owner_key != binding.owner_key
            || document.envelope.route != route.route.mailbox
            || !route.route.services.iter().any(|host| host == service)
            || serde_json::to_value(&route.control)? != authority["control"]
        {
            return Err(invalid(
                "This message belongs to another device, reply owner or selected mailbox",
            ));
        }
        let key = self.social_storage_key().await?;
        let signer = self.signer.lock().await.clone();
        let thread_of = store::concepts::ensure(&self.store.pool, "thread-of").await?;
        let message_in = store::concepts::ensure(&self.store.pool, "message-in").await?;
        let mut tx = self.social_write_tx().await?;
        if owner::extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await? != authority {
            return Err(invalid(
                "Reply authority changed while receiving the message",
            ));
        }
        self.social_require_local_write_on(&mut tx).await?;
        mailbox::anchor_control(&mut tx, &document.authorization.control).await?;
        store::sqlx::query("DELETE FROM social_private_seen WHERE expires_at<=?")
            .bind(now.timestamp())
            .execute(&mut *tx)
            .await?;
        let account_id = format!("account:{context}");
        let account_row: (String, i64) = store::sqlx::query_as(
            "SELECT body,version FROM social_device_state WHERE id=? AND kind='account'",
        )
        .bind(&account_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| invalid("This device's private account is missing"))?;
        let mut state: session::AccountState =
            session::open_local(&account_id, &account_row.0, &key)?;
        if state.route != route.route {
            return Err(invalid(
                "The authorized route does not match this device's live keys",
            ));
        }
        let seen: Option<(String, String)> =
            store::sqlx::query_as("SELECT hash,receipt FROM social_private_seen WHERE envelope=?")
                .bind(&document.envelope.id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((held, receipt)) = seen {
            if held != hash {
                return Err(invalid(
                    "This envelope identity already names different ciphertext",
                ));
            }
            let mut receipt: RecipientReceipt = serde_json::from_str(&receipt)?;
            receipt.at = now.timestamp().max(route.certificate.issued_at);
            receipt.certificate = route.certificate.clone();
            receipt.signature = state
                .signing_key()?
                .sign_bytes(&signing_bytes("recipient-receipt", &receipt)?);
            store::sqlx::query("UPDATE social_private_seen SET receipt=? WHERE envelope=?")
                .bind(serde_json::to_string(&receipt)?)
                .bind(&document.envelope.id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(json!({"duplicate":true,"receipt":receipt}));
        }
        let seen_count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_private_seen")
            .fetch_one(&mut *tx)
            .await?;
        if seen_count >= 16384 {
            return Err(invalid(
                "This device's retained receipt work budget is full",
            ));
        }
        let bytes = B64
            .decode(&document.envelope.ciphertext)
            .map_err(|_| invalid("Malformed ciphertext"))?;
        let encrypted =
            vodozemac::olm::OlmMessage::from_parts(document.envelope.message_type as usize, &bytes)
                .map_err(|_| invalid("Malformed session message"))?;
        let session_id = scoped_uid(
            "session",
            &(
                context,
                &document.envelope.identity_key,
                &document.envelope.session_id,
            ),
        )?;
        let held: Option<(String, i64)> = store::sqlx::query_as(
            "SELECT body,version FROM social_device_state WHERE id=? AND kind='session'",
        )
        .bind(&session_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (live, plaintext) = if let Some((body, _)) = &held {
            let mut live = session::open_session(body, &key)?;
            let plaintext = live.decrypt(&encrypted).map_err(|_| {
                invalid("The private message cannot be decrypted by this saved session")
            })?;
            (live, plaintext)
        } else {
            let vodozemac::olm::OlmMessage::PreKey(prekey) = &encrypted else {
                return Err(invalid(
                    "This device needs a fresh initial session before receiving this reply",
                ));
            };
            let mut account = state.account(&key)?;
            let incoming = account
                .create_inbound_session(
                    vodozemac::olm::SessionConfig::version_1(),
                    vodozemac::Curve25519PublicKey::from_base64(&document.envelope.identity_key)
                        .map_err(|_| invalid("Invalid sender session key"))?,
                    prekey,
                )
                .map_err(|_| {
                    invalid("The introduction could not establish an authenticated session")
                })?;
            state.account = account.pickle().encrypt(&key);
            (incoming.session, incoming.plaintext)
        };
        if live.session_id() != document.envelope.session_id
            || plaintext.len() > MAX_MESSAGE_CONTENT_BYTES
        {
            return Err(invalid(
                "The decrypted session identity or message size differs",
            ));
        }
        let content: PrivateContent = serde_json::from_slice(&plaintext)?;
        validate_content(&content)?;
        if document_hash("private-content", &content)? != document.envelope.content_hash
            || content.message != document.envelope.message
            || content.author_owner != document.envelope.sender_owner
            || content.kind.purpose() != document.envelope.purpose
            || content.issued_at > document.envelope.created_at.saturating_add(300)
        {
            return Err(invalid(
                "The private message differs from its authenticated envelope",
            ));
        }
        let mut owners = [binding.owner_key.as_str(), content.author_owner.as_str()];
        owners.sort();
        let root = scoped_uid("r", &(&organ, &content.conversation, owners))?;
        let thread = scoped_uid("r", &(&root, "social-thread"))?;
        let message = scoped_uid("r", &(&root, &content.author_owner, &content.message))?;
        let retained = owner::extension_on(&mut tx, &message, MESSAGE_NAMESPACE).await?;
        let expected = message_metadata(&content, &document.envelope.content_hash)?;
        let logical_duplicate = retained == expected;
        if !retained.is_null() && retained != json!({}) && !logical_duplicate {
            return Err(invalid(
                "This retained Message identity has different immutable content",
            ));
        }
        if !logical_duplicate
            && admission::map_on(&mut tx, context).await?[&content.author_owner]["blocked"] == true
        {
            return Err(invalid(
                "This private identity is blocked, including new conversation tokens",
            ));
        }
        let participant = owner::extension_on(&mut tx, &root, PARTICIPANTS_NAMESPACE).await?;
        let mut participant: ConversationParticipant = if participant.is_null()
            || participant == json!({})
        {
            let ContentKind::Introduction {
                post, alias, reply, ..
            } = &content.kind
            else {
                return Err(invalid(
                    "Retained participant history must arrive before this conversation continues",
                ));
            };
            let publication = owner::extension_on(&mut tx, context, PUBLICATION_NAMESPACE).await?;
            let published: Snippet = serde_json::from_value(publication["published"].clone())?;
            if published.id != *post
                || published.state != PostState::Active
                || validate_snippet(&published, now.timestamp()).is_err()
                || published
                    .reply
                    .as_ref()
                    .is_none_or(|reply| reply.control.owner_key != binding.owner_key)
                || now.timestamp() - content.issued_at > AUTHORITY_LIFETIME
            {
                return Err(invalid(
                    "This public post is no longer accepting introductions",
                ));
            }
            let (pending, bytes): (i64, i64) = store::sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(e.fds AS BLOB))),0) FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND r.deleted_at IS NULL AND json_extract(e.fds,'$.state')='pending' AND CAST(json_extract(e.fds,'$.started_at') AS INTEGER)>?")
                .bind(PARTICIPANTS_NAMESPACE).bind(&organ).bind(now.timestamp() - AUTHORITY_LIFETIME).fetch_one(&mut *tx).await?;
            if pending >= 32 || bytes + plaintext.len() as i64 > 1024 * 1024 {
                return Err(invalid("Your private stranger request partition is full"));
            }
            ConversationParticipant {
                token: content.conversation.clone(),
                context: context.into(),
                local_owner: binding.owner_key.clone(),
                peer_owner: content.author_owner.clone(),
                alias: alias.clone(),
                routes: vec![reply.as_ref().clone()],
                state: ConversationState::Pending,
                incoming: true,
                started_at: content.issued_at,
                local_accepted: false,
                peer_accepted: true,
                provisional_sent: 0,
            }
        } else {
            serde_json::from_value(participant)?
        };
        if participant.token != content.conversation
            || participant.context != context
            || participant.local_owner != binding.owner_key
            || participant.peer_owner != content.author_owner
            || !logical_duplicate
                && matches!(
                    participant.state,
                    ConversationState::Blocked
                        | ConversationState::Declined
                        | ConversationState::Closed
                )
            || !logical_duplicate
                && participant.state == ConversationState::Pending
                && now.timestamp() - participant.started_at > AUTHORITY_LIFETIME
        {
            return Err(invalid(
                "This private participant, conversation or provisional window is unavailable",
            ));
        }
        if !logical_duplicate
            && participant.state == ConversationState::Pending
            && plaintext.len() > MAX_CONTENT_BYTES
        {
            return Err(invalid(
                "Accept this conversation before receiving larger attachments",
            ));
        }
        let (text, title) = if logical_duplicate {
            let text: String = store::sqlx::query_scalar("SELECT body FROM record WHERE uid=?")
                .bind(&message)
                .fetch_one(&mut *tx)
                .await?;
            (text, participant.alias.clone())
        } else {
            reveal::control_on(
                &mut tx,
                &root,
                &participant,
                &content,
                false,
                now.timestamp(),
            )
            .await?;
            let (text, title) = match &content.kind {
                ContentKind::Introduction { text, alias, .. } => (
                    text.as_str(),
                    if alias.is_empty() {
                        "Anonymous request"
                    } else {
                        alias.as_str()
                    },
                ),
                ContentKind::Text { text } | ContentKind::Message { text, .. } => {
                    (text.as_str(), participant.alias.as_str())
                }
                ContentKind::Accept => {
                    participant.peer_accepted = true;
                    if participant.local_accepted {
                        participant.state = ConversationState::Accepted;
                    }
                    ("Conversation accepted", participant.alias.as_str())
                }
                ContentKind::Decline => {
                    participant.state = ConversationState::Declined;
                    ("Request declined", participant.alias.as_str())
                }
                ContentKind::Close => {
                    participant.state = ConversationState::Closed;
                    ("Conversation closed", participant.alias.as_str())
                }
                ContentKind::Reveal { .. } => {
                    ("Organ profile revealed", participant.alias.as_str())
                }
                ContentKind::ContactRequest => {
                    ("Contact connection requested", participant.alias.as_str())
                }
                ContentKind::ContactAccept => {
                    ("Contact connection accepted", participant.alias.as_str())
                }
                ContentKind::FreshRoute { reply } => {
                    request_auth::validate_route(reply, now.timestamp())?;
                    if reply.control.owner_key != participant.peer_owner {
                        return Err(invalid(
                            "The refreshed route belongs to another participant",
                        ));
                    }
                    participant.routes = vec![reply.as_ref().clone()];
                    ("Messaging keys refreshed", participant.alias.as_str())
                }
                _ => {
                    return Err(invalid(
                        "This private conversation control is not enabled yet",
                    ));
                }
            };
            (text.to_owned(), title.to_owned())
        };
        if participant.state == ConversationState::Pending
            && content.kind.purpose() != EnvelopePurpose::Control
        {
            pending_budget_on(
                &mut tx,
                &organ,
                &root,
                &message,
                serde_json::to_vec(&content)?.len() + text.len() + 4096,
                now.timestamp(),
            )
            .await?;
        }
        record_on(
            &mut tx,
            &root,
            nucleus::RecordKind::Conversation,
            &organ,
            &root,
            &title,
            "",
        )
        .await?;
        record_on(
            &mut tx,
            &thread,
            nucleus::RecordKind::Thread,
            &organ,
            &root,
            &title,
            "",
        )
        .await?;
        link_on(&mut tx, &thread, &root, &thread_of).await?;
        let created = record_on(
            &mut tx,
            &message,
            nucleus::RecordKind::Message,
            &organ,
            &root,
            &text.chars().take(80).collect::<String>(),
            &text,
        )
        .await?;
        let metadata = owner::extension_on(&mut tx, &message, MESSAGE_NAMESPACE).await?;
        let expected = message_metadata(&content, &document.envelope.content_hash)?;
        if !created && metadata != expected {
            return Err(invalid(
                "This retained Message identity has different immutable content",
            ));
        }
        if created {
            if !content.kind.parts().is_empty() {
                store::message_content::save_on(&mut tx, &message, content.kind.parts()).await?;
            }
            store::records::set_extension_on(&mut tx, &message, MESSAGE_NAMESPACE, &expected)
                .await?;
        }
        link_on(&mut tx, &message, &thread, &message_in).await?;
        store::records::set_extension_on(
            &mut tx,
            &root,
            PARTICIPANTS_NAMESPACE,
            &serde_json::to_value(&participant)?,
        )
        .await?;
        let identity = scoped_uid("msg", &(&root, &content.author_owner, &content.message))?;
        store::sqlx::query("INSERT INTO social_message_identity(message,content_hash,record_uid,conversation) VALUES(?,?,?,?) ON CONFLICT(message) DO NOTHING")
            .bind(&identity).bind(&document.envelope.content_hash).bind(&message).bind(&root).execute(&mut *tx).await?;
        let fact = if created {
            saved_message_on(
                &mut tx,
                &message,
                &root,
                &cell,
                &content,
                now,
                signer.as_ref(),
            )
            .await?
        } else {
            None
        };
        store::social::put_device_state_on(
            &mut tx,
            &account_id,
            "account",
            context,
            &session::seal_local(&account_id, &state, &key)?,
            Some(account_row.1),
            now.timestamp(),
        )
        .await?;
        store::social::put_device_state_on(
            &mut tx,
            &session_id,
            "session",
            context,
            &live.pickle().encrypt(&key),
            held.as_ref().map(|(_, version)| *version),
            now.timestamp(),
        )
        .await?;
        store::sqlx::query("INSERT INTO social_session_peer(context,peer,session_id) VALUES(?,?,?) ON CONFLICT(context,peer) DO UPDATE SET session_id=excluded.session_id")
            .bind(context).bind(&document.envelope.identity_key).bind(&session_id).execute(&mut *tx).await?;
        let mut receipt = RecipientReceipt {
            envelope: document.envelope.id.clone(),
            envelope_hash: hash.clone(),
            message: document.envelope.message.clone(),
            content_hash: document.envelope.content_hash.clone(),
            stage: ReceiptStage::RecipientDurable,
            at: now.timestamp().max(route.certificate.issued_at),
            certificate: route.certificate.clone(),
            signature: String::new(),
        };
        receipt.signature = state
            .signing_key()?
            .sign_bytes(&signing_bytes("recipient-receipt", &receipt)?);
        store::sqlx::query("INSERT INTO social_private_seen(envelope,hash,cipher_hash,message,content_hash,receipt,expires_at) VALUES(?,?,?,?,?,?,?)")
            .bind(&document.envelope.id).bind(hash).bind(nucleus::fact::sha256_hex(&bytes)).bind(identity)
            .bind(&document.envelope.content_hash).bind(serde_json::to_string(&receipt)?).bind(document.envelope.expires_at).execute(&mut *tx).await?;
        tx.commit().await?;
        if let Some(fact) = fact {
            self.publish_committed_fact(fact);
        }
        self.notify_query_changed();
        self.notify_notifications_changed();
        Ok(
            json!({"conversation":root,"thread":thread,"message":message,"duplicate":!created,"receipt":receipt}),
        )
    }
}
