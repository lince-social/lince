use super::*;
use crate::{
    DecimalValue,
    transfer::{outcome::Role, receipt::*},
    vocabulary::reference::MeaningReference,
};

fn id(value: &str) -> CanonicalHash {
    hash("lince.transfer.history.test.v1", &value).unwrap()
}

fn publication() -> Publication {
    let scope = Scope {
        audience: crate::trust_analysis::contacts::Audience::Public,
        upfront_terms: None,
        public_persons: Vec::new(),
        source: Source {
            origin: crate::new_uid("r"),
            occurrence: id("occurrence"),
            activated_at: 900,
            agreement_revision: 1,
            agreement_commitment: id("agreement"),
            activation_commitment: id("activation"),
        },
        party_attribution: PartyAttribution::LaterOrganEndorsement,
        original_authorities: vec![],
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
        stages: vec![Stage::Need, Stage::Receiver, Stage::Settled, Stage::Dispute],
        agreed_quantity: None,
        canonical_unit: None,
        issued_at: 1000,
        expires_at: 2000,
    };
    let event = Event {
        scope: scope.id().unwrap(),
        source_commitment: id("reported"),
        recorded_at: 950,
        issued_at: 1010,
        supersedes: None,
        content: Content::Statement {
            role: Role::Receiver,
            measurement: scope.measurements[0].clone(),
            quantity: DecimalValue::parse_inferred("12").unwrap(),
            need: None,
            withdrawn: false,
        },
    };
    Publication {
        person_approvals: Vec::new(),
        scope,
        approvals: [
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
        .collect(),
        events: vec![SignedEvent {
            event,
            key_id: "test".into(),
            signature: "a".repeat(88),
        }],
    }
}

fn copies(publication: &Publication, record: &str) -> Vec<Copy> {
    project(publication)
        .unwrap()
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
                provenance: super::Provenance::Record {
                    record: record.into(),
                    version: 1,
                },
                scope: publication.scope.id().unwrap(),
                attribution: publication.scope.party_attribution,
                projection,
                content: event.content.clone(),
                recorded_at: event.recorded_at,
                canonical_unit: publication.scope.canonical_unit.clone(),
                agreed_quantity: publication.scope.agreed_quantity.clone(),
            }
        })
        .collect()
}

fn republish(publication: &Publication) -> Publication {
    let mut next = publication.clone();
    next.scope.expires_at += 1000;
    let mut ids: BTreeMap<CanonicalHash, CanonicalHash> = BTreeMap::new();
    for signed in &mut next.events {
        let before = signed.event.id().unwrap();
        signed.event.scope = next.scope.id().unwrap();
        signed.event.issued_at += 1;
        if let Some(parent) = &mut signed.event.supersedes {
            *parent = ids[parent].clone();
        }
        if let Content::Statement {
            need: Some(need), ..
        } = &mut signed.event.content
        {
            *need = ids[need].clone();
        }
        ids.insert(before, signed.event.id().unwrap());
    }
    next
}

