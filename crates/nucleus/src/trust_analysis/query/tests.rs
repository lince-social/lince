use super::*;
use crate::{transfer::receipt::*, vocabulary::reference::MeaningReference};

fn hash(value: &str) -> CanonicalHash {
    canonical_hash("lince.trust-query.test.v1", &value).unwrap()
}

fn publication() -> Publication {
    let scope = Scope {
        audience: crate::trust_analysis::contacts::Audience::Public,
        upfront_terms: None,
        public_persons: Vec::new(),
        source: Source {
            origin: crate::new_uid("r"),
            occurrence: hash("occurrence"),
            activated_at: 900,
            agreement_revision: 1,
            agreement_commitment: hash("agreement"),
            activation_commitment: hash("activation"),
        },
        party_attribution: PartyAttribution::LaterOrganEndorsement,
        original_authorities: Vec::new(),
        giver: crate::new_uid("r"),
        receiver: crate::new_uid("r"),
        measurements: vec![Measurement {
            meaning: MeaningReference {
                vocabulary_uid: crate::new_uid("g"),
                concept_uid: crate::new_uid("c"),
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
                    provenance: crate::transfer::receipt_history::Provenance::Record {
                        record: crate::new_uid("r"),
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
    query.issuers.include.push(super::super::Condition {
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
    let record = crate::new_uid("r");
    let original = hash("collected-original");
    for copy in &mut source.copies {
        copy.provenance = if matches!(copy.content, Content::Need { .. }) {
            Provenance::Record { record: record.clone(), version: 1 }
        } else {
            Provenance::Artifact { hash: original.clone() }
        };
        if let Content::Statement { role: Role::Receiver, quantity, .. } = &mut copy.content {
            *quantity = decimal("12.000000000000000001");
        }
        if let Content::Settled { quantity } = &mut copy.content {
            *quantity = RecordedFloat::new(10.0).unwrap();
        }
    }
    source.copies.extend(source.copies.clone());
    let group = super::super::evidence::ReceiptGroup {
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
    let analysis = analyze(vec![group.source_evidence()], &contexts(&publication), None, &query).unwrap();
    assert!(analysis.protocol.iter().any(|row| matches!(&row.content, Content::Need { requested, .. } if requested.canonical() == "10")));
    assert!(analysis.protocol.iter().any(|row| matches!(&row.content, Content::Settled { quantity } if quantity.value().unwrap() == 10.0)));
    let amounts = analysis.rows.iter().map(|row| (row.level, row.quantity.canonical())).collect::<Vec<_>>();
    assert!(amounts.contains(&(EvidenceLevel::Giver, "100".into())));
    assert!(amounts.contains(&(EvidenceLevel::NeedLinkedReceiver, "12.000000000000000001".into())));
    assert_eq!(analysis.groups[0].raw_quantity.unwrap().canonical(), "12.000000000000000001");
    assert_eq!(analysis.groups[0].occurrences, 1);
    let row = analysis.rows.iter().find(|row| row.selected).unwrap();
    assert_eq!(row.originals, vec![Provenance::Artifact { hash: original.clone() }]);
    assert!(row.records.is_empty());
    assert!(analysis.protocol.iter().any(|row| row.originals.contains(&Provenance::Record { record: record.clone(), version: 1 })));
    let mut incomplete = group.source_evidence();
    incomplete.partial = true;
    let withheld = analyze(vec![incomplete], &contexts(&publication), None, &query).unwrap();
    assert!(withheld.groups.is_empty());
    assert!(withheld.withheld[0].originals.contains(&Provenance::Artifact { hash: original }));
    assert!(withheld.withheld[0].originals.contains(&Provenance::Record { record, version: 1 }));
}

#[test]
fn mixed_source_policy_keeps_endpoint_eligibility_raw_amounts_and_distance_indices_separate() {
    use super::super::{Condition, contacts::{Edge, RelationKind}};
    let first = publication();
    let mut second = first.clone();
    second.scope.source.occurrence = hash("second-occurrence");
    second.scope.receiver = crate::new_uid("r");
    second.events.clear();
    let mut dependencies = BTreeMap::<CanonicalHash, CanonicalHash>::new();
    for (index, signed) in first.events.iter().enumerate() {
        let mut content = signed.event.content.clone();
        if let Content::Statement { need: Some(need), .. } = &mut content {
            *need = dependencies.get(need).unwrap().clone();
        }
        let next = push(&mut second, content, None, &format!("second-{index}"));
        dependencies.insert(signed.event.id().unwrap(), next);
    }
    let anchor = Identity::Person(crate::new_uid("r"));
    let direct = Identity::Organ(first.scope.receiver.clone());
    let middle = Identity::Organ(crate::new_uid("r"));
    let distant = Identity::Organ(second.scope.receiver.clone());
    let mut context = contexts(&first);
    context.extend(contexts(&second));
    for identity in [&anchor, &middle] {
        context.insert(identity.clone(), IdentityContext { readable:true, ..Default::default() });
    }
    let edges = [(&anchor, &direct), (&direct, &middle), (&middle, &distant)].into_iter().enumerate().map(|(index, (from,to))| Edge { from:from.clone(), to:to.clone(), kind:RelationKind::Mutual, proofs:vec![hash(&format!("edge-{index}"))] }).collect::<Vec<_>>();
    let policy = paths::Policy::for_anchor(anchor);
    let paths = paths::search(&edges, &context, &policy).unwrap();
    let mut query = query(&first);
    query.issuers.include[0].identities.push(distant.clone());
    query.contacts = Some(policy.clone());
    let sources = || {
        let mut remote = source(&second);
        for copy in &mut remote.copies { copy.provenance = Provenance::Artifact { hash:hash("remote-second-occurrence") }; }
        vec![source(&first), remote]
    };
    let initial = analyze(sources(), &context, Some(&paths), &query).unwrap();
    assert_eq!(initial.groups[0].raw_quantity.unwrap().canonical(), "8");
    assert!(initial.groups[0].weighted_index.unwrap().exact_numeric_cmp(decimal("5")).is_eq());
    let exclusion = Condition { identities:vec![distant.clone()], ..Default::default() };
    query.endpoints.exclude.push(exclusion.clone());
    assert_eq!(analyze(sources(), &context, Some(&paths), &query).unwrap().groups[0].raw_quantity.unwrap().canonical(), "8");
    query.counterparties.exclude.push(exclusion.clone());
    assert_eq!(analyze(sources(), &context, Some(&paths), &query).unwrap().groups[0].raw_quantity.unwrap().canonical(), "4");
    query.counterparties.exclude.clear();
    query.issuers.exclude.push(exclusion);
    assert_eq!(analyze(sources(), &context, Some(&paths), &query).unwrap().groups[0].raw_quantity.unwrap().canonical(), "4");
    query.issuers.exclude.clear();
    let mut excluded_path = policy;
    excluded_path.intermediaries.exclude.push(Condition { identities:vec![middle], ..Default::default() });
    let filtered = paths::search(&edges, &context, &excluded_path).unwrap();
    assert_eq!(analyze(sources(), &context, Some(&filtered), &query).unwrap().groups[0].raw_quantity.unwrap().canonical(), "4");
    query.issuer_cap = Some(decimal("0.5"));
    let capped = analyze(sources(), &context, Some(&paths), &query).unwrap();
    assert_eq!(capped.groups[0].raw_quantity.unwrap().canonical(), "8");
    assert!(capped.groups[0].weighted_index.unwrap().exact_numeric_cmp(decimal("5")).is_lt());
    query.contacts = None;
    query.issuers.include.clear();
    assert!(analyze(sources(), &context, None, &query).unwrap().groups.is_empty());
}

#[test]
fn recognizing_evidence_does_not_choose_a_source_to_rely_on() {
    let publication = publication();
    let context = contexts(&publication);
    let mut query = Query::for_subject(Identity::Organ(publication.scope.giver.clone()));
    let result = analyze(vec![source(&publication)], &context, None, &query).unwrap();
    assert!(result.groups.is_empty());
    assert_eq!(result.rows.len(), 2);
    assert!(
        result
            .rows
            .iter()
            .all(|row| !row.source_qualified && !row.selected)
    );
    query.rely_on_all_visible = true;
    let result = analyze(vec![source(&publication)], &context, None, &query).unwrap();
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].raw_quantity.unwrap().canonical(), "4");
}

#[test]
fn unknown_cap_allocation_withholds_all_indices_in_that_measurement_basis() {
    let mut publication = publication();
    if let Content::Statement { quantity, .. } = &mut publication.events[2].event.content {
        *quantity = decimal("100000000000000000000000000000000000000");
    }
    let mut query = query(&publication);
    query.levels.push(EvidenceLevel::Giver);
    query.weights.need_linked_receiver = decimal("100");
    query.issuer_cap = Some(decimal("1"));
    let result = analyze(
        vec![source(&publication)],
        &contexts(&publication),
        None,
        &query,
    )
    .unwrap();
    assert_eq!(result.groups.len(), 2);
    assert!(
        result
            .groups
            .iter()
            .all(|group| group.weighted_index.is_none() && group.arithmetic_error)
    );
    assert!(
        result
            .groups
            .iter()
            .all(|group| group.raw_quantity.is_some())
    );
}

#[test]
fn issuer_and_counterparty_filters_have_independent_identity_known_and_proximity_gates() {
    let publication = publication();
    let receiver = Identity::Organ(publication.scope.receiver.clone());
    let mut context = contexts(&publication);
    let mut query = query(&publication);
    query.issuers.exclude.push(super::super::Condition {
        identities: vec![receiver.clone()],
        ..Default::default()
    });
    let result = analyze(vec![source(&publication)], &context, None, &query).unwrap();
    assert!(result.groups.is_empty());
    assert!(
        result
            .rows
            .iter()
            .all(|row| row.level == EvidenceLevel::Giver)
    );
    query.issuers.exclude.clear();
    query.counterparties.include.push(super::super::Condition {
        known: Some(true),
        lower: Some(crate::visibility::Bound {
            value: 1,
            inclusive: false,
        }),
        upper: Some(crate::visibility::Bound {
            value: 3,
            inclusive: true,
        }),
        ..Default::default()
    });
    assert!(
        analyze(vec![source(&publication)], &context, None, &query)
            .unwrap()
            .rows
            .is_empty()
    );
    let receiver_context = context.get_mut(&receiver).unwrap();
    receiver_context.known = true;
    receiver_context.proximity = Some(2);
    assert_eq!(
        analyze(vec![source(&publication)], &context, None, &query)
            .unwrap()
            .groups
            .len(),
        1
    );
    query.counterparties.exclude.push(super::super::Condition {
        identities: vec![receiver],
        ..Default::default()
    });
    assert!(
        analyze(vec![source(&publication)], &context, None, &query)
            .unwrap()
            .rows
            .is_empty()
    );
}

#[test]
fn protocol_context_keeps_the_exact_historical_need_referenced_by_a_receipt() {
    let mut publication = publication();
    let original = publication.events[0].event.id().unwrap();
    let mut revised = publication.events[0].event.content.clone();
    if let Content::Need { requested, .. } = &mut revised {
        *requested = decimal("20");
    }
    push(&mut publication, revised, Some(original), "revised-need");
    let result = analyze(
        vec![source(&publication)],
        &contexts(&publication),
        None,
        &query(&publication),
    )
    .unwrap();
    assert_eq!(result.groups[0].raw_quantity.unwrap().canonical(), "4");
    assert!(result.protocol.iter().any(|row| !row.current
        && matches!(&row.content,Content::Need {requested,..} if requested.canonical()=="10")));
    assert!(result.protocol.iter().any(|row| row.current
        && matches!(&row.content,Content::Need {requested,..} if requested.canonical()=="20")));
}

#[test]
fn corrections_conflicts_withdrawals_and_disputes_do_not_inflate_contributions() {
    let input = |publication: &Publication| {
        let mut source = source(publication);
        for copy in &mut source.copies {
            if !matches!(copy.content, Content::Need { .. }) && publication.events.iter().find(|signed| signed.event.id().unwrap() == copy.projection.event).unwrap().event.supersedes.is_none() {
                copy.provenance = Provenance::Artifact {hash:hash("mixed-original-before-control")};
            }
        }
        source
    };
    let mut publication = publication();
    let old = publication.events[2].event.id().unwrap();
    let mut corrected = publication.events[2].event.content.clone();
    if let Content::Statement { quantity, .. } = &mut corrected {
        *quantity = decimal("3");
    }
    let next = push(
        &mut publication,
        corrected.clone(),
        Some(old.clone()),
        "receiver-correction",
    );
    let context = contexts(&publication);
    let query = query(&publication);
    let result = analyze(vec![input(&publication)], &context, None, &query).unwrap();
    assert_eq!(result.groups[0].raw_quantity.unwrap().canonical(), "3");
    assert!(!result.rows.iter().find(|row| row.selected).unwrap().records.is_empty());
    let mut forked = publication.clone();
    if let Content::Statement { quantity, .. } = &mut corrected {
        *quantity = decimal("2");
    }
    push(&mut forked, corrected, Some(old), "fork");
    let result = analyze(vec![input(&forked)], &context, None, &query).unwrap();
    assert!(result.groups.is_empty());
    assert_eq!(result.withheld[0].reason, "conflicting_current_outcomes");
    let mut withdrawn = publication.clone();
    let mut statement = withdrawn.events[2].event.content.clone();
    if let Content::Statement {
        quantity,
        withdrawn,
        ..
    } = &mut statement
    {
        *quantity = decimal("0");
        *withdrawn = true;
    }
    push(&mut withdrawn, statement, Some(next), "withdrawn");
    let result = analyze(vec![input(&withdrawn)], &context, None, &query).unwrap();
    assert!(result.groups.is_empty());
    assert!(result.rows.iter().any(|row| row.withdrawn && !row.selected));
    push(
        &mut publication,
        Content::Dispute {
            role: Role::Receiver,
            active: true,
        },
        None,
        "dispute",
    );
    let result = analyze(vec![input(&publication)], &context, None, &query).unwrap();
    assert!(result.groups.is_empty());
    assert!(result.rows.iter().all(|row| row.disputed && !row.selected));
}

#[test]
fn contact_distance_and_caps_change_indices_without_changing_raw_quantity() {
    let publication = publication();
    let receiver = Identity::Organ(publication.scope.receiver.clone());
    let anchor = Identity::Person(crate::new_uid("r"));
    let first = Identity::Person(crate::new_uid("r"));
    let second = Identity::Person(crate::new_uid("r"));
    let mut query = query(&publication);
    query.contacts = Some(paths::Policy::for_anchor(anchor.clone()));
    let paths = paths::ResultSet {
        paths: vec![paths::Path {
            identities: vec![anchor, first, second, receiver],
            proofs: vec![vec![hash("a")], vec![hash("b")], vec![hash("c")]],
            unilateral: false,
        }],
        observed_only: true,
        identity_limit_reached: false,
        depth_limit_reached: false,
    };
    let context = contexts(&publication);
    let result = analyze(vec![source(&publication)], &context, Some(&paths), &query).unwrap();
    assert_eq!(result.groups[0].raw_quantity.unwrap().canonical(), "4");
    assert!(
        result.groups[0]
            .weighted_index
            .unwrap()
            .exact_numeric_cmp(decimal("1"))
            .is_eq()
    );
    assert_eq!(
        result
            .rows
            .iter()
            .find(|row| row.selected)
            .unwrap()
            .distance,
        Some(3)
    );
    query.issuer_cap = Some(decimal("0.8"));
    query.first_contact_cap = Some(decimal("0.6"));
    let result = analyze(vec![source(&publication)], &context, Some(&paths), &query).unwrap();
    assert_eq!(result.groups[0].raw_quantity.unwrap().canonical(), "4");
    assert_eq!(result.groups[0].weighted_index.unwrap().canonical(), "0.6");
    assert!(result.groups[0].capped);
    assert!(
        analyze(vec![source(&publication)], &context, None, &query)
            .unwrap()
            .partial
    );
}

#[test]
fn person_history_requires_explicit_attribution_and_denied_parties_never_qualify() {
    let publication = publication();
    let mut query = query(&publication);
    query.subject = Identity::Person(crate::new_uid("r"));
    let mut context = contexts(&publication);
    context.insert(
        query.subject.clone(),
        IdentityContext {
            readable: true,
            ..Default::default()
        },
    );
    assert!(
        analyze(vec![source(&publication)], &context, None, &query)
            .unwrap()
            .groups
            .is_empty()
    );
    let mut attributed = source(&publication);
    for copy in &mut attributed.copies {
        copy.persons.push(PublicPerson {
            role: ApprovalRole::Giver,
            person: query.subject.uid().into(),
        });
    }
    assert_eq!(
        analyze(vec![attributed], &context, None, &query)
            .unwrap()
            .groups
            .len(),
        1
    );
    query = self::query(&publication);
    context
        .get_mut(&Identity::Organ(publication.scope.receiver.clone()))
        .unwrap()
        .blocked = true;
    let result = analyze(vec![source(&publication)], &context, None, &query).unwrap();
    assert!(result.rows.is_empty());
    assert!(result.groups.is_empty());
}

#[test]
fn meanings_units_and_evidence_classes_stay_separate_and_rounding_is_explicit() {
    let publication = publication();
    let mut query = query(&publication);
    query.levels = vec![EvidenceLevel::NeedLinkedReceiver, EvidenceLevel::Giver];
    let context = contexts(&publication);
    let result = analyze(vec![source(&publication)], &context, None, &query).unwrap();
    assert_eq!(result.groups.len(), 2);
    assert_eq!(
        result
            .groups
            .iter()
            .find(|group| group.level == EvidenceLevel::Giver)
            .unwrap()
            .raw_quantity
            .unwrap()
            .canonical(),
        "100"
    );
    let mut other = publication.clone();
    other.scope.source.occurrence = hash("other-occurrence");
    other.scope.measurements[0].meaning.revision = 2;
    let mut changed: BTreeMap<CanonicalHash, CanonicalHash> = BTreeMap::new();
    for signed in &mut other.events {
        let old = signed.event.id().unwrap();
        signed.event.scope = other.scope.id().unwrap();
        match &mut signed.event.content {
            Content::Need { measurement, .. } => *measurement = other.scope.measurements[0].clone(),
            Content::Statement {
                measurement,
                quantity,
                role,
                ..
            } => {
                *measurement = other.scope.measurements[0].clone();
                if *role == Role::Giver {
                    *quantity = decimal("0.000000000000000001");
                }
            }
            _ => {}
        }
        if let Content::Statement {
            need: Some(need), ..
        } = &mut signed.event.content
        {
            *need = changed[need].clone();
        }
        changed.insert(old, signed.event.id().unwrap());
    }
    let source = source(&other);
    query.weights.giver = decimal("0.1");
    let result = analyze(
        vec![self::source(&publication), source],
        &context,
        None,
        &query,
    )
    .unwrap();
    let tiny = result
        .groups
        .iter()
        .find(|group| {
            group.measurement.meaning.revision == 2 && group.level == EvidenceLevel::Giver
        })
        .unwrap();
    assert_eq!(
        tiny.raw_quantity.unwrap().canonical(),
        "0.000000000000000001"
    );
    assert!(tiny.index_rounded);
    assert!(tiny.weighted_index.unwrap().is_zero());
}
