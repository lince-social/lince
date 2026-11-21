use super::*;

#[test]
fn mixed_source_policy_keeps_endpoint_eligibility_raw_amounts_and_distance_indices_separate() {
    use crate::trust_analysis::contacts::{Edge, RelationKind};
    let first = publication();
    let mut second = first.clone();
    second.scope.source.occurrence = hash("second-occurrence");
    second.scope.receiver = new_uid("r");
    second.events.clear();
    let mut dependencies = BTreeMap::<CanonicalHash, CanonicalHash>::new();
    for (index, signed) in first.events.iter().enumerate() {
        let mut content = signed.event.content.clone();
        if let Content::Statement {
            need: Some(need), ..
        } = &mut content
        {
            *need = dependencies.get(need).unwrap().clone();
        }
        let next = push(&mut second, content, None, &format!("second-{index}"));
        dependencies.insert(signed.event.id().unwrap(), next);
    }
    let anchor = Identity::Person(new_uid("r"));
    let direct = Identity::Organ(first.scope.receiver.clone());
    let middle = Identity::Organ(new_uid("r"));
    let distant = Identity::Organ(second.scope.receiver.clone());
    let mut context = contexts(&first);
    context.extend(contexts(&second));
    for identity in [&anchor, &middle] {
        context.insert(
            identity.clone(),
            IdentityContext {
                readable: true,
                ..Default::default()
            },
        );
    }
    let edges = [(&anchor, &direct), (&direct, &middle), (&middle, &distant)]
        .into_iter()
        .enumerate()
        .map(|(index, (from, to))| Edge {
            from: from.clone(),
            to: to.clone(),
            kind: RelationKind::Mutual,
            proofs: vec![hash(&format!("edge-{index}"))],
        })
        .collect::<Vec<_>>();
    let policy = paths::Policy::for_anchor(anchor);
    let paths = paths::search(&edges, &context, &policy).unwrap();
    let mut query = query(&first);
    query.issuers.include[0].identities.push(distant.clone());
    query.contacts = Some(policy.clone());
    let sources = || {
        let mut remote = source(&second);
        for copy in &mut remote.copies {
            copy.provenance = Provenance::Artifact {
                hash: hash("remote-second-occurrence"),
            };
        }
        vec![source(&first), remote]
    };
    let initial = analyze(sources(), &context, Some(&paths), &query).unwrap();
    assert_eq!(initial.groups[0].raw_quantity.unwrap().canonical(), "8");
    assert!(
        initial.groups[0]
            .weighted_index
            .unwrap()
            .exact_numeric_cmp(decimal("5"))
            .is_eq()
    );
    let exclusion = Condition {
        identities: vec![distant.clone()],
        ..Default::default()
    };
    query.endpoints.exclude.push(exclusion.clone());
    assert_eq!(
        analyze(sources(), &context, Some(&paths), &query)
            .unwrap()
            .groups[0]
            .raw_quantity
            .unwrap()
            .canonical(),
        "8"
    );
    query.counterparties.exclude.push(exclusion.clone());
    assert_eq!(
        analyze(sources(), &context, Some(&paths), &query)
            .unwrap()
            .groups[0]
            .raw_quantity
            .unwrap()
            .canonical(),
        "4"
    );
    query.counterparties.exclude.clear();
    query.issuers.exclude.push(exclusion);
    assert_eq!(
        analyze(sources(), &context, Some(&paths), &query)
            .unwrap()
            .groups[0]
            .raw_quantity
            .unwrap()
            .canonical(),
        "4"
    );
    query.issuers.exclude.clear();
    let mut excluded_path = policy;
    excluded_path.intermediaries.exclude.push(Condition {
        identities: vec![middle],
        ..Default::default()
    });
    let filtered = paths::search(&edges, &context, &excluded_path).unwrap();
    assert_eq!(
        analyze(sources(), &context, Some(&filtered), &query)
            .unwrap()
            .groups[0]
            .raw_quantity
            .unwrap()
            .canonical(),
        "4"
    );
    query.issuer_cap = Some(decimal("0.5"));
    let capped = analyze(sources(), &context, Some(&paths), &query).unwrap();
    assert_eq!(capped.groups[0].raw_quantity.unwrap().canonical(), "8");
    assert!(
        capped.groups[0]
            .weighted_index
            .unwrap()
            .exact_numeric_cmp(decimal("5"))
            .is_lt()
    );
    query.contacts = None;
    query.issuers.include.clear();
    assert!(
        analyze(sources(), &context, None, &query)
            .unwrap()
            .groups
            .is_empty()
    );
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
fn issuer_and_counterparty_filters_have_independent_identity_known_and_proximity_gates() {
    let publication = publication();
    let receiver = Identity::Organ(publication.scope.receiver.clone());
    let mut context = contexts(&publication);
    let mut query = query(&publication);
    query.issuers.exclude.push(Condition {
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
    query.counterparties.include.push(Condition {
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
    query.counterparties.exclude.push(Condition {
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
fn person_history_requires_explicit_attribution_and_denied_parties_never_qualify() {
    let publication = publication();
    let mut query = query(&publication);
    query.subject = Identity::Person(new_uid("r"));
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
