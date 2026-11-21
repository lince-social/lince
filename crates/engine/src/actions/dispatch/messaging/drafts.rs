// non-lean-yet

use super::super::{
    Action, ActionOutcome, Cause, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError,
    MessageState, NewFact, RecordKind, Utc, VerifiedActionAuthorship, invalid_dispatch,
    message_draft_head,
};

impl Engine {
    pub(in super::super) async fn dispatch_messaging_drafts(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
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
                let (author, operator) = self.message_authorship(None, actor.as_deref()).await?;
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
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
