use crate::{
    Engine, EngineError,
    actions::{Action, ActionOutcome},
};
use chrono::{DateTime, Utc};
use nucleus::transfer::surplus::{
    Command, Definition, Mode, NAMESPACE, Preview, Recipe, Review, Saved,
};
use serde_json::{Value, json};

mod preview;
mod refresh;
mod storage;

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "stock_policy_changed",
        message: message.to_string(),
    }
}
fn number(value: &str) -> Result<nucleus::DecimalValue, EngineError> {
    nucleus::DecimalValue::parse_inferred(value).map_err(invalid)
}
fn max_zero(value: nucleus::DecimalValue) -> nucleus::DecimalValue {
    if value.is_negative() {
        store::exact::zero()
    } else {
        value
    }
}

impl Engine {
    async fn stock_policy_actor(
        &self,
        controller: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        if actor.is_some_and(|actor| actor != controller)
            || !store::people::is_active(&self.store.pool, controller).await?
        {
            return Err(EngineError::Forbidden(
                "Use your own active Person for stock planning".into(),
            ));
        }
        for permission in ["record:read", "transfer:read"] {
            self.require_permission(actor, permission).await?;
        }
        Ok(())
    }

    async fn prepare_stock_policy(
        &self,
        mut definition: Definition,
        actor: Option<&str>,
    ) -> Result<Review, EngineError> {
        definition.validate().map_err(invalid)?;
        self.stock_policy_actor(&definition.controller_uid, actor)
            .await?;
        if let Some(saved) = storage::find(&self.store.pool, &definition).await? {
            if saved.review.definition.controller_uid != definition.controller_uid
                || saved.review.definition.stock_uid != definition.stock_uid
            {
                return Err(invalid(
                    "Stock policy belongs to another controller or Record",
                ));
            }
            definition.uid = saved.review.definition.uid;
        }
        let existing = storage::load(&self.store.pool, &definition.uid).await?;
        let (preview, basis, unit_uid, source) = preview::inspect(self, &definition, actor).await?;
        Ok(Review {
            definition,
            unit_uid,
            expected_revision: existing.map_or(0, |saved| saved.revision),
            source,
            preview,
            basis,
        })
    }

    pub(crate) async fn stock_policy_command(
        &self,
        request: Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let value = match request {
            Command::Inspect { mut definition } => {
                self.stock_policy_actor(&definition.controller_uid, actor)
                    .await?;
                if let Some(saved) = storage::find(&self.store.pool, &definition).await? {
                    if saved.review.definition.controller_uid != definition.controller_uid
                        || saved.review.definition.stock_uid != definition.stock_uid
                    {
                        return Err(invalid(
                            "Stock policy belongs to another controller or Record",
                        ));
                    }
                    definition = saved.review.definition;
                }
                let review = self.prepare_stock_policy(definition, actor).await?;
                json!({"stock":{"preview":review.preview,"definition":review.definition,"saved":storage::load(&self.store.pool,&review.definition.uid).await?}})
            }
            Command::Review { definition } => {
                let review = self.prepare_stock_policy(definition, actor).await?;
                json!({"stock":{"commitment":review.commitment().map_err(invalid)?,"review":review}})
            }
            Command::Enable { review, commitment } => {
                self.enable_stock_policy(*review, commitment, actor, now)
                    .await?
            }
            Command::Pause {
                policy_uid,
                controller_uid,
                expected_revision,
            } => {
                let _guard = self.stock_policy_execution.lock().await;
                self.stock_policy_actor(&controller_uid, actor).await?;
                self.require_permission(actor, "record:update").await?;
                let mut saved = storage::load(&self.store.pool, &policy_uid)
                    .await?
                    .ok_or_else(|| invalid("Stock policy unavailable"))?;
                if saved.review.definition.controller_uid != controller_uid
                    || saved.revision != expected_revision
                {
                    return Err(invalid("Stock policy changed; inspect it again"));
                }
                saved.enabled = false;
                saved.reason = "Paused by controller".into();
                storage::save(self, &mut saved, expected_revision).await?;
                json!({"stock":{"saved":saved}})
            }
        };
        Ok(ActionOutcome {
            data: Some(value),
            ..Default::default()
        })
    }

