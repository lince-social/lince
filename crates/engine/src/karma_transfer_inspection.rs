use chrono::{DateTime, Utc};
use nucleus::karma::Consequence;
use serde_json::{Value, json};
use store::sqlx::Row;

use crate::{Engine, EngineError};

impl Engine {
    pub(crate) async fn inspect_transfer_karma(
        &self,
        transfer: &str,
        person: Option<&str>,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Value, EngineError> {
        self.require_permission(actor, "frequency:read").await?;
        let transfer = self.resolve_karma_transfer(transfer).await?;
        let mapped = self.actor_person(actor).await?;
        if actor.is_some() && mapped.is_none() {
            return Err(EngineError::Forbidden(
                "Connect your Person before inspecting agreement Rules".into(),
            ));
        }
        let local = self.signer_actor_uid().await;
        let person = self
            .transfer_action_person(actor, person, local.as_deref())
            .await?;
        if actor.is_none() {
            self.transfer_person_signer(&person, None).await?;
        }
        let state = self.transfer_karma_snapshot(&transfer, actor).await?;
        if !state.participants.contains_key(&person) {
            return Err(EngineError::Forbidden(
                "Your agreement on this Transfer is unavailable".into(),
            ));
        }
        let mut rules = Vec::new();
        for rule in store::recurrence::all(&self.store.pool).await? {
            if !rule.consequences.iter().any(|effect| matches!(effect, Consequence::SetTransferAgreement { transfer: target, person: participant, .. } if *target == transfer && *participant == person)) {
                continue;
            }
            if store::karma_stages::for_rule(&self.store.pool, &rule.uid)
                .await?
                .is_some()
            {
                continue;
            }
            let inputs = match &rule.condition {
                Some(condition) => {
                    self.karma_condition_records(
                        condition
                            .parsed()
                            .map_err(|error| EngineError::Consequence(error.to_string()))?,
                        actor,
                    )
                    .await
                }
                None => Ok(Vec::new()),
            };
            if inputs.is_err() {
                continue;
            }
            let computed = if let Some(condition) = &rule.condition {
                crate::karma_transfers::scope(
                    actor,
                    Box::pin(self.evaluate_rule_condition(condition, now, now)),
                )
                .await
            } else {
                Ok(None)
            };
            let effects: Vec<_> = rule.consequences.iter().filter_map(|effect| {
                if let Consequence::SetTransferAgreement { transfer: target, person: participant, level, after_ms } = effect {
                    if *target != transfer || *participant != person { return None; }
                    let target = level.or(computed.as_ref().ok().copied().flatten()).and_then(|value| nucleus::karma::transfer_consequence::level(value).ok());
                    let maximum = if level.is_some() { Some(target == Some(2)) } else if target == Some(2) { Some(true) } else { None };
                    Some(json!({"fixed":level.is_some(),"target":target,"after_ms":after_ms,"can_raise_to_maximum":maximum}))
                } else { None }
            }).collect();
            let pending = store::sqlx::query("SELECT b.rule_uid, b.intended_at_ms, b.status FROM karma_transfer_stage s JOIN karma_schedule_boundary b ON b.uid = s.boundary_uid WHERE s.parent_rule_uid = ? AND b.current = 1 AND b.status NOT IN ('retired', 'expired', 'cancelled', 'parent-changed') ORDER BY b.intended_at_ms")
                .bind(&rule.uid).fetch_all(&self.store.pool).await?.into_iter().map(|row| json!({"rule":row.get::<String,_>("rule_uid"),"at_ms":row.get::<i64,_>("intended_at_ms"),"status":row.get::<String,_>("status")})).collect::<Vec<_>>();
            let commands = store::sqlx::query("SELECT c.command_uid, c.status, k.cancelled, k.reason, k.dispatched_at FROM karma_transfer_command k JOIN transfer_remote_command c ON c.command_uid = k.command_uid WHERE k.rule_uid = ? OR k.rule_uid IN (SELECT b.rule_uid FROM karma_transfer_stage s JOIN karma_schedule_boundary b ON b.uid = s.boundary_uid WHERE s.parent_rule_uid = ?)")
                .bind(&rule.uid).bind(&rule.uid).fetch_all(&self.store.pool).await?.into_iter().map(|row| json!({"command":row.get::<String,_>("command_uid"),"status":row.get::<String,_>("status"),"cancelled":row.get::<bool,_>("cancelled"),"reason":row.get::<Option<String>,_>("reason"),"dispatched_at":row.get::<Option<String>,_>("dispatched_at")})).collect::<Vec<_>>();
            let identity = store::karma_fields::identity(&self.store.pool, &rule.uid).await?;
            rules.push(json!({"uid":rule.uid,"revision":rule.revision,"name":identity.map(|identity|identity.name).unwrap_or_else(||"Agreement Rule".into()),"paused":rule.is_paused(),"effects":effects,"pending":pending,"commands":commands,"calculation_error":computed.err().map(|error|error.to_string())}));
        }
        Ok(
            json!({"transfer":transfer,"person":person,"state":state,"rules":rules,"at_ms":now.timestamp_millis()}),
        )
    }
}
