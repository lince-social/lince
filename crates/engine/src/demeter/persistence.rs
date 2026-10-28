use super::*;
use crate::{
    actions::Action,
    record_change::{Mutation, Request},
    record_creation::{Assertion, Draft},
    record_policy::ScalarChange,
};

#[derive(Clone, Serialize, Deserialize)]
struct ManagedTask {
    controller: Option<String>,
    plan: String,
    hash: String,
    state: String,
    task: production::Task,
}

fn identity(plan: &str, key: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("demeter:{plan}:{key}"));
    format!(
        "r_{}",
        nucleus::ulid_from(0, u128::from_be_bytes(digest[..16].try_into().unwrap()))
    )
}

impl Engine {
    pub(super) async fn demeter_tasks(
        &self,
        plan: &str,
        actor: Option<&str>,
    ) -> Result<Vec<Value>, EngineError> {
        let ids:Vec<String>=store::sqlx::query_scalar("SELECT e.record_uid FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND json_extract(e.fds,'$.plan')=? AND r.deleted_at IS NULL ORDER BY json_extract(e.fds,'$.task.on'),r.head LIMIT 10000").bind(TASK).bind(plan).fetch_all(&self.store.pool).await?;
        let mut tasks = Vec::new();
        for uid in ids {
            self.planning_read(&uid, actor).await?;
            let Some(record) = store::records::get(&self.store.pool, &uid).await? else {
                continue;
            };
            let meta: ManagedTask = serde_json::from_value(
                store::records::get_extension(&self.store.pool, &uid, TASK)
                    .await?
                    .ok_or_else(|| invalid("Managed task disappeared"))?,
            )
            .map_err(invalid)?;
            if actor.is_some() && meta.controller.as_deref() != actor {
                return Err(EngineError::Forbidden("Use your own generated work".into()));
            }
            let state = if meta.state == "superseded" {
                "superseded"
            } else if record.quantity.to_f64() >= 0.0 {
                "completed"
            } else if record.quantity != meta.task.amount.checked_neg().unwrap() {
                "started"
            } else {
                "pending"
            };
            tasks.push(json!({"uid":uid,"state":state,"task":meta.task,"quantity":record.quantity,"hash":meta.hash}));
        }
        Ok(tasks)
    }
    pub(super) async fn load_demeter(
        &self,
        token: &str,
        actor: Option<&str>,
    ) -> Result<(String, Saved), EngineError> {
        let uid = self.planning_read(token, actor).await?;
        let saved: Saved = serde_json::from_value(
            store::records::get_extension(&self.store.pool, &uid, PLAN)
                .await?
                .ok_or_else(|| invalid("Choose a saved Demeter plan"))?,
        )
        .map_err(invalid)?;
        if actor.is_some() && saved.controller.as_deref() != actor {
            return Err(EngineError::Forbidden(
                "Use your own personal meal plan".into(),
            ));
        }
        Ok((uid, saved))
    }

