use serde_json::{Value, json};
use store::Store;

pub(super) async fn visible_policy(
    store: &Store,
    transfer: &str,
    exchange: &str,
    person: &str,
    visible: Option<&std::collections::HashSet<String>>,
) -> Result<bool, super::ProteinError> {
    let Some(visible) = visible else {
        return Ok(true);
    };
    Ok(
        store::transfer_accounting::policy(&store.pool, transfer, exchange, person)
            .await?
            .is_none_or(|policy| {
                policy
                    .effects
                    .iter()
                    .all(|effect| visible.contains(&effect.record))
            }),
    )
}

pub(super) async fn prepare(
    store: &Store,
    transfer: &str,
    occurrence: &str,
    revision: u64,
    person: &str,
) -> Result<Value, super::ProteinError> {
    let row: (f64, String, String, bool, bool, bool) = store::sqlx::query_as(
        "SELECT o.quantity, p.party_uid, p.state, o.delivery_claimed, o.receipt_claimed, o.disputed
         FROM transfer_occurrence o JOIN promise p ON p.uid = o.promise_uid WHERE o.uid = ? AND o.transfer_uid = ?"
    ).bind(occurrence).bind(transfer).fetch_one(&store.pool).await?;
    if row.1 != person {
        return Ok(Value::Null);
    }
    let allocated = store::transfer_accounting::allocated(&store.pool, occurrence, true).await?;
    let remaining = store::exact::difference(
        store::exact::difference(
            store::transfer_accounting::amount(row.0)?,
            store::transfer_cancellations::cancelled(&store.pool, occurrence).await?,
        )?,
        allocated,
    )?
    .to_f64()
    .max(0.0);
    let ready = remaining > 0.0 && row.2 == "active" && row.3 && row.4 && !row.5;
    Ok(json!({
        "canonical_quantity":remaining,"expected_remaining_quantity":remaining,
        "capabilities":{"begin":ready},
        "blocking_reasons":{"begin":if ready { vec![] } else { vec!["occurrence_not_ready_or_already_prepared"] }},
        "action_payload":{"action":"begin-transfer-settlement","transfer":transfer,"occurrence":occurrence,
            "expected_revision":revision,"expected_remaining_quantity":remaining,"canonical_quantity":remaining,
            "request_id":Value::Null,"person":person}
    }))
}