#[test]
fn transient_fragments_merge_dependencies_and_preserve_real_record_or_hash_provenance() {
    use crate::trust_analysis::evidence::{self, Receipt};
    let mut parent = publication();
    let measurement = parent.scope.measurements[0].clone();
    let need = SignedEvent {
        event: Event {
            scope: parent.scope.id().unwrap(),
            source_commitment: id("need"),
            recorded_at: 940,
            issued_at: 1005,
            supersedes: None,
            content: Content::Need {
                need_commitment: id("frozen-need"),
                measurement,
                requested: DecimalValue::parse_inferred("10").unwrap(),
            },
        },
        key_id: "test".into(),
        signature: "a".repeat(88),
    };
    let need_id = need.event.id().unwrap();
    if let Content::Statement { need, .. } = &mut parent.events[0].event.content {
        *need = Some(need_id.clone());
    }
    let parent_id = parent.events[0].event.id().unwrap();
    let mut child = parent.clone();
    child.events[0].event.supersedes = Some(parent_id.clone());
    child.events[0].event.source_commitment = id("corrected");
    child.events[0].event.issued_at += 1;
    if let Content::Statement { quantity, .. } = &mut child.events[0].event.content {
        *quantity = DecimalValue::parse_inferred("3").unwrap();
    }
    let mut need_only = parent.clone();
    need_only.events = vec![need];
    let wrap = |publication| {
        Document::ProofFragment(Bundle {
            publication,
            authorities: Vec::new(),
        })
    };
    let child = wrap(child);
    let parent = wrap(parent);
    let need = wrap(need_only);
    let artifact = |document: &Document| Provenance::Artifact {
        hash: crate::trust_analysis::serving::Artifact::Receipt(Box::new(document.clone()))
            .hash()
            .unwrap(),
    };
    let mut inputs = vec![Receipt {
        document: &child,
        provenance: artifact(&child),
    }];
    let missing = evidence::receipts(&inputs, 1100).unwrap();
    assert!(missing[0].partial);
    assert!(missing[0].missing.contains(&parent_id));
    assert!(missing[0].missing.contains(&need_id));
    inputs.push(Receipt {
        document: &parent,
        provenance: Provenance::Record {
            record: "retained-original".into(),
            version: 2,
        },
    });
    assert!(evidence::receipts(&inputs, 1100).unwrap()[0].partial);
    inputs.push(Receipt {
        document: &need,
        provenance: artifact(&need),
    });
    inputs.push(Receipt {
        document: &child,
        provenance: artifact(&child),
    });
    let mut groups = evidence::receipts(&inputs, 1100).unwrap();
    assert_eq!(groups.len(), 1);
    let group = groups.remove(0);
    assert!(!group.partial);
    assert_eq!(group.history.outcomes.len(), 3);
    let corrected = group.history.outcomes.iter().find(|outcome| outcome.copies.iter().any(|copy| matches!(&copy.content, Content::Statement { quantity, .. } if quantity.canonical() == "3"))).unwrap();
    assert_eq!(corrected.copies.len(), 1);
    assert!(
        group
            .history
            .slots
            .iter()
            .any(|slot| slot.heads == vec![corrected.id.clone()])
    );
    let encoded = serde_json::to_value(&corrected.copies[0]).unwrap();
    assert_eq!(encoded["provenance"], "artifact");
    assert!(encoded.get("record").is_none());
    assert!(encoded.get("version").is_none());
    assert!(
        group
            .history
            .outcomes
            .iter()
            .flat_map(|outcome| &outcome.copies)
            .any(|copy| copy.provenance.record() == Some("retained-original"))
    );
}

#[test]
fn personal_attribution_is_per_copy_and_conflicting_identities_withhold_the_source() {
    use crate::transfer::receipt::{ApprovalRole, PublicPerson};
    let publication = publication();
    let original = copies(&publication, "unattributed").remove(0);
    let mut named = original.clone();
    named.provenance = super::Provenance::Record {
        record: "explicitly-attributed".into(),
        version: 1,
    };
    named.persons = vec![PublicPerson {
        role: ApprovalRole::Receiver,
        person: crate::new_uid("r"),
    }];
    let view = merge(vec![original.clone(), named.clone()]);
    assert!(!view.source_conflict);
    assert!(view.person_conflicts.is_empty());
    assert!(
        view.outcomes[0]
            .copies
            .iter()
            .any(|copy| copy.persons.is_empty())
    );
    let mut other = named.clone();
    other.provenance = super::Provenance::Record {
        record: "contradictory-attribution".into(),
        version: 1,
    };
    other.persons[0].person = crate::new_uid("r");
    let view = merge(vec![original, named, other]);
    assert!(view.source_conflict);
    assert_eq!(view.person_conflicts.len(), 2);
    assert_eq!(view.outcomes.len(), 1);
    assert!(view.slots.iter().all(|slot| slot.conflicted));
}

