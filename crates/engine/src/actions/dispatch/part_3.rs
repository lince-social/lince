use super::*;

impl Engine {
    pub(super) fn dispatch_part_3(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                action @ Action::SetAssertionOrder { .. } => {
                    outcome = self.edit_record_relations_as(action, actor.as_deref(), now).await?;
                }
                Action::CreateThread { target, head } => {
                    let _thread_guard = self.thread_creation_lock.lock().await;
                    let target_uid = self.resolve(&target).await?;
                    if store::transfers::get(&self.store.pool, &target_uid)
                        .await?
                        .is_some()
                    {
                        self.require_transfer_thread_writer(&target_uid, actor.as_deref(), now)
                            .await?;
                    }
                    let generated;
                    let title = if head.trim().is_empty() {
                        let predicate =
                            store::concepts::ensure(&self.store.pool, "thread-of").await?;
                        let threads = store::assertions::subjects_pointing_to(
                            &self.store.pool,
                            &predicate,
                            &target_uid,
                        )
                        .await?;
                        let number = (1..)
                            .find(|number| {
                                !threads
                                    .iter()
                                    .any(|thread| thread.head == format!("Thread {number}"))
                            })
                            .unwrap();
                        generated = format!("Thread {number}");
                        generated.as_str()
                    } else {
                        head.trim()
                    };
                    let replica_root =
                        store::replica::root_of(&self.store.pool, &target_uid).await?;
                    let thread = store::records::create_in_root(
                        &self.store.pool,
                        store::records::NewRecord {
                            slug: None,
                            kind: RecordKind::Thread,
                            head: title,
                            body: "",
                            quantity: if replica_root.is_some() {
                                store::exact::one()
                            } else {
                                store::exact::zero()
                            },
                        },
                        replica_root.as_deref(),
                    )
                    .await?;
                    let thread_of = store::concepts::ensure(&self.store.pool, "thread-of").await?;
                    store::assertions::assert(
                        &self.store.pool,
                        store::assertions::NewAssertion {
                            subject_uid: &thread.uid,
                            predicate_uid: &thread_of,
                            object_uid: Some(&target_uid),
                            role: store::assertions::AssertionRole::Ordinary,
                            quantity: None,
                            unit_uid: None,
                            asserted_by: actor.as_deref(),
                        },
                    )
                    .await?;
                    if replica_root.is_some() {
                        outcome.facts = self
                            .append(
                                NewFact {
                                    actor_uid: actor,
                                    ..NewFact::quantity(
                                        target_uid,
                                        store::exact::zero(),
                                        Cause {
                                            kind: CauseKind::Sync,
                                            uid: Some(thread.uid.clone()),
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
                                        thread.uid.clone(),
                                        store::exact::one(),
                                        Cause::user_edit(),
                                    )
                                },
                                now,
                            )
                            .await?;
                        outcome.facts.extend(
                            self.annotate(
                                target_uid,
                                actor,
                                serde_json::json!({ "thread": { "created": thread.uid } }),
                                now,
                            )
                            .await?,
                        );
                    }
                    outcome.created = Some(thread.uid);
                }
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
                    if let Some(root) =
                        store::replica::root_of(&self.store.pool, &thread_uid).await?
                        && store::records::get_extension(
                            &self.store.pool,
                            &root,
                            nucleus::social::requests::PARTICIPANTS_NAMESPACE,
                        )
                        .await?
                        .is_some()
                    {
                        if state != MessageState::Finished
                            || !references.is_empty()
                            || parent.is_some()
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
                    let replica_root =
                        store::replica::root_of(&self.store.pool, &thread_uid).await?;
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
                        store::message_content::save(&self.store.pool, &message.uid, &content)
                            .await?;
                    }
                    let message_in =
                        store::concepts::ensure(&self.store.pool, "message-in").await?;
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
                            store::concepts::ensure(&self.store.pool, "references").await?;
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
                        let parent_row = store::records::get(&self.store.pool, &parent_uid)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(parent.clone()))?;
                        if parent_row.kind != RecordKind::Message.as_str() {
                            return Err(EngineError::Consequence(format!(
                                "`{parent}` is a {} record, not a message",
                                parent_row.kind
                            )));
                        }
                        let parent_threads = store::assertions::objects_from_subject(
                            &self.store.pool,
                            &parent_uid,
                            &message_in,
                        )
                        .await?;
                        if !parent_threads.iter().any(|row| row.uid == thread_uid) {
                            return Err(EngineError::Consequence(
                                "reply parent is not in the target thread".into(),
                            ));
                        }
                        let reply_to =
                            store::concepts::ensure(&self.store.pool, "reply-to").await?;
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
                    let mut metadata = store::records::get_extension(
                        &self.store.pool,
                        &message_uid,
                        "lince.message",
                    )
                    .await?
                    .and_then(|value| value.as_object().cloned())
                    .ok_or_else(|| EngineError::Conflict {
                        code: "message_lifecycle_missing",
                        message: "the message has no persisted lifecycle metadata".into(),
                    })?;
                    if metadata.get("state").and_then(serde_json::Value::as_str) != Some("writing")
                    {
                        return Err(EngineError::Conflict {
                            code: "message_not_writing",
                            message: "only a writing message may grow, finish or be interrupted"
                                .into(),
                        });
                    }
                    if let Some(actor) = actor.as_deref()
                        && metadata.get("operator").and_then(serde_json::Value::as_str)
                            != Some(actor)
                    {
                        return Err(EngineError::Forbidden(
                            "only the persisted message operator may revise it".into(),
                        ));
                    }
                    if state == MessageState::Finished && body.trim().is_empty() {
                        let references =
                            store::concepts::resolve(&self.store.pool, "references").await?;
                        let has_references = match references {
                            Some(predicate) => !store::assertions::objects_from_subject(
                                &self.store.pool,
                                &message_uid,
                                &predicate,
                            )
                            .await?
                            .is_empty(),
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
                Action::CreateMessageDraft {
                    conversation,
                    thread,
                    body,
                    pinned,
                    timing,
                    position,
                } => {
                    let (conversation_uid, thread_uid) = self
                        .validate_message_draft_target(&conversation, &thread)
                        .await?;
                    let (author, operator) =
                        self.message_authorship(None, actor.as_deref()).await?;
                    let head = message_draft_head(&body);
                    let draft = store::records::create(
                        &self.store.pool,
                        store::records::NewRecord {
                            slug: None,
                            kind: RecordKind::MessageDraft,
                            head: &head,
                            body: &body,
                            quantity: store::exact::zero(),
                        },
                    )
                    .await?;
                    store::records::set_extension(
                        &self.store.pool,
                        &draft.uid,
                        "lince.message-draft",
                        &serde_json::json!({
                            "conversation": conversation_uid,
                            "thread": thread_uid,
                            "author": author,
                            "operator": operator,
                            "pinned": pinned,
                            "timing": timing.as_str(),
                            "position": position,
                        }),
                    )
                    .await?;
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(
                                    draft.uid.clone(),
                                    store::exact::one(),
                                    Cause::user_edit(),
                                )
                            },
                            now,
                        )
                        .await?;
                    outcome.created = Some(draft.uid);
                }
                Action::ReviseMessageDraft {
                    draft,
                    body,
                    pinned,
                    timing,
                    position,
                } => {
                    let (draft_uid, mut metadata) = self
                        .message_draft_metadata(&draft, actor.as_deref())
                        .await?;
                    let head = message_draft_head(&body);
                    self.write_record_text(&draft_uid, Some(&head), Some(&body))
                        .await?;
                    metadata.insert("pinned".into(), serde_json::json!(pinned));
                    metadata.insert("timing".into(), serde_json::json!(timing.as_str()));
                    metadata.insert("position".into(), serde_json::json!(position));
                    store::records::set_extension(
                        &self.store.pool,
                        &draft_uid,
                        "lince.message-draft",
                        &serde_json::Value::Object(metadata),
                    )
                    .await?;
                    outcome.facts = self
                        .annotate(
                            draft_uid,
                            actor,
                            serde_json::json!({ "message_draft": "revised" }),
                            now,
                        )
                        .await?;
                }
                Action::DeleteMessageDraft { draft } => {
                    let (draft_uid, _) = self
                        .message_draft_metadata(&draft, actor.as_deref())
                        .await?;
                    self.check_delete_permission(&draft_uid, actor.as_deref())
                        .await?;
                    outcome.facts = self
                        .annotate(
                            draft_uid.clone(),
                            actor,
                            serde_json::json!({ "message_draft": "deleted" }),
                            now,
                        )
                        .await?;
                    store::records::mark_deleted(&self.store.pool, &draft_uid).await?;
                }
                Action::SendMessageDraft { draft } => {
                    let (draft_uid, metadata) = self
                        .message_draft_metadata(&draft, actor.as_deref())
                        .await?;
                    let draft_row = store::records::get(&self.store.pool, &draft_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(draft_uid.clone()))?;
                    let thread = metadata
                        .get("thread")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| EngineError::Conflict {
                            code: "message_draft_invalid",
                            message: "the draft has no target thread".into(),
                        })?
                        .to_string();
                    let pinned = metadata
                        .get("pinned")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let operator = metadata.get("operator").and_then(serde_json::Value::as_str);
                    let author = metadata
                        .get("author")
                        .and_then(serde_json::Value::as_str)
                        .filter(|author| Some(*author) != operator)
                        .map(str::to_string);
                    outcome = Box::pin(self.act_at_with_authorship(
                        Action::CreateMessage {
                            content: Vec::new(),
                            thread,
                            body: draft_row.body,
                            author,
                            state: MessageState::Finished,
                            parent: None,
                            references: Vec::new(),
                        },
                        actor.clone(),
                        now,
                        verified_authorship,
                    ))
                    .await?;
                    if !pinned {
                        outcome.facts.extend(
                            self.annotate(
                                draft_uid.clone(),
                                actor,
                                serde_json::json!({ "message_draft": "sent" }),
                                now,
                            )
                            .await?,
                        );
                        store::records::mark_deleted(&self.store.pool, &draft_uid).await?;
                    }
                }
                Action::CreateTransferThread {
                    transfer,
                    head,
                    request_id: _,
                    person: _,
                } => {
                    outcome = Box::pin(self.act_at_with_authorship(
                        Action::CreateThread {
                            target: transfer,
                            head,
                        },
                        actor,
                        now,
                        verified_authorship,
                    ))
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
                Action::CreatePromise {
                    record,
                    delta,
                    window_end,
                    party,
                    open,
                } => {
                    let record_uid = self.resolve(&record).await?;
                    let state = if open {
                        PromiseState::Open
                    } else {
                        PromiseState::Proposed
                    };
                    let party = if open {
                        Some(
                            self.transfer_action_person(actor.as_deref(), party.as_deref(), None)
                                .await?,
                        )
                    } else {
                        match party {
                            Some(token) => Some(self.resolve(token.trim()).await?),
                            None => None,
                        }
                    };
                    outcome.created = Some(
                        store::misc::insert_promise(
                            &self.store.pool,
                            store::misc::NewPromise {
                                record_uid: Some(record_uid),
                                delta,
                                window_end,
                                party_uid: party,
                                state: Some(state),
                                ..Default::default()
                            },
                        )
                        .await?,
                    );
                }
                Action::PromiseTransition { promise, to } => {
                    let row = store::misc::get_promise(&self.store.pool, &promise)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(promise.clone()))?;
                    if row.transfer_uid.is_some() {
                        return Err(EngineError::Conflict {
                            code: "transfer_phase_3_not_available",
                            message:
                                "bundled promise state follows the revision-bound Transfer workflow"
                                    .into(),
                        });
                    }
                    let next = PromiseState::transition(row.state, to)?;
                    store::misc::set_promise_state(&self.store.pool, &promise, next).await?;
                    if let Some(record_uid) = row.record_uid {
                        outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid,
                                delta: nucleus::fact::zero_delta(),
                                at: None,
                                actor_uid: actor,
                                cause: Cause { kind: CauseKind::Action, uid: Some(promise) },
                                payload: Some(
                                    serde_json::json!({
                                        "promise": { "from": row.state.as_str(), "to": next.as_str() }
                                    })
                                    .to_string(),
                                ),
                            },
                            now,
                        )
                        .await?;
                    }
                }
                Action::EditPromiseDelta { promise, delta } => {
                    if !delta.is_finite() || delta == 0.0 {
                        return Err(EngineError::Consequence(
                            "promise delta must be finite and non-zero".into(),
                        ));
                    }
                    let row = store::misc::get_promise(&self.store.pool, &promise)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(promise.clone()))?;
                    if let Some(transfer_uid) = row.transfer_uid.as_deref() {
                        self.require_transfer_editor(transfer_uid, actor.as_deref())
                            .await?;
                        return Err(EngineError::Conflict {
                        code: "transfer_revision_required",
                        message: "bundled promises must use revise-transfer-promise with expected_revision and request_id".into(),
                    });
                    }
                    store::misc::set_promise_delta(&self.store.pool, &promise, delta).await?;
                }
                Action::CreateTransfer {
                    slug,
                    head,
                    agreement,
                    agreement_pct,
                    satiation,
                    mut source,
                    reserve_default,
                    require_confirmation,
                } => {
                    if transfer_phase_locked() {
                        return Err(EngineError::Conflict {
                        code: "transfer_draft_action_required",
                        message: "use create-transfer-draft so the initial terms are atomic and revisioned".into(),
                    });
                    }
                    if head.trim().is_empty() || head.chars().count() > 200 {
                        return Err(EngineError::Consequence(
                            "transfer title must contain 1 to 200 characters".into(),
                        ));
                    }
                    let agreement_kind = nucleus::transfer::AgreementType::parse(&agreement)
                        .ok_or_else(|| {
                            EngineError::Consequence(format!(
                                "unknown transfer agreement `{agreement}`"
                            ))
                        })?;
                    match agreement_kind {
                        nucleus::transfer::AgreementType::Percentage => {
                            if !agreement_pct.is_some_and(|pct| (1..=100).contains(&pct)) {
                                return Err(EngineError::Consequence(
                                    "percentage agreement requires a threshold from 1 to 100"
                                        .into(),
                                ));
                            }
                        }
                        _ if agreement_pct.is_some() => {
                            return Err(EngineError::Consequence(
                                "agreement_pct is valid only for percentage agreement".into(),
                            ));
                        }
                        _ => {}
                    }
                    if satiation
                        .as_deref()
                        .is_some_and(|value| value != "first_completes")
                    {
                        return Err(EngineError::Consequence(
                            "satiation must be `first_completes` or omitted".into(),
                        ));
                    }
                    if satiation.as_deref() == Some("first_completes") && source.is_none() {
                        return Err(EngineError::Consequence(
                            "first_completes requires a source record".into(),
                        ));
                    }
                    if reserve_default.as_deref().is_some_and(|value| {
                        !matches!(value, "none" | "proposed" | "agreed" | "active")
                    }) {
                        return Err(EngineError::Consequence(
                            "reserve_default must be none, proposed, agreed, active, or omitted"
                                .into(),
                        ));
                    }
                    if let Some(token) = source.as_deref() {
                        source = Some(self.resolve(token).await?);
                    }
                    let creator_person = self.require_transfer_creator(actor.as_deref()).await?;
                    let visibility_actor = actor.clone();
                    let transfer = store::transfers::create(
                        &self.store.pool,
                        store::transfers::NewTransfer {
                            slug: slug.as_deref(),
                            head: &head,
                            agreement_type: &agreement,
                            agreement_pct,
                            satiation: satiation.as_deref(),
                            source_uid: source.as_deref(),
                            reserve_default: reserve_default.as_deref(),
                            require_confirmation,
                        },
                    )
                    .await?;
                    if let Some(person) = creator_person {
                        store::transfers::add_party(&self.store.pool, &transfer, &person).await?;
                    }
                    if let Some(subject) = visibility_actor.as_deref() {
                        store::visibility::grant(
                            &self.store.pool,
                            "actor",
                            Some(subject),
                            &transfer,
                        )
                        .await?;
                    }
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(
                                    transfer.clone(),
                                    store::exact::one(),
                                    Cause::user_edit(),
                                )
                            },
                            now,
                        )
                        .await?;
                    outcome.created = Some(transfer);
                }
                _ => {
                    return Err(EngineError::Consequence(
                        "Invalid internal action dispatch".into(),
                    ));
                }
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
