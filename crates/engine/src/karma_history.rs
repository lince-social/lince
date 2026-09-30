use std::cell::RefCell;

use nucleus::DecimalValue;
use serde::{Deserialize, Serialize};
use store::sqlx::Row;

use crate::{Engine, EngineError};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reading {
    pub function: String,
    pub reference: String,
    pub window_seconds: Option<i64>,
    pub depth: usize,
    pub value: Option<DecimalValue>,
    pub error: Option<String>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Evaluation {
    pub readings: Vec<Reading>,
    pub computed: Option<DecimalValue>,
    pub gate_passed: Option<bool>,
    pub carried: Option<DecimalValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub target: String,
    pub source: Option<String>,
    pub bindings: Vec<nucleus::karma::ConditionBinding>,
    pub gate: Option<nucleus::karma::Gate>,
    pub carry: Option<nucleus::karma::Carry>,
    pub consequences: nucleus::karma::Consequences,
    pub inputs: Option<Vec<String>>,
    pub evaluation: Evaluation,
}

tokio::task_local! {
    pub(crate) static CAPTURE: RefCell<Evidence>;
}

pub(crate) fn reading(
    function: &str,
    reference: &str,
    window_seconds: Option<i64>,
    depth: usize,
    result: &Result<DecimalValue, EngineError>,
) {
    let _ = CAPTURE.try_with(|capture| {
        capture.borrow_mut().evaluation.readings.push(Reading {
            function: function.into(),
            reference: reference.into(),
            window_seconds,
            depth,
            value: result.as_ref().ok().copied(),
            error: result.as_ref().err().map(ToString::to_string),
        });
    });
}

pub(crate) fn decision(computed: DecimalValue, gate_passed: bool, carried: Option<DecimalValue>) {
    let _ = CAPTURE.try_with(|capture| {
        let mut capture = capture.borrow_mut();
        capture.evaluation.computed = Some(computed);
        capture.evaluation.gate_passed = Some(gate_passed);
        capture.evaluation.carried = carried;
    });
}

pub(crate) fn computed(value: DecimalValue) {
    let _ = CAPTURE.try_with(|capture| capture.borrow_mut().evaluation.computed = Some(value));
}

pub(crate) async fn persist(
    connection: &mut store::sqlx::SqliteConnection,
    rule: &store::recurrence::Recurrence,
    event: &crate::rule_runtime::RuleEvent,
) -> Result<(), EngineError> {
    let Ok(evidence) = CAPTURE.try_with(|capture| capture.borrow().clone()) else {
        return Ok(());
    };
    store::sqlx::query("INSERT OR IGNORE INTO karma_rule_evidence VALUES (?, ?, ?, ?, ?)")
        .bind(&event.id)
        .bind(&rule.uid)
        .bind(rule.revision)
        .bind(event.attempt)
        .bind(serde_json::to_string(&evidence).map_err(EngineError::Json)?)
        .execute(connection)
        .await?;
    Ok(())
}

impl Engine {
    pub(crate) async fn capture_rule_evidence(
        &self,
        rule: &store::recurrence::Recurrence,
    ) -> Evidence {
        let inputs = match &rule.condition {
            None => Some(Vec::new()),
            Some(condition) => match condition.parsed() {
                Ok(condition) => self
                    .karma_condition_records(condition, rule.actor_uid.as_deref())
                    .await
                    .ok(),
                Err(_) => None,
            },
        };
        Evidence {
            target: crate::karma_transfer_effects::target(rule).into(),
            source: rule
                .condition
                .as_ref()
                .map(|condition| condition.source.clone()),
            bindings: rule
                .condition
                .as_ref()
                .map(|condition| condition.bindings.clone())
                .unwrap_or_default(),
            gate: rule
                .condition
                .as_ref()
                .map(|condition| condition.gate.clone()),
            carry: rule
                .condition
                .as_ref()
                .map(|condition| condition.carry.clone()),
            consequences: rule.consequences.clone(),
            inputs,
            evaluation: Evaluation::default(),
        }
    }

    pub(crate) async fn inspect_karma_history(
        &self,
        rule_uid: &str,
        limit: u32,
        actor: Option<&str>,
    ) -> Result<serde_json::Value, EngineError> {
        self.require_permission(actor, "frequency:read").await?;
        if !(1..=100).contains(&limit) {
            return Err(crate::karma_preview::invalid(
                "Choose 1–100 Rule applications",
            ));
        }
        self.refuse_unreadable(actor, &[rule_uid.into()]).await?;
        let rows = store::sqlx::query("SELECT a.*, e.evidence FROM karma_rule_application a LEFT JOIN karma_rule_evidence e USING(event_id, rule_uid, rule_revision, attempt) WHERE a.rule_uid = ? ORDER BY a.at DESC, a.rowid DESC LIMIT ?")
            .bind(rule_uid).bind(limit).fetch_all(&self.store.pool).await?;
        let mut applications = Vec::new();
        for row in rows {
            let evidence: Option<String> = row.try_get("evidence")?;
            let evidence = evidence
                .map(|value| serde_json::from_str::<Evidence>(&value))
                .transpose()
                .map_err(EngineError::Json)?;
            let Some(evidence) = evidence else {
                applications.push(serde_json::json!({"event": row.try_get::<String, _>("event_id")?, "revision": row.try_get::<i64, _>("rule_revision")?, "evidence": null, "unavailable": "No evaluation evidence was recorded for this application"}));
                continue;
            };
            let inputs = evidence.inputs.as_ref().ok_or_else(|| {
                EngineError::Forbidden("This historical evaluation has unavailable inputs".into())
            })?;
            self.refuse_unreadable_karma_inputs(actor, inputs).await?;
            self.refuse_unreadable_karma_inputs(actor, &[evidence.target.clone()])
                .await?;
            let event: String = row.try_get("event_id")?;
            let revision: i64 = row.try_get("rule_revision")?;
            let attempt: i64 = row.try_get("attempt")?;
            let mut effects = Vec::new();
            for position in 0..evidence.consequences.len() {
                let uid = format!("{event}:{rule_uid}:{revision}:{position}");
                let outcomes = store::sqlx::query("SELECT o.status, o.result, o.at, o.attempt FROM karma_effect_outcome o JOIN effect_queue q ON q.uid = o.effect_uid WHERE q.request_id = ? ORDER BY o.attempt")
                    .bind(&uid).fetch_all(&self.store.pool).await?;
                for outcome in outcomes {
                    let reason = if evidence.consequences.as_slice()[position].transfer_target().is_some() { Some(outcome.try_get::<String, _>("result")?) } else { None };
                    effects.push(serde_json::json!({"position": position, "attempt": outcome.try_get::<i64, _>("attempt")?, "status": outcome.try_get::<String, _>("status")?, "at": outcome.try_get::<String, _>("at")?, "reason": reason}));
                }
                let queued =
                    store::sqlx::query("SELECT status, attempts FROM effect_queue WHERE request_id = ? AND status IN ('queued', 'running', 'uncertain')")
                        .bind(&uid)
                        .fetch_optional(&self.store.pool)
                        .await?;
                if let Some(queued) = queued {
                    effects.push(serde_json::json!({"position": position, "attempt": queued.try_get::<i64, _>("attempts")?, "status": queued.try_get::<String, _>("status")?}));
                }
            }
            applications.push(serde_json::json!({"event": event, "revision": revision, "attempt": attempt, "status": row.try_get::<String, _>("status")?, "reason": row.try_get::<Option<String>, _>("reason")?, "at": row.try_get::<String, _>("at")?, "intended_at": row.try_get::<String, _>("intended_at")?, "frequency": row.try_get::<Option<String>, _>("frequency_uid")?, "evidence": evidence, "effects": effects}));
        }
        Ok(serde_json::json!({"rule": rule_uid, "applications": applications}))
    }
}
