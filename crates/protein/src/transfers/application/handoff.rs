//! Private application quotes and recorded effects for a delivery handoff.

use super::{effect_amounts, visible_policy};
use crate::ProteinError;
use serde_json::{Value, json};
use std::collections::HashSet;
use store::{
    Store,
    exact::sum_exact,
    facts, organs,
    sqlx::{Error::RowNotFound, query_scalar},
    transfer_accounting::{Binding, amount, applied, effective, handoff_exchange},
    transfer_delivery::application_effect_handoff,
    transfer_effects::{quote, recorded, records},
};

pub(in crate::transfers) async fn preview(
    store: &Store,
    uid: &str,
    signer: Option<&str>,
    visible: Option<&HashSet<String>>,
) -> Result<Value, ProteinError> {
    let handoff = application_effect_handoff(&store.pool, uid)
        .await?
        .ok_or(RowNotFound)?;
    let local = organs::local(&store.pool).await?.ok_or(RowNotFound)?;
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
    let exchange = handoff_exchange(&store.pool, &handoff).await?;
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
    let binding = Binding {
        transfer: &handoff.transfer_uid,
        exchange: &exchange,
        occurrence: Some(&handoff.occurrence_uid),
        person: &handoff.participant_person_uid,
        record: None,
        unit: handoff.canonical_unit_uid.as_deref(),
        outgoing: handoff.application_direction < 0,
    };
    let effective = effective(&store.pool, &binding).await;
    let applied = applied(&store.pool, &handoff.occurrence_uid, binding.person).await?;
    let before = applied.local.to_f64();
    let canonical_after = sum_exact([applied.canonical, amount(handoff.canonical_quantity)?])?;
    let quoted = quote(&store.pool, &binding, canonical_after).await;
    let group = quoted.as_ref().ok().and_then(Option::as_ref);
    let application = effective.as_ref().ok();
    let amounts = application.and_then(|application| {
        let group = quoted.as_ref().ok()?.as_ref();
        effect_amounts(group, &application.formula, canonical_after, applied.local).ok()
    });
    let exact_delta = amounts.map(|(delta, _)| delta.to_string());
    let exact_after = amounts.map(|(_, after)| after.to_string());
    let delta = amounts.map(|(delta, _)| delta.to_f64());
    let after = amounts.map(|(_, after)| after.to_f64());
    let hash = application.map(|value| value.formula_hash.as_str());
    let version = application.map_or(0, |value| value.version);
    let record = application.and_then(|value| value.record.as_deref());
    let effects_hash = group.map(|group| &group.hash);
    let error = effective
        .as_ref()
        .err()
        .or_else(|| quoted.as_ref().err())
        .map(ToString::to_string);
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
        let active: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_remote_reference WHERE uid = ? AND state = 'active')")
            .bind(&handoff.reference_uid).fetch_one(&store.pool).await?;
        if !active {
            blockers.push("delivery_access_inactive");
        }
    }
    value["local_application"] = json!(handoff.local_application_uid);
    value["private_preview"] = json!({ "formula_hash": hash, "formula_version": version,
        "effects":group.map(|group| &group.effects),
        "effects_hash":effects_hash,
        "local_delta": delta, "local_cumulative_before": before, "local_cumulative_after": after,
        "local_delta_exact":exact_delta,"local_cumulative_after_exact":exact_after,
        "policy":application, "error":error });
    value["capabilities"]["apply"] = json!(blockers.is_empty());
    value["blocking_reasons"] = json!({ "apply": blockers });
    value["action_payloads"] = json!({ "apply": {
        "action": "apply-transfer-application", "transfer": handoff.transfer_uid, "handoff": handoff.uid,
        "local_record": record, "expected_formula_hash": hash, "expected_formula_version": version,
        "expected_effects_hash":effects_hash,
        "expected_local_delta": delta, "expected_local_cumulative_before": before,
        "request_id": null, "person": handoff.participant_person_uid,
    }});
    if let Some(application) = &handoff.local_application_uid {
        let primary: String = query_scalar(
            "SELECT application_fact_uid FROM transfer_local_application WHERE uid = ?",
        )
        .bind(application)
        .fetch_one(&store.pool)
        .await?;
        let records = records(&store.pool, &primary).await?;
        if visible.is_some_and(|visible| records.iter().any(|record| !visible.contains(record))) {
            if let Some(fields) = value.as_object_mut() {
                fields.remove("private_preview");
                fields.remove("action_payloads");
            }
            return Ok(value);
        }
        let recorded = recorded(&store.pool, &primary).await?;
        let fact = facts::get(&store.pool, &primary)
            .await?
            .ok_or(RowNotFound)?;
        value["private_preview"] =
            json!({"applied":true,"local_delta_exact":fact.delta.to_string(),"effects":recorded});
        if let Some(actions) = value["action_payloads"].as_object_mut() {
            actions.remove("apply");
        }
        let correction: Option<String> = query_scalar("SELECT fact_uid FROM transfer_private_application_correction WHERE application_uid = ?")
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
