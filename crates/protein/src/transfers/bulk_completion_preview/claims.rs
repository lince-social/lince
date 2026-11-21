//! Claim history, eligibility, and review rows for a selected occurrence.

use crate::ProteinError;
use nucleus::transfer::{
    OccurrenceClaimRole::{Delivery, Receipt},
    TransferBulkClaimReviewItem,
};
use serde_json::{Value, json};
use store::{
    sqlx::SqlitePool,
    transfers::{
        TransferOccurrenceRow, occurrence_claim_events, occurrence_settlement_progress,
        source_group_state_for_transfer,
    },
};

#[derive(Default)]
pub(super) struct Review {
    pub(super) items: Vec<Value>,
    pub(super) eligible: Vec<TransferBulkClaimReviewItem>,
}

impl Review {
    pub(super) fn block(&mut self, uid: &str, reason: &str) {
        self.items.push(json!({
            "occurrence": uid,
            "role": null,
            "eligible": false,
            "blockers": [reason],
        }));
    }

    pub(super) async fn include(
        &mut self,
        pool: &SqlitePool,
        occurrence: &TransferOccurrenceRow,
        person: &str,
        revision: Option<u64>,
        in_tree: bool,
    ) -> Result<(), ProteinError> {
        let uid = &occurrence.uid;
        let transfer = &occurrence.transfer_uid;
        let outside = (!in_tree).then_some("occurrence_outside_selected_tree");
        let roles = [
            (
                Delivery,
                &occurrence.giver_person_uid,
                occurrence.delivery_claimed,
            ),
            (
                Receipt,
                &occurrence.receiver_person_uid,
                occurrence.receipt_claimed,
            ),
        ]
        .into_iter()
        .filter(|(_, participant, _)| *participant == person);
        if roles.clone().next().is_none() {
            self.items.push(json!({
                "occurrence": uid,
                "transfer": transfer,
                "transfer_revision": revision,
                "role": null,
                "eligible": false,
                "blockers": outside.into_iter().chain(["not_occurrence_participant"]).collect::<Vec<_>>(),
            }));
            return Ok(());
        }
        let missing = roles.clone().any(|(_, _, claimed)| !claimed);
        let events = occurrence_claim_events(pool, uid).await?;
        let settled = occurrence_settlement_progress(pool, uid)
            .await?
            .is_some_and(|progress| progress.settled);
        let satiated = source_group_state_for_transfer(pool, transfer)
            .await?
            .is_some_and(|state| state.satiated);
        let (mut delivery, mut receipt) = ((0usize, None), (0usize, None));
        for event in &events {
            let (version, token) = match event.role {
                Delivery => &mut delivery,
                Receipt => &mut receipt,
            };
            *version += 1;
            *token = Some(event.uid.as_str());
        }
        let activation = occurrence.activation_fact_uid.as_str();
        let occurrence_token = events.last().map_or(activation, |event| event.uid.as_str());
        let disputed = occurrence.disputed || occurrence.system_disputed;
        for (role, _, claimed) in roles
            .filter(|(_, _, claimed)| !missing || !claimed)
            .take(if missing { 2 } else { 1 })
        {
            let blockers: Vec<_> = [
                outside,
                revision.is_none().then_some("transfer_missing"),
                claimed.then_some("claim_already_asserted"),
                disputed.then_some("occurrence_disputed"),
                settled.then_some("occurrence_already_settled"),
                satiated.then_some("transfer_satiated"),
            ]
            .into_iter()
            .flatten()
            .collect();
            let action = revision.filter(|_| blockers.is_empty()).map(|revision| {
                self.eligible.push(TransferBulkClaimReviewItem {
                    occurrence_uid: uid.clone(),
                    transfer_uid: transfer.clone(),
                    transfer_revision: revision,
                    role,
                    delivery_claimed: occurrence.delivery_claimed,
                    receipt_claimed: occurrence.receipt_claimed,
                });
                json!({
                    "occurrence": uid,
                    "transfer": transfer,
                    "expected_revision": revision,
                    "role": role.as_str(),
                    "expected_delivery_claimed": occurrence.delivery_claimed,
                    "expected_receipt_claimed": occurrence.receipt_claimed,
                })
            });
            let (version, token) = match role {
                Delivery => delivery,
                Receipt => receipt,
            };
            self.items.push(json!({
                "occurrence": uid,
                "transfer": transfer,
                "transfer_revision": revision,
                "role": role.as_str(),
                "current_claimed": claimed,
                "claim_version": version,
                "claim_token": token.unwrap_or(activation),
                "occurrence_state_token": occurrence_token,
                "delivery_claim_token": delivery.1,
                "receipt_claim_token": receipt.1,
                "occurrence_state": {
                    "delivery_claimed": occurrence.delivery_claimed,
                    "receipt_claimed": occurrence.receipt_claimed,
                    "disputed": occurrence.disputed,
                },
                "eligible": action.is_some(),
                "blockers": blockers,
                "action": action,
            }));
        }
        Ok(())
    }
}
