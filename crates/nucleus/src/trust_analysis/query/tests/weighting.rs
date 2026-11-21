use super::*;

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
fn contact_distance_and_caps_change_indices_without_changing_raw_quantity() {
    let publication = publication();
    let receiver = Identity::Organ(publication.scope.receiver.clone());
    let anchor = Identity::Person(new_uid("r"));
    let first = Identity::Person(new_uid("r"));
    let second = Identity::Person(new_uid("r"));
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
