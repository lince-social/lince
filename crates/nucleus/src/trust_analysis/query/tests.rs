mod eligibility;
mod projection;
mod weighting;

use std::collections::BTreeMap;

use super::*;
use crate::{
    DecimalValue,
    karma::{CanonicalHash, canonical_hash},
    new_uid,
    transfer::{
        outcome::Role,
        receipt_history::{self, Copy, Provenance},
    },
    trust_analysis::{Condition, Identity, IdentityContext, contacts::Audience, paths},
};
use crate::{transfer::receipt::*, vocabulary::reference::MeaningReference};

fn decimal(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).unwrap()
}

fn hash(value: &str) -> CanonicalHash {
    canonical_hash("lince.trust-query.test.v1", &value).unwrap()
}

fn publication() -> Publication {
    let scope = Scope {
        audience: Audience::Public,
        upfront_terms: None,
        public_persons: Vec::new(),
        source: Source {
            origin: new_uid("r"),
            occurrence: hash("occurrence"),
            activated_at: 900,
            agreement_revision: 1,
            agreement_commitment: hash("agreement"),
            activation_commitment: hash("activation"),
        },
        party_attribution: PartyAttribution::LaterOrganEndorsement,
        original_authorities: Vec::new(),
        giver: new_uid("r"),
        receiver: new_uid("r"),
        measurements: vec![Measurement {
            meaning: MeaningReference {
                vocabulary_uid: new_uid("g"),
                concept_uid: new_uid("c"),
                revision: 1,
                definition_hash: "a".repeat(64),
            },
            unit: None,
            object_commitment: None,
        }],
        stages: vec![
            Stage::Need,
            Stage::Giver,
            Stage::Receiver,
            Stage::Settled,
            Stage::Dispute,
        ],
        agreed_quantity: None,
        canonical_unit: None,
        issued_at: 1000,
        expires_at: 2000,
    };
    let approvals = [
        ApprovalRole::Origin,
        ApprovalRole::Giver,
        ApprovalRole::Receiver,
    ]
    .into_iter()
    .map(|role| Approval {
        role,
        issued_at: 1000,
        key_id: "test".into(),
        signature: "a".repeat(88),
    })
    .collect();
    let mut publication = Publication {
        scope,
        approvals,
        person_approvals: Vec::new(),
        events: Vec::new(),
    };
    let measurement = publication.scope.measurements[0].clone();
    let need = push(
        &mut publication,
        Content::Need {
            measurement: measurement.clone(),
            requested: decimal("10"),
            need_commitment: hash("need"),
        },
        None,
        "need",
    );
    push(
        &mut publication,
        Content::Statement {
            role: Role::Giver,
            measurement: measurement.clone(),
            quantity: decimal("100"),
            need: None,
            withdrawn: false,
        },
        None,
        "giver",
    );
    push(
        &mut publication,
        Content::Statement {
            role: Role::Receiver,
            measurement,
            quantity: decimal("4"),
            need: Some(need),
            withdrawn: false,
        },
        None,
        "receiver",
    );
    push(
        &mut publication,
        Content::Settled {
            quantity: RecordedFloat::new(4.0).unwrap(),
        },
        None,
        "settled",
    );
    publication
}

fn push(
    publication: &mut Publication,
    content: Content,
    supersedes: Option<CanonicalHash>,
    label: &str,
) -> CanonicalHash {
    let event = Event {
        scope: publication.scope.id().unwrap(),
        source_commitment: hash(label),
        recorded_at: 950,
        issued_at: 1010,
        supersedes,
        content,
    };
    let id = event.id().unwrap();
    publication.events.push(SignedEvent {
        event,
        key_id: "test".into(),
        signature: "a".repeat(88),
    });
    id
}

fn source(publication: &Publication) -> SourceEvidence {
    let projections = receipt_history::project(publication).unwrap();
    SourceEvidence {
        origin: publication.scope.source.origin.clone(),
        occurrence: publication.scope.source.occurrence.clone(),
        giver: publication.scope.giver.clone(),
        receiver: publication.scope.receiver.clone(),
        partial: false,
        copies: projections
            .into_iter()
            .map(|projection| {
                let event = &publication
                    .events
                    .iter()
                    .find(|signed| signed.event.id().unwrap() == projection.event)
                    .unwrap()
                    .event;
                Copy {
                    persons: publication.scope.public_persons.clone(),
                    provenance: Provenance::Record {
                        record: new_uid("r"),
                        version: 1,
                    },
                    scope: publication.scope.id().unwrap(),
                    attribution: PartyAttribution::OriginalOrganAuthority,
                    projection,
                    content: event.content.clone(),
                    recorded_at: event.recorded_at,
                    canonical_unit: None,
                    agreed_quantity: None,
                }
            })
            .collect(),
    }
}

fn contexts(publication: &Publication) -> BTreeMap<Identity, IdentityContext> {
    [
        &publication.scope.source.origin,
        &publication.scope.giver,
        &publication.scope.receiver,
    ]
    .into_iter()
    .map(|uid| {
        (
            Identity::Organ(uid.clone()),
            IdentityContext {
                readable: true,
                ..Default::default()
            },
        )
    })
    .collect()
}

