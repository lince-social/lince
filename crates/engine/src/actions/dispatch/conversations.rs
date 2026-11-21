// non-lean-yet

use super::{
    Action, ActionOutcome, Cause, CauseKind, ControlFlow, DateTime, DispatchFuture, Engine,
    EngineError, NewFact, Utc, invalid_dispatch,
};

impl Engine {
    pub(super) fn dispatch_conversations(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchFuture<'_> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::DeleteConversation { conversation } => {
                    let uid = self.resolve(&conversation).await?;
                    let root = store::replica::root_of(&self.store.pool, &uid)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence("that is not part of a conversation".into())
                        })?;
                    self.social_require_safe_generic_deletion(&root).await?;
                    let removed =
                        store::replica::delete_root_locally(&self.store.pool, &root).await?;
                    outcome.data = Some(serde_json::json!({ "removed": removed }));
                }
                Action::SendRecordCopy { thread, record } => {
                    let thread_uid = self.resolve(&thread).await?;
                    let source_uid = self.resolve(&record).await?;
                    let root = store::replica::root_of(&self.store.pool, &thread_uid)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(
                                "that thread is not inside a conversation".into(),
                            )
                        })?;
                    let source = store::records::get(&self.store.pool, &source_uid)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("no such record to copy".into()))?;
                    if let Some(existing) =
                        store::replica::root_of(&self.store.pool, &source_uid).await?
                    {
                        if existing != root {
                            return Err(EngineError::Consequence(
                                "that record belongs to another conversation and cannot be copied \
                             into this one"
                                    .into(),
                            ));
                        }
                    }
                    let copy = store::records::create_in_root(
                        &self.store.pool,
                        store::records::NewRecord {
                            slug: None,
                            kind: nucleus::RecordKind::parse(&source.kind)
                                .unwrap_or(nucleus::RecordKind::Plain),
                            head: &source.head,
                            body: &source.body,
                            quantity: store::exact::zero(),
                        },
                        Some(&root),
                    )
                    .await?;
                    self.link_in(&copy.uid, &thread_uid, crate::threads::MESSAGE_IN_PREDICATE)
                        .await?;
                    outcome.created = Some(copy.uid.clone());
                    outcome.facts = self
                        .annotate(
                            copy.uid,
                            actor,
                            serde_json::json!({ "copied_from": source_uid }),
                            now,
                        )
                        .await?;
                }
                Action::StartConversation { contact, title } => {
                    let contact_uid = self.resolve(&contact).await?;
                    let (conversation, thread) =
                        self.start_conversation(&contact_uid, title.trim()).await?;
                    outcome.facts = self
                        .append(
                            NewFact {
                                actor_uid: actor,
                                ..NewFact::quantity(
                                    contact_uid,
                                    store::exact::zero(),
                                    Cause {
                                        kind: CauseKind::Sync,
                                        uid: Some(conversation.clone()),
                                    },
                                )
                            },
                            now,
                        )
                        .await?;
                    outcome.created = Some(
                        serde_json::json!({
                            "conversation": conversation,
                            "thread": thread,
                        })
                        .to_string(),
                    );
                }
                Action::OpenThread {
                    conversation,
                    title,
                } => {
                    let conversation_uid = self.resolve(&conversation).await?;
                    outcome.created =
                        Some(self.open_thread(&conversation_uid, title.trim()).await?);
                }
                Action::AcceptThreadInvite { invite } => {
                    let root = self.accept_invite(&invite).await?;
                    outcome.created = Some(root);
                }
                Action::DeclineThreadInvite { invite } => {
                    self.decline_invite(&invite).await?;
                }
                Action::ShareMyKey { thread } => {
                    let thread_uid = self.resolve(&thread).await?;
                    let local = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| EngineError::Consequence("no local Organ".into()))?;
                    let invite = store::records::get_extension(
                        &self.store.pool,
                        &local.uid,
                        "lince.pairing",
                    )
                    .await?
                    .and_then(|fields| {
                        fields
                            .get("invite")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
                    .ok_or_else(|| {
                        EngineError::Consequence(
                            "this Cell has no pairing code yet — it needs a network endpoint"
                                .into(),
                        )
                    })?;
                    let uid = self.send_message(&thread_uid, &local.head, &invite).await?;
                    outcome.created = Some(uid);
                }
                Action::ReadMessageAttachment { message, index } => {
                    let uid = self.resolve(&message).await?;
                    if !self.may_read_record(actor.as_deref(), &uid).await? {
                        return Err(EngineError::Forbidden(
                            "This attachment is not available to this Person".into(),
                        ));
                    }
                    let parts = store::message_content::load(&self.store.pool, &uid).await?;
                    let part = parts.get(index).ok_or_else(|| {
                        EngineError::Consequence("Attachment is unavailable".into())
                    })?;
                    if !matches!(part, nucleus::message::MessagePart::Attachment { .. }) {
                        return Err(EngineError::Consequence(
                            "This message part is not an attachment".into(),
                        ));
                    }
                    outcome.data = Some(serde_json::to_value(part).map_err(EngineError::Json)?);
                }
                _ => return Err(invalid_dispatch()),
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
