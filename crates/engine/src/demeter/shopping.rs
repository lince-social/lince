use super::*;
use crate::{actions::Action, record_creation::Draft};
use nucleus::karma::{
    Cadence, Consequence, Slug, TimestampMs,
    rule_field::{RuleConsequence, RuleFieldInput, RuleIdentity},
};
use persistence::ManagedTask;
use protein::{Predicate, Protein, Source};

impl Engine {
    pub(super) async fn demeter_profile(
        &self,
        target: Option<&str>,
        profile: Option<Profile>,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let mut outcome = ActionOutcome::default();
        let uid = match (target, &profile) {
            (Some(target), _) => self.planning_read(target, actor).await?,
            (None, Some(profile)) => {
                profile.energy().map_err(invalid)?;
                let draft = Draft {
                    head: "Nutrition profile".into(),
                    ..Default::default()
                };
                let uid = draft.uid.clone();
                outcome = self
                    .create_record_draft(draft, actor.map(str::to_owned), now)
                    .await?;
                uid
            }
            _ => return Err(invalid("Choose a profile Record")),
        };
        let data = if let Some(profile) = profile {
            profile.energy().map_err(invalid)?;
            let value = serde_json::to_value(profile)?;
            outcome.facts.extend(
                self.managed_write(&uid, PROFILE, &value, None, actor, now)
                    .await?,
            );
            value
        } else {
            self.planning_data(&uid, PROFILE, actor, &mut BTreeMap::new())
                .await?
                .1
        };
        outcome.data = Some(json!({"record":uid,"profile":data}));
        Ok(outcome)
    }

    async fn demeter_shopping_query(
        &self,
        plan: &str,
        actor: Option<&str>,
    ) -> Result<Option<Protein>, EngineError> {
        let Some(predicate) =
            store::vocabulary::resolve(&self.store.pool, "shopping-for", actor).await?
        else {
            return Ok(None);
        };
        Ok(Some(Protein {
            source: Source::Record,
            filter: vec![Predicate::All(vec![
                Predicate::Relation {
                    kind: predicate,
                    direction: protein::LinkDirection::Out,
                    other: Some(plan.into()),
                },
                Predicate::QuantityLt(DecimalValue::from_integer(0)),
            ])],
            fields: None,
            include: protein::Include {
                extension: Some(protein::ExtensionInclude {
                    namespace: TASK.into(),
                }),
                ..Default::default()
            },
            aggregate: None,
            order: Vec::new(),
            limit: Some(500),
        }))
    }

    pub(super) async fn demeter_shopping(
        &self,
        plan: &str,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        let (uid, _) = self.load_demeter(plan, actor).await?;
        let query = self.demeter_shopping_query(&uid, actor).await?;
        let slug = shopping_slug(&uid);
        let reminder = self
            .shopping_rule(&uid, &format!("{slug}-rule"), actor)
            .await?;
        let rows = if let Some(query) = &query {
            protein::execute_for(&self.store, query, actor)
                .await
                .map_err(invalid)?
        } else {
            Vec::new()
        };
        Ok(ActionOutcome {
            data: Some(json!({"plan":uid,"query":query,"rows":rows,"reminder":reminder})),
            ..Default::default()
        })
    }

    pub(super) async fn complete_demeter_shopping(
        &self,
        plan: &str,
        need: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let (plan, saved) = self.load_demeter(plan, actor).await?;
        let (need, data) = self
            .planning_data(need, TASK, actor, &mut BTreeMap::new())
            .await?;
        let task: ManagedTask = serde_json::from_value(data)?;
        if task.plan != plan || task.controller != saved.controller || task.task.kind != "buy" {
            return Err(invalid("Choose a shopping Need from this plan"));
        }
        let mut outcome = self
            .act_at_with_authorship(
                Action::SetQuantityExact {
                    target: need,
                    amount: "0".into(),
                },
                actor.map(str::to_owned),
                now,
                None,
            )
            .await?;
        outcome.data = self.demeter_shopping(&plan, actor).await?.data;
        Ok(outcome)
    }

    pub(super) async fn poll_demeter(
        &self,
        run: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let mut outcome = self.poll_optimization(run, actor, now, false).await?;
        let Some(data) = &mut outcome.data else {
            return Ok(outcome);
        };
        if !matches!(data["state"].as_str(), Some("optimal" | "feasible")) {
            return Ok(outcome);
        }
        let Some(plan) = data["plan"].as_str() else {
            return Ok(outcome);
        };
        let (_, saved) = self.load_demeter(plan, actor).await?;
        if !saved.config.auto_shopping
            || saved.last_run.as_deref() != Some(run)
            || saved.accepted_hash.as_deref() == data["hash"].as_str()
        {
            return Ok(outcome);
        }
        match self.accept_demeter(plan, run, actor, now).await {
            Ok(accepted) => {
                outcome.facts.extend(accepted.facts);
                data["shopping_published"] = json!(true);
                data["managed_tasks"] = accepted.data.unwrap_or_default()["managed_tasks"].clone();
            }
            Err(error) => data["shopping_error"] = json!(error.to_string()),
        }
        Ok(outcome)
    }

