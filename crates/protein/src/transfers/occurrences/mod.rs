//! Occurrence rows, capabilities, and settlement totals for a Transfer.

mod actions;
mod claims;
mod outcome;
mod settlement;
mod settlement_history;
mod totals;

use self::{
    actions::{Actions, Rules, participates},
    totals::Totals,
};
use super::{
    application,
    projection::{self, RecordIndex},
    status::correction_status,
    viewer::TransferViewer,
};
use crate::ProteinError;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use store::{
    Store,
    misc::PromiseRow,
    sqlx::Error::Protocol,
    transfer_cancellations,
    transfers::{self, OccurrenceSettlementProgress, TransferOccurrenceRow},
};

pub(super) struct Occurrences<'a> {
    pub uid: &'a str,
    pub revision: i64,
    pub viewer: &'a TransferViewer,
    pub visible: Option<&'a HashSet<String>>,
    pub installed_signer_actor: Option<&'a str>,
    pub creator_person: Option<&'a str>,
    pub create_person: Option<&'a str>,
    pub need_outcomes: &'a Value,
    pub promises: &'a [PromiseRow],
    pub occurrences: &'a [TransferOccurrenceRow],
    pub records_by_uid: &'a RecordIndex<'a>,
    pub settlement_progress_by_occurrence: &'a HashMap<&'a str, OccurrenceSettlementProgress>,
    pub availability_by_record: &'a HashMap<String, Value>,
    pub concept_names: &'a HashMap<String, String>,
    pub local_organ_uid: Option<&'a str>,
    pub parties: &'a [(String, String, i64)],
}

pub(super) struct OccurrenceProjection {
    rows: Vec<Value>,
    actions: [(&'static str, Option<&'static str>); 9],
    resource_progress: Vec<Value>,
    pub all_settled: bool,
    pub private_slices: usize,
    pub compensated_slices: usize,
    pub status_counts: BTreeMap<&'static str, usize>,
}

impl OccurrenceProjection {
    pub(super) fn attach(self, row: &mut Value) {
        for (action, blocker) in self.actions {
            row["capabilities"][action] = Value::Bool(blocker.is_none());
            row["blocking_reasons"][action] = blocker.as_slice().into();
        }
        row["occurrences"] = Value::Array(self.rows);
        row["settlement_progress"]["by_resource"] = Value::Array(self.resource_progress);
    }
}

