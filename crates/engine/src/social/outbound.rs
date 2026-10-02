use super::*;
use nucleus::social::requests::*;
use serde::{Deserialize, Serialize};
use store::sqlx::{Row, Sqlite, Transaction};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RequestDraft {
    pub post: Snippet,
    pub text: String,
    pub alias: String,
    pub services: Vec<String>,
    pub token: String,
    pub message: String,
    pub cell: String,
    pub operational_key: String,
    pub issued_at: i64,
    pub signature: String,
}

pub(super) async fn work_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    root: &str,
    context: &str,
    expiry: i64,
) -> Result<(), EngineError> {
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_message_work")
        .fetch_one(&mut **tx)
        .await?;
    if count >= 4096 {
        return Err(invalid("The retained private sending queue is full"));
    }
    store::sqlx::query("INSERT INTO social_message_work(record_uid,conversation,context,expires_at) VALUES(?,?,?,?) ON CONFLICT(record_uid) DO UPDATE SET next_attempt=0,error=NULL")
        .bind(record).bind(root).bind(context).bind(expiry).execute(&mut **tx).await?;
    Ok(())
}

impl Engine {
    pub(super) async fn social_resume_request(
        &self,
        record: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_own_record(record, actor).await?;
        let held = store::records::get_extension(&self.store.pool, record, REQUEST_DRAFT_NAMESPACE)
            .await?
            .ok_or_else(|| invalid("No retained private introduction draft"))?;
        if held.get("materialized").is_some() {
            return Err(invalid(
                "This introduction already has a Conversation; resume its saved Message instead",
            ));
        }
        let mut draft: RequestDraft = serde_json::from_value(held["draft"].clone())?;
        let now = nucleus::execution::now().timestamp();
        if now.saturating_sub(draft.issued_at) >= AUTHORITY_LIFETIME {
            return Err(invalid("This introduction draft has expired"));
        }
        validate_snippet(&draft.post, now)?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let signer = self.operational_key_for(&organ).await?;
        draft.cell = cell;
        draft.operational_key = signer.public_key_b64();
        draft.signature = signer.sign_bytes(&signing_bytes("private-request-draft", &draft)?);
        let mut next = held.clone();
        next["draft"] = serde_json::to_value(&draft)?;
        next["error"] = Value::Null;
        let mut tx = self.social_write_tx().await?;
        if owner::extension_on(&mut tx, record, REQUEST_DRAFT_NAMESPACE).await? != held {
            return Err(invalid("The introduction draft changed; refresh it"));
        }
        store::records::set_extension_on(&mut tx, record, REQUEST_DRAFT_NAMESPACE, &next).await?;
        work_on(
            &mut tx,
            record,
            record,
            record,
            draft.issued_at + AUTHORITY_LIFETIME,
        )
        .await?;
        tx.commit().await?;
        let ready = self.social_prepare_reply_keys(record, draft.services).await;
        if let Err(error) = ready {
            self.social_note_work_error(record, &error.to_string())
                .await?;
        }
        self.notify_query_changed();
        Ok(
            json!({"record":record,"status":"Introduction resumed on this device. History remains; fresh messaging keys wait for owner authorization"}),
        )
    }

