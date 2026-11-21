//! Promise rows, their agreement status, and controls available to the viewer.

use super::{
    projection::{RecordIndex, attach_action_access, first_by_key},
    viewer::TransferViewer,
};
use chrono::{DateTime, Utc};
use nucleus::transfer::TransferRevisionPromise;
use serde_json::{Value, json};
use std::collections::HashMap;
use store::{
    misc::PromiseRow,
    transfer_agreement::AgreementReadinessProjection,
    transfers::{
        OccurrenceSettlementProgress, OpenPromiseClaimPairRow, PromiseSuccessorRow,
        TransferOccurrenceRow, TransferRow,
    },
};

pub(super) struct Promises<'a> {
    pub transfer: &'a TransferRow,
    pub promises: &'a [PromiseRow],
    pub revision_promises: &'a [TransferRevisionPromise],
    pub occurrences: &'a [TransferOccurrenceRow],
    pub successor_links: &'a [PromiseSuccessorRow],
    pub readiness: &'a AgreementReadinessProjection,
    pub occurrence_roles: &'a HashMap<&'a str, Result<(), &'static str>>,
    pub open_claim_pairs: &'a HashMap<&'a str, Vec<OpenPromiseClaimPairRow>>,
    pub settlement_progress: &'a HashMap<&'a str, OccurrenceSettlementProgress>,
    pub availability: &'a HashMap<String, Value>,
    pub records: &'a RecordIndex<'a>,
    pub concept_names: &'a HashMap<String, String>,
    pub viewer: &'a TransferViewer,
    pub signer: Option<&'a str>,
    pub signer_is_party: bool,
    pub claim_person: Option<&'a str>,
    pub claim_access_blocker: Option<&'static str>,
    pub show_lineage: bool,
}

