use std::collections::HashSet;

use nucleus::performance::Span;
use serde_json::{Value, from_str, json};
use store::{
    Store,
    facts::for_record,
    transfers::{
        AgreementTransitionEventRow, PromiseSuccessorRow, TransferCorrectionLinkRow,
        TransferInvitationRow, TransferOccurrenceRow, TransferSourceGroupState, invitation_events,
        occurrence_claim_events, occurrence_dispute_events, occurrence_settlement_compensations,
        occurrence_settlement_slices, revision_fact,
    },
};

use super::{proof::transfer_fact_proof, revision::parse_transfer_revision_evidence};
use crate::ProteinError;

struct TransferTimelineSeed {
    uid: String,
    kind: String,
    at: String,
    fact_uid: String,
    detail: Value,
}

pub(in crate::transfers) struct Timeline<'a> {
    pub transfer_uid: &'a str,
    pub revision: u64,
    pub invitations: &'a [TransferInvitationRow],
    pub agreement_events: &'a [AgreementTransitionEventRow],
    pub occurrences: &'a [TransferOccurrenceRow],
    pub source_group: Option<&'a TransferSourceGroupState>,
    pub correction_lineage: &'a [TransferCorrectionLinkRow],
    pub promise_successors: &'a [PromiseSuccessorRow],
    pub recipient_details_authorized: bool,
    pub viewer_person: Option<&'a str>,
}

