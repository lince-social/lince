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
        self.karma_condition_records(Condition::parse(source).map_err(invalid)?, actor).await?;
        let condition = RuleCondition {
            source: source.into(),
            bindings: Vec::new(),
            gate: Gate::Always,
            carry: Carry::Value,
        };
        let value = crate::karma_transfers::scope(actor, Box::pin(self.evaluate_rule_condition(&condition, now, now))).await?;
        Ok(
            serde_json::json!({"source":source,"value":value.map(|value| value.to_string()),"at":now.to_rfc3339()}),
        )
    }

    pub(crate) async fn save_karma_rule(
        &self,
        rule_uid: Option<String>,
        expected_revision: Option<i64>,
        fields: [RuleFieldInput; 3],
        identity: Option<nucleus::karma::rule_field::RuleIdentity>,
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
        let identity = identity.map(|identity| nucleus::karma::rule_field::RuleIdentity {
            name: identity.name.trim().into(),
            slug: identity.slug.trim().into(),
        });
        if let Some(identity) = &identity {
            if identity.name.is_empty()
                || identity.name.len() > 256
                || identity.name.chars().any(char::is_control)
            {
                return Err(invalid(
                    "Use a rule name of 1–256 bytes without control characters",
                ));
            }
            if identity.slug.len() > 256 || !nucleus::valid_slug(&identity.slug) {
                return Err(invalid(
                    "Use a lowercase rule slug with letters, numbers, hyphens or dots",
                ));
            }
        }
        let mut selections = Vec::new();
        let mut shared_bindings = None;
        let mut shared_target = None;
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
                        self.refuse_unreadable_karma_inputs(actor, &[crate::karma_transfer_effects::target(&rule).into()])
                            .await?;
                        match kind {
                            RuleFieldKind::Condition => {
                                let bindings =
                                    rule.condition.clone()
                                        .map(|condition| condition.bindings.into_iter().filter(|binding| !binding.reading.starts_with("consequence.")).collect::<Vec<_>>())
                                        .ok_or_else(|| invalid("Shared condition is missing"))?;
                                if shared_bindings
                                    .as_ref()
                                    .is_some_and(|saved| saved != &bindings)
                                {
                                    return Err(invalid(
                                        "Shared condition has conflicting bindings",
                                    ));
                                }
                                shared_bindings = Some(bindings);
                            }
                            RuleFieldKind::Consequence => {
                                let value = (crate::karma_transfer_effects::target(&rule).to_owned(), rule.condition.as_ref().map_or_else(Vec::new, |condition| condition.bindings.iter().filter(|binding| binding.reading.starts_with("consequence.")).cloned().collect::<Vec<_>>()));
                                if shared_target
                                    .as_ref()
                                    .is_some_and(|saved| saved != &value)
                                {
                                    return Err(invalid(
                                        "Shared consequence has conflicting targets",
                                    ));
                                }
                                shared_target = Some(value);
                            }
                            RuleFieldKind::Threshold => {}
                        }
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
                self.authorize_karma_rule(&rule, actor).await?;
                Some(rule)
            }
            None => None,
        };
        let sources: Vec<_> = selections
            .iter()
            .map(|selection| selection.field.source.clone())
            .collect();
        let mut rule = self
            .prepare_editor_rule(
                previous,
                &sources,
                shared_bindings,
                shared_target,
                actor,
                now,
            )
            .await?;
        rule.actor_uid = actor.map(str::to_owned);
        store::karma_fields::save_rule(
            &self.store.pool,
            &rule,
            &selections,
            identity.as_ref(),
            &request_id,
            now,
        )
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
            self.authorize_karma_rule(&current, actor)
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
                .prepare_editor_rule(Some(current), &sources, None, None, actor, now)
                .await?;
            self.authorize_karma_rule(&rule, rule.actor_uid.as_deref())
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

    pub(crate) async fn authorize_rule_target(
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
        shared_bindings: Option<Vec<nucleus::karma::ConditionBinding>>,
        shared_target: Option<(String, Vec<nucleus::karma::ConditionBinding>)>,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Recurrence, EngineError> {
        nucleus::karma::rule_field::check_condition_source(&sources[0]).map_err(invalid)?;
        let source = self
            .canonical_condition(Some(sources[0].clone()))
            .await?
            .ok_or_else(|| invalid("A condition is required"))?;
        let bindings = store::karma_bindings::resolve(
            &self.store.pool,
            &source,
            shared_bindings.as_deref().unwrap_or_else(|| {
                previous
                    .as_ref()
                    .and_then(|rule| rule.condition.as_ref())
                    .map_or(&[], |condition| condition.bindings.as_slice())
            }),
        )
        .await?;
        let mut condition = RuleCondition {
            source,
            bindings,
            gate: Gate::parse(&sources[1]).map_err(invalid)?,
            carry: previous
                .as_ref()
                .and_then(|rule| rule.condition.as_ref())
                .map_or(Carry::Value, |condition| condition.carry.clone()),
        };
        let consequence = RuleConsequence::parse(&sources[2]).map_err(invalid)?;
        let previous_target = if let Some(previous) = &previous {
            store::karma_fields::for_rule(&self.store.pool, &previous.uid)
                .await?
                .into_iter()
                .find(|field| field.kind == RuleFieldKind::Consequence)
                .and_then(|field| RuleConsequence::parse(&field.source).ok())
                .filter(|old| old.target == consequence.target)
                .map(|_| previous.record_uid.clone())
        } else {
            None
        };
        let previous_effect_bindings = shared_target.as_ref().map(|(_, bindings)| bindings.as_slice()).unwrap_or_else(|| previous.as_ref().and_then(|rule| rule.condition.as_ref()).map_or(&[], |condition| condition.bindings.as_slice()));
        let consequences = self.bind_transfer_effects(consequence.consequences, &mut condition.bindings, previous_effect_bindings).await?;
        let target = match self.transfer_rule_anchor(&consequences, actor).await? {
            Some(target) => target,
            None => {
                let target = match shared_target.map(|(target, _)| target).or(previous_target) {
                    Some(target) => target,
                    None => self.resolve(&consequence.target).await?,
                };
                self.authorize_rule_target(&target, actor).await?;
                target
            }
        };
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