pub(super) async fn project(
    store: &Store,
    input: &Occurrences<'_>,
) -> Result<OccurrenceProjection, ProteinError> {
    let &Occurrences {
        uid,
        revision,
        viewer,
        visible,
        installed_signer_actor,
        create_person,
        need_outcomes,
        promises,
        occurrences,
        records_by_uid,
        settlement_progress_by_occurrence,
        availability_by_record,
        concept_names,
        ..
    } = input;
    let mut rows = Vec::with_capacity(occurrences.len());
    let mut all_settled = !occurrences.is_empty();
    let actions = Actions::new(input);
    let can_cancel = actions.can_cancel;
    let no_options = Value::Array(Vec::new());
    let mut can_confirm_delivery = false;
    let mut can_confirm_receipt = false;
    let mut can_settle_occurrence = false;
    let mut can_dispute_occurrence = false;
    let mut can_retract_dispute = false;
    let mut can_compensate_settlement = false;
    let mut can_create_remainder_draft = false;
    let mut can_create_reversing_transfer = false;
    let mut private_settlement_slices = 0usize;
    let mut private_compensated_slices = 0usize;
    let mut totals = Totals::default();
    let promises_by_uid = if occurrences.is_empty() {
        HashMap::new()
    } else {
        projection::first_by_key(promises, |promise| promise.uid.as_str())
    };
    for occurrence in occurrences {
        let origin_promise = promises_by_uid
            .get(occurrence.promise_uid.as_str())
            .copied();
        let source_owner = origin_promise.and_then(|promise| promise.party_uid.as_deref());
        let origin_record = occurrence
            .record_uid
            .as_deref()
            .and_then(|record_uid| records_by_uid.get(record_uid).copied());
        let private_effects_visible = if let Some(owner) = source_owner {
            application::visible_policy(
                store,
                &occurrence.transfer_uid,
                occurrence
                    .exchange_uid
                    .as_deref()
                    .unwrap_or(&occurrence.promise_uid),
                owner,
                visible,
            )
            .await?
        } else {
            false
        };
        let viewer_owns_origin = (viewer.local || viewer.person.as_deref() == source_owner)
            && private_effects_visible
            && occurrence
                .record_uid
                .as_ref()
                .is_none_or(|record| visible.is_none_or(|visible| visible.contains(record)));
        let viewer_is_source_owner = actions.source_owner(source_owner);
        let claim_events = transfers::occurrence_claim_events(&store.pool, &occurrence.uid).await?;
        let dispute_events =
            transfers::occurrence_dispute_events(&store.pool, &occurrence.uid).await?;
        let settlement_compensations =
            transfers::occurrence_settlement_compensations(&store.pool, &occurrence.uid).await?;
        let (outcome_history, outcome_measures) =
            outcome::read(store, occurrence, visible, create_person).await?;
        let progress = settlement_progress_by_occurrence
            .get(occurrence.uid.as_str())
            .ok_or_else(|| {
                Protocol("Transfer occurrence settlement progress is unavailable".into())
            })?;
        all_settled &= progress.settled;
        let occurrence_status = totals.add(occurrence, progress);

        let effective_remainder_policy = match source_owner {
            Some(owner) => Some(
                transfers::effective_occurrence_remainder_policy(
                    &store.pool,
                    &occurrence.uid,
                    owner,
                )
                .await?,
            ),
            None => None,
        };
        let Rules {
            delivery: delivery_blockers,
            receipt: receipt_blockers,
            remainder: remainder_draft_blockers,
            reverse: reversing_transfer_blockers,
            dispute: dispute_blockers,
            retract_dispute: retract_dispute_blockers,
            participant_disputed,
        } = actions.for_occurrence(
            occurrence,
            progress,
            source_owner,
            effective_remainder_policy,
            &dispute_events,
        );
        let can_create_remainder = remainder_draft_blockers.is_empty();
        let can_create_reversing = reversing_transfer_blockers.is_empty();
        let can_dispute = dispute_blockers.is_empty();
        let can_retract = retract_dispute_blockers.is_empty();
        can_confirm_delivery |= delivery_blockers.is_empty();
        can_confirm_receipt |= receipt_blockers.is_empty();
        can_create_remainder_draft |= can_create_remainder;
        can_create_reversing_transfer |= can_create_reversing;
        can_dispute_occurrence |= can_dispute;
        can_retract_dispute |= can_retract;

        let owner_blockers = actions.settlement_blockers(source_owner);
        let disclosure = viewer_owns_origin.then(|| owner_blockers.clone());
        let settlement::Settlement {
            blockers: settlement_blockers,
            application,
            preview: settlement_preview,
        } = settlement::project(
            store,
            input,
            occurrence,
            progress,
            owner_blockers,
            &settlement::Source {
                origin_promise,
                origin_record,
                source_owner,
                viewer_is_source_owner,
                viewer_owns_origin,
            },
        )
        .await?;
        let can_settle = settlement_blockers.is_empty() && settlement_preview.is_some();
        can_settle_occurrence |= can_settle;

        let history = settlement_history::project(
            store,
            &progress.slices,
            &settlement_compensations,
            disclosure.as_deref(),
        )
        .await?;
        private_settlement_slices += history.private_slices;
        private_compensated_slices += history.compensated_slices;
        can_compensate_settlement |= history.can_compensate;
        let settlement_history = history.rows;
        let claims = claims::project(&claim_events);
        let is_outcome_recipient = create_person == Some(occurrence.receiver_person_uid.as_str());
        let mut projected_occurrence = json!({
            "uid": occurrence.uid,
            "outcomes": null,
            "outcome_measure_options": null,
            "need_outcome_options": if is_outcome_recipient { &need_outcomes["options"] } else { &no_options },
            "need_outcome_options_more": is_outcome_recipient && need_outcomes["more"] == true,
            "exchange_path": occurrence.exchange_path_uid,
            "exchange": occurrence.exchange_uid,
            "promise": occurrence.promise_uid,
            "opposite_promise": occurrence.opposite_promise_uid,
            "revision": occurrence.revision,
            "concept": occurrence.concept_uid,
            "concept_name": occurrence.concept_uid.as_deref()
                .and_then(|concept| concept_names.get(concept)),
            "unit": occurrence.unit_uid,
            "unit_name": occurrence.unit_uid.as_deref()
                .and_then(|unit| concept_names.get(unit)),
            "quantity": occurrence.quantity,
            "status": occurrence_status,
            "availability": if viewer_owns_origin {
                occurrence.record_uid.as_deref()
                    .and_then(|record_uid| availability_by_record.get(record_uid))
            } else {
                None
            },
            "reservation": origin_promise.map(|promise| json!({
                "reserve_from": promise.reserve_from,
                "source": "frozen_transfer_policy",
            })),
            "giver": occurrence.giver_person_uid,
            "receiver": occurrence.receiver_person_uid,
            "window_start": occurrence.window_start,
            "window_end": occurrence.window_end,
            "place": occurrence.location,
            "activation": {
                "event": occurrence.activation_event_uid,
                "fact": occurrence.activation_fact_uid,
                "at": occurrence.created_at,
            },
            "disputed": occurrence.disputed,
            "dispute": {
                "participant_disputed": participant_disputed,
                "history": dispute_events.iter().map(|event| json!({
                    "uid": event.uid,
                    "disputed": event.disputed,
                    "person": event.actor_person_uid,
                    "fact": event.fact_uid,
                    "request_id": event.idempotency_key,
                    "at": event.created_at,
                })).collect::<Vec<_>>(),
            },
            "system_dispute": {
                "disputed": occurrence.system_disputed,
                "fact": occurrence.system_dispute_fact_uid,
                "at": occurrence.system_disputed_at,
            },
            "delivery": {
                "claimed": occurrence.delivery_claimed,
                "history": null,
            },
            "delivery_claimed": occurrence.delivery_claimed,
            "receipt": {
                "claimed": occurrence.receipt_claimed,
                "history": null,
            },
            "receipt_claimed": occurrence.receipt_claimed,
            "confirmed_conclusion": occurrence.delivery_claimed && occurrence.receipt_claimed,
            "claim_history": null,
            "settlement_progress": {
                "canonical_quantity": progress.canonical_quantity,
                "settled_quantity": progress.settled_quantity,
                "cancelled_quantity": progress.cancelled_quantity,
                "cancelled_quantity_exact": progress.cancelled_exact.to_string(),
                "remaining_quantity_exact": progress.remaining_exact.to_string(),
                "remaining_quantity": progress.remaining_quantity,
                "partially_settled": progress.partially_settled,
                "settled": progress.settled,
                "slices": null,
            },
            "capabilities": {
                "report_outcome": occurrence.giver_person_uid != occurrence.receiver_person_uid
                    && actions.report_as.is_some_and(|person| participates(occurrence, person)),
                "confirm_delivery": !occurrence.delivery_claimed && delivery_blockers.is_empty(),
                "correct_delivery": occurrence.delivery_claimed && delivery_blockers.is_empty(),
                "confirm_receipt": !occurrence.receipt_claimed && receipt_blockers.is_empty(),
                "correct_receipt": occurrence.receipt_claimed && receipt_blockers.is_empty(),
                "set_application_formula": receipt_blockers.is_empty(),
                "settle": can_settle,
                "dispute": can_dispute,
                "retract_dispute": can_retract,
                "create_remainder_draft": can_create_remainder,
                "create_reversing_transfer": can_create_reversing,
            },
            "blocking_reasons": {
                "delivery": delivery_blockers,
                "receipt": receipt_blockers,
                "set_application_formula": receipt_blockers,
                "settle": settlement_blockers,
                "dispute": dispute_blockers,
                "retract_dispute": retract_dispute_blockers,
                "create_remainder_draft": remainder_draft_blockers,
                "create_reversing_transfer": reversing_transfer_blockers,
            },
            "action_payloads": {
                "create_remainder_draft": {
                    "action": "create-transfer-remainder-draft",
                    "occurrence": occurrence.uid,
                    "expected_revision": revision,
                    "expected_remaining_quantity": progress.remaining_quantity,
                    "request_id": Value::Null,
                    "person": actions.correction_actor,
                },
                "create_reversing_transfer": {
                    "action": "create-reversing-transfer-draft",
                    "occurrence": occurrence.uid,
                    "expected_revision": revision,
                    "canonical_quantity": occurrence.quantity,
                    "request_id": Value::Null,
                    "person": actions.correction_actor,
                },
            },
        });
        projected_occurrence["outcomes"] = outcome_history;
        projected_occurrence["outcome_measure_options"] = Value::Array(outcome_measures);
        projected_occurrence["claim_history"] = Value::Array(claims.all);
        projected_occurrence["delivery"]["history"] = Value::Array(claims.delivery);
        projected_occurrence["receipt"]["history"] = Value::Array(claims.receipt);
        projected_occurrence["settlement_progress"]["slices"] = Value::Array(settlement_history);
        if viewer_is_source_owner
            && occurrence.record_uid.is_none()
            && installed_signer_actor == source_owner
            && let Some(owner) = source_owner
        {
            projected_occurrence["application_preparation"] =
                application::prepare(store, uid, &occurrence.uid, revision as u64, owner).await?;
        }
        if viewer_owns_origin && let Some(object) = projected_occurrence.as_object_mut() {
            object.insert("record".into(), json!(occurrence.record_uid));
            object.insert(
                "record_head".into(),
                json!(origin_record.map(|record| record.head.as_str())),
            );
            object.insert(
                "record_slug".into(),
                json!(origin_record.and_then(|record| record.slug.as_deref())),
            );
            if let Some(application) = application {
                object.insert("application".into(), application.into_row());
            }
            if let Some(preview) = settlement_preview {
                object.insert("settlement_preview".into(), preview);
            }
            object.insert(
                "correction_status".into(),
                correction_status(
                    occurrence.system_disputed,
                    participant_disputed,
                    progress.slices.len(),
                    settlement_compensations.len(),
                )
                .into(),
            );
            object.insert(
                "private_compensated_slices".into(),
                json!(settlement_compensations.len()),
            );
        }
        let cancellation_ready = can_cancel
            && progress.remaining_exact.is_positive()
            && !occurrence.disputed
            && !occurrence.system_disputed;
        projected_occurrence["capabilities"]["propose_cancellation"] = json!(cancellation_ready);
        projected_occurrence["action_payloads"]["propose_cancellation"] = json!({
            "action":"propose-transfer-cancellation","transfer":uid,"occurrence":occurrence.uid,
            "expected_revision":revision,"expected_remaining_quantity":progress.remaining_exact,
            "person":actions.acting,"request_id":null
        });
        let mut cancellations =
            transfer_cancellations::for_occurrence(&store.pool, uid, &occurrence.uid).await?;
        for cancellation in &mut cancellations {
            cancellation["capabilities"] = json!({"apply_cancellation":can_cancel && cancellation["status"] == "pending" && cancellation["all_agreed"] == true});
            cancellation["action_payloads"] = json!({"apply_cancellation":{
                "action":"apply-transfer-cancellation","transfer":uid,"cancellation":cancellation["uid"],
                "expected_revision":cancellation["revision"],"person":actions.acting,"request_id":null
            }});
        }
        projected_occurrence["cancellations"] = Value::Array(cancellations);
        rows.push(projected_occurrence);
    }
    let settle_blocker = if occurrences.is_empty() {
        "transfer_has_no_occurrences"
    } else if all_settled {
        "all_occurrences_settled"
    } else {
        "no_settleable_occurrence"
    };
    let compensate_blocker = if private_settlement_slices > 0
        && private_settlement_slices == private_compensated_slices
    {
        "all_private_settlements_compensated"
    } else {
        "no_compensatable_private_settlement"
    };
    let actions = [
        (
            "confirm",
            (!(can_confirm_delivery || can_confirm_receipt))
                .then_some("no_authorized_occurrence_claim"),
        ),
        (
            "confirm_delivery",
            (!can_confirm_delivery).then_some("no_authorized_delivery_claim"),
        ),
        (
            "confirm_receipt",
            (!can_confirm_receipt).then_some("no_authorized_receipt_claim"),
        ),
        ("settle", (!can_settle_occurrence).then_some(settle_blocker)),
        (
            "dispute",
            (!can_dispute_occurrence).then_some("no_authorized_occurrence_dispute"),
        ),
        (
            "retract_dispute",
            (!can_retract_dispute).then_some("no_authorized_dispute_retraction"),
        ),
        (
            "compensate_settlement",
            (!can_compensate_settlement).then_some(compensate_blocker),
        ),
        (
            "create_remainder_draft",
            (!can_create_remainder_draft).then_some("no_remainder_draft_available"),
        ),
        (
            "create_reversing_transfer",
            (!can_create_reversing_transfer).then_some("no_reversing_transfer_available"),
        ),
    ];
    Ok(OccurrenceProjection {
        rows,
        actions,
        resource_progress: totals.by_resource(concept_names),
        all_settled,
        private_slices: private_settlement_slices,
        compensated_slices: private_compensated_slices,
        status_counts: totals.statuses,
    })
}
