// non-lean-yet

use super::super::{
    Action, ActionOutcome, Cause, CauseKind, ControlFlow, DateTime, DispatchOutcome, Engine,
    EngineError, NewFact, RecordKind, Utc, VerifiedActionAuthorship, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_messaging_threads(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::CreateThread { target, head } => {
                let _thread_guard = self.thread_creation_lock.lock().await;
                let target_uid = self.resolve(&target).await?;
                if store::transfers::exists(&self.store.pool, &target_uid).await? {
                    self.require_transfer_thread_writer(&target_uid, actor.as_deref(), now)
                        .await?;
                }
                let generated;
                let title = if head.trim().is_empty() {
                    let predicate =
                        store::vocabulary::feature_write(&self.store.pool, "thread").await?;
                    let heads: std::collections::HashSet<_> =
                        store::assertions::subject_heads(&self.store.pool, &predicate, &target_uid)
                            .await?
                            .into_iter()
                            .collect();
                    generated = (1..=heads.len().saturating_add(1))
                        .map(|number| format!("Thread {number}"))
                        .find(|head| !heads.contains(head))
                        .ok_or_else(|| EngineError::Consequence("No unused thread title".into()))?;
                    generated.as_str()
                } else {
                    head.trim()
                };
                let replica_root = store::replica::root_of(&self.store.pool, &target_uid).await?;
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
                let thread_of =
                    store::vocabulary::feature_write(&self.store.pool, "thread").await?;
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
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