    pub(super) async fn save_demeter(
        &self,
        token: Option<&str>,
        expected_revision: u64,
        mut config: Config,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        config.validate().map_err(invalid)?;
        for food in &mut config.foods {
            food.food = self.planning_read(&food.food, actor).await?;
        }
        for reference in &mut config.references {
            *reference = self.planning_read(reference, actor).await?;
        }
        for offer in &mut config.offers {
            *offer = self.planning_read(offer, actor).await?;
        }
        let mut outcome = ActionOutcome::default();
        let (uid, mut saved, before) = if let Some(token) = token {
            let (uid, saved) = self.load_demeter(token, actor).await?;
            if saved.revision != expected_revision {
                return Err(invalid("Plan changed; reload before saving"));
            }
            let before = serde_json::to_value(&saved)?;
            (uid, saved, Some(before))
        } else {
            if expected_revision != 0 {
                return Err(invalid("A new plan starts at revision zero"));
            }
            let draft=Draft {head:config.name.clone(),body:"Personal nutrition, market offers, and recurring production configuration. Source food definitions stay separate.".into(),..Default::default()};
            let uid = draft.uid.clone();
            outcome = self
                .create_record_draft(draft, actor.map(str::to_owned), now)
                .await?;
            (
                uid,
                Saved {
                    controller: actor.map(str::to_owned),
                    revision: 0,
                    config: config.clone(),
                    last_run: None,
                    accepted_hash: None,
                },
                None,
            )
        };
        saved.revision = saved
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("Plan revision overflow"))?;
        saved.config = config;
        outcome.facts.extend(
            self.managed_write(
                &uid,
                PLAN,
                &serde_json::to_value(&saved)?,
                before.as_ref(),
                actor,
                now,
            )
            .await?,
        );
        outcome.data = Some(json!({"plan":uid,"saved":saved,"stale":true}));
        Ok(outcome)
    }

    async fn demeter_term(&self, name: &str, actor: Option<&str>) -> Result<String, EngineError> {
        if name.trim() != name || name.is_empty() || name.len() > 128 {
            return Err(invalid("Choose a short vocabulary name"));
        }
        if let Some(uid) = store::vocabulary::resolve(&self.store.pool, name, actor).await? {
            return Ok(uid);
        }
        self.require_permission(actor, "permission:assign").await?;
        let lingua = store::linguas::ensure_local(&self.store.pool).await?;
        self.act(
            Action::CreateConcept {
                lingua,
                name: name.into(),
                parents: Vec::new(),
            },
            actor.map(str::to_owned),
        )
        .await?
        .created
        .ok_or_else(|| invalid("Vocabulary creation returned no identity"))
    }

    pub(super) async fn demeter_vocabulary(
        &self,
        currency: &str,
        actor: Option<&str>,
        _now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let currency = self.demeter_term(currency, actor).await?;
        let price = self.demeter_term("price", actor).await?;
        let gram = self.demeter_term("g", actor).await?;
        let each = self.demeter_term("each", actor).await?;
        Ok(ActionOutcome {
            data: Some(json!({"currency":currency,"price_predicate":price,"g":gram,"each":each})),
            ..Default::default()
        })
    }

    pub(super) async fn save_demeter_offer(
        &self,
        draft: OfferDraft,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        if !nucleus::valid_uid(&draft.id, "r")
            || draft.vendor.len() > 256
            || draft.source_url.len() > 8192
            || draft.price.to_f64() < 0.0
            || draft.package_g.to_f64() <= 0.0
            || draft.usable_yield.to_f64() <= 0.0
            || draft
                .max_packages
                .is_some_and(|v| v.to_f64() < 0.0 || v.to_f64().fract() != 0.0)
        {
            return Err(invalid(
                "Check offer identity, nonnegative price, positive package/yield, and whole package availability",
            ));
        }
        demeter::date(&draft.observed_on).map_err(invalid)?;
        let available = demeter::date(&draft.available_on).map_err(invalid)?;
        if draft
            .valid_until
            .as_deref()
            .map(demeter::date)
            .transpose()
            .map_err(invalid)?
            .is_some_and(|d| d < available)
        {
            return Err(invalid("Offer expiry cannot precede availability"));
        }
        let food = self.planning_read(&draft.food, actor).await?;
        if store::records::get_extension(&self.store.pool, &food, "nutrition")
            .await?
            .is_none()
        {
            return Err(invalid("Choose an exact measured food preparation"));
        }
        let currency = store::vocabulary::resolve(&self.store.pool, &draft.currency, actor)
            .await?
            .ok_or_else(|| invalid("Create or choose a currency unit"))?;
        let predicate = store::vocabulary::resolve(&self.store.pool, &draft.price_predicate, actor)
            .await?
            .ok_or_else(|| invalid("Create or choose the price assertion"))?;
        let market = MarketOffer {
            controller: actor.map(str::to_owned),
            offer: demeter::Offer {
                id: draft.id.clone(),
                food: food.clone(),
                currency: currency.clone(),
                price: draft.price,
                package_g: draft.package_g,
                usable_yield: draft.usable_yield,
                available_on: draft.available_on,
                valid_until: draft.valid_until,
                max_packages: draft.max_packages,
            },
            predicate: predicate.clone(),
            vendor: draft.vendor.clone(),
            observed_on: draft.observed_on,
            source_url: draft.source_url,
        };
        if store::records::get(&self.store.pool, &draft.id)
            .await?
            .is_some()
        {
            self.planning_read(&draft.id, actor).await?;
            let existing =
                store::records::get_extension(&self.store.pool, &draft.id, OFFER).await?;
            if existing.as_ref() != Some(&serde_json::to_value(&market)?) {
                return Err(invalid(
                    "Offer identity already exists with different data; save a new observation",
                ));
            }
            return Ok(ActionOutcome {
                created: Some(draft.id.clone()),
                data: Some(json!({"offer":draft.id,"market":market})),
                ..Default::default()
            });
        }
        let record = Draft {
            uid: draft.id.clone(),
            head: format!("{} — {} g package", draft.vendor, draft.package_g),
            body: format!(
                "Personal price observation for [[{food}]]. Usable grams per purchased gram: {}. Source: {}",
                draft.usable_yield, market.source_url
            ),
            assertions: vec![Assertion {
                predicate,
                object: None,
                quantity: Some(draft.price.to_string()),
                unit: Some(currency),
            }],
            ..Default::default()
        };
        let mut outcome = self
            .create_record_draft(record, actor.map(str::to_owned), now)
            .await?;
        outcome.facts.extend(
            self.managed_write(
                &draft.id,
                OFFER,
                &serde_json::to_value(&market)?,
                None,
                actor,
                now,
            )
            .await?,
        );
        outcome.data = Some(json!({"offer":draft.id,"market":market}));
        Ok(outcome)
    }

    pub(super) async fn accept_demeter(
        &self,
        plan: &str,
        run: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, "record:create").await?;
        self.require_permission(actor, "record:update").await?;
        let (uid, mut saved) = self.load_demeter(plan, actor).await?;
        let before = serde_json::to_value(&saved)?;
        let (_, run) = self.load_optimization_run(run, actor).await?;
        if run.kind != "demeter"
            || run.plan.as_deref() != Some(&uid)
            || !["optimal", "feasible"].contains(&run.state.as_str())
        {
            return Err(invalid("Accept a completed feasible run for this plan"));
        }
        let snapshot = self.demeter_snapshot(&uid, &saved, actor).await?;
        let hash = demeter::fingerprint(&snapshot).map_err(invalid)?;
        if hash != run.hash {
            return Err(invalid("Result is stale; solve again before accepting"));
        }
        if saved.accepted_hash.as_deref() == Some(&hash) {
            return Ok(ActionOutcome {
                data: Some(json!({"plan":uid,"accepted":true,"already_accepted":true})),
                ..Default::default()
            });
        }
        let frozen = snapshot.clone();
        let (meal, projection) = tokio::task::spawn_blocking(move || {
            let meal = frozen
                .input
                .solve(run.options, &std::sync::atomic::AtomicBool::new(false))?;
            let projection = production::project(
                &frozen.input,
                &meal,
                &frozen.choices,
                &frozen.templates,
                frozen.horizon_days,
                frozen.buy_all,
            )?;
            Ok::<_, String>((meal, projection))
        })
        .await
        .map_err(invalid)?
        .map_err(invalid)?;
        let latest = self.demeter_snapshot(&uid, &saved, actor).await?;
        if demeter::fingerprint(&latest).map_err(invalid)? != hash {
            return Err(invalid("Inputs changed during acceptance; solve again"));
        }
        let mut units = BTreeMap::new();
        for task in &projection.tasks {
            if !units.contains_key(&task.unit) {
                let unit = store::vocabulary::resolve(&self.store.pool, &task.unit, actor)
                    .await?
                    .ok_or_else(|| {
                        invalid(format!(
                            "Define the {} unit before accepting production tasks",
                            task.unit
                        ))
                    })?;
                units.insert(task.unit.clone(), unit);
            }
        }
        let previous:Vec<String>=store::sqlx::query_scalar("SELECT e.record_uid FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND json_extract(e.fds,'$.plan')=? AND r.deleted_at IS NULL LIMIT 10001").bind(TASK).bind(&uid).fetch_all(&self.store.pool).await?;
        if previous.len() > production::MAX_TASKS {
            return Err(invalid("Too many managed tasks for this plan"));
        }
        let wanted: std::collections::BTreeSet<_> = projection
            .tasks
            .iter()
            .map(|t| identity(&uid, &t.key))
            .collect();
        let mut outcome = ActionOutcome::default();
        let mut preserved = 0;
        let mut established = Vec::new();
        for old_uid in &previous {
            self.planning_read(old_uid, actor).await?;
            let Some(record) = store::records::get(&self.store.pool, old_uid).await? else {
                continue;
            };
            let meta: ManagedTask = serde_json::from_value(
                store::records::get_extension(&self.store.pool, old_uid, TASK)
                    .await?
                    .ok_or_else(|| invalid("Generated work disappeared"))?,
            )
            .map_err(invalid)?;
            let ended = snapshot.choices.iter().any(|c| {
                c.food == meta.task.food
                    && c.template == meta.task.process
                    && c.confirmed
                    && c.existing_units == Some(0)
            });
            if meta.controller != saved.controller {
                return Err(EngineError::Forbidden(
                    "Generated task controller changed".into(),
                ));
            }
            if !ended
                && meta.state != "superseded"
                && meta.task.kind == "establish"
                && record.quantity.to_f64() >= 0.0
            {
                if let Some(batch) = meta.task.key.strip_suffix(":establish") {
                    established.push(format!("{batch}:"));
                }
            }
        }
        for task in &projection.tasks {
            let task_uid = identity(&uid, &task.key);
            let negative = task
                .amount
                .checked_neg()
                .ok_or_else(|| invalid("Task amount overflow"))?;
            let metadata = ManagedTask {
                controller: actor.map(str::to_owned),
                plan: uid.clone(),
                hash: hash.clone(),
                state: "pending".into(),
                task: task.clone(),
            };
            let existing = store::records::get(&self.store.pool, &task_uid).await?;
            if let Some(record) = existing {
                self.planning_read(&task_uid, actor).await?;
                let old: ManagedTask = serde_json::from_value(
                    store::records::get_extension(&self.store.pool, &task_uid, TASK)
                        .await?
                        .ok_or_else(|| invalid("Generated task identity is occupied"))?,
                )
                .map_err(invalid)?;
                if old.plan != uid || old.controller != saved.controller {
                    return Err(EngineError::Forbidden(
                        "Generated task belongs to another controller".into(),
                    ));
                }
                if old.state != "superseded"
                    && record.quantity != old.task.amount.checked_neg().unwrap()
                {
                    preserved += 1;
                    continue;
                }
                let changed = self
                    .change_record(
                        Request {
                            id: nucleus::new_uid("op"),
                            record_uid: task_uid.clone(),
                            mutation: Mutation::Quantity {
                                value: negative.to_string(),
                            },
                        },
                        actor,
                    )
                    .await?;
                outcome.facts.extend(changed.facts);
            } else {
                let source = if nucleus::valid_uid(&task.process, "r") {
                    format!(" [[{}]]", task.process)
                } else {
                    String::new()
                };
                let draft = Draft {
                    uid: task_uid.clone(),
                    head: task.title.clone(),
                    body: format!(
                        "Generated from [[{uid}]] for [[{}]].{} Expected amount: {} {}. Forecasts require confirmation as actual inventory.",
                        task.food, source, task.amount, task.unit
                    ),
                    quantity: negative.to_string(),
                    work: json!({"due":task.on}),
                    ..Default::default()
                };
                outcome.facts.extend(
                    self.create_record_draft(draft, actor.map(str::to_owned), now)
                        .await?
                        .facts,
                );
            }
            outcome.facts.extend(
                self.change_record_scalar_as(
                    &task_uid,
                    ScalarChange::Unit(Some(&units[&task.unit])),
                    actor,
                    now,
                )
                .await?,
            );
            outcome.facts.extend(
                self.managed_write(
                    &task_uid,
                    TASK,
                    &serde_json::to_value(metadata)?,
                    None,
                    actor,
                    now,
                )
                .await?,
            );
        }
        for old_uid in previous.into_iter().filter(|u| !wanted.contains(u)) {
            self.planning_read(&old_uid, actor).await?;
            let mut old: ManagedTask = serde_json::from_value(
                store::records::get_extension(&self.store.pool, &old_uid, TASK)
                    .await?
                    .unwrap(),
            )
            .map_err(invalid)?;
            let record = store::records::get(&self.store.pool, &old_uid)
                .await?
                .unwrap();
            if old.controller != saved.controller {
                return Err(EngineError::Forbidden(
                    "Generated task controller changed".into(),
                ));
            }
            let ongoing = ["care", "input", "harvest"].contains(&old.task.kind.as_str())
                && established
                    .iter()
                    .any(|batch| old.task.key.starts_with(batch));
            if old.state == "pending"
                && record.quantity == old.task.amount.checked_neg().unwrap()
                && !ongoing
            {
                outcome.facts.extend(
                    self.change_record(
                        Request {
                            id: nucleus::new_uid("op"),
                            record_uid: old_uid.clone(),
                            mutation: Mutation::Quantity { value: "0".into() },
                        },
                        actor,
                    )
                    .await?
                    .facts,
                );
                old.state = "superseded".into();
                outcome.facts.extend(
                    self.managed_write(
                        &old_uid,
                        TASK,
                        &serde_json::to_value(old)?,
                        None,
                        actor,
                        now,
                    )
                    .await?,
                );
            } else {
                preserved += 1;
            }
        }
        saved.accepted_hash = Some(hash);
        outcome.facts.extend(
            self.managed_write(
                &uid,
                PLAN,
                &serde_json::to_value(&saved)?,
                Some(&before),
                actor,
                now,
            )
            .await?,
        );
        outcome.data = Some(
            json!({"plan":uid,"accepted":true,"preserved":preserved,"projection":projection,"result":meal,"managed_tasks":self.demeter_tasks(&uid,actor).await?}),
        );
        Ok(outcome)
    }
}