    async fn enable_stock_policy(
        &self,
        review: Review,
        commitment: String,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Value, EngineError> {
        let guard = self.stock_policy_execution.lock().await;
        self.stock_policy_actor(&review.definition.controller_uid, actor)
            .await?;
        for permission in [
            "record:create",
            "record:update",
            "transfer:create",
            "transfer:update",
            "frequency:create",
            "frequency:update",
        ] {
            self.require_permission(actor, permission).await?;
            self.require_permission(Some(&review.definition.controller_uid), permission)
                .await?;
        }
        if review.commitment().map_err(invalid)? != commitment {
            return Err(invalid("Stock settings changed after review"));
        }
        self.transfer_person_signer(&review.definition.controller_uid, None)
            .await?;
        self.authorize_rule_target(
            &review.definition.stock_uid,
            Some(&review.definition.controller_uid),
        )
        .await?;
        let existing = storage::load(&self.store.pool, &review.definition.uid).await?;
        let replay = existing
            .as_ref()
            .is_some_and(|saved| saved.commitment == commitment);
        if !replay
            && self
                .prepare_stock_policy(review.definition.clone(), actor)
                .await?
                != review
        {
            return Err(invalid(
                "Stock, reservations or Transfer terms changed; review the current preview",
            ));
        }
        if !replay {
            let mut tx = store::write_tx(&self.store.pool).await?;
            if storage::find_uid_on(&mut tx, &review.definition)
                .await?
                .is_some_and(|uid| uid != review.definition.uid)
            {
                return Err(invalid(
                    "A stock policy was saved concurrently; review the existing policy",
                ));
            }
            for permission in [
                "record:read",
                "record:create",
                "record:update",
                "transfer:read",
                "transfer:create",
                "transfer:update",
                "frequency:create",
                "frequency:update",
            ] {
                self.require_permission_on(&mut tx, actor, permission)
                    .await?;
                self.require_permission_on(
                    &mut tx,
                    Some(&review.definition.controller_uid),
                    permission,
                )
                .await?;
            }
            let targets = vec![
                review.definition.stock_uid.clone(),
                review.source.transfer.clone(),
            ];
            let graph = crate::access::policy_graph_on(&mut tx, &targets)
                .await
                .map_err(invalid)?;
            let readable = self
                .readable_on(&mut tx, Some(&review.definition.controller_uid), &graph)
                .await?;
            if targets.iter().any(|target| !readable.contains(target)) {
                return Err(invalid(
                    "Stock or upstream Transfer access changed after review",
                ));
            }
            let balance =
                store::transfer_balances::read_on(&mut tx, &review.definition.stock_uid).await?;
            let unit: Option<String> = store::sqlx::query_scalar(
                "SELECT unit_uid FROM record WHERE uid=? AND deleted_at IS NULL",
            )
            .bind(&review.definition.stock_uid)
            .fetch_one(&mut *tx)
            .await?;
            let basis = nucleus::karma::canonical_hash(
                "lince.stock-policy-basis.v1",
                &json!({"balance":balance,"unit":unit}),
            )
            .map_err(invalid)?;
            if basis.as_str() != review.basis {
                return Err(invalid("Stock, unit or reservations changed after review"));
            }
            store::transfers::karma_snapshot::check_on(
                &mut tx,
                &review.source.transfer,
                Some(&nucleus::transfer::karma::Guard::exact(&review.source)),
            )
            .await?;
            let revision:Option<i64>=store::sqlx::query_scalar("SELECT CAST(json_extract(fds,'$.revision') AS INTEGER) FROM record_extension WHERE record_uid=? AND namespace=?")
                .bind(&review.definition.uid).bind(NAMESPACE).fetch_optional(&mut *tx).await?;
            if revision.map_or(0, |revision| revision as u64) != review.expected_revision {
                return Err(invalid("Stock policy changed after review"));
            }
            if existing.is_none() {
                let count:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM record_extension WHERE namespace=? AND json_extract(fds,'$.review.definition.controller_uid')=?").bind(NAMESPACE).bind(&review.definition.controller_uid).fetch_one(&mut *tx).await?;
                if count >= 32 {
                    return Err(invalid("Keep at most 32 stock policies per Person"));
                }
                let organ:String=store::sqlx::query_scalar("SELECT uid FROM record WHERE slug='local-organ' AND kind='organ' AND deleted_at IS NULL").fetch_one(&mut *tx).await?;
                store::records::create_with_uid_on(
                    &mut tx,
                    &review.definition.uid,
                    store::records::NewRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: "Stock planning policy",
                        body: "",
                        quantity: store::exact::zero(),
                    },
                    &organ,
                    None,
                )
                .await?;
            }
            let saved = Saved {
                review: review.clone(),
                commitment: commitment.clone(),
                revision: review.expected_revision + 1,
                rule_uid: None,
                enabled: true,
                reason: "Installing reviewed stock rule".into(),
                pending_need: existing.and_then(|saved| saved.pending_need),
                managed_revision: review.source.revision,
            };
            store::records::set_stock_policy_on(&mut tx, &review.definition.uid, &saved).await?;
            tx.commit().await?;
        }
        let mut saved = storage::load(&self.store.pool, &review.definition.uid)
            .await?
            .ok_or_else(|| invalid("Stock policy unavailable"))?;
        if saved.rule_uid.is_none() {
            let rule = self
                .install_stock_policy_rule(&review, &commitment, now)
                .await?;
            let revision = saved.revision;
            saved.rule_uid = Some(rule);
            storage::save(self, &mut saved, revision).await?;
        }
        drop(guard);
        self.refresh_stock_policy(
            &review.definition.uid,
            &commitment,
            Some(&review.definition.controller_uid),
            now,
        )
        .await?;
        let current = storage::load(&self.store.pool, &review.definition.uid).await?;
        let (preview, _, _, _) = preview::inspect(self, &review.definition, actor).await?;
        Ok(json!({"stock":{"saved":current,"preview":preview}}))
    }