    pub(super) async fn demeter_shopping_reminder(
        &self,
        plan: &str,
        first_at: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let first = DateTime::parse_from_rfc3339(first_at)
            .map_err(|_| invalid("Enter an RFC 3339 date and time with its UTC offset"))?;
        let (uid, saved) = self.load_demeter(plan, actor).await?;
        let slug = shopping_slug(&uid);
        let clock_slug = format!("{slug}-frequency");
        let rule_slug = format!("{slug}-rule");
        if let Some(rule) = self.shopping_rule(&uid, &rule_slug, actor).await? {
            return Ok(ActionOutcome {
                data: Some(json!({"rule":rule,"frequency":clock_slug,"protein":slug})),
                ..Default::default()
            });
        }
        if first <= now {
            return Err(invalid("Choose a first shopping reminder in the future"));
        }
        if !saved.config.auto_shopping {
            return Err(invalid(
                "Save the plan with automatic shopping enabled first",
            ));
        }
        // The ordinary authoring actions enforce authority for each artefact.
        self.require_permission(actor, "karma:update").await?;
        self.require_permission(actor, "organ:update").await?;
        self.demeter_term("shopping-for", actor).await?;
        let query = self
            .demeter_shopping_query(&uid, actor)
            .await?
            .ok_or_else(|| invalid("The shopping vocabulary is unavailable"))?;
        let name = format!("Shopping — {}", saved.config.name);
        let mut outcome = self
            .act_at_with_authorship(
                Action::SaveProtein {
                    slug: slug.clone(),
                    head: name.clone(),
                    ast: serde_json::to_value(query)?,
                },
                actor.map(str::to_owned),
                now,
                None,
            )
            .await?;
        let frequency = nucleus::karma::simple_frequency::frequency_from_cadence(
            Slug::new(&clock_slug).map_err(invalid)?,
            name.clone(),
            &Cadence::every_days(u32::from(saved.config.days)),
            TimestampMs::from_millis(first.timestamp_millis()).map_err(invalid)?,
        )
        .map_err(invalid)?;
        let clock = self
            .act_at_with_authorship(
                Action::SaveKarmaFrequency {
                    request_id: clock_slug.clone(),
                    frequency_uid: None,
                    expected_handle_revision: None,
                    frequency,
                    restart: true,
                },
                actor.map(str::to_owned),
                now,
                None,
            )
            .await?;
        outcome.facts.extend(clock.facts);
        let resolve_list = serde_json::to_string(&crate::actions::Action::Demeter {
            request: Command::ShoppingDue { plan: uid.clone() },
        })?;
        let consequence = serde_json::to_string(&RuleConsequence {
            target: uid,
            consequences: vec![Consequence::RunAction {
                action: resolve_list,
            }],
        })?;
        let rule = self
            .act_at_with_authorship(
                Action::SaveKarmaRule {
                    identity: Some(RuleIdentity {
                        name,
                        slug: rule_slug.clone(),
                    }),
                    rule: None,
                    expected_revision: None,
                    fields: [format!("freq(@{clock_slug})"), ">0".into(), consequence]
                        .map(|source| RuleFieldInput::Text { source }),
                    request_id: rule_slug,
                },
                actor.map(str::to_owned),
                now,
                None,
            )
            .await?;
        outcome.facts.extend(rule.facts);
        outcome.data = Some(json!({"rule":rule.created,"frequency":clock.created,"protein":slug}));
        Ok(outcome)
    }

    async fn shopping_rule(
        &self,
        plan: &str,
        slug: &str,
        actor: Option<&str>,
    ) -> Result<Option<String>, EngineError> {
        let identity: Option<(String, Option<String>)> = store::sqlx::query_as(
            "SELECT uid, actor_uid FROM recurrence WHERE slug = ? AND record_uid = ? LIMIT 1",
        )
        .bind(slug)
        .bind(plan)
        .fetch_optional(&self.store.pool)
        .await?;
        if let Some((_, owner)) = &identity
            && actor.is_some()
            && owner.as_deref() != actor
        {
            return Err(EngineError::Forbidden(
                "The shopping reminder belongs to another user".into(),
            ));
        }
        Ok(identity.map(|(rule, _)| rule))
    }

    pub(super) async fn demeter_shopping_due(
        &self,
        plan: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let (uid, saved) = self.load_demeter(plan, actor).await?;
        let outcome = self.demeter_shopping(&uid, actor).await?;
        let rows = outcome
            .data
            .as_ref()
            .and_then(|data| data["rows"].as_array());
        if let Some(rows) = rows.filter(|rows| !rows.is_empty()) {
            let items: Vec<_> = rows
                .iter()
                .take(8)
                .filter_map(|row| {
                    let head = row["head"].as_str()?;
                    let quantity = row["quantity"].as_str()?.trim_start_matches('-');
                    Some(format!("{head} ({quantity} g)"))
                })
                .collect();
            let extra = rows.len().saturating_sub(items.len());
            let more = if extra > 0 {
                format!(" and {extra} more")
            } else {
                String::new()
            };
            let message = format!(
                "{}: {}{more}. Open the live shopping Protein to check off items.",
                saved.config.name,
                items.join(", ")
            );
            let occurrence = crate::rule_runtime::EFFECT_OCCURRENCE
                .try_with(|event| event.event_id.clone())
                .unwrap_or_else(|_| now.date_naive().to_string());
            let mut tx = self.store.pool.begin().await?;
            crate::rule_runtime::queue_effect_tx(
                &mut tx,
                &format!("demeter-shopping:{uid}:{occurrence}"),
                "notify",
                json!({"message":message,"actor":actor,"demeter_shopping":true}),
                &uid,
                now,
            )
            .await?;
            tx.commit().await?;
            self.effects_changed
                .send_modify(|change| *change = change.wrapping_add(1));
        }
        Ok(outcome)
    }
}

fn shopping_slug(plan: &str) -> String {
    format!("shopping-{}", plan.to_ascii_lowercase().replace('_', "-"))
}
