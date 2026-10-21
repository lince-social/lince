use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use serde_json::json;

use crate::{Engine, EngineError, actions::Action, karma_transfer_effects::EvaluatedTransfer};

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "reopen_need_changed",
        message: message.to_string(),
    }
}

impl Engine {
    pub(crate) async fn reopen_transfer_need(
        &self,
        evaluated: &EvaluatedTransfer,
        mode: (
            Option<DecimalValue>,
            Option<nucleus::transfer::karma::AutomationStage>,
        ),
        request: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<String, EngineError> {
        let (target, repeat) = mode;
        let _guard = self.transfer_need_execution.lock().await;
        self.require_transfer_origin_authority(&evaluated.transfer)
            .await?;
        self.require_transfer_draft_creator(&evaluated.transfer, actor)
            .await?;
        for permission in [
            "record:read",
            "transfer:read",
            "transfer:create",
            "transfer:update",
        ] {
            self.require_permission(actor, permission).await?;
        }
        self.transfer_person_signer(&evaluated.person, None).await?;
        let replayed = store::transfers::revision_for_request(&self.store.pool, request)
            .await?
            .is_some();
        if !replayed && repeat.is_none() {
            let children: Vec<String> = store::sqlx::query_scalar("SELECT t.record_uid FROM transfer t JOIN record r ON r.uid=t.record_uid WHERE t.parent_uid=? AND r.deleted_at IS NULL AND r.quantity_mantissa!='0' LIMIT 4097")
            .bind(&evaluated.transfer).fetch_all(&self.store.pool).await?;
            if children.len() > 4096 {
                return Err(invalid(
                    "Archive this Need's completed occurrences before reopening",
                ));
            }
            for child in children {
                if !store::location::transfer_ended(&self.store.pool, &child).await? {
                    return Ok("Need already has an unresolved demand".into());
                }
            }
        }
        let source = store::transfers::get(&self.store.pool, &evaluated.transfer)
            .await?
            .ok_or_else(|| invalid("Need template unavailable"))?;
        let input = store::transfers::publication_input(
            &self.store.pool,
            &evaluated.transfer,
            evaluated.revision,
            &evaluated.person,
            request,
        )
        .await?;
        if input.creator_person_uid != evaluated.person {
            return Err(invalid("Only the Need's creator may reopen its definition"));
        }
        let mut promises = input.promises;
        if let Some(target) = target {
            if target.is_negative() || target.is_zero() || promises.len() != 1 {
                return Err(invalid(
                    "Target-based reopening requires a positive target and one contribution",
                ));
            }
            let record = source
                .source_uid
                .as_deref()
                .ok_or_else(|| invalid("Choose the Record whose quantity this Need replenishes"))?;
            self.authorize_rule_target(record, actor).await?;
            let balance = store::transfer_balances::read(&self.store.pool, record).await?;
            let record = store::records::get(&self.store.pool, record)
                .await?
                .ok_or_else(|| invalid("Quantity Record unavailable"))?;
            if promises[0].unit_uid != record.unit_uid {
                return Err(invalid("Need and quantity Record must use the same unit"));
            }
            let amount = store::exact::difference(target, balance.surplus)?;
            if amount.is_zero() || amount.is_negative() {
                return Ok("Need target already met by available quantity and commitments".into());
            }
            let value = amount.to_f64();
            if !value.is_finite()
                || DecimalValue::from_f64_lossy(value)
                    .map_err(invalid)?
                    .exact_numeric_cmp(amount)
                    != std::cmp::Ordering::Equal
            {
                return Err(invalid(
                    "Need quantity is not exactly representable by Transfer terms",
                ));
            }
            promises[0].delta = value.copysign(promises[0].delta);
        }
        let promises = promises.into_iter().map(|promise| json!({
            "item":promise.item,"record":promise.record_uid.unwrap_or_default(),"party":promise.person_uid,
            "open":promise.open,"delta":promise.delta,"unit":promise.unit_uid,
            "window_start":promise.window_start,"window_end":promise.window_end,"place":promise.location,
            "condition":promise.condition,"reserve_from":promise.reserve_from,"reuse_policy":promise.open_reuse_policy
        })).collect::<Vec<_>>();
        let dependencies = input.dependencies.into_iter().map(|dependency| json!({
            "scope":dependency.scope,"promise":dependency.promise_uid,"upstream_kind":dependency.upstream_kind,
            "upstream":dependency.upstream_uid,"required_state":dependency.required_state
        })).collect::<Vec<_>>();
        if dependencies
            .iter()
            .any(|dependency| dependency["promise"].is_string())
        {
            return Err(invalid(
                "Reopening a Need with promise-scoped dependencies requires a reviewed draft",
            ));
        }
        let invitees = evaluated
            .reads
            .get(&evaluated.transfer)
            .into_iter()
            .flat_map(|state| state.participants.keys())
            .filter(|person| *person != &evaluated.person)
            .collect::<Vec<_>>();
        let action: Action = serde_json::from_value(json!({
            "action":"create-transfer-draft","request_id":request,"creator":evaluated.person,
            "head":input.terms.head,"agreement":input.terms.agreement_type,"agreement_pct":input.terms.agreement_pct,
            "satiation":input.terms.satiation.unwrap_or_else(||"none".into()),"parent":evaluated.transfer,
            "source":source.source_uid,"visibility":source.visibility,"max_proximity":source.max_proximity,
            "reserve_default":input.terms.reserve_default,"require_confirmation":input.terms.require_confirmation,
            "default_place":input.terms.default_place,"invitees":invitees,"promises":promises,"dependencies":dependencies,
            "public_history":input.terms.public_history
        }))?;
        let outcome = Box::pin(self.act_at(action, actor.map(str::to_owned), now)).await?;
        if let Some(stage) =
            repeat.filter(|stage| *stage != nucleus::transfer::karma::AutomationStage::Draft)
        {
            use nucleus::transfer::karma::AutomationStage;
            let transfer = outcome
                .created
                .ok_or_else(|| invalid("Repeated proposal has no Transfer"))?;
            let publication = format!("{request}:publish");
            if store::transfers::revision_for_request(&self.store.pool, &publication)
                .await?
                .is_none()
            {
                let revision = store::transfers::get(&self.store.pool, &transfer)
                    .await?
                    .ok_or_else(|| invalid("Repeated Transfer unavailable"))?
                    .revision as u64;
                if revision != 1 {
                    return Err(invalid(
                        "Repeated proposal changed before publication; review it manually",
                    ));
                }
                Box::pin(self.act_at(
                    Action::PublishTransfer {
                        transfer: transfer.clone(),
                        expected_revision: revision,
                        request_id: publication,
                        person: Some(evaluated.person.clone()),
                        expected: None,
                        expected_state: None,
                    },
                    actor.map(str::to_owned),
                    now,
                ))
                .await?;
            }
            if matches!(stage, AutomationStage::Commit | AutomationStage::Activate) {
                let revision = store::transfers::get(&self.store.pool, &transfer)
                    .await?
                    .ok_or_else(|| invalid("Repeated Transfer unavailable"))?
                    .revision as u64;
                if revision != 2 {
                    return Err(invalid(
                        "Repeated terms changed before agreement; review them manually",
                    ));
                }
                Box::pin(self.act_at(
                    Action::AssignTransferAgreementLevel {
                        transfer: transfer.clone(),
                        expected_revision: revision,
                        request_id: format!("{request}:commit"),
                        person: Some(evaluated.person.clone()),
                        level: 2,
                        expected: None,
                        expected_state: None,
                    },
                    actor.map(str::to_owned),
                    now,
                ))
                .await?;
                if stage == AutomationStage::Activate {
                    let promises =
                        store::transfers::promises_of(&self.store.pool, &transfer).await?;
                    let effects = promises
                        .iter()
                        .filter(|promise| {
                            promise.party_uid.as_deref() == Some(evaluated.person.as_str())
                        })
                        .map(
                            |promise| nucleus::karma::Consequence::ActivateTransferFulfillment {
                                transfer: transfer.clone(),
                                person: evaluated.person.clone(),
                                promise: promise.uid.clone(),
                                fulfillment: "once".into(),
                            },
                        )
                        .collect::<Vec<_>>();
                    if effects.is_empty() {
                        return Err(invalid(
                            "Choose a contribution owned by the acting Person before enabling activation",
                        ));
                    }
                    let consequence =
                        serde_json::to_string(&json!({"target":transfer,"consequences":effects}))?;
                    Box::pin(
                        self.act_at(
                            Action::SaveKarmaRule {
                                rule: None,
                                expected_revision: None,
                                identity: None,
                                fields: [
                                    format!("transfer_ready(@{transfer})"),
                                    "> 0".into(),
                                    consequence,
                                ]
                                .map(|source| {
                                    nucleus::karma::rule_field::RuleFieldInput::Text { source }
                                }),
                                request_id: format!("{request}:activation"),
                            },
                            actor.map(str::to_owned),
                            now,
                        ),
                    )
                    .await?;
                }
            }
        }
        Ok("Proposed a new ordinary Need occurrence; previous receipts are unchanged".into())
    }
}