#[test]
fn copies_and_scopes_preserve_one_outcome_and_normalize_correction_links() {
    let mut first = publication();
    let mut corrected = first.events[0].clone();
    corrected.event.source_commitment = id("correction");
    corrected.event.supersedes = Some(first.events[0].event.id().unwrap());
    corrected.event.issued_at += 1;
    if let Content::Statement { quantity, .. } = &mut corrected.event.content {
        *quantity = DecimalValue::parse_inferred("10").unwrap();
    }
    first.events.push(corrected);
    let next = republish(&first);
    assert_ne!(
        first.events[0].event.id().unwrap(),
        next.events[0].event.id().unwrap()
    );
    let mut all = copies(&first, "original");
    all.extend(copies(&next, "replica"));
    all.extend(copies(&next, "replica"));
    let view = merge(all);
    assert!(!view.source_conflict);
    assert_eq!(view.outcomes.len(), 2);
    assert!(
        view.outcomes
            .iter()
            .all(|outcome| !outcome.conflicted && outcome.copies.len() == 2)
    );
    assert_eq!(view.slots.len(), 1);
    assert_eq!(view.slots[0].heads.len(), 1);
    assert!(!view.slots[0].conflicted);
}

#[test]
fn altered_amount_time_or_measurement_under_one_source_commitment_conflicts() {
    let first = publication();
    for change in 0..3 {
        let mut next = republish(&first);
        if change == 0 {
            if let Content::Statement { quantity, .. } = &mut next.events[0].event.content {
                *quantity = DecimalValue::parse_inferred("1000").unwrap();
            }
        }
        if change == 1 {
            next.events[0].event.recorded_at += 1;
        }
        if change == 2 {
            next.scope.measurements[0].meaning.revision += 1;
            if let Content::Statement { measurement, .. } = &mut next.events[0].event.content {
                *measurement = next.scope.measurements[0].clone();
            }
            next.events[0].event.scope = next.scope.id().unwrap();
        }
        let view = merge([copies(&first, "first"), copies(&next, "second")].concat());
        assert_eq!(view.outcomes.len(), 1);
        assert!(view.outcomes[0].conflicted);
        assert!(view.slots.iter().all(|slot| slot.conflicted));
    }
}

#[test]
fn competing_roots_and_corrections_are_not_selected_by_copy_count() {
    let first = publication();
    let mut other = republish(&first);
    other.events[0].event.source_commitment = id("other-root");
    let view = merge(
        [
            copies(&first, "first"),
            copies(&other, "other"),
            copies(&other, "copy"),
        ]
        .concat(),
    );
    assert_eq!(view.slots[0].heads.len(), 2);
    assert!(view.slots[0].conflicted);
    let mut a = copies(&first, "a").remove(0);
    let mut b = a.clone();
    b.provenance = super::Provenance::Record {
        record: "b".into(),
        version: 1,
    };
    b.projection.outcome = id("second");
    a.projection.parent = Some(b.projection.outcome.clone());
    b.projection.parent = Some(a.projection.outcome.clone());
    let view = merge(vec![a, b]);
    assert!(view.outcomes.iter().all(|outcome| outcome.conflicted));
    assert_eq!(view.slots[0].heads.len(), 2);
}

#[test]
fn omitted_need_context_does_not_upgrade_a_copy_or_conflict_with_a_disclosed_need() {
    let first = publication();
    let a = copies(&first, "a").remove(0);
    let mut b = a.clone();
    b.provenance = super::Provenance::Record {
        record: "b".into(),
        version: 1,
    };
    b.projection.need = Some(id("need"));
    let view = merge(vec![a.clone(), b.clone()]);
    assert!(!view.outcomes[0].conflicted);
    assert!(view.outcomes[0].copies[0].projection.need.is_none());
    b.projection.need = Some(id("different-need"));
    let mut c = a;
    c.provenance = super::Provenance::Record {
        record: "c".into(),
        version: 1,
    };
    c.projection.need = Some(id("need"));
    assert!(merge(vec![b, c]).outcomes[0].conflicted);
}

