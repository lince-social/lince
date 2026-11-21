//! Read-only Transfer projection for a delivery recipient.

use super::{
    application, karma_state, rows::execute_transfers, transfer_query_error, viewer::TransferViewer,
};
use crate::{Include, Predicate, Protein, ProteinError, Source};
use serde_json::Value;
use std::collections::HashSet;
use store::{
    Store, sqlx::query_scalar, transfer_delivery::policies_for_transfer,
    visibility::hidden_from_organ,
};

/// Projects a Transfer for an active delivery recipient and removes local-only fields.
///
/// # Errors
/// Returns an error on a failed read, missing delivery policy, or hidden Transfer.
pub async fn transfer_delivery_projection(
    store: &Store,
    transfer_uid: &str,
    recipient_person_uid: &str,
    recipient_organ_uid: &str,
) -> Result<Value, ProteinError> {
    if hidden_from_organ(&store.pool, recipient_organ_uid)
        .await?
        .contains(transfer_uid)
    {
        return Err(transfer_query_error(
            "protein_transfer_visibility_denied",
            "The Organ cannot view this Transfer's Record data",
        ));
    }
    if !policies_for_transfer(&store.pool, transfer_uid)
        .await?
        .iter()
        .any(|policy| {
            policy.state == "active"
                && policy.recipient_person_uid == recipient_person_uid
                && policy.recipient_organ_uid == recipient_organ_uid
        })
    {
        return Err(transfer_query_error(
            "protein_transfer_delivery_policy_missing",
            "recipient has no active Transfer delivery policy",
        ));
    }
    let protein = Protein {
        source: Source::Transfer,
        filter: vec![Predicate::UidEq(transfer_uid.to_string())],
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(1),
    };
    let visible = HashSet::from([transfer_uid.to_string()]);
    let rows = execute_transfers(
        store,
        &protein,
        Some(&visible),
        Some(recipient_organ_uid),
        Some(recipient_person_uid),
        Some(TransferViewer::delivery(
            recipient_organ_uid,
            recipient_person_uid,
        )),
    )
    .await?;
    let mut row = rows
        .into_iter()
        .find(|row| row.get("kind").and_then(Value::as_str) == Some("transfer"))
        .ok_or_else(|| {
            transfer_query_error(
                "protein_transfer_delivery_not_visible",
                "recipient cannot view this Transfer",
            )
        })?;
    let projected_revision = row
        .get("revision")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if let Some(occurrences) = row.get_mut("occurrences").and_then(Value::as_array_mut) {
        for occurrence in occurrences.iter_mut().filter_map(Value::as_object_mut) {
            let Some(occurrence_uid) = occurrence.get("uid").and_then(Value::as_str) else {
                continue;
            };
            let source_owner = query_scalar::<_, String>(
                "SELECT p.party_uid FROM transfer_occurrence o
                 JOIN promise p ON p.uid = o.promise_uid WHERE o.uid = ?",
            )
            .bind(occurrence_uid)
            .fetch_optional(&store.pool)
            .await?;
            if source_owner.as_deref() != Some(recipient_person_uid) {
                continue;
            }
            let preparation = application::prepare(
                store,
                transfer_uid,
                occurrence_uid,
                projected_revision,
                recipient_person_uid,
            )
            .await?;
            occurrence.insert("application_preparation".into(), preparation);
        }
    }
    strip_delivery_mutation_and_proof(&mut row);
    karma_state::append(store, &mut row).await?;
    if let Some(object) = row.as_object_mut() {
        object.insert("delivery_read_only".into(), Value::Bool(true));
    }
    Ok(row)
}

fn strip_delivery_mutation_and_proof(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for field in [
                "proof",
                "fact_signature",
                "action_intent",
                "record_quantity",
                "availability",
                "application",
                "settlement_preview",
                "private_compensated_slices",
                "private_settlement_slices",
                "private_preview",
                "local_application",
                "expected_local_delta",
                "expected_local_cumulative_before",
                "compensation",
            ] {
                object.remove(field);
            }
            for child in object.values_mut() {
                strip_delivery_mutation_and_proof(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                strip_delivery_mutation_and_proof(child);
            }
        }
        _ => {}
    }
}
