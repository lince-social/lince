//! Preview of settling one Transfer occurrence.

use super::{
    application, status::effective_settlement_application, transfer_predicate_name,
    transfer_query_error, viewer::TransferViewer,
};
use crate::{Predicate, Protein, ProteinError};
use nucleus::{PromiseState, transfer::application::amount};
use serde_json::{Value, json};
use std::collections::HashSet;
use store::{
    Store,
    exact::sum_exact,
    misc::get_promise,
    organs, records,
    sqlx::Error::Protocol,
    transfer_accounting::{Binding, applied},
    transfer_effects::quote,
    transfers::{
        effective_occurrence_remainder_policy, occurrence, occurrence_settlement_progress,
    },
};

fn parse_request(protein: &Protein) -> Result<(&str, f64), ProteinError> {
    let include = &protein.include;
    if protein.aggregate.is_some()
        || !protein.order.is_empty()
        || protein.limit.is_some()
        || include.facts.is_some()
        || include.promises.is_some()
        || include.links.is_some()
        || include.threads.is_some()
        || include.availability
        || include.extension.is_some()
        || include.numeric_extensions
        || include.projection.is_some()
        || include.contact
    {
        return Err(transfer_query_error(
            "protein_transfer_settlement_preview_invalid_query",
            "include, aggregate, order, and limit are not supported",
        ));
    }
    let mut occurrence_uid = None;
    let mut canonical_quantity = None;
    for predicate in &protein.filter {
        match predicate {
            Predicate::UidEq(uid) if occurrence_uid.is_none() => {
                occurrence_uid = Some(uid.as_str());
            }
            Predicate::QuantityEq(quantity) if canonical_quantity.is_none() => {
                canonical_quantity = Some(quantity.to_f64());
            }
            predicate => {
                return Err(transfer_query_error(
                    "protein_transfer_settlement_preview_unsupported_predicate",
                    transfer_predicate_name(predicate),
                ));
            }
        }
    }
    match (occurrence_uid, canonical_quantity) {
        (Some(occurrence_uid), Some(canonical_quantity)) => {
            Ok((occurrence_uid, canonical_quantity))
        }
        _ => Err(transfer_query_error(
            "protein_transfer_settlement_preview_invalid_query",
            "one direct uid_eq and one direct quantity_eq predicate are required",
        )),
    }
}

pub(crate) async fn execute_transfer_settlement_preview(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let (occurrence_uid, requested_quantity) = parse_request(protein)?;
    if !requested_quantity.is_finite() || requested_quantity <= 0.0 {
        return Ok(Vec::new());
    }
    let Some(occurrence) = occurrence(&store.pool, occurrence_uid).await? else {
        return Ok(Vec::new());
    };
    let exchange = occurrence
        .exchange_uid
        .as_deref()
        .unwrap_or(&occurrence.promise_uid);
    let Some(source_promise) = get_promise(&store.pool, &occurrence.promise_uid).await? else {
        return Ok(Vec::new());
    };
    let Some(owner_person_uid) = source_promise.party_uid.as_deref() else {
        return Ok(Vec::new());
    };
    let viewer = TransferViewer::resolve(store, subject).await?;
    if !viewer.update_identity_blockers().is_empty()
        || installed_signer_actor != Some(owner_person_uid)
        || (!viewer.local && viewer.person.as_deref() != Some(owner_person_uid))
    {
        return Ok(Vec::new());
    }
    let Some(local_record_uid) = source_promise.record_uid.as_deref() else {
        return Ok(Vec::new());
    };
    if occurrence.record_uid.as_deref() != Some(local_record_uid)
        || source_promise.state != PromiseState::Active
        || !occurrence.delivery_claimed
        || !occurrence.receipt_claimed
        || occurrence.disputed
        || !source_promise.delta.is_finite()
        || !((source_promise.delta < 0.0 && occurrence.giver_person_uid == owner_person_uid)
            || (source_promise.delta > 0.0 && occurrence.receiver_person_uid == owner_person_uid))
    {
        return Ok(Vec::new());
    }
    let Some(local_record) = records::get(&store.pool, local_record_uid).await? else {
        return Ok(Vec::new());
    };
    let local_organ_uid = organs::local(&store.pool).await?.map(|organ| organ.uid);
    if local_record
        .organ_uid
        .as_deref()
        .is_some_and(|organ| Some(organ) != local_organ_uid.as_deref())
    {
        return Ok(Vec::new());
    }
    let Some(progress) = occurrence_settlement_progress(&store.pool, occurrence_uid).await? else {
        return Ok(Vec::new());
    };
    if progress.remaining_quantity <= 0.0 || requested_quantity > progress.remaining_quantity {
        return Ok(Vec::new());
    }
    if visible.is_some_and(|visible| !visible.contains(local_record_uid))
        || !application::visible_policy(
            store,
            &occurrence.transfer_uid,
            exchange,
            owner_person_uid,
            visible,
        )
        .await?
    {
        return Ok(Vec::new());
    }
    let application =
        effective_settlement_application(store, &occurrence, &source_promise, owner_person_uid)
            .await?;
    if application.error.is_some() {
        return Ok(Vec::new());
    }
    let local_record_uid = application.record.as_deref().unwrap_or(local_record_uid);
    let applied = applied(&store.pool, occurrence_uid, owner_person_uid).await?;
    let canonical_after = sum_exact([
        applied.canonical,
        amount(requested_quantity).map_err(|error| Protocol(error.to_string()))?,
    ])?;
    let group = quote(
        &store.pool,
        &Binding {
            transfer: &occurrence.transfer_uid,
            exchange,
            occurrence: Some(occurrence_uid),
            person: owner_person_uid,
            record: Some(local_record_uid),
            unit: occurrence.unit_uid.as_deref(),
            outgoing: source_promise.delta < 0.0,
        },
        canonical_after,
    )
    .await?;
    let (delta, after) = application::effect_amounts(
        group.as_ref(),
        &application.formula,
        canonical_after,
        applied.local,
    )?;
    let local_delta = delta.to_f64();
    let remainder_policy =
        effective_occurrence_remainder_policy(&store.pool, occurrence_uid, owner_person_uid)
            .await?;
    let remainder_policy = remainder_policy.as_str();
    Ok(vec![json!({
        "kind": "transfer_settlement_preview",
        "uid": occurrence.uid,
        "transfer": occurrence.transfer_uid,
        "occurrence": occurrence.uid,
        "promise": occurrence.promise_uid,
        "person": owner_person_uid,
        "revision": occurrence.revision,
        "status": "ready",
        "canonical_unit": occurrence.unit_uid,
        "canonical_quantity": requested_quantity,
        "settled_quantity": progress.settled_quantity,
        "remaining_quantity": progress.remaining_quantity,
        "remaining_after": progress.remaining_quantity - requested_quantity,
        "local_record": local_record_uid,
        "local_delta": local_delta,
        "local_delta_exact": delta.to_string(),
        "local_cumulative_after_exact": after.to_string(),
        "local_cumulative_after": after.to_f64(),
        "application_formula_hash": application.formula_hash,
        "application_formula_version": application.version,
        "remainder_policy": remainder_policy,
        "expected_remaining_quantity": progress.remaining_quantity,
        "expected_local_delta": local_delta,
        "effects":group.as_ref().map(|group| &group.effects),
        "expected_effects_hash":group.as_ref().map(|group| &group.hash),
        "expected_application_formula_hash": application.formula_hash,
        "expected_application_formula_version": application.version,
        "expected_remainder_policy": remainder_policy,
        "capabilities": { "settle": true },
        "blocking_reasons": { "settle": Vec::<&str>::new() },
    })])
}