fn query(publication: &Publication) -> Query {
    let mut query = Query::for_subject(Identity::Organ(publication.scope.giver.clone()));
    query.issuers.include.push(Condition {
        identities: vec![
            Identity::Organ(publication.scope.giver.clone()),
            Identity::Organ(publication.scope.receiver.clone()),
        ],
        ..Default::default()
    });
    query
}

#[test]
fn receiver_need_and_settlement_are_required_and_replicas_never_multiply_amounts() {
    let publication = publication();
    let context = contexts(&publication);
    let query = query(&publication);
    let analysis = analyze(
        vec![source(&publication), source(&publication)],
        &context,
        None,
        &query,
    )
    .unwrap();
    assert_eq!(analysis.groups.len(), 1);
    assert_eq!(analysis.groups[0].raw_quantity.unwrap().canonical(), "4");
    assert_eq!(analysis.groups[0].weighted_index.unwrap().canonical(), "4");
    assert_eq!(analysis.groups[0].occurrences, 1);
    assert_eq!(analysis.rows.len(), 2);
    assert_eq!(analysis.protocol.len(), 2);
    assert!(analysis.protocol.iter().any(
        |row| matches!(&row.content,Content::Need {requested,..} if requested.canonical()=="10")
    ));
    assert!(analysis.protocol.iter().any(
        |row| matches!(&row.content,Content::Settled {quantity} if quantity.value().unwrap()==4.0)
    ));
    let mut unsettled = publication.clone();
    unsettled
        .events
        .retain(|signed| !matches!(signed.event.content, Content::Settled { .. }));
    let result = analyze(vec![source(&unsettled)], &context, None, &query).unwrap();
    assert!(result.groups.is_empty());
    assert!(
        result
            .rows
            .iter()
            .any(|row| row.level == EvidenceLevel::Receiver)
    );
    let mut unbound = publication.clone();
    for signed in &mut unbound.events {
        if let Content::Statement { need, .. } = &mut signed.event.content {
            *need = None;
        }
    }
    assert!(
        analyze(vec![source(&unbound)], &context, None, &query)
            .unwrap()
            .groups
            .is_empty()
    );
}

#[test]
fn assembled_mixed_originals_keep_exact_amounts_and_both_provenance_types() {
    let publication = publication();
    let mut source = source(&publication);
    let record = new_uid("r");
    let original = hash("collected-original");
    for copy in &mut source.copies {
        copy.provenance = if matches!(copy.content, Content::Need { .. }) {
            Provenance::Record {
                record: record.clone(),
                version: 1,
            }
        } else {
            Provenance::Artifact {
                hash: original.clone(),
            }
        };
        if let Content::Statement {
            role: Role::Receiver,
            quantity,
            ..
        } = &mut copy.content
        {
            *quantity = decimal("12.000000000000000001");
        }
        if let Content::Settled { quantity } = &mut copy.content {
            *quantity = RecordedFloat::new(10.0).unwrap();
        }
    }
    source.copies.extend(source.copies.clone());
    let group = crate::trust_analysis::evidence::ReceiptGroup {
        origin: source.origin,
        occurrence: source.occurrence,
        giver: source.giver,
        receiver: source.receiver,
        history: receipt_history::merge(source.copies),
        missing: Vec::new(),
        pending: Vec::new(),
        partial: false,
    };
    let query = query(&publication);
    let analysis = analyze(
        vec![group.source_evidence()],
        &contexts(&publication),
        None,
        &query,
    )
    .unwrap();
    assert!(analysis.protocol.iter().any(|row| matches!(&row.content, Content::Need { requested, .. } if requested.canonical() == "10")));
    assert!(analysis.protocol.iter().any(|row| matches!(&row.content, Content::Settled { quantity } if quantity.value().unwrap() == 10.0)));
    let amounts = analysis
        .rows
        .iter()
        .map(|row| (row.level, row.quantity.canonical()))
        .collect::<Vec<_>>();
    assert!(amounts.contains(&(EvidenceLevel::Giver, "100".into())));
    assert!(amounts.contains(&(
        EvidenceLevel::NeedLinkedReceiver,
        "12.000000000000000001".into()
    )));
    assert_eq!(
        analysis.groups[0].raw_quantity.unwrap().canonical(),
        "12.000000000000000001"
    );
    assert_eq!(analysis.groups[0].occurrences, 1);
    let row = analysis.rows.iter().find(|row| row.selected).unwrap();
    assert_eq!(
        row.originals,
        vec![Provenance::Artifact {
            hash: original.clone()
        }]
    );
    assert!(row.records.is_empty());
    assert!(
        analysis
            .protocol
            .iter()
            .any(|row| row.originals.contains(&Provenance::Record {
                record: record.clone(),
                version: 1
            }))
    );
    let mut incomplete = group.source_evidence();
    incomplete.partial = true;
    let withheld = analyze(vec![incomplete], &contexts(&publication), None, &query).unwrap();
    assert!(withheld.groups.is_empty());
    assert!(
        withheld.withheld[0]
            .originals
            .contains(&Provenance::Artifact { hash: original })
    );
    assert!(
        withheld.withheld[0]
            .originals
            .contains(&Provenance::Record { record, version: 1 })
    );
}
