use super::{
    Bound, Condition, Data, MAX_CONDITIONS, MAX_ORGANS_PER_CONDITION, Policy, rules::PolicyView,
};

#[test]
fn persons_intersect_organ_ranges_and_exclusions_bound_organ_replication() {
    let organ = crate::new_uid("r");
    let person = crate::new_uid("r");
    let other = crate::new_uid("r");
    let mut policy = Policy {
        include: vec![Condition {
            organs: vec![organ.clone()],
            persons: vec![person.clone()],
            upper: Some(Bound {
                value: 2,
                inclusive: true,
            }),
            radius_metres: Some(1000),
            ..Default::default()
        }],
        exclude: vec![],
    };
    policy.validate_for(Data::Place).unwrap();
    assert!(!policy.decide(&organ, Some(1)).allowed);
    assert!(
        !policy
            .decide_principal(&organ, Some(&other), Some(1))
            .allowed
    );
    assert!(
        !policy
            .decide_principal(&other, Some(&person), Some(1))
            .allowed
    );
    assert!(
        !policy
            .decide_principal(&organ, Some(&person), Some(3))
            .allowed
    );
    let decision = policy.decide_principal(&organ, Some(&person), Some(2));
    assert!(decision.allowed);
    assert_eq!(decision.radius_metres, Some(1000));
    policy.include.push(Condition {
        organs: vec![organ.clone()],
        ..Default::default()
    });
    policy.exclude.push(Condition {
        persons: vec![person.clone()],
        ..Default::default()
    });
    assert!(
        !policy
            .decide_principal(&organ, Some(&person), Some(1))
            .allowed
    );
    assert!(
        policy
            .decide_principal(&organ, Some(&other), Some(1))
            .allowed
    );
    assert!(!policy.decide(&organ, Some(1)).allowed);
    let [exclusion] = policy.exclude.as_mut_slice() else {
        panic!("fixture has one exclusion: {policy:?}");
    };
    exclusion.persons.push(person);
    assert!(policy.validate().is_err());
}

#[test]
fn composed_ranges_intersect_organs_and_exclusions_win() {
    let a = crate::new_uid("r");
    let b = crate::new_uid("r");
    let policy = Policy {
        include: vec![
            Condition {
                upper: Some(Bound {
                    value: 3,
                    inclusive: false,
                }),
                ..Default::default()
            },
            Condition {
                organs: vec![b.clone()],
                lower: Some(Bound {
                    value: 6,
                    inclusive: true,
                }),
                ..Default::default()
            },
        ],
        exclude: vec![Condition {
            organs: vec![a.clone()],
            lower: Some(Bound {
                value: 1,
                inclusive: true,
            }),
            upper: Some(Bound {
                value: 2,
                inclusive: true,
            }),
            ..Default::default()
        }],
    };
    policy.validate().unwrap();
    assert!(policy.decide(&a, Some(0)).allowed);
    let denied = policy.decide(&a, Some(2));
    assert_eq!(denied.included_by, vec![0]);
    assert_eq!(denied.excluded_by, vec![0]);
    assert!(!denied.allowed);
    assert!(!policy.decide(&a, Some(3)).allowed);
    assert!(policy.decide(&b, Some(6)).allowed);
    assert!(!policy.decide(&a, Some(6)).allowed);
    assert!(!policy.decide(&b, None).allowed);
    assert!(!policy.decide("", Some(1)).allowed);
    assert!(!Policy::default().decide(&b, Some(0)).allowed);
}

#[test]
fn invalid_ranges_and_oversized_policies_are_rejected() {
    for (lower, upper) in [
        (
            Some(Bound {
                value: u32::MAX,
                inclusive: false,
            }),
            None,
        ),
        (
            None,
            Some(Bound {
                value: 0,
                inclusive: false,
            }),
        ),
        (
            Some(Bound {
                value: 3,
                inclusive: false,
            }),
            Some(Bound {
                value: 4,
                inclusive: false,
            }),
        ),
    ] {
        assert!(
            Policy {
                include: vec![Condition {
                    lower,
                    upper,
                    ..Default::default()
                }],
                exclude: vec![]
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        Policy {
            include: vec![Condition::default(); MAX_CONDITIONS + 1],
            exclude: vec![]
        }
        .validate()
        .is_err()
    );
    let organs = (0..MAX_ORGANS_PER_CONDITION)
        .map(|_| crate::new_uid("r"))
        .collect::<Vec<_>>();
    assert!(
        Policy {
            include: vec![
                Condition {
                    organs,
                    ..Default::default()
                };
                MAX_CONDITIONS
            ],
            exclude: vec![]
        }
        .validate()
        .is_err()
    );
}

#[test]
fn borrowed_policy_carries_the_same_limit_bytes() {
    let policy = Policy {
        include: vec![Condition {
            radius_metres: Some(200),
            ..Default::default()
        }],
        exclude: vec![Condition::default()],
    };
    let borrowed = PolicyView {
        include: &policy.include,
        exclude: &policy.exclude,
    };
    assert_eq!(
        serde_json::to_vec(&borrowed).unwrap(),
        serde_json::to_vec(&policy).unwrap()
    );
}

#[test]
fn overlapping_inclusions_keep_the_most_precise_radius() {
    let mut policy = Policy {
        include: [900, 200]
            .into_iter()
            .map(|radius_metres| Condition {
                radius_metres: Some(radius_metres),
                ..Default::default()
            })
            .collect(),
        exclude: Vec::new(),
    };
    let result = policy.decide("organ", None);
    assert_eq!(result.included_by, [0, 1]);
    assert_eq!(result.radius_metres, Some(200));
    policy.include.push(Condition::default());
    let result = policy.decide("organ", None);
    assert_eq!(result.included_by, [0, 1, 2]);
    assert_eq!(result.radius_metres, None);
}
