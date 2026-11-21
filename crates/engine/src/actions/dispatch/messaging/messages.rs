// non-lean-yet

use super::super::{
    Action, ActionOutcome, Cause, CauseKind, ControlFlow, DateTime, DispatchOutcome, Engine,
    EngineError, MessageState, NewFact, RecordKind, Utc, VerifiedActionAuthorship,
    invalid_dispatch, message_head,
};

impl Engine {
    pub(in super::super) async fn dispatch_messaging_messages(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::CreateMessage {
                content,
                thread,
                body,
                author,
                state,
                parent,
                references,
            } => {
                let mut content = content;
                for part in &mut content {
                    if let nucleus::message::MessagePart::Question { question } = part {
                        if question.state != nucleus::question::State::Pending {
                            return Err(EngineError::Consequence(
                                "New questions must wait for an answer.".into(),
                            ));
                        }
                        if question.responder == "me" {
                            question.responder =
                                self.message_authorship(None, actor.as_deref()).await?.1;
                        }
                        let recipient = self.resolve(&question.responder).await?;
                        let row = store::records::get(&self.store.pool, &recipient)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(recipient.clone()))?;
                        if row.kind != RecordKind::Person.as_str()
                            && row.kind != RecordKind::Organ.as_str()
                        {
                            return Err(EngineError::Consequence(
                                "Choose a Person or Organ as the question responder.".into(),
                            ));
                        }
                        question.responder = recipient;
                    }
                }
                nucleus::message::validate(&content).map_err(EngineError::Consequence)?;
                if state == MessageState::Interrupted {
                    return Err(EngineError::Consequence(
                        "a new message may be writing or finished, not interrupted".into(),
                    ));
                }
                let thread_uid = self.resolve(&thread).await?;
                let thread_row = store::records::get(&self.store.pool, &thread_uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(thread.clone()))?;
                if thread_row.kind != RecordKind::Thread.as_str() {
                    return Err(EngineError::Consequence(format!(
                        "`{thread}` is a {} record, not a thread",
                        thread_row.kind
                    )));
                }
                if let Some(root) = store::replica::root_of(&self.store.pool, &thread_uid).await?
                    && store::records::get_extension(
                        &self.store.pool,
                        &root,
                        nucleus::social::requests::PARTICIPANTS_NAMESPACE,
                    )
                    .await?
                    .is_some()
                {
                    if state != MessageState::Finished || !references.is_empty() || parent.is_some()
                    {
                        return Err(EngineError::Consequence("Private conversations support finished messages with text and attached files. Shared Record references and replies to Messages require separate consent".into()));
                    }
                    let data = self
                        .social_send_message(&root, body, content, actor.as_deref())
                        .await?;
                    outcome.created = data["message"].as_str().map(str::to_owned);
                    outcome.data = Some(data);
                    return Ok(ControlFlow::Break(outcome));
                }
                if let Some(transfer_uid) = self.transfer_for_thread(&thread_uid).await? {
                    self.require_transfer_thread_writer(&transfer_uid, actor.as_deref(), now)
                        .await?;
                }
                let references = self.resolve_message_references(references).await?;
                let body = body.trim();
                if state == MessageState::Finished
                    && body.is_empty()
                    && references.is_empty()
                    && content.is_empty()
                {
                    return Err(EngineError::Consequence(
                        "message body and Record references cannot both be empty".into(),
                    ));
                }
                let head = if body.is_empty() && !content.is_empty() {
                    "Shared message contents".into()
                } else if body.is_empty() {
                    format!(
                        "Shared {} Record{}",
                        references.len(),
                        if references.len() == 1 { "" } else { "s" }
                    )
                } else {
                    message_head(body)
                };
                let replica_root = store::replica::root_of(&self.store.pool, &thread_uid).await?;
                let message = store::records::create_in_root(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: RecordKind::Message,
                        head: &head,
                        body,
                        quantity: if replica_root.is_some() {
                            store::exact::one()
                        } else {
                            store::exact::zero()
                        },
                    },
                    replica_root.as_deref(),
                )
                .await?;
                if !content.is_empty() {
                    store::message_content::save(&self.store.pool, &message.uid, &content).await?;
                }
                let message_in =
                    store::vocabulary::feature_write(&self.store.pool, "message").await?;
                store::assertions::assert(
                    &self.store.pool,
                    store::assertions::NewAssertion {
                        subject_uid: &message.uid,
                        predicate_uid: &message_in,
                        object_uid: Some(&thread_uid),
                        role: store::assertions::AssertionRole::Ordinary,
                        quantity: None,
                        unit_uid: None,
                        asserted_by: actor.as_deref(),
                    },
                )
                .await?;
                if !references.is_empty() {
                    let references_kind =
                        store::vocabulary::feature_write(&self.store.pool, "reference").await?;
                    for reference in &references {
                        store::assertions::assert(
                            &self.store.pool,
                            store::assertions::NewAssertion {
                                subject_uid: &message.uid,
                                predicate_uid: &references_kind,
                                object_uid: Some(reference),
                                role: store::assertions::AssertionRole::Ordinary,
                                quantity: None,
                                unit_uid: None,
                                asserted_by: actor.as_deref(),
                            },
                        )
                        .await?;
                    }
                }
                if let Some(parent) = parent {
                    let parent_uid = self.resolve(&parent).await?;
                    let parent_row = store::records::routing(&self.store.pool, &parent_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(parent.clone()))?;
                    if parent_row.kind != RecordKind::Message.as_str() {
                        return Err(EngineError::Consequence(format!(
                            "`{parent}` is a {} record, not a message",
                            parent_row.kind
                        )));
                    }
                    if !store::assertions::object_exists(
                        &self.store.pool,
                        &parent_uid,
                        &message_in,
                        Some(&thread_uid),
                    )
                    .await?
                    {
                        return Err(EngineError::Consequence(
                            "reply parent is not in the target thread".into(),
                        ));
                    }
                    let reply_to =
                        store::vocabulary::feature_write(&self.store.pool, "reply").await?;
                    store::assertions::assert(
                        &self.store.pool,
                        store::assertions::NewAssertion {
                            subject_uid: &message.uid,
                            predicate_uid: &reply_to,
                            object_uid: Some(&parent_uid),
                            role: store::assertions::AssertionRole::Ordinary,
                            quantity: None,
                            unit_uid: None,
                            asserted_by: actor.as_deref(),
                        },
                    )
                    .await?;
                }
                let (author, operator) = self
                    .message_authorship(author.as_deref(), actor.as_deref())
                    .await?;
                store::records::set_extension(
                    &self.store.pool,
                    &message.uid,
                    "lince.message",
                    &serde_json::json!({
                        "author": author,
                        "operator": operator,
                        "state": state.as_str(),
                    }),
                )
                .await?;
                if replica_root.is_some() {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(
                                    thread_uid,
                                    store::exact::zero(),
                                    Cause {
                                        kind: CauseKind::Sync,
                                        uid: Some(message.uid.clone()),
                                    },
                                )
                            },
                            now,
                        )
                        .await?;
                } else {
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor.clone(),
                                ..NewFact::quantity(
                                    message.uid.clone(),
                                    store::exact::one(),
                                    Cause::user_edit(),
                                )
                            },
                            now,
                        )
                        .await?;
                    outcome.facts.extend(
                        self.annotate(
                            thread_uid,
                            actor,
                            serde_json::json!({
                                "message": {
                                    "created": message.uid,
                                    "references": references,
                                }
                            }),
                            now,
                        )
                        .await?,
                    );
                }
                outcome.created = Some(message.uid);
            }
            Action::ReviseMessage {
                message,
                body,
                state,
            } => {
                let message_uid = self.resolve(&message).await?;
                let message_row = store::records::get(&self.store.pool, &message_uid)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(message.clone()))?;
                if message_row.kind != RecordKind::Message.as_str() {
                    return Err(EngineError::Consequence(format!(
                        "`{message}` is a {} record, not a message",
                        message_row.kind
                    )));
                }
                let thread_uid = self.thread_for_message(&message_uid).await?;
                if let Some(transfer_uid) = self.transfer_for_thread(&thread_uid).await? {
                    self.require_transfer_thread_writer(&transfer_uid, actor.as_deref(), now)
                        .await?;
                }
                let mut metadata =
                    store::records::get_extension(&self.store.pool, &message_uid, "lince.message")
                        .await?
                        .and_then(|value| value.as_object().cloned())
                        .ok_or_else(|| EngineError::Conflict {
                            code: "message_lifecycle_missing",
                            message: "the message has no persisted lifecycle metadata".into(),
                        })?;
                if metadata.get("state").and_then(serde_json::Value::as_str) != Some("writing") {
                    return Err(EngineError::Conflict {
                        code: "message_not_writing",
                        message: "only a writing message may grow, finish or be interrupted".into(),
                    });
                }
                if let Some(actor) = actor.as_deref()
                    && metadata.get("operator").and_then(serde_json::Value::as_str) != Some(actor)
                {
                    return Err(EngineError::Forbidden(
                        "only the persisted message operator may revise it".into(),
                    ));
                }
                if state == MessageState::Finished && body.trim().is_empty() {
                    let references =
                        store::vocabulary::feature_read(&self.store.pool, "reference").await?;
                    let has_references = match references {
                        Some(predicate) => {
                            store::assertions::object_exists(
                                &self.store.pool,
                                &message_uid,
                                &predicate,
                                None,
                            )
                            .await?
                        }
                        None => false,
                    };
                    let has_content = store::records::get_extension(
                        &self.store.pool,
                        &message_uid,
                        "lince.message-content",
                    )
                    .await?
                    .is_some_and(|value| {
                        value["parts"]
                            .as_array()
                            .is_some_and(|parts| !parts.is_empty())
                    });
                    if !has_references && !has_content {
                        return Err(EngineError::Consequence(
                            "a finished message body and Record references cannot both be empty"
                                .into(),
                        ));
                    }
                }
                let head = if body.trim().is_empty() {
                    match state {
                        MessageState::Writing => "Writing message".into(),
                        MessageState::Finished => "Shared Records".into(),
                        MessageState::Interrupted => "Interrupted message".into(),
                    }
                } else {
                    message_head(&body)
                };
                self.write_record_text(&message_uid, Some(&head), Some(&body))
                    .await?;
                metadata.insert("state".into(), serde_json::json!(state.as_str()));
                store::records::set_extension(
                    &self.store.pool,
                    &message_uid,
                    "lince.message",
                    &serde_json::Value::Object(metadata),
                )
                .await?;
                outcome.facts = self
                    .annotate(
                        message_uid,
                        actor,
                        serde_json::json!({ "message": { "state": state.as_str() } }),
                        now,
                    )
                    .await?;
            }
            Action::CreateTransferMessage {
                transfer,
                thread,
                body,
                parent,
                references,
                request_id: _,
                person: _,
            } => {
                let transfer_uid = self.resolve(&transfer).await?;
                let thread_uid = self.resolve(&thread).await?;
                if self.transfer_for_thread(&thread_uid).await?.as_deref()
                    != Some(transfer_uid.as_str())
                {
                    return Err(EngineError::Conflict {
                        code: "transfer_thread_target_mismatch",
                        message: "message thread belongs to another Transfer".into(),
                    });
                }
                outcome = Box::pin(self.act_at_with_authorship(
                    Action::CreateMessage {
                        content: Vec::new(),
                        thread: thread_uid,
                        body,
                        author: None,
                        state: MessageState::Finished,
                        parent,
                        references,
                    },
                    actor,
                    now,
                    verified_authorship,
                ))
                .await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