pub(super) async fn preview(
    store: &Store,
    uid: &str,
    signer: Option<&str>,
    visible: Option<&std::collections::HashSet<String>>,
) -> Result<Value, super::ProteinError> {
    let handoff = store::transfer_delivery::application_effect_handoff(&store.pool, uid)
        .await?
        .ok_or(store::sqlx::Error::RowNotFound)?;
    let local = store::organs::local(&store.pool)
        .await?
        .ok_or(store::sqlx::Error::RowNotFound)?;
    let mut value = json!({
        "uid": handoff.uid, "person": handoff.participant_person_uid,
        "organ": handoff.participant_organ_uid, "occurrence": handoff.occurrence_uid,
        "canonical_quantity": handoff.canonical_quantity, "canonical_unit": handoff.canonical_unit_uid,
        "canonical_cumulative_before": handoff.canonical_cumulative_before,
        "canonical_cumulative_after": handoff.canonical_cumulative_after,
        "canonical_remaining_after": handoff.canonical_remaining_after,
        "state": if handoff.origin_state == "accepted" { &handoff.origin_state } else { &handoff.state },
        "local_state": handoff.state, "origin_state": handoff.origin_state,
        "capabilities": { "apply": false },
    });
    if signer != Some(handoff.participant_person_uid.as_str())
        || local.uid != handoff.participant_organ_uid
    {
        return Ok(value);
    }
    let exchange = store::transfer_accounting::handoff_exchange(&store.pool, &handoff).await?;
    if !visible_policy(
        store,
        &handoff.transfer_uid,
        &exchange,
        &handoff.participant_person_uid,
        visible,
    )
    .await?
    {
        return Ok(value);
    }
    let application = store::transfer_accounting::effective(
        &store.pool,
        &store::transfer_accounting::Binding {
            transfer: &handoff.transfer_uid,
            exchange: &exchange,
            occurrence: Some(&handoff.occurrence_uid),
            person: &handoff.participant_person_uid,
            record: None,
            unit: handoff.canonical_unit_uid.as_deref(),
            outgoing: handoff.application_direction < 0,
        },
    )
    .await;
    let applied = store::transfer_accounting::applied(
        &store.pool,
        &handoff.occurrence_uid,
        &handoff.participant_person_uid,
    )
    .await?;
    let before = applied.local.to_f64();
    let group = store::transfer_effects::quote(
        &store.pool,
        &store::transfer_accounting::Binding {
            transfer: &handoff.transfer_uid,
            exchange: &exchange,
            occurrence: Some(&handoff.occurrence_uid),
            person: &handoff.participant_person_uid,
            record: None,
            unit: handoff.canonical_unit_uid.as_deref(),
            outgoing: handoff.application_direction < 0,
        },
        store::exact::sum_exact([
            applied.canonical,
            store::transfer_accounting::amount(handoff.canonical_quantity)?,
        ])?,
    )
    .await;
    let after = application.as_ref().ok().and_then(|application| {
        if let Some(group) = group.as_ref().ok()?.as_ref() {
            return group
                .effects
                .first()
                .map(|effect| (effect.delta, effect.cumulative_after));
        }
        let quantity = nucleus::transfer::application::amount(handoff.canonical_quantity).ok()?;
        let total = store::exact::sum_exact([applied.canonical, quantity]).ok()?;
        store::transfer_accounting::calculate(&application.formula, total, applied.local).ok()
    });
    let exact_delta = after.map(|value| value.0.to_string());
    let exact_after = after.map(|value| value.1.to_string());
    let delta = after.map(|value| value.0.to_f64());
    let after = after.map(|value| value.1.to_f64());
    let hash = application
        .as_ref()
        .ok()
        .map(|value| value.formula_hash.as_str());
    let version = application
        .as_ref()
        .ok()
        .map(|value| value.version)
        .unwrap_or(0);
    let record = application
        .as_ref()
        .ok()
        .and_then(|value| value.record.as_deref());
    let earlier_applied = applied.canonical.to_f64() == handoff.canonical_cumulative_before;
    let mut blockers = Vec::new();
    if handoff.state != "pending" {
        blockers.push("application_already_applied");
    }
    if !earlier_applied {
        blockers.push("earlier_application_pending");
    }
    if delta.is_none() {
        blockers.push("invalid_application_formula");
    }
    if !handoff.reference_uid.is_empty() {
        let active: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_remote_reference WHERE uid = ? AND state = 'active')")
            .bind(&handoff.reference_uid).fetch_one(&store.pool).await?;
        if !active {
            blockers.push("delivery_access_inactive");
        }
    }
    value["local_application"] = json!(handoff.local_application_uid);
    value["private_preview"] = json!({ "formula_hash": hash, "formula_version": version,
        "effects":group.as_ref().ok().and_then(Option::as_ref).map(|group| &group.effects),
        "effects_hash":group.as_ref().ok().and_then(Option::as_ref).map(|group| &group.hash),
        "local_delta": delta, "local_cumulative_before": before, "local_cumulative_after": after,
        "local_delta_exact":exact_delta,"local_cumulative_after_exact":exact_after,
        "policy":application.as_ref().ok(), "error":application.as_ref().err().or_else(||group.as_ref().err()).map(ToString::to_string) });
    value["capabilities"]["apply"] = json!(blockers.is_empty());
    value["blocking_reasons"] = json!({ "apply": blockers });
    value["action_payloads"] = json!({ "apply": {
        "action": "apply-transfer-application", "transfer": handoff.transfer_uid, "handoff": handoff.uid,
        "local_record": record, "expected_formula_hash": hash, "expected_formula_version": version,
        "expected_effects_hash":group.as_ref().ok().and_then(Option::as_ref).map(|group| &group.hash),
        "expected_local_delta": delta, "expected_local_cumulative_before": before,
        "request_id": null, "person": handoff.participant_person_uid,
    }});
    if let Some(application) = &handoff.local_application_uid {
        let primary: String = store::sqlx::query_scalar(
            "SELECT application_fact_uid FROM transfer_local_application WHERE uid = ?",
        )
        .bind(application)
        .fetch_one(&store.pool)
        .await?;
        let records = store::transfer_effects::records(&store.pool, &primary).await?;
        if visible.is_some_and(|visible| records.iter().any(|record| !visible.contains(record))) {
            value
                .as_object_mut()
                .map(|value| value.remove("private_preview"));
            value
                .as_object_mut()
                .map(|value| value.remove("action_payloads"));
            return Ok(value);
        }
        let recorded = store::transfer_effects::recorded(&store.pool, &primary).await?;
        let fact = store::facts::get(&store.pool, &primary)
            .await?
            .ok_or(store::sqlx::Error::RowNotFound)?;
        value["private_preview"] =
            json!({"applied":true,"local_delta_exact":fact.delta.to_string(),"effects":recorded});
        value["action_payloads"]
            .as_object_mut()
            .map(|value| value.remove("apply"));
        let correction: Option<String> = store::sqlx::query_scalar("SELECT fact_uid FROM transfer_private_application_correction WHERE application_uid = ?")
            .bind(application).fetch_optional(&store.pool).await?;
        value["correction"] = json!(correction);
        value["capabilities"]["compensate"] = json!(correction.is_none());
        value["action_payloads"]["compensate"] = json!({
            "action":"compensate-transfer-application","application":application,
            "person":handoff.participant_person_uid,"request_id":null
        });
    }
    Ok(value)
}

