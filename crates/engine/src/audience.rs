use chrono::{DateTime, Utc};
use nucleus::visibility::{
    Condition, Data, Policy,
    plan::{Command, Definition, MAX_BYTES, PRIVATE_NAMESPACE, Review, Saved, Status},
};
use serde_json::{Value, json};
use store::sqlx::{Sqlite, Transaction};

use crate::{
    Engine, EngineError,
    actions::{Action, ActionOutcome},
};

mod storage;

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "audience_plan_changed",
        message: message.to_string(),
    }
}

impl Engine {
    pub(crate) async fn validate_audience_rule(
        &self,
        uid: &str,
        commitment: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let saved = storage::load(&self.store.pool, uid)
            .await?
            .ok_or_else(|| invalid("Audience plan unavailable"))?;
        if actor != Some(saved.review.definition.controller_uid.as_str())
            || commitment != saved.commitment
        {
            return Err(EngineError::Forbidden(
                "Only the reviewed controller can install this audience rule".into(),
            ));
        }
        Ok(())
    }
    async fn audience_authority(
        &self,
        controller: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        if actor.is_some_and(|actor| actor != controller) {
            return Err(EngineError::Forbidden("Use your own audience plan".into()));
        }
        if !store::people::is_active(&self.store.pool, controller).await? {
            return Err(EngineError::Forbidden("Choose an active Person".into()));
        }
        self.require_permission(actor, "record:read").await?;
        self.require_permission(actor, "transfer:read").await?;
        Ok(())
    }

    async fn prepare_audience(
        &self,
        definition: Definition,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Review, EngineError> {
        definition
            .validate(now.timestamp_millis())
            .map_err(invalid)?;
        self.audience_authority(&definition.controller_uid, actor)
            .await?;
        let source = self
            .transfer_karma_snapshot(&definition.source_uid, actor)
            .await?;
        if source.revision != definition.source_revision
            || !source.participants.contains_key(&definition.controller_uid)
        {
            return Err(invalid(
                "Review the current Transfer as one of its participants",
            ));
        }
        if source.ready
            || source.active
            || store::location::transfer_ended(&self.store.pool, &definition.source_uid).await?
        {
            return Err(invalid(
                "Agreed or ended Transfers cannot expand their audience",
            ));
        }
        let existing = storage::load(&self.store.pool, &definition.uid).await?;
        if let Some(saved) = &existing {
            let old = &saved.review.definition;
            if old.controller_uid != definition.controller_uid
                || old.source_uid != definition.source_uid
                || old.projection_uid != definition.projection_uid
            {
                return Err(invalid(
                    "Keep the same controller, source and publication for this plan",
                ));
            }
        }
        let mut tx = self.store.pool.begin().await?;
        let replies = storage::replies_on(&mut tx, &definition).await?;
        tx.rollback().await?;
        Ok(Review {
            definition,
            expected_revision: existing.map_or(0, |saved| saved.revision),
            source,
            replies,
        })
    }

    pub(crate) async fn audience_command(
        &self,
        request: Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let stop = matches!(&request, Command::Stop { .. });
        let value = match request {
            Command::Inspect {
                source_uid,
                controller_uid,
            } => {
                self.audience_authority(&controller_uid, actor).await?;
                if !nucleus::valid_uid(&source_uid, "r") {
                    return Err(invalid("Choose a source Record"));
                }
                let rows: Vec<String> = store::sqlx::query_scalar("SELECT e.record_uid FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.deleted_at IS NULL AND json_extract(e.fds,'$.review.definition.source_uid')=? AND json_extract(e.fds,'$.review.definition.controller_uid')=? ORDER BY r.created_at DESC,r.uid LIMIT 33")
                    .bind(PRIVATE_NAMESPACE).bind(&source_uid).bind(&controller_uid).fetch_all(&self.store.pool).await?;
                let mut plans = Vec::new();
                for uid in rows.iter().take(32) {
                    if self.may_read_record(actor, uid).await?
                        && let Some(saved) = storage::load(&self.store.pool, uid).await?
                    {
                        plans.push(saved);
                    }
                }
                json!({"plans":plans,"partial":rows.len()>32,"coverage":"Replies observed on this Lince only; network coverage is incomplete"})
            }
            Command::Review { definition } => {
                let review = self.prepare_audience(definition, actor, now).await?;
                json!({"commitment":review.commitment().map_err(invalid)?,"review":review,"coverage":"Replies observed on this Lince only; network coverage is incomplete"})
            }
            Command::Enable { review, commitment } => {
                self.enable_audience(*review, commitment, actor, now)
                    .await?
            }
            Command::Pause {
                plan_uid,
                controller_uid,
                expected_revision,
            }
            | Command::Stop {
                plan_uid,
                controller_uid,
                expected_revision,
            } => {
                self.audience_authority(&controller_uid, actor).await?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "permission:assign")
                    .await?;
                self.require_permission_on(&mut tx, actor, "record:update")
                    .await?;
                let mut saved = storage::load_on(&mut tx, &plan_uid)
                    .await?
                    .ok_or_else(|| invalid("Audience plan unavailable"))?;
                if saved.revision != expected_revision
                    || saved.review.definition.controller_uid != controller_uid
                {
                    return Err(invalid("Audience plan changed; inspect it again"));
                }
                saved.status = if stop {
                    Status::Stopped
                } else {
                    Status::Paused
                };
                saved.reason = if stop {
                    "Stopped by controller"
                } else {
                    "Paused by controller; review before resuming"
                }
                .into();
                if stop {
                    storage::publish_on(&mut tx, &mut saved, &Policy::default()).await?;
                }
                storage::save_on(&mut tx, &mut saved).await?;
                tx.commit().await?;
                json!({"plan":saved})
            }
        };
        Ok(ActionOutcome {
            data: Some(value),
            ..Default::default()
        })
    }