    pub(super) async fn social_archive_request(
        &self,
        record: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_own_record(record, actor).await?;
        let now = nucleus::execution::now().timestamp();
        let mut tx = self.social_write_tx().await?;
        let draft = owner::extension_on(&mut tx, record, REQUEST_DRAFT_NAMESPACE).await?;
        if draft.get("draft").is_some() {
            if draft.get("materialized").is_some() {
                return Err(invalid(
                    "Archive the materialized Conversation instead of its key context",
                ));
            }
        } else {
            let p: ConversationParticipant = serde_json::from_value(
                owner::extension_on(&mut tx, record, PARTICIPANTS_NAMESPACE).await?,
            )?;
            if p.state == ConversationState::Accepted
                || p.state == ConversationState::Pending && p.started_at + AUTHORITY_LIFETIME > now
            {
                return Err(invalid(
                    "Close or decline this conversation before archiving it",
                ));
            }
            let pending:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_private_outbox o JOIN record m ON m.uid=o.record_uid WHERE m.replica_root=? AND o.state IN ('pending','stored','held') AND o.expires_at>?) OR EXISTS(SELECT 1 FROM social_message_work WHERE conversation=? AND expires_at>?)")
                .bind(record).bind(now).bind(record).bind(now).fetch_one(&mut *tx).await?;
            if pending {
                return Err(invalid(
                    "Keep this Conversation until its final messages are saved by the recipient or their delivery window ends",
                ));
            }
            let context_draft =
                owner::extension_on(&mut tx, &p.context, REQUEST_DRAFT_NAMESPACE).await?;
            if context_draft["materialized"]["conversation"] == record {
                let mut retained = context_draft;
                retained["archived"] = json!(true);
                store::records::set_extension_on(
                    &mut tx,
                    &p.context,
                    REQUEST_DRAFT_NAMESPACE,
                    &retained,
                )
                .await?;
            }
        }
        store::records::mark_deleted_on(&mut tx, record).await?;
        store::sqlx::query("DELETE FROM social_message_work WHERE record_uid=? OR conversation=?")
            .bind(record)
            .bind(record)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"status":"Archived on your devices. This cannot erase the other person's retained copy"}),
        )
    }

    pub(super) async fn social_requests(
        &self,
        actor: Option<&str>,
        after: Option<&str>,
    ) -> Result<Value, EngineError> {
        if after.is_some_and(|uid| !nucleus::valid_uid(uid, "r")) {
            return Err(invalid("Invalid private Requests page"));
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        let rows = store::sqlx::query("SELECT r.uid,r.head,e.namespace,e.fds FROM record r JOIN record_extension e ON e.record_uid=r.uid WHERE r.organ_uid=? AND r.deleted_at IS NULL AND (?='' OR r.uid<?) AND (e.namespace=? OR e.namespace=? AND json_extract(e.fds,'$.materialized') IS NULL) ORDER BY r.uid DESC LIMIT 32")
            .bind(&organ).bind(after.unwrap_or_default()).bind(after.unwrap_or_default()).bind(PARTICIPANTS_NAMESPACE).bind(REQUEST_DRAFT_NAMESPACE).fetch_all(&self.store.pool).await?;
        let full = rows.len() == 32;
        let mut last = None;
        let mut next = None;
        let mut size = 2048;
        let mut requests = Vec::new();
        for row in rows {
            let uid: String = row.get("uid");
            if self.social_own_record(&uid, actor).await.is_err() {
                last = Some(uid);
                continue;
            }
            let namespace: String = row.get("namespace");
            let state: Value = serde_json::from_str(&row.get::<String, _>("fds"))?;
            let messages = store::sqlx::query("SELECT r.uid,r.body,e.fds,(SELECT fds FROM record_extension WHERE record_uid=r.uid AND namespace=?) AS delivery FROM record r JOIN record_extension e ON e.record_uid=r.uid AND e.namespace=? WHERE r.replica_root=? AND r.deleted_at IS NULL ORDER BY CAST(json_extract(e.fds,'$.content.issued_at') AS INTEGER) DESC,r.uid LIMIT 16")
                .bind(DELIVERY_NAMESPACE).bind(MESSAGE_NAMESPACE).bind(&uid).fetch_all(&self.store.pool).await?;
            let mut messages: Vec<Value> = messages.into_iter().map(|r| Ok(json!({"uid":r.get::<String,_>("uid"),"body":r.get::<String,_>("body"),"message":serde_json::from_str::<Value>(&r.get::<String,_>("fds"))?,"delivery":r.get::<Option<String>,_>("delivery").map(|s| serde_json::from_str::<Value>(&s)).transpose()?}))).collect::<Result<_,serde_json::Error>>()?;
            for message in &mut messages {
                let destinations=store::sqlx::query("SELECT d.service,SUM(d.state='pending') AS pending,SUM(d.state='stored') AS stored,SUM(d.state='cancelled') AS cancelled,MAX(d.error) AS error FROM social_private_destination d JOIN social_private_outbox o ON o.id=d.envelope WHERE o.record_uid=? GROUP BY d.service ORDER BY d.service LIMIT 8").bind(message["uid"].as_str().unwrap_or_default()).fetch_all(&self.store.pool).await?;
                message["destinations"]=json!(destinations.into_iter().map(|d|json!({"service":d.get::<String,_>("service"),"pending":d.get::<i64,_>("pending"),"stored":d.get::<i64,_>("stored"),"cancelled":d.get::<i64,_>("cancelled"),"error":d.get::<Option<String>,_>("error")})).collect::<Vec<_>>());
            }
            let reveal = store::records::get_extension(&self.store.pool, &uid, REVEAL_NAMESPACE)
                .await?
                .unwrap_or(Value::Null);
            let verification = if namespace == PARTICIPANTS_NAMESPACE {
                let p: ConversationParticipant = serde_json::from_value(state.clone())?;
                self.social_reveal_status(&p, &reveal, nucleus::execution::now().timestamp())
                    .await?
            } else {
                Value::Null
            };
            let request = json!({"record":uid,"title":row.get::<String,_>("head"),"draft":namespace==REQUEST_DRAFT_NAMESPACE,"state":state,"reveal":reveal,"verification":verification,"messages":messages});
            let length = serde_json::to_vec(&request)?.len();
            if size + length > 2 * 1024 * 1024 {
                if requests.is_empty() {
                    return Err(invalid(
                        "This private Request exceeds its display bound; resolve oversized retained state",
                    ));
                }
                next = last.clone();
                break;
            }
            last = Some(uid);
            size += length;
            requests.push(request);
        }
        if next.is_none() && full {
            next = last;
        }
        let can_edit = self.social_require_local_write().await.is_ok()
            && self
                .require_permission(actor, "record:update")
                .await
                .is_ok();
        let mut blocks = Vec::new();
        for context in admission::contexts(&self.store.pool, &organ).await? {
            if self
                .social_block_context_visible(&context, actor)
                .await
                .is_err()
            {
                continue;
            }
            let mut tx = self.social_write_tx().await?;
            let map = admission::map_on(&mut tx, &context).await?;
            tx.commit().await?;
            for (peer, entry) in map
                .as_object()
                .ok_or_else(|| invalid("Invalid private block map"))?
            {
                if entry["blocked"] == true {
                    if blocks.len() == 256 {
                        return Err(invalid("The retained block list exceeds its bound"));
                    }
                    blocks.push(json!({"context":context,"peer":peer}));
                }
            }
        }
        Ok(
            json!({"requests":requests,"blocks":blocks,"receive_failures":self.social_receive_failures(actor).await?,"next_after":next,"can_edit":can_edit,"status":"Private history syncs with your authorized devices. Mailbox storage and recipient import are separate delivery stages"}),
        )
    }

    pub(super) async fn social_open_request(
        &self,
        post: Snippet,
        text: String,
        alias: String,
        services: Vec<String>,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let now = nucleus::execution::now().timestamp();
        validate_snippet(&post, now)?;
        if post.state != PostState::Active || post.reply.is_none() {
            return Err(invalid(
                "This announcement is not accepting private introductions",
            ));
        }
        nucleus::social::text(&text, 8000, false).map_err(invalid)?;
        if text.len() > MAX_INTRO_BYTES {
            return Err(invalid("Keep an introduction within 2 KiB of UTF-8 text"));
        }
        nucleus::social::text(&alias, 80, true).map_err(invalid)?;
        if text.trim().is_empty() {
            return Err(invalid("Write your private introduction"));
        }
        if services.is_empty()
            || services.len() > 8
            || services
                .iter()
                .any(|host| host.parse::<iroh::EndpointId>().is_err())
        {
            return Err(invalid("Choose one to eight valid pinned mailbox hosts"));
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let signer = self.operational_key_for(&organ).await?;
        let mut draft = RequestDraft {
            post,
            text,
            alias,
            services: services.clone(),
            token: nucleus::new_uid("talk"),
            message: nucleus::new_uid("msg"),
            cell,
            operational_key: signer.public_key_b64(),
            issued_at: now,
            signature: String::new(),
        };
        draft.signature = signer.sign_bytes(&signing_bytes("private-request-draft", &draft)?);
        let body = json!({"draft":draft});
        if serde_json::to_vec(&body)?.len() > 64 * 1024 {
            return Err(invalid("The private introduction draft is oversized"));
        }
        let context = nucleus::new_uid("r");
        let mut tx = self.social_write_tx().await?;
        let (count,bytes):(i64,i64) = store::sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(e.fds AS BLOB))),0) FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND r.deleted_at IS NULL AND json_extract(e.fds,'$.materialized') IS NULL AND CAST(json_extract(e.fds,'$.draft.issued_at') AS INTEGER)>?")
            .bind(REQUEST_DRAFT_NAMESPACE).bind(&organ).bind(now-AUTHORITY_LIFETIME).fetch_one(&mut *tx).await?;
        if count >= 32 || bytes + serde_json::to_vec(&body)?.len() as i64 > 1024 * 1024 {
            return Err(invalid("Your pending introduction draft partition is full"));
        }
        conversation::record_on(
            &mut tx,
            &context,
            nucleus::RecordKind::MessageDraft,
            &organ,
            &context,
            "Private introduction draft",
            "",
        )
        .await?;
        store::records::set_extension_on(&mut tx, &context, REQUEST_DRAFT_NAMESPACE, &body).await?;
        work_on(
            &mut tx,
            &context,
            &context,
            &context,
            now + AUTHORITY_LIFETIME,
        )
        .await?;
        tx.commit().await?;
        self.social_own_record(&context, actor).await?;
        let prepared = self.social_prepare_reply_keys(&context, services).await;
        if let Err(error) = prepared {
            self.social_note_work_error(&context, &error.to_string())
                .await?;
        }
        self.notify_query_changed();
        Ok(
            json!({"record":context,"status":"Private introduction saved. Delivery waits for fresh owner-authorized keys and your selected mailboxes"}),
        )
    }

    pub(super) async fn social_materialize_request(
        &self,
        context: &str,
    ) -> Result<(), EngineError> {
        let live: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND deleted_at IS NULL)",
        )
        .bind(context)
        .fetch_one(&self.store.pool)
        .await?;
        if !live {
            return Err(invalid("This private introduction draft was deleted"));
        }
        let held =
            store::records::get_extension(&self.store.pool, context, REQUEST_DRAFT_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("No private introduction draft"))?;
        if held["archived"] == true {
            return Err(invalid(
                "This introduction context is archived; start a fresh Request",
            ));
        }
        if held.get("materialized").is_some() {
            return Ok(());
        }
        let draft: RequestDraft = serde_json::from_value(held["draft"].clone())?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        if draft.cell != cell {
            return Err(invalid(
                "Resume this introduction explicitly before sending it from another device",
            ));
        }
        self.social_prepare_reply_keys(context, draft.services.clone())
            .await?;
        let now = nucleus::execution::now().timestamp();
        validate_snippet(&draft.post, now)?;
        if draft.post.state != PostState::Active
            || now.saturating_sub(draft.issued_at) > AUTHORITY_LIFETIME
            || draft.issued_at > now + 300
            || !crate::roster::verify_with(
                &draft.operational_key,
                &signing_bytes("private-request-draft", &draft)?,
                &draft.signature,
            )
        {
            return Err(invalid(
                "This introduction draft is expired or could not be authenticated",
            ));
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        if let Some(roster) = self.roster_of(&organ).await? {
            if !crate::roster::roster_signature_is_valid(&roster)
                || !roster.roster.cells.iter().any(|c| {
                    c.cell_uid == draft.cell
                        && c.operational_key == draft.operational_key
                        && c.may(crate::roster::CAP_WRITE)
                })
            {
                return Err(invalid(
                    "The introduction's originating device was removed or changed",
                ));
            }
        } else if self.operational_key_for(&organ).await?.public_key_b64() != draft.operational_key
        {
            return Err(invalid("The private draft uses another device's key"));
        }
        let ready = self.social_reply_key_status(context).await?;
        let local: CertifiedRoute =
            serde_json::from_value(ready["route"].clone()).map_err(|_| {
                invalid("Waiting for the owner to authorize this introduction's device keys")
            })?;
        let peer = draft
            .post
            .reply
            .as_ref()
            .ok_or_else(|| invalid("The public reply route is unavailable"))?;
        request_auth::validate_route(&local, now)?;
        request_auth::validate_route(peer, now)?;
        if local.control.owner_key == peer.control.owner_key {
            return Err(invalid("This is your own private reply identity"));
        }
        let mut owners = [
            local.control.owner_key.as_str(),
            peer.control.owner_key.as_str(),
        ];
        owners.sort();
        let root = conversation::scoped_uid("r", &(&organ, &draft.token, owners))?;
        let participant = ConversationParticipant {
            token: draft.token.clone(),
            context: context.into(),
            local_owner: local.control.owner_key.clone(),
            peer_owner: peer.control.owner_key.clone(),
            alias: if draft.post.alias.is_empty() {
                "Anonymous participant".into()
            } else {
                draft.post.alias.clone()
            },
            routes: vec![peer.clone()],
            state: ConversationState::Pending,
            incoming: false,
            started_at: draft.issued_at,
            local_accepted: true,
            peer_accepted: false,
            provisional_sent: 0,
        };
        let content = PrivateContent {
            protocol: "lince.private-content.1".into(),
            conversation: draft.token,
            message: draft.message,
            author_owner: local.control.owner_key.clone(),
            issued_at: now,
            kind: ContentKind::Introduction {
                post: draft.post.id,
                text: draft.text,
                alias: draft.alias,
                reply: Box::new(local),
            },
        };
        self.social_save_content(&root, participant, content, Some((context, held)), false)
            .await?;
        Ok(())
    }

    pub(crate) async fn social_send_text(
        &self,
        root: &str,
        text: String,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_send_message(root, text, Vec::new(), actor)
            .await
    }

    pub(crate) async fn social_send_message(
        &self,
        root: &str,
        text: String,
        parts: Vec<nucleus::message::MessagePart>,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_authorized(actor, "record:create", async move {
            self.social_own_record(root, actor).await?;
            let participant = self.social_participant(root).await?;
            let content = PrivateContent {
                protocol: "lince.private-content.1".into(),
                conversation: participant.token.clone(),
                message: nucleus::new_uid("msg"),
                author_owner: participant.local_owner.clone(),
                issued_at: nucleus::execution::now().timestamp(),
                kind: if parts.is_empty() {
                    ContentKind::Text { text }
                } else {
                    ContentKind::Message {
                        text,
                        content: parts,
                    }
                },
            };
            self.social_save_content(root, participant, content, None, false)
                .await
        })
        .await
    }

    pub(super) async fn social_participant(
        &self,
        root: &str,
    ) -> Result<ConversationParticipant, EngineError> {
        serde_json::from_value(
            store::records::get_extension(&self.store.pool, root, PARTICIPANTS_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("This Conversation is not a private social request"))?,
        )
        .map_err(Into::into)
    }

    pub(super) async fn social_save_content(
        &self,
        root: &str,
        mut participant: ConversationParticipant,
        content: PrivateContent,
        draft: Option<(&str, Value)>,
        blocked: bool,
    ) -> Result<Value, EngineError> {
        self.social_require_local_write().await?;
        conversation::validate_content(&content)?;
        if !matches!(
            participant.state,
            ConversationState::Pending | ConversationState::Accepted
        ) {
            return Err(invalid("This private conversation is closed"));
        }
        let now = nucleus::execution::now();
        if participant.state == ConversationState::Pending
            && now.timestamp().saturating_sub(participant.started_at) > AUTHORITY_LIFETIME
        {
            return Err(invalid("This provisional conversation has expired"));
        }
        if content.kind.purpose() == EnvelopePurpose::Content
            && participant.state == ConversationState::Pending
        {
            if serde_json::to_vec(&content)?.len() > MAX_CONTENT_BYTES {
                return Err(invalid(
                    "Accept this conversation before sending larger attachments",
                ));
            }
            if participant.provisional_sent >= 3 {
                return Err(invalid(
                    "Accept this conversation before sending more than three provisional replies",
                ));
            }
            participant.provisional_sent += 1;
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let signer = self.signer.lock().await.clone();
        let thread_of = store::concepts::ensure(&self.store.pool, "thread-of").await?;
        let message_in = store::concepts::ensure(&self.store.pool, "message-in").await?;
        let thread = conversation::scoped_uid("r", &(root, "social-thread"))?;
        let message =
            conversation::scoped_uid("r", &(root, &content.author_owner, &content.message))?;
        let hash = document_hash("private-content", &content)?;
        let mut tx = self.social_write_tx().await?;
        self.social_require_local_write_on(&mut tx).await?;
        if admission::map_on(&mut tx, &participant.context).await?[&participant.peer_owner]["blocked"]
            == true
            && !matches!(content.kind, ContentKind::Decline | ContentKind::Close)
        {
            return Err(invalid(
                "This private identity is blocked; review consent before sending",
            ));
        }
        let previous = owner::extension_on(&mut tx, root, PARTICIPANTS_NAMESPACE).await?;
        if draft.is_none() {
            let old: ConversationParticipant = serde_json::from_value(previous.clone())?;
            let mut expected = participant.clone();
            expected.provisional_sent = old.provisional_sent;
            if serde_json::to_value(expected)? != previous {
                return Err(invalid(
                    "Another device changed this conversation; refresh before sending",
                ));
            }
        }
        match content.kind {
            ContentKind::Accept => {
                participant.local_accepted = true;
                if participant.peer_accepted {
                    participant.state = ConversationState::Accepted;
                }
            }
            ContentKind::Decline => participant.state = ConversationState::Declined,
            ContentKind::Close => participant.state = ConversationState::Closed,
            _ => {}
        }
        if blocked {
            participant.state = ConversationState::Blocked;
            admission::block_on(
                &mut tx,
                &participant.context,
                &participant.peer_owner,
                true,
                now.timestamp(),
            )
            .await?;
        }
        reveal::control_on(&mut tx, root, &participant, &content, true, now.timestamp()).await?;
        conversation::record_on(
            &mut tx,
            root,
            nucleus::RecordKind::Conversation,
            &organ,
            root,
            &participant.alias,
            "",
        )
        .await?;
        conversation::record_on(
            &mut tx,
            &thread,
            nucleus::RecordKind::Thread,
            &organ,
            root,
            &participant.alias,
            "",
        )
        .await?;
        conversation::link_on(&mut tx, &thread, root, &thread_of).await?;
        let text = match &content.kind {
            ContentKind::Introduction { text, .. }
            | ContentKind::Text { text }
            | ContentKind::Message { text, .. } => text.as_str(),
            ContentKind::Accept => "Conversation accepted",
            ContentKind::Decline => "Request declined",
            ContentKind::Close => "Conversation closed",
            ContentKind::Reveal { .. } => "Organ profile revealed",
            ContentKind::ContactRequest => "Contact connection requested",
            ContentKind::ContactAccept => "Contact connection accepted",
            _ => "Private conversation control",
        };
        if participant.state == ConversationState::Pending
            && content.kind.purpose() != EnvelopePurpose::Control
        {
            conversation::pending_budget_on(
                &mut tx,
                &organ,
                root,
                &message,
                serde_json::to_vec(&content)?.len() + text.len() + 4096,
                now.timestamp(),
            )
            .await?;
        }
        let created = conversation::record_on(
            &mut tx,
            &message,
            nucleus::RecordKind::Message,
            &organ,
            root,
            &text.chars().take(80).collect::<String>(),
            text,
        )
        .await?;
        let expected = conversation::message_metadata(&content, &hash)?;
        if !created && owner::extension_on(&mut tx, &message, MESSAGE_NAMESPACE).await? != expected
        {
            return Err(invalid("This retained Message has conflicting content"));
        }
        if created {
            if !content.kind.parts().is_empty() {
                store::message_content::save_on(&mut tx, &message, content.kind.parts()).await?;
            }
            store::records::set_extension_on(&mut tx, &message, MESSAGE_NAMESPACE, &expected)
                .await?;
        }
        conversation::link_on(&mut tx, &message, &thread, &message_in).await?;
        store::records::set_extension_on(
            &mut tx,
            root,
            PARTICIPANTS_NAMESPACE,
            &serde_json::to_value(&participant)?,
        )
        .await?;
        let expiry = if participant.state != ConversationState::Accepted {
            (participant.started_at + AUTHORITY_LIFETIME).min(now.timestamp() + AUTHORITY_LIFETIME)
        } else {
            now.timestamp() + 30 * 86400
        };
        store::records::set_extension_on(&mut tx,&message,DELIVERY_NAMESPACE,&json!({"origin_cell":cell,"context":participant.context,"expires_at":expiry,"stage":"waiting","error":null})).await?;
        work_on(&mut tx, &message, root, &participant.context, expiry).await?;
        if let Some((context, held)) = draft {
            if owner::extension_on(&mut tx, context, REQUEST_DRAFT_NAMESPACE).await? != held {
                return Err(invalid("The private introduction draft changed"));
            }
            let mut updated = held;
            updated["materialized"] = json!({"conversation":root,"message":message});
            store::records::set_extension_on(&mut tx, context, REQUEST_DRAFT_NAMESPACE, &updated)
                .await?;
            store::sqlx::query("DELETE FROM social_message_work WHERE record_uid=?")
                .bind(context)
                .execute(&mut *tx)
                .await?;
        }
        let fact = if created {
            conversation::saved_message_on(
                &mut tx,
                &message,
                root,
                &cell,
                &content,
                now,
                signer.as_ref(),
            )
            .await?
        } else {
            None
        };
        tx.commit().await?;
        if let Some(fact) = fact {
            self.publish_committed_fact(fact);
        }
        self.notify_query_changed();
        Ok(
            json!({"conversation":root,"thread":thread,"message":message,"status":"Message saved. Waiting for authorized keys and selected mailbox delivery"}),
        )
    }

    pub(super) async fn social_resume_private(
        &self,
        message: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_own_record(message, actor).await?;
        let root: Option<String> = store::sqlx::query_scalar(
            "SELECT replica_root FROM record WHERE uid=? AND deleted_at IS NULL",
        )
        .bind(message)
        .fetch_one(&self.store.pool)
        .await?;
        let root = root.ok_or_else(|| invalid("No private conversation root"))?;
        let participant = self.social_participant(&root).await?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let mut delivery =
            store::records::get_extension(&self.store.pool, message, DELIVERY_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("This is not your outgoing private message"))?;
        if delivery["stage"] == "recipient-refused" {
            return Err(invalid(
                "The recipient refused this Message; compose a new message if needed",
            ));
        }
        if delivery["stage"] == "recipient-durable" {
            return Err(invalid("The recipient already saved this message"));
        }
        let expiry = delivery["expires_at"]
            .as_i64()
            .ok_or_else(|| invalid("Missing delivery lifetime"))?;
        if expiry <= nucleus::execution::now().timestamp() {
            return Err(invalid(
                "This message's delivery window has expired; compose a new message",
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
        delivery["origin_cell"] = json!(cell);
        delivery["stage"] = json!("waiting");
        delivery["error"] = Value::Null;
        let mut tx = self.social_write_tx().await?;
        store::records::set_extension_on(&mut tx, message, DELIVERY_NAMESPACE, &delivery).await?;
        work_on(&mut tx, message, &root, &participant.context, expiry).await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"message":message,"status":"Retained message resumed on this device. Fresh authorized keys will preserve its logical identity"}),
        )
    }
}