    async fn install_stock_policy_rule(
        &self,
        review: &Review,
        commitment: &str,
        now: DateTime<Utc>,
    ) -> Result<String, EngineError> {
        let rules:Vec<String>=store::sqlx::query_scalar("SELECT uid FROM recurrence WHERE record_uid=? AND json_extract(consequences_json,'$[0].kind')='refresh-stock-policy' AND json_extract(consequences_json,'$[0].policy')=? LIMIT 2")
            .bind(&review.definition.stock_uid).bind(&review.definition.uid).fetch_all(&self.store.pool).await?;
        if rules.len() > 1 {
            return Err(invalid("Conflicting stock rules require review"));
        }
        let consequences = vec![nucleus::karma::Consequence::RefreshStockPolicy {
            policy: review.definition.uid.clone(),
            commitment: commitment.into(),
        }];
        let condition = match &review.definition.recipe {
            Recipe::Offer { transfer_uid, .. } => format!(
                "@{} + transfer_revision(@{transfer_uid})",
                review.definition.stock_uid
            ),
            Recipe::Replenish {
                upstream_transfer_uid,
                ..
            } => format!(
                "1 + @{} + transfer_revision(@{upstream_transfer_uid})",
                review.definition.stock_uid
            ),
        };
        let cadence = nucleus::karma::Cadence::every_days(1);
        let request = format!("stock-rule:{}:{commitment}", review.definition.uid);
        let action = if let Some(uid) = rules.first() {
            let old = store::recurrence::get(&self.store.pool, uid)
                .await?
                .ok_or_else(|| invalid("Stock rule unavailable"))?;
            if old.actor_uid.as_deref() != Some(review.definition.controller_uid.as_str()) {
                return Err(invalid("Stock rule belongs to another Person"));
            }
            if old.consequences.as_slice() == consequences.as_slice() {
                return Ok(uid.clone());
            }
            Action::ReviseRecurrence {
                recurrence: uid.clone(),
                expected_revision: old.revision,
                request_id: request,
                consequences,
                condition: Some(condition),
                gate: Some("always".into()),
                carry: None,
                note: Some("Reviewed stock planning".into()),
                cadence,
                anchor_at: Some(now.to_rfc3339()),
            }
        } else {
            Action::CreateRecurrence {
                target: review.definition.stock_uid.clone(),
                consequences,
                condition: Some(condition),
                gate: Some("always".into()),
                carry: None,
                note: Some("Reviewed stock planning".into()),
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
            .ok_or_else(|| invalid("Stock rule unavailable"))
    }

    pub(crate) async fn refresh_stock_policy(
        &self,
        uid: &str,
        commitment: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        let _guard = self.stock_policy_execution.lock().await;
        let Some(mut saved) = storage::load(&self.store.pool, uid).await? else {
            return Ok(());
        };
        if saved.commitment != commitment || !saved.enabled || saved.rule_uid.is_none() {
            return Ok(());
        }
        if actor != Some(saved.review.definition.controller_uid.as_str()) {
            return Err(EngineError::Forbidden(
                "Only this stock policy's controller can run it".into(),
            ));
        }
        self.stock_policy_actor(&saved.review.definition.controller_uid, actor)
            .await?;
        self.require_permission(actor, "record:update").await?;
        self.require_permission(actor, "transfer:update").await?;
        let (preview, basis, unit, source) =
            preview::inspect(self, &saved.review.definition, actor).await?;
        if unit != saved.review.unit_uid {
            return Err(invalid(
                "Stock unit changed; review its meaning before planning",
            ));
        }
        match saved.review.definition.recipe.clone() {
            Recipe::Offer { .. } => {
                self.refresh_stock_offer(&mut saved, &preview, &basis, &source, actor, now)
                    .await?
            }
            Recipe::Replenish { .. } => {
                if source.revision != saved.review.source.revision
                    || source
                        .participants
                        .keys()
                        .ne(saved.review.source.participants.keys())
                {
                    let revision = saved.revision;
                    saved.enabled = false;
                    saved.reason =
                        "Upstream terms or participants changed; review the stock policy again"
                            .into();
                    storage::save(self, &mut saved, revision).await?;
                    return Ok(());
                }
                self.refresh_stock_need(&mut saved, &preview, &basis, actor, now)
                    .await?
            }
        }
        Ok(())
    }
}