pub(in crate::transfers) async fn transfer_timeline(
    store: &Store,
    timeline: &Timeline<'_>,
) -> Result<Vec<Value>, ProteinError> {
    let &Timeline {
        transfer_uid,
        revision,
        invitations,
        agreement_events,
        occurrences,
        source_group,
        correction_lineage,
        promise_successors,
        recipient_details_authorized,
        viewer_person,
    } = timeline;
    let _timeline = Span::new("protein-transfer-timeline-await");
    let mut seeds = Vec::new();
    for number in 1..=revision {
        let Some(fact) = revision_fact(&store.pool, transfer_uid, number).await? else {
            continue;
        };
        let evidence = parse_transfer_revision_evidence(&fact);
        seeds.push(TransferTimelineSeed {
            uid: format!("revision:{number}"),
            kind: "revision".into(),
            at: fact.at.to_rfc3339(),
            fact_uid: fact.uid,
            detail: json!({
                "revision": number,
                "action": evidence.as_ref().map(|value| value.action.as_str()),
                "previous_revision": evidence.as_ref().and_then(|value| value.previous_revision),
            }),
        });
    }
    for invitation in invitations {
        for event in invitation_events(&store.pool, &invitation.uid).await? {
            seeds.push(TransferTimelineSeed {
                uid: event.uid,
                kind: "invitation".into(),
                at: event.created_at,
                fact_uid: event.fact_uid,
                detail: json!({
                    "invitation": event.invitation_uid,
                    "event": event.kind,
                    "attempt": event.attempt,
                    "revision": event.revision,
                    "person": (recipient_details_authorized
                        || viewer_person == Some(invitation.addressed_person_uid.as_str()))
                        .then_some(invitation.addressed_person_uid.as_str()),
                    "request_id": event.idempotency_key,
                }),
            });
        }
    }
    for event in agreement_events {
        seeds.push(TransferTimelineSeed {
            uid: event.uid.clone(),
            kind: "agreement".into(),
            at: event.created_at.clone(),
            fact_uid: event.fact_uid.clone(),
            detail: json!({
                "revision": event.revision,
                "party": recipient_details_authorized
                    .then_some(event.party_uid.as_str()),
                "person": (recipient_details_authorized
                    || viewer_person == Some(event.person_uid.as_str()))
                    .then_some(event.person_uid.as_str()),
                "from_level": event.from_level,
                "to_level": event.to_level,
                "request_id": event.idempotency_key,
            }),
        });
    }
    let mut activation_facts = HashSet::new();
    for occurrence in occurrences {
        if activation_facts.insert(occurrence.activation_fact_uid.as_str()) {
            seeds.push(TransferTimelineSeed {
                uid: occurrence.activation_event_uid.clone(),
                kind: "activation".into(),
                at: occurrence.created_at.clone(),
                fact_uid: occurrence.activation_fact_uid.clone(),
                detail: json!({ "revision": occurrence.revision }),
            });
        }
        for event in occurrence_claim_events(&store.pool, &occurrence.uid).await? {
            seeds.push(TransferTimelineSeed {
                uid: event.uid,
                kind: "claim".into(),
                at: event.created_at,
                fact_uid: event.fact_uid,
                detail: json!({
                    "occurrence": event.occurrence_uid,
                    "role": event.role.as_str(),
                    "asserted": event.asserted,
                    "person": event.actor_person_uid,
                    "request_id": event.idempotency_key,
                }),
            });
        }
        for slice in occurrence_settlement_slices(&store.pool, &occurrence.uid).await? {
            seeds.push(TransferTimelineSeed {
                uid: slice.uid,
                kind: "settlement".into(),
                at: slice.created_at,
                fact_uid: slice.evidence_fact_uid,
                detail: json!({
                    "occurrence": slice.occurrence_uid,
                    "promise": slice.promise_uid,
                    "person": slice.owner_person_uid,
                    "canonical_quantity": slice.canonical_quantity,
                    "canonical_unit": slice.canonical_unit_uid,
                    "remaining_after": slice.remaining_after,
                    "request_id": slice.idempotency_key,
                }),
            });
        }
        for event in occurrence_dispute_events(&store.pool, &occurrence.uid).await? {
            seeds.push(TransferTimelineSeed {
                uid: event.uid,
                kind: "dispute".into(),
                at: event.created_at,
                fact_uid: event.fact_uid,
                detail: json!({
                    "occurrence": event.occurrence_uid,
                    "person": event.actor_person_uid,
                    "asserted": event.disputed,
                    "request_id": event.idempotency_key,
                }),
            });
        }
        for correction in occurrence_settlement_compensations(&store.pool, &occurrence.uid).await? {
            seeds.push(TransferTimelineSeed {
                uid: correction.uid,
                kind: "settlement_compensation".into(),
                at: correction.created_at,
                fact_uid: correction.compensation_fact_uid,
                detail: json!({
                    "occurrence": correction.occurrence_uid,
                    "settlement": correction.settlement_uid,
                    "person": correction.owner_person_uid,
                    "inverse_delta": correction.inverse_delta,
                    "request_id": correction.idempotency_key,
                }),
            });
        }
    }
    if let Some(group) = source_group {
        seeds.push(TransferTimelineSeed {
            uid: group.result.uid.clone(),
            kind: "first_completes_result".into(),
            at: group.result.created_at.clone(),
            fact_uid: group.result.fact_uid.clone(),
            detail: json!({
                "source": group.result.source_uid,
                "winner": group.result.winner_transfer_uid,
                "winner_revision": group.result.winner_revision,
                "settlement": group.result.settlement_uid,
            }),
        });
        if let Some(loser) = &group.loser {
            seeds.push(TransferTimelineSeed {
                uid: loser.uid.clone(),
                kind: "first_completes_loser".into(),
                at: loser.created_at.clone(),
                fact_uid: loser.fact_uid.clone(),
                detail: json!({
                    "winner": group.result.winner_transfer_uid,
                    "loser": loser.transfer_uid,
                    "loser_revision": loser.transfer_revision,
                }),
            });
        }
    }
    if recipient_details_authorized {
        for correction in correction_lineage {
            seeds.push(TransferTimelineSeed {
                uid: format!("correction:{}:{transfer_uid}", correction.uid),
                kind: format!("correction_{}", correction.kind),
                at: correction.created_at.clone(),
                fact_uid: correction.fact_uid.clone(),
                detail: json!({
                    "role": if correction.source_transfer_uid == transfer_uid {
                        "source"
                    } else {
                        "created"
                    },
                    "source_transfer": correction.source_transfer_uid,
                    "source_occurrence": correction.source_occurrence_uid,
                    "created_transfer": correction.created_transfer_uid,
                    "source_revision": correction.source_revision,
                    "canonical_quantity": correction.canonical_quantity,
                    "actor": correction.actor_person_uid,
                    "request_id": correction.idempotency_key,
                }),
            });
        }
        for successor in promise_successors {
            seeds.push(TransferTimelineSeed {
                uid: format!("promise-successor:{}", successor.uid),
                kind: "promise_successor".into(),
                at: successor.created_at.clone(),
                fact_uid: successor.fact_uid.clone(),
                detail: json!({
                    "predecessor": successor.predecessor_promise_uid,
                    "successor": successor.successor_promise_uid,
                    "revision": successor.revision,
                    "actor": successor.actor_person_uid,
                    "request_id": successor.idempotency_key,
                }),
            });
        }
    }
    for fact in for_record(&store.pool, transfer_uid, 10_000).await? {
        let Some(payload) = fact
            .payload
            .as_deref()
            .and_then(|payload| from_str::<Value>(payload).ok())
        else {
            continue;
        };
        let Some(confirmation) = payload.get("confirmation").and_then(Value::as_str) else {
            continue;
        };
        seeds.push(TransferTimelineSeed {
            uid: format!("confirmation:{}", fact.uid),
            kind: "legacy_confirmation".into(),
            at: fact.at.to_rfc3339(),
            fact_uid: fact.uid,
            detail: json!({ "confirmation": confirmation }),
        });
    }
    seeds.sort_by(|left, right| {
        left.at
            .cmp(&right.at)
            .then_with(|| left.uid.cmp(&right.uid))
    });
    let mut timeline = Vec::with_capacity(seeds.len());
    for seed in seeds {
        let proof = transfer_fact_proof(store, &seed.fact_uid).await?;
        timeline.push(Value::from_iter([
            ("uid", seed.uid.into()),
            ("kind", seed.kind.into()),
            ("at", seed.at.into()),
            ("fact", seed.fact_uid.into()),
            ("detail", seed.detail),
            ("proof", proof),
        ]));
    }
    Ok(timeline)
}
