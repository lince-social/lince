//! Origin-side social delivery and local settlement handoffs.

mod recipients;

use super::application;
use crate::ProteinError;
use serde_json::{Value, json};
use std::collections::HashSet;
use store::{
    Store, cells, executor, organs,
    sqlx::{Error::RowNotFound, query_as},
};

pub(super) async fn origin_social_delivery_projection(
    store: &Store,
    transfer_uid: &str,
    transfer_revision: u64,
    acting_person: Option<&str>,
    can_admin_delivery: bool,
    visible: Option<&HashSet<String>>,
) -> Result<Value, ProteinError> {
    let pool = &store.pool;
    let local_organ = organs::local(pool).await?.ok_or(RowNotFound)?;
    let recipients =
        recipients::read(pool, transfer_uid, acting_person, can_admin_delivery).await?;
    let handoffs = settlement_handoffs(
        store,
        transfer_uid,
        &local_organ.uid,
        acting_person.filter(|_| can_admin_delivery),
        visible,
    )
    .await?;
    let designated_cell = executor::designated(pool, transfer_uid).await?;
    let this_cell = cells::local(pool).await?.map(|cell| cell.uid);
    Ok(json!({
        "authority": {
            "role": "origin",
            "origin_organ": local_organ.uid,
            "local_organ": local_organ.uid,
            "canonical_writes": "local",
        },
        "executor": {
            "designated_cell": designated_cell,
            "this_cell": this_cell,
        },
        "revision": transfer_revision,
        "recipients": recipients.configured,
        "eligible_recipients": recipients.eligible,
        "package_receipts": recipients.receipts,
        "application_handoffs": handoffs,
        "capabilities": {
            "configure_recipient": can_admin_delivery,
            "designate_executor": can_admin_delivery,
        },
        "blocking_reasons": { "configure_recipient": (!can_admin_delivery).then_some("delivery_admin_identity_required").as_slice() },
        "action_payloads": if can_admin_delivery { json!({
            "configure_recipient": {
                "action": "configure-transfer-delivery",
                "transfer": transfer_uid,
                "recipient_person": Value::Null,
                "recipient_organ": Value::Null,
                "person": acting_person,
                "request_id": Value::Null,
                "mode": "hosted",
            },
            "designate_executor": {
                "action": "designate-transfer-executor",
                "transfer_uid": transfer_uid,
                "cell_uid": Value::Null,
            },
        }) } else { json!({}) },
    }))
}

async fn settlement_handoffs(
    store: &Store,
    transfer: &str,
    local_organ: &str,
    authorized_person: Option<&str>,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let rows = query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            f64,
            Option<String>,
            String,
            Option<String>,
            String,
        ),
    >(
        "SELECT h.uid, h.participant_person_uid, h.participant_organ_uid,
                h.occurrence_uid, d.canonical_quantity, d.canonical_unit_uid,
                h.state, h.attestation_uid, h.created_at
         FROM transfer_application_handoff h
         JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid
         WHERE h.transfer_uid = ? ORDER BY h.created_at, h.uid",
    )
    .bind(transfer)
    .fetch_all(&store.pool)
    .await?;
    let mut handoffs = Vec::with_capacity(rows.len());
    for (uid, person, organ, occurrence, quantity, unit, state, attestation, at) in rows {
        handoffs.push(
            if organ == local_organ && authorized_person == Some(&person) {
                application::preview(store, &uid, authorized_person, visible).await?
            } else {
                json!({
                    "uid": uid,
                    "person": person,
                    "organ": organ,
                    "occurrence": occurrence,
                    "canonical_quantity": quantity,
                    "canonical_unit": unit,
                    "state": state,
                    "attestation": attestation,
                    "at": at,
                })
            },
        );
    }
    Ok(handoffs)
}
