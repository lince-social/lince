use chrono::{DateTime, Utc};
use nucleus::karma::rule_field::{RuleConsequence, RuleFieldInput, RuleFieldKind};
use nucleus::karma::{Cadence, Carry, Condition, Consequences, Gate};
use store::karma_fields::{Field, Selection};
use store::recurrence::{Recurrence, RuleCondition};

use crate::{
    Engine, EngineError,
    actions::{Action, ActionOutcome},
};

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_editor_invalid",
        message: message.to_string(),
    }
}

impl Engine {
    pub(crate) async fn preview_karma_reading(
        &self,
        source: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<serde_json::Value, EngineError> {
        check_source(source)?;
        let mut pending = vec![(source.to_owned(), 0)];
        let mut budget = 128;
        while let Some((source, depth)) = pending.pop() {
            nucleus::karma::rule_field::check_condition_source(&source).map_err(invalid)?;
            if depth >= 4 {
                return Err(invalid("Reading nesting is too deep"));
            }
            for token in Condition::parse(&source).map_err(invalid)?.reads() {
                budget -= 1;
                if budget == 0 {
                    return Err(invalid("Too many readings"));
                }
                if token.func == nucleus::expr::ASSERTION {
                    let concept = store::concepts::resolve(&self.store.pool, &token.slug)
                        .await?
                        .ok_or_else(|| invalid("Unknown concept"))?;
                    self.refuse_unreadable(
                        actor,
                        &store::ledger::records_with_concept(&self.store.pool, &concept).await?,
                    )
                    .await?;
                    continue;
                }
                if actor.is_some()
                    && !matches!(
                        token.func.as_str(),
                        "quantity"
                            | "signal"
                            | "value"
                            | "sum"
                            | "sum_pos"
                            | "sum_neg"
                            | "hours_since_fact"
                            | "freq"
                            | "distance"
                            | "promise_state"
                    )
                {
                    return Err(invalid(
                        "This reading has no private-safe hover preview yet",
                    ));
                }
                if token.func == "freq" {
                    self.require_permission(actor, "frequency:read").await?;
                }
                for slug in token.slug.split('|') {
                    let uid = self.resolve(slug).await?;
                    self.refuse_unreadable(actor, std::slice::from_ref(&uid))
                        .await?;
                    if token.func == "value" {
                        let rule = store::recurrence::for_record(&self.store.pool, &uid)
                            .await?
                            .into_iter()
                            .find(|rule| rule.condition.is_some() && !rule.is_paused())
                            .ok_or_else(|| invalid("Record has no value rule"))?;
                        pending.push((rule.condition.unwrap().source, depth + 1));
                    }
                }
            }
        }
        let condition = RuleCondition {
            source: source.into(),
            gate: Gate::Always,
            carry: Carry::Value,
        };
        let value = Box::pin(self.evaluate_rule_condition(&condition, now, now)).await?;
        Ok(
            serde_json::json!({"source":source,"value":value.map(|value| value.to_string()),"at":now.to_rfc3339()}),
        )
    }

    pub(crate) async fn save_karma_rule(
        &self,
        rule_uid: Option<String>,
        expected_revision: Option<i64>,
        fields: [RuleFieldInput; 3],
        request_id: String,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(
            actor,
            if rule_uid.is_some() {
                "frequency:update"
            } else {
                "frequency:create"
            },
        )
        .await?;
        check_request(&request_id)?;
        if let Some(uid) = store::karma_fields::replay(&self.store.pool, &request_id).await? {
            return Ok(ActionOutcome {
                created: Some(uid),
                ..Default::default()
            });
        }
        let mut selections = Vec::new();
        for (kind, input) in RuleFieldKind::ALL.into_iter().zip(fields) {
            let selection = match input {
                RuleFieldInput::Text { source } => Selection {
                    field: Field {
                        uid: nucleus::new_uid("kf"),
                        kind,
                        source: source.trim().into(),
                        revision: 1,
                    },
                    fresh: true,
                },
                RuleFieldInput::Reference { uid, revision } => {
                    let field = store::karma_fields::get(&self.store.pool, &uid)
                        .await?
                        .ok_or_else(|| invalid("Shared field not found"))?;
                    if field.kind != kind || field.revision != revision {
                        return Err(invalid("Shared field changed. Refresh before saving."));
                    }
                    let readers = store::karma_fields::readers(&self.store.pool, &uid).await?;
                    if readers.is_empty() {
                        return Err(invalid("Shared field is no longer in use"));
                    }
                    for reader in readers {
                        let rule = store::recurrence::get(&self.store.pool, &reader)
                            .await?
                            .ok_or_else(|| invalid("Rule not found"))?;
                        self.refuse_unreadable(actor, &[rule.record_uid]).await?;
                    }
                    Selection {
                        field,
                        fresh: false,
                    }
                }
            };
            check_source(&selection.field.source)?;
            selections.push(selection);
        }
        let previous = match rule_uid {
            Some(uid) => {
                let rule = store::recurrence::get(&self.store.pool, &uid)
                    .await?
                    .ok_or_else(|| invalid("Rule not found"))?;
                if Some(rule.revision) != expected_revision {
                    return Err(invalid("Rule changed. Refresh before saving."));
                }
                self.authorize_rule_target(&rule.record_uid, actor).await?;
                Some(rule)
            }
            None => None,
        };
        let sources: Vec<_> = selections
            .iter()
            .map(|selection| selection.field.source.clone())
            .collect();
        let mut rule = self
            .prepare_editor_rule(previous, &sources, actor, now)
            .await?;
        rule.actor_uid = actor.map(str::to_owned);
        store::karma_fields::save_rule(&self.store.pool, &rule, &selections, &request_id, now)
            .await?;
        Ok(ActionOutcome {
            created: Some(rule.uid),
            ..Default::default()
        })
    }

    pub(crate) async fn revise_karma_field(
        &self,
        uid: String,
        expected_revision: i64,
        source: String,
        request_id: String,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        check_request(&request_id)?;
        check_source(&source)?;
        if store::karma_fields::replay(&self.store.pool, &request_id)
            .await?
            .is_some()
        {
            return Ok(ActionOutcome::default());
        }
        let mut field = store::karma_fields::get(&self.store.pool, &uid)
            .await?
            .ok_or_else(|| invalid("Shared field not found"))?;
        if field.revision != expected_revision {
            return Err(invalid("Shared field changed. Refresh before saving."));
        }
        field.source = source.trim().into();
        let readers = store::karma_fields::readers(&self.store.pool, &uid).await?;
        if readers.is_empty() {
            return Err(invalid("Shared field is no longer in use"));
        }
        let mut rules = Vec::new();
        for reader in readers {
            let current = store::recurrence::get(&self.store.pool, &reader)
                .await?
                .ok_or_else(|| invalid("Rule not found"))?;
            self.authorize_rule_target(&current.record_uid, actor)
                .await?;
            let fields = store::karma_fields::for_rule(&self.store.pool, &reader).await?;
            let sources: Vec<_> = RuleFieldKind::ALL
                .into_iter()
                .map(|kind| {
                    if kind == field.kind {
                        Ok(field.source.clone())
                    } else {
                        fields
                            .iter()
                            .find(|value| value.kind == kind)
                            .map(|value| value.source.clone())
                            .ok_or_else(|| invalid("Rule field missing"))
                    }
                })
                .collect::<Result<_, _>>()?;
            let rule = self
                .prepare_editor_rule(Some(current), &sources, actor, now)
                .await?;
            self.authorize_rule_target(&rule.record_uid, rule.actor_uid.as_deref())
                .await?;
            Box::pin(self.validate_automatic_rule(
                &rule.consequences,
                rule.condition.as_ref(),
                rule.actor_uid.as_deref(),
            ))
            .await?;
            rules.push(rule);
        }
        store::karma_fields::revise_field(&self.store.pool, &field, &rules, &request_id, now)
            .await?;
        Ok(ActionOutcome::default())
    }

    async fn authorize_rule_target(
        &self,
        target: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.authorize_action(
            &Action::SetQuantityExact {
                target: target.into(),
                amount: "0".into(),
            },
            actor,
        )
        .await?;
        self.reject_direct_transfer_record_mutation(target).await
    }

    async fn prepare_editor_rule(
        &self,
        previous: Option<Recurrence>,
        sources: &[String],
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Recurrence, EngineError> {
        nucleus::karma::rule_field::check_condition_source(&sources[0]).map_err(invalid)?;
        let source = self
            .canonical_condition(Some(sources[0].clone()))
            .await?
            .ok_or_else(|| invalid("A condition is required"))?;
        let parsed = Condition::parse(&source).map_err(invalid)?;
        for token in parsed.reads() {
            if token.func != nucleus::expr::ASSERTION && token.func != "freq" {
                for slug in token.slug.split('|') {
                    self.resolve(slug).await?;
                }
            }
        }
        let condition = RuleCondition {
            source,
            gate: Gate::parse(&sources[1]).map_err(invalid)?,
            carry: previous
                .as_ref()
                .and_then(|rule| rule.condition.as_ref())
                .map_or(Carry::Value, |condition| condition.carry.clone()),
        };
        let consequence = RuleConsequence::parse(&sources[2]).map_err(invalid)?;
        let target = self.resolve(&consequence.target).await?;
        self.authorize_rule_target(&target, actor).await?;
        let consequences = self.resolve_consequences(consequence.consequences).await?;
        Box::pin(self.validate_automatic_rule(&consequences, Some(&condition), actor)).await?;
        let mut rule = previous.unwrap_or_else(|| Recurrence {
            uid: nucleus::new_uid("rec"),
            record_uid: target.clone(),
            consequences: Consequences::new(vec![nucleus::karma::Consequence::SetQuantity {
                value: None,
            }])
            .unwrap(),
            condition: None,
            note: None,
            cadence: Cadence::once(),
            anchor_at: now.to_rfc3339(),
            state: "active".into(),
            revision: 0,
            actor_uid: actor.map(str::to_owned),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        });
        rule.record_uid = target;
        rule.condition = Some(condition);
        rule.consequences = consequences;
        Ok(rule)
    }
}

fn check_source(source: &str) -> Result<(), EngineError> {
    if source.trim().is_empty() || source.len() > 16_384 {
        Err(invalid("Fields must contain 1–16384 bytes"))
    } else {
        Ok(())
    }
}

fn check_request(request: &str) -> Result<(), EngineError> {
    if request.trim().is_empty() || request.len() > 256 {
        Err(invalid("A request ID is required"))
    } else {
        Ok(())
    }
}