#[test]
fn contradictory_source_bindings_units_or_agreed_values_withhold_the_source() {
    let first = publication();
    let a = copies(&first, "a").remove(0);
    for change in 0..3 {
        let mut b = a.clone();
        b.provenance = super::Provenance::Record {
            record: "b".into(),
            version: 1,
        };
        if change == 0 {
            b.projection.binding = id("changed-binding");
        }
        if change == 1 {
            b.canonical_unit = Some(first.scope.measurements[0].meaning.clone());
        }
        if change == 2 {
            b.agreed_quantity = Some(RecordedFloat::new(12.0).unwrap());
        }
        let mut c = a.clone();
        if change == 1 {
            let mut unit = first.scope.measurements[0].meaning.clone();
            unit.revision += 1;
            c.canonical_unit = Some(unit);
        }
        if change == 2 {
            c.agreed_quantity = Some(RecordedFloat::new(10.0).unwrap());
        }
        let view = merge(vec![c, b]);
        assert!(view.source_conflict);
        assert!(view.slots[0].conflicted);
    }
}

#[test]
fn zero_received_and_withdrawal_are_different_semantics() {
    let mut first = publication();
    if let Content::Statement { quantity, .. } = &mut first.events[0].event.content {
        *quantity = DecimalValue::parse_inferred("0").unwrap();
    }
    let mut next = republish(&first);
    if let Content::Statement { withdrawn, .. } = &mut next.events[0].event.content {
        *withdrawn = true;
    }
    let view = merge([copies(&first, "zero"), copies(&next, "withdrawn")].concat());
    assert!(view.outcomes[0].conflicted);
}

#[test]
fn normalized_need_links_preserve_the_exact_confirmed_binding_across_scopes() {
    let mut first = publication();
    let mut need = first.events[0].clone();
    need.event.source_commitment = id("need-source");
    need.event.content = Content::Need {
        measurement: first.scope.measurements[0].clone(),
        requested: DecimalValue::parse_inferred("10").unwrap(),
        need_commitment: id("private-need"),
    };
    if let Content::Statement { need: linked, .. } = &mut first.events[0].event.content {
        *linked = Some(need.event.id().unwrap());
    }
    first.events.insert(0, need);
    let second = republish(&first);
    let view = merge([copies(&first, "a"), copies(&second, "b")].concat());
    assert_eq!(view.outcomes.len(), 2);
    assert!(view.outcomes.iter().all(|outcome| !outcome.conflicted));
    let statement = view
        .outcomes
        .iter()
        .find(|outcome| outcome.copies[0].projection.need.is_some())
        .unwrap();
    assert_eq!(
        statement.copies[0].projection.need,
        statement.copies[1].projection.need
    );
    assert_ne!(statement.copies[0].content, statement.copies[1].content);
}

#[test]
fn one_organ_in_two_roles_cannot_merge_its_distinct_dispute_statements() {
    let mut first = publication();
    first.scope.receiver = first.scope.giver.clone();
    first.events[0].event.scope = first.scope.id().unwrap();
    first.events[0].event.content = Content::Dispute {
        role: Role::Receiver,
        active: true,
    };
    let mut giver = first.events[0].clone();
    giver.event.content = Content::Dispute {
        role: Role::Giver,
        active: true,
    };
    first.events.push(giver);
    let view = merge(copies(&first, "both-roles"));
    assert_eq!(view.outcomes.len(), 2);
    assert_eq!(view.slots.len(), 2);
    assert!(view.slots.iter().all(|slot| !slot.conflicted));
}

#[test]
fn full_event_budget_handles_long_histories_and_cycles_without_recursive_traversal() {
    let first = publication();
    let base = copies(&first, "long-history").remove(0);
    let mut previous = None;
    let mut chain = Vec::new();
    for index in 0..5000 {
        let mut next = base.clone();
        next.projection.outcome = id(&format!("outcome-{index}"));
        next.projection.event = id(&format!("event-{index}"));
        next.projection.parent = previous;
        previous = Some(next.projection.outcome.clone());
        chain.push(next);
    }
    let view = merge(chain.clone());
    assert_eq!(view.outcomes.len(), 5000);
    assert_eq!(view.slots[0].heads, vec![previous.clone().unwrap()]);
    assert!(!view.slots[0].conflicted);
    chain[0].projection.parent = previous;
    let view = merge(chain);
    assert_eq!(view.slots[0].heads.len(), 5000);
    assert!(view.outcomes.iter().all(|outcome| outcome.conflicted));
}