pub(super) async fn policies(
    store: &Store,
    transfer: &mut Value,
    signer: Option<&str>,
    visible: Option<&std::collections::HashSet<String>>,
) -> Result<(), super::ProteinError> {
    transfer
        .as_object_mut()
        .map(|value| value.remove("private_applications"));
    let Some(person) = signer else {
        return Ok(());
    };
    let local: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record p JOIN record o ON o.slug = 'local-organ' AND o.kind = 'organ' WHERE p.uid = ? AND p.organ_uid = o.uid AND p.deleted_at IS NULL)")
        .bind(person).fetch_one(&store.pool).await?;
    if !local {
        return Ok(());
    }
    let Some(uid) = transfer["uid"].as_str() else {
        return Ok(());
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut applications = Vec::new();
    for promise in transfer["promises"].as_array().into_iter().flatten() {
        let Some(exchange) = promise["exchange"].as_str() else {
            continue;
        };
        if (promise["giver"] != person && promise["receiver"] != person) || !seen.insert(exchange) {
            continue;
        }
        let policy = store::transfer_accounting::policy(&store.pool, uid, exchange, person).await?;
        if !visible_policy(store, uid, exchange, person, visible).await? {
            continue;
        }
        let formula = match &policy {
            Some(policy) => policy.formula.clone(),
            None if promise["giver"] == person => "-incoming()".into(),
            None => store::config::transfer_application_formula(&store.pool).await?,
        };
        let record = policy
            .as_ref()
            .map(|value| value.record.as_str())
            .or_else(|| {
                if promise["party"] == person {
                    promise["record"].as_str()
                } else {
                    None
                }
            });
        let balance = match record {
            Some(record) if visible.is_none_or(|visible| visible.contains(record)) => {
                Some(store::transfer_balances::read(&store.pool, record).await?)
            }
            Some(_) => continue,
            None => None,
        };
        let stock_limit = match record {
            Some(record) => Some(
                json!({"current":store::transfer_stock::get(&store.pool, record).await?,
                "version":store::transfer_stock::version(&store.pool, record).await?}),
            ),
            None => None,
        };
        let stock_version = stock_limit
            .as_ref()
            .map(|value| value["version"].clone())
            .unwrap_or(json!(0));
        let effects = policy
            .as_ref()
            .map(|policy| json!(policy.effects))
            .unwrap_or_else(|| json!([{"record":record,"formula":formula,"mode":"quantity"}]));
        let mut balances = Vec::new();
        for effect in effects.as_array().into_iter().flatten() {
            if let Some(record) = effect["record"].as_str() {
                balances.push(json!({"record":record,"balance":store::transfer_balances::read(&store.pool, record).await?}));
            }
        }
        applications.push(json!({
            "balance":balance,"balances":balances,"record":record,"stock_limit":stock_limit,
            "exchange":exchange,"person":person,"agreed_amount":promise["delta"].as_f64().map(f64::abs),
            "policy":policy,"capabilities":{"set_private_policy":true,"set_stock_limit":record.is_some(),"remove_stock_limit":stock_limit.as_ref().is_some_and(|value|value["current"].is_object())},
            "action_payloads":{
                "set_stock_limit":{"action":"set-record-stock-limit","record":record,"person":person,"minimum":store::exact::zero(),"expected_version":stock_version,"request_id":null},
                "remove_stock_limit":{"action":"set-record-stock-limit","record":record,"person":person,"minimum":null,"expected_version":stock_version,"request_id":null},
                "set_private_policy":{
                "action":"set-transfer-private-application-policy","transfer":uid,"exchange":exchange,
                "person":person,"effects":effects,
                "expected_version":policy.as_ref().map(|value| value.version).unwrap_or(0),"request_id":null
            }}
        }));
    }
    transfer["private_applications"] = json!(applications);
    Ok(())
}