    async fn enable_audience(
        &self,
        review: Review,
        commitment: String,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Value, EngineError> {
        self.audience_authority(&review.definition.controller_uid, actor)
            .await?;
        for permission in [
            "record:create",
            "record:update",
            "permission:assign",
            "frequency:create",
            "frequency:update",
        ] {
            self.require_permission(actor, permission).await?;
            self.require_permission(Some(&review.definition.controller_uid), permission)
                .await?;
        }
        if commitment != review.commitment().map_err(invalid)? {
            return Err(invalid(
                "The public fields or audience stages changed after review",
            ));
        }
        let existing = storage::load(&self.store.pool, &review.definition.uid).await?;
        let replay = existing
            .as_ref()
            .is_some_and(|saved| saved.commitment == commitment);
        if !replay
            && self
                .prepare_audience(review.definition.clone(), actor, now)
                .await?
                != review
        {
            return Err(invalid(
                "Transfer, replies or audience plan changed; review again",
            ));
        }
        if !replay {
            let mut tx = store::write_tx(&self.store.pool).await?;
            for permission in [
                "record:create",
                "record:update",
                "permission:assign",
                "frequency:create",
                "frequency:update",
            ] {
                self.require_permission_on(&mut tx, actor, permission)
                    .await?;
            }
            let current = storage::load_on(&mut tx, &review.definition.uid).await?;
            if current.as_ref().map_or(0, |saved| saved.revision) != review.expected_revision {
                return Err(invalid("Audience plan changed after review"));
            }
            let source =
                store::transfers::karma_snapshot::read_on(&mut tx, &review.definition.source_uid)
                    .await?;
            if source != review.source
                || storage::replies_on(&mut tx, &review.definition).await? != review.replies
            {
                return Err(invalid("Transfer or replies changed after review"));
            }
            let local: String = store::sqlx::query_scalar("SELECT uid FROM record WHERE slug='local-organ' AND kind='organ' AND deleted_at IS NULL")
                .fetch_one(&mut *tx).await?;
            if current.is_none() {
                let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record_extension WHERE namespace=? AND json_extract(fds,'$.review.definition.controller_uid')=?")
                    .bind(PRIVATE_NAMESPACE).bind(&review.definition.controller_uid).fetch_one(&mut *tx).await?;
                if count >= 32 {
                    return Err(invalid("Keep at most 32 audience plans per Person"));
                }
                for (uid, head, body) in [
                    (&review.definition.uid, "Audience plan", String::new()),
                    (
                        &review.definition.projection_uid,
                        review.definition.projection.title.as_str(),
                        serde_json::to_string(&review.definition.projection)?,
                    ),
                ] {
                    store::records::create_with_uid_on(
                        &mut tx,
                        uid,
                        store::records::NewRecord {
                            slug: None,
                            kind: nucleus::RecordKind::Plain,
                            head,
                            body: &body,
                            quantity: store::exact::zero(),
                        },
                        &local,
                        None,
                    )
                    .await?;
                }
                let private = Policy {
                    include: vec![Condition {
                        organs: vec![local],
                        persons: vec![review.definition.controller_uid.clone()],
                        ..Default::default()
                    }],
                    exclude: vec![],
                };
                store::data_visibility::save_on(
                    &mut tx,
                    &review.definition.uid,
                    Data::Record,
                    None,
                    &private,
                    0,
                )
                .await?;
                store::data_visibility::save_on(
                    &mut tx,
                    &review.definition.projection_uid,
                    Data::Record,
                    None,
                    &Policy::default(),
                    0,
                )
                .await?;
            } else {
                store::records::set_authoring_text_on(
                    &mut tx,
                    &review.definition.projection_uid,
                    Some(&review.definition.projection.title),
                    Some(&serde_json::to_string(&review.definition.projection)?),
                )
                .await?;
            }
            let same_fields = current.as_ref().is_some_and(|saved| {
                matches!(saved.status, Status::Active | Status::Paused)
                    && saved.review.definition == review.definition
            });
            if !same_fields && let Some(mut previous) = current.clone() {
                storage::publish_on(&mut tx, &mut previous, &Policy::default()).await?;
            }
            let stage = current
                .as_ref()
                .filter(|_| same_fields)
                .and_then(|saved| saved.stage);
            let policy_revision: i64 = store::sqlx::query_scalar(
                "SELECT revision FROM data_visibility WHERE record_uid=? AND data='record'",
            )
            .bind(&review.definition.projection_uid)
            .fetch_one(&mut *tx)
            .await?;
            let mut saved = Saved {
                review: review.clone(),
                commitment: commitment.clone(),
                revision: review.expected_revision,
                stage,
                next_at_ms: now.timestamp_millis(),
                status: Status::Active,
                reason: "Installing reviewed Karma rule".into(),
                rule_uid: None,
                policy_revision: policy_revision as u64,
            };
            storage::save_on(&mut tx, &mut saved).await?;
            tx.commit().await?;
        }
        let mut saved = storage::load(&self.store.pool, &review.definition.uid)
            .await?
            .ok_or_else(|| invalid("Audience plan unavailable"))?;
        if saved.rule_uid.is_none() {
            let rule = self
                .install_audience_rule(&review, &commitment, now)
                .await?;
            let mut tx = store::write_tx(&self.store.pool).await?;
            saved = storage::load_on(&mut tx, &review.definition.uid)
                .await?
                .ok_or_else(|| invalid("Audience plan unavailable"))?;
            if saved.commitment != commitment {
                return Err(invalid("Audience plan changed while installing its rule"));
            }
            saved.rule_uid = Some(rule);
            if let Some(index) = saved
                .stage
                .and_then(|stage| stage.checked_add(1))
                .filter(|index| *index < review.definition.stages.len())
            {
                saved.next_at_ms = now
                    .timestamp_millis()
                    .saturating_add(review.definition.stages[index].after_ms as i64);
            }
            storage::save_on(&mut tx, &mut saved).await?;
            tx.commit().await?;
        }
        self.advance_audience(
            &review.definition.uid,
            &commitment,
            Some(&review.definition.controller_uid),
            now,
        )
        .await?;
        Ok(
            json!({"plan":storage::load(&self.store.pool, &review.definition.uid).await?,"coverage":"Replies observed on this Lince only; network coverage is incomplete"}),
        )
    }

    async fn install_audience_rule(
        &self,
        review: &Review,
        commitment: &str,
        now: DateTime<Utc>,
    ) -> Result<String, EngineError> {
        let rules: Vec<String> = store::sqlx::query_scalar("SELECT uid FROM recurrence WHERE record_uid=? AND json_extract(consequences_json,'$[0].kind')='advance-audience' ORDER BY uid LIMIT 2")
            .bind(&review.definition.uid).fetch_all(&self.store.pool).await?;
        if rules.len() > 1 {
            return Err(invalid("Audience plan has conflicting Karma rules"));
        }
        let consequences = vec![nucleus::karma::Consequence::AdvanceAudience {
            plan: review.definition.uid.clone(),
            commitment: commitment.into(),
        }];
        let cadence = nucleus::karma::Cadence::every(nucleus::karma::CadenceStep {
            seconds: 30,
            ..Default::default()
        });
        let request = format!("audience-rule:{}:{commitment}", review.definition.uid);
        let action = if let Some(uid) = rules.first() {
            let current = store::recurrence::get(&self.store.pool, uid)
                .await?
                .ok_or_else(|| invalid("Audience rule unavailable"))?;
            if current.actor_uid.as_deref() != Some(review.definition.controller_uid.as_str()) {
                return Err(invalid("Audience rule belongs to another controller"));
            }
            if current.consequences.as_slice() == consequences.as_slice() {
                return Ok(uid.clone());
            }
            Action::ReviseRecurrence {
                recurrence: uid.clone(),
                expected_revision: current.revision,
                request_id: request,
                consequences,
                condition: None,
                gate: None,
                carry: None,
                note: Some("Reviewed audience expansion; incomplete local reply coverage".into()),
                cadence,
                anchor_at: Some(now.to_rfc3339()),
            }
        } else {
            Action::CreateRecurrence {
                target: review.definition.uid.clone(),
                consequences,
                condition: None,
                gate: None,
                carry: None,
                note: Some("Reviewed audience expansion; incomplete local reply coverage".into()),
                cadence,
                anchor_at: Some(now.to_rfc3339()),
                request_id: Some(request),
            }
        };
        let outcome =
            Box::pin(self.act(action, Some(review.definition.controller_uid.clone()))).await?;
        outcome
            .created
            .or_else(|| rules.first().cloned())
            .ok_or_else(|| invalid("Audience rule unavailable"))
    }

    pub(crate) async fn advance_audience(
        &self,
        uid: &str,
        commitment: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        let Some(initial) = storage::load(&self.store.pool, uid).await? else {
            return Ok(());
        };
        if actor != Some(initial.review.definition.controller_uid.as_str()) {
            return Err(EngineError::Forbidden(
                "Only the controller's Karma rule can advance this plan".into(),
            ));
        }
        if initial.commitment != commitment
            || initial.status == Status::Stopped
            || initial.rule_uid.is_none()
        {
            return Ok(());
        }
        let authority = self
            .audience_authority(&initial.review.definition.controller_uid, actor)
            .await;
        let readable = self
            .may_read_record(actor, &initial.review.definition.source_uid)
            .await
            .unwrap_or(false);
        let mut tx = store::write_tx(&self.store.pool).await?;
        let Some(mut saved) = storage::load_on(&mut tx, uid).await? else {
            return Ok(());
        };
        if saved.commitment != commitment
            || saved.status == Status::Stopped
            || saved.rule_uid.is_none()
        {
            return Ok(());
        }
        let allowed = authority.is_ok()
            && readable
            && self
                .require_permission_on(&mut tx, actor, "record:update")
                .await
                .is_ok()
            && self
                .require_permission_on(&mut tx, actor, "permission:assign")
                .await
                .is_ok()
            && self
                .require_permission_on(&mut tx, actor, "transfer:read")
                .await
                .is_ok()
            && self
                .audience_source_readable_on(&mut tx, actor, &saved.review.definition.source_uid)
                .await
                .unwrap_or(false);
        let ended = storage::ended_on(&mut tx, &saved.review.definition.source_uid).await?;
        if !allowed || ended || now.timestamp_millis() >= saved.review.definition.expires_at_ms {
            saved.status = Status::Stopped;
            saved.reason = "Stopped: permission, termination or expiry changed".into();
            storage::publish_on(&mut tx, &mut saved, &Policy::default()).await?;
            storage::save_on(&mut tx, &mut saved).await?;
            tx.commit().await?;
            return Ok(());
        }
        let source =
            store::transfers::karma_snapshot::read_on(&mut tx, &saved.review.definition.source_uid)
                .await?;
        let replies = storage::replies_on(&mut tx, &saved.review.definition).await;
        if source.ready || source.active {
            saved.status = Status::Stopped;
            saved.reason = "Stopped: permission, agreement, termination or expiry changed".into();
            storage::publish_on(&mut tx, &mut saved, &Policy::default()).await?;
        } else if saved.status != Status::Active {
            return Ok(());
        } else if source.revision != saved.review.definition.source_revision
            || !storage::projection_matches_on(&mut tx, &saved.review.definition).await?
            || !storage::policy_matches_on(&mut tx, &saved).await?
        {
            saved.status = Status::ReviewRequired;
            saved.reason =
                "The source revision or public fields changed; review before publishing".into();
            storage::publish_on(&mut tx, &mut saved, &Policy::default()).await?;
        } else if replies
            .as_ref()
            .map_or(true, |digest| digest != &saved.review.replies)
            || source.participants != saved.review.source.participants
        {
            saved.status = Status::Paused;
            saved.reason =
                "A locally observed reply or participation changed; review before resuming".into();
        } else if now.timestamp_millis() >= saved.next_at_ms {
            let index = saved.stage.map_or(0, |index| index + 1);
            if let Some(stage) = saved.review.definition.stages.get(index) {
                let policy = saved.review.definition.policy(stage);
                storage::publish_on(&mut tx, &mut saved, &policy).await?;
                saved.stage = Some(index);
                saved.next_at_ms = saved
                    .review
                    .definition
                    .stages
                    .get(index + 1)
                    .map_or(saved.review.definition.expires_at_ms, |stage| {
                        now.timestamp_millis().saturating_add(stage.after_ms as i64)
                    });
                saved.reason = "Published reviewed stage; reply coverage remains incomplete".into();
            } else {
                return Ok(());
            }
        } else {
            return Ok(());
        }
        storage::save_on(&mut tx, &mut saved).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn audience_source_readable_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        source: &str,
    ) -> Result<bool, EngineError> {
        let graph = crate::access::policy_graph_on(tx, &[source.to_owned()])
            .await
            .map_err(invalid)?;
        Ok(self.readable_on(tx, actor, &graph).await?.contains(source))
    }
}