#[test]
fn fragment_resolution_handles_the_full_budget_without_recursive_ancestry() {
    let publication = publication();
    let mut events = BTreeMap::new();
    let mut parent = None;
    for index in 0..5000 {
        let mut event = publication.events[0].event.clone();
        event.source_commitment = id(&format!("fragment-source-{index}"));
        event.supersedes = parent;
        let key = event.id().unwrap();
        parent = Some(key.clone());
        events.insert(key, event);
    }
    let selected = events.keys().cloned().collect();
    let result =
        crate::transfer::receipt_fragment::resolve(&publication.scope, &events, &selected, 1100)
            .unwrap();
    assert_eq!(result.projections.len(), 5000);
    assert!(result.unresolved.is_empty());
    assert!(result.missing.is_empty());
    let mut extra = publication.events[0].event.clone();
    extra.source_commitment = id("over-budget");
    events.insert(extra.id().unwrap(), extra);
    assert!(
        crate::transfer::receipt_fragment::resolve(&publication.scope, &events, &selected, 1100)
            .is_err()
    );
}

#[test]
fn fragments_propagate_missing_history_and_ignore_unrelated_dangling_events() {
    use crate::transfer::receipt_fragment::{State, resolve};
    let publication = publication();
    let root = publication.events[0].event.clone();
    let mut child = root.clone();
    child.supersedes = Some(root.id().unwrap());
    child.source_commitment = id("child");
    let mut dangling = root.clone();
    dangling.supersedes = Some(id("unavailable"));
    dangling.source_commitment = id("unrelated");
    let mut events = [
        (child.id().unwrap(), child.clone()),
        (dangling.id().unwrap(), dangling),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    let selected = [child.id().unwrap()].into_iter().collect();
    let result = resolve(&publication.scope, &events, &selected, 1100).unwrap();
    assert!(result.projections.is_empty());
    assert_eq!(result.unresolved[0].state, State::Incomplete);
    assert_eq!(result.missing, vec![root.id().unwrap()]);
    events.insert(root.id().unwrap(), root);
    let result = resolve(&publication.scope, &events, &selected, 1100).unwrap();
    assert_eq!(result.projections.len(), 1);
    assert!(result.unresolved.is_empty());
    assert!(result.missing.is_empty());
}

#[test]
fn fragment_dependencies_cannot_change_role_need_type_or_publication_order() {
    use crate::transfer::receipt_fragment::{State, resolve};
    let publication = publication();
    let root = publication.events[0].event.clone();
    for mode in 0..3 {
        let mut invalid = root.clone();
        invalid.source_commitment = id(&format!("invalid-link-{mode}"));
        if mode == 0 {
            invalid.content = Content::Dispute {
                role: Role::Giver,
                active: true,
            };
            invalid.supersedes = Some(root.id().unwrap());
        } else if mode == 1 {
            if let Content::Statement { need, .. } = &mut invalid.content {
                *need = Some(root.id().unwrap());
            }
        } else {
            invalid.issued_at -= 1;
            invalid.supersedes = Some(root.id().unwrap());
        }
        let mut child = invalid.clone();
        child.supersedes = Some(invalid.id().unwrap());
        child.source_commitment = id(&format!("invalid-descendant-{mode}"));
        let selected = [invalid.id().unwrap(), child.id().unwrap()]
            .into_iter()
            .collect();
        let events = [root.clone(), invalid, child]
            .into_iter()
            .map(|event| (event.id().unwrap(), event))
            .collect();
        let result = resolve(&publication.scope, &events, &selected, 1100).unwrap();
        assert!(result.projections.is_empty());
        assert_eq!(result.unresolved.len(), 2);
        assert!(
            result
                .unresolved
                .iter()
                .all(|issue| issue.state == State::Invalid)
        );
    }
}