pub(super) fn project(input: &Promises<'_>) -> Vec<Value> {
    let promises = input.promises;
    if promises.is_empty() {
        return Vec::new();
    }
    let signed_by_uid = first_by_key(input.revision_promises, |promise| promise.uid.as_str());
    let occurrences = first_by_key(input.occurrences, |occurrence| {
        occurrence.promise_uid.as_str()
    });
    let (mut successors, mut predecessors) = (HashMap::new(), HashMap::new());
    for link in input.successor_links {
        successors.insert(link.predecessor_promise_uid.as_str(), link);
        predecessors.insert(link.successor_promise_uid.as_str(), link);
    }
    let viewer = input.viewer;
    let installed_signer_actor = input.signer;
    let claim_person = input.claim_person;
    let identity_blockers = viewer.update_identity_blockers();
    promises
        .iter()
        .map(|row| {
            let signed = signed_by_uid.get(row.uid.as_str()).copied();
            let record = row
                .record_uid
                .as_deref()
                .and_then(|uid| input.records.get(uid));
            let concept = signed
                .and_then(|promise| promise.concept_uid.as_deref())
                .or_else(|| record.and_then(|record| record.identity_predicate_uid.as_deref()));
            let unit = signed.and_then(|promise| promise.unit_uid.as_deref());
            let signed_delta = signed.map_or(row.delta, |promise| promise.delta);
            let person = signed
                .and_then(|promise| promise.person_uid.as_deref())
                .or(row.party_uid.as_deref());
            let state = row.state.as_str();
            let is_open = state == "open";
            let proposer = is_open.then_some(person).flatten();
            let mut claim_blockers = identity_blockers.clone();
            let agreement = input.readiness.promises.get(&row.uid);
            if !is_open {
                claim_blockers.push("promise_not_open");
            }
            claim_blockers.extend(input.claim_access_blocker);
            if claim_person.is_none() || installed_signer_actor != claim_person {
                claim_blockers.push("missing_person_signer");
            }
            if proposer.is_none() {
                claim_blockers.push("open_proposer_missing");
            } else if claim_person == proposer {
                claim_blockers.push("open_proposer_cannot_claim_own_promise");
            }
            let mut activate_blockers = identity_blockers.clone();
            if !viewer.local && viewer.person.as_deref() != person {
                activate_blockers.push("cannot_activate_another_party_promise");
            }
            if person.is_none() || person != installed_signer_actor {
                activate_blockers.push("missing_person_signer");
            }
            if !agreement.is_some_and(|value| value.ready) {
                activate_blockers.push("promise_agreement_not_ready");
            }
            if state != "agreed"
                && !(input.transfer.agreement_type == "dependency" && state == "proposed")
            {
                activate_blockers.push("promise_not_agreed");
            }
            if is_open {
                activate_blockers.push("open_promise_template");
            }
            let occurrence = occurrences.get(row.uid.as_str());
            if occurrence.is_some() {
                activate_blockers.push("promise_already_activated");
            }
            if let Some(Err(code)) = input.occurrence_roles.get(row.uid.as_str()) {
                activate_blockers.push(code);
            }
            let projected_window_end = signed
                .and_then(|promise| promise.window_end.as_deref())
                .or(row.window_end.as_deref());
            let expired = matches!(state, "proposed" | "agreed")
                && projected_window_end.is_some_and(|window_end| {
                    DateTime::parse_from_rfc3339(window_end)
                        .is_ok_and(|window_end| window_end <= Utc::now())
                });
            let remaining_quantity = occurrence
                .and_then(|occurrence| input.settlement_progress.get(occurrence.uid.as_str()))
                .map_or_else(
                    || signed_delta.abs(),
                    |progress| progress.remaining_quantity,
                );
            let successor_lineage = successors.get(row.uid.as_str()).copied();
            let predecessor_lineage = predecessors.get(row.uid.as_str()).copied();
            let mut reopen_blockers = identity_blockers.clone();
            if person != installed_signer_actor || !input.signer_is_party {
                reopen_blockers.push("not_promise_owner_with_signer");
            }
            if !matches!(state, "broken" | "withdrawn") && !expired {
                reopen_blockers.push("promise_not_reopenable");
            }
            if !remaining_quantity.is_finite() || remaining_quantity <= 0.0 {
                reopen_blockers.push("promise_fully_settled");
            }
            if successor_lineage.is_some() {
                reopen_blockers.push("promise_already_has_successor");
            }
            let proposer_record = proposer.and_then(|uid| input.records.get(uid));
            let show_private_record = viewer.local || viewer.person.as_deref() == person;
            let mut value = json!({
                "uid": row.uid,
                "item": signed.and_then(|promise| promise.item.as_ref()),
                "record": row.record_uid,
                "record_head": record.map(|record| record.head.as_str()),
                "record_slug": record.and_then(|record| record.slug.as_deref()),
                "record_quantity": if show_private_record {
                    record.map(|record| record.quantity)
                } else {
                    None
                },
                "availability": if show_private_record {
                    row.record_uid.as_deref()
                        .and_then(|uid| input.availability.get(uid))
                } else {
                    None
                },
                "concept": concept,
                "concept_name": concept.and_then(|uid| input.concept_names.get(uid)),
                "unit": unit,
                "unit_name": unit.and_then(|uid| input.concept_names.get(uid)),
                "delta": signed_delta,
                "direction": if signed_delta < 0.0 {
                    "give"
                } else if signed_delta > 0.0 {
                    "receive"
                } else {
                    "invalid"
                },
                "proposer_delta": is_open.then_some(signed_delta),
                "state": state,
                "party": person,
                "open": is_open,
                "proposer": proposer,
                "proposer_head": proposer_record.map(|record| record.head.as_str()),
                "proposer_slug": proposer_record.and_then(|record| record.slug.as_deref()),
                "withdrawn": state == "withdrawn",
                "window_start": signed.and_then(|promise| promise.window_start.as_deref()),
                "window_end": projected_window_end,
                "place": signed.and_then(|promise| promise.location.as_ref()),
                "reuse_policy": signed
                    .map(|promise| promise.open_reuse_policy.as_str())
                    .unwrap_or_else(|| row.open_reuse_policy.as_str()),
                "source_promise": signed
                    .and_then(|promise| promise.source_promise_uid.as_deref()),
                "predecessor": predecessor_lineage.filter(|_| input.show_lineage)
                    .map(|lineage| lineage_row(lineage, &lineage.predecessor_promise_uid)),
                "successor": successor_lineage.filter(|_| input.show_lineage)
                    .map(|lineage| lineage_row(lineage, &lineage.successor_promise_uid)),
                "claim_pairs": input.open_claim_pairs
                    .get(row.uid.as_str())
                    .into_iter()
                    .flatten()
                    .map(|pair| {
                        let proposer_person = input.records.get(pair.proposer_person_uid.as_str());
                        let claimant_person = input.records.get(pair.claimant_person_uid.as_str());
                        json!({
                            "uid": pair.uid,
                            "source": pair.source_promise_uid,
                            "proposer_promise": pair.proposer_promise_uid,
                            "claimant_promise": pair.claimant_promise_uid,
                            "proposer": pair.proposer_person_uid,
                            "proposer_head": proposer_person
                                .map(|person| person.head.as_str()),
                            "proposer_slug": proposer_person
                                .and_then(|person| person.slug.as_deref()),
                            "claimant": pair.claimant_person_uid,
                            "claimant_head": claimant_person
                                .map(|person| person.head.as_str()),
                            "claimant_slug": claimant_person
                                .and_then(|person| person.slug.as_deref()),
                            "revision": pair.revision,
                            "reuse_policy": pair.reuse_policy.as_str(),
                            "request_id": pair.idempotency_key,
                            "at": pair.created_at,
                        })
                    })
                    .collect::<Vec<_>>(),
                "condition": row.condition,
                "reserve_from": row.reserve_from,
                "agreement_ready": agreement.is_some_and(|value| value.ready),
                "agreement_eligible": agreement.is_some_and(|value| value.eligible),
                "agreement_blockers": agreement
                    .map(|value| value.blockers.as_slice())
                    .unwrap_or_default(),
                "capabilities": null,
                "blocking_reasons": null,
                "action_payloads": {
                    "reopen": {
                        "action": "reopen-transfer-promise",
                        "transfer": input.transfer.record_uid,
                        "promise": row.uid,
                        "expected_revision": input.transfer.revision,
                        "request_id": Value::Null,
                        "person": person,
                        "window_end": Value::Null,
                        "open": false,
                    },
                },
            });
            attach_action_access(
                &mut value,
                &[
                    ("claim", &claim_blockers),
                    ("activate", &activate_blockers),
                    ("reopen", &reopen_blockers),
                ],
            );
            value
        })
        .collect::<Vec<_>>()
}

fn lineage_row(lineage: &PromiseSuccessorRow, related_promise: &str) -> Value {
    json!({
        "lineage": lineage.uid,
        "promise": related_promise,
        "revision": lineage.revision,
        "actor": lineage.actor_person_uid,
        "fact": lineage.fact_uid,
        "request_id": lineage.idempotency_key,
        "at": lineage.created_at,
    })
}
