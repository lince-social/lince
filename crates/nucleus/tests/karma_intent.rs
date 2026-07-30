use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::{
    BudgetDenial, BudgetUsage, CanonicalHash, Capability, CapabilitySet, DecimalValue,
    DelegationGrantRevision, DelegationGrantSchema, DelegationGrantSpec, DurationMs, GrantBudget,
    GrantProgramRevisionScope, GrantQuantityLimit, GrantRevisionChange, GrantTarget,
    GrantTargetScope, GrantTemplateScope, GrantWindowLimit, IntentAmount, IntentAuthorization,
    IntentStatus, K5_2_INTENT_STATES, KarmaIntent, KarmaIntentSchema, ReferenceKind, Slug,
    TimestampMs,
    TypedUid, authorize_intent,
};

const PERSON_UID: &str = "p_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const PROGRAM_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const RECORD_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAW";
const UNIT_UID: &str = "c_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const OTHER_UNIT_UID: &str = "c_01ARZ3NDEKTSV4RRFFQ69G5FAW";

/// Absence of a limit is unlimited, so adding one narrows and removing one
/// widens. Nothing may raise a limit that was already agreed.
#[test]
fn budget_narrowing_accepts_only_strictly_smaller_limits() {
    let open = grant(GrantBudget::default());

    // Adding any limit to an unlimited budget is a narrowing.
    let capped = grant(GrantBudget {
        max_intents: Some(5),
        ..GrantBudget::default()
    });
    assert_eq!(
        open.compare_replacement(&capped).unwrap(),
        GrantRevisionChange::Narrowing
    );
    // Removing it again is a widening.
    assert_eq!(
        capped.compare_replacement(&open).unwrap(),
        GrantRevisionChange::Widening
    );

    let tighter = grant(GrantBudget {
        max_intents: Some(2),
        ..GrantBudget::default()
    });
    assert_eq!(
        capped.compare_replacement(&tighter).unwrap(),
        GrantRevisionChange::Narrowing
    );
    assert_eq!(
        tighter.compare_replacement(&capped).unwrap(),
        GrantRevisionChange::Widening
    );

    // Windows narrow by allowing fewer events, or the same events over longer.
    let window = |count, duration| {
        grant(GrantBudget {
            per_window: Some(GrantWindowLimit {
                count,
                duration_ms: DurationMs::new(duration),
            }),
            ..GrantBudget::default()
        })
    };
    assert_eq!(
        window(10, 1_000)
            .compare_replacement(&window(4, 1_000))
            .unwrap(),
        GrantRevisionChange::Narrowing
    );
    assert_eq!(
        window(10, 1_000)
            .compare_replacement(&window(10, 5_000))
            .unwrap(),
        GrantRevisionChange::Narrowing
    );
    assert_eq!(
        window(10, 1_000)
            .compare_replacement(&window(11, 1_000))
            .unwrap(),
        GrantRevisionChange::Widening
    );
    assert_eq!(
        window(10, 1_000)
            .compare_replacement(&window(10, 500))
            .unwrap(),
        GrantRevisionChange::Widening
    );
    // A lower rate carrying a larger burst is not a narrowing: 100 per 20s
    // permits a spike of 100 that 5 per 1s never allowed.
    assert_ne!(
        window(5, 1_000)
            .compare_replacement(&window(100, 20_000))
            .unwrap(),
        GrantRevisionChange::Narrowing
    );

    // Quantity narrows only within the same unit and scale.
    let quantity = |unit, scale, mantissa| {
        grant(GrantBudget {
            quantity_limit: Some(GrantQuantityLimit {
                unit_uid: uid(ReferenceKind::Unit, unit),
                limit: DecimalValue::from_mantissa(scale, mantissa).unwrap(),
            }),
            ..GrantBudget::default()
        })
    };
    assert_eq!(
        quantity(UNIT_UID, 3, 5_000)
            .compare_replacement(&quantity(UNIT_UID, 3, 1_000))
            .unwrap(),
        GrantRevisionChange::Narrowing
    );
    assert_ne!(
        quantity(UNIT_UID, 3, 5_000)
            .compare_replacement(&quantity(OTHER_UNIT_UID, 3, 1_000))
            .unwrap(),
        GrantRevisionChange::Narrowing
    );
    assert_ne!(
        quantity(UNIT_UID, 3, 5_000)
            .compare_replacement(&quantity(UNIT_UID, 2, 100))
            .unwrap(),
        GrantRevisionChange::Narrowing
    );

    // Tightening one dimension while loosening another is refused outright.
    let mixed = grant(GrantBudget {
        max_intents: Some(2),
        per_window: Some(GrantWindowLimit {
            count: 99,
            duration_ms: DurationMs::new(1_000),
        }),
        ..GrantBudget::default()
    });
    assert_eq!(
        capped
            .compare_replacement(&grant(GrantBudget {
                max_intents: Some(9),
                per_window: Some(GrantWindowLimit {
                    count: 1,
                    duration_ms: DurationMs::new(1_000)
                }),
                ..GrantBudget::default()
            }))
            .unwrap(),
        GrantRevisionChange::Mixed
    );
    assert_eq!(
        capped.compare_replacement(&mixed).unwrap(),
        GrantRevisionChange::Narrowing,
        "lowering the cap and adding a window are both restrictions"
    );
}

#[test]
fn an_unspent_budget_authorizes_and_freezes_its_reservation() {
    let revision = grant(GrantBudget {
        max_intents: Some(3),
        per_window: Some(GrantWindowLimit {
            count: 2,
            duration_ms: DurationMs::new(1_000),
        }),
        quantity_limit: Some(GrantQuantityLimit {
            unit_uid: uid(ReferenceKind::Unit, UNIT_UID),
            limit: decimal(3, 5_000),
        }),
    });
    let usage = BudgetUsage {
        intents: 1,
        window_index: Some(1),
        window_intents: 1,
        quantity: Some(decimal(3, 1_000)),
    };
    let outcome = authorize_intent(
        &revision,
        &request(at(1_500)),
        &usage,
        Some(&amount(UNIT_UID, 3, 2_000)),
    )
    .unwrap();
    assert!(outcome.allowed);
    assert!(outcome.budget_denials.is_empty());
    let snapshot = outcome.budget.unwrap();
    assert_eq!(snapshot.window_index, Some(1));
    assert_eq!(snapshot.intents_before, 1);
    assert_eq!(snapshot.intents_after, 2);
    assert_eq!(snapshot.window_intents_after, 2);
    assert_eq!(snapshot.quantity_before, Some(decimal(3, 1_000)));
    assert_eq!(snapshot.quantity_after, Some(decimal(3, 3_000)));
}

#[test]
fn every_budget_dimension_denies_on_its_own() {
    let full = GrantBudget {
        max_intents: Some(3),
        per_window: Some(GrantWindowLimit {
            count: 2,
            duration_ms: DurationMs::new(1_000),
        }),
        quantity_limit: Some(GrantQuantityLimit {
            unit_uid: uid(ReferenceKind::Unit, UNIT_UID),
            limit: decimal(3, 5_000),
        }),
    };
    let revision = grant(full.clone());
    let spendable = |intents, window_intents, quantity| BudgetUsage {
        intents,
        window_index: Some(1),
        window_intents,
        quantity: Some(decimal(3, quantity)),
    };

    let exhausted_cap = authorize_intent(
        &revision,
        &request(at(1_500)),
        &spendable(3, 0, 0),
        Some(&amount(UNIT_UID, 3, 1_000)),
    )
    .unwrap();
    assert!(!exhausted_cap.allowed);
    assert_eq!(
        exhausted_cap.budget_denials,
        vec![BudgetDenial::IntentCapExhausted]
    );
    assert!(exhausted_cap.budget.is_none(), "a denial reserves nothing");

    let exhausted_window = authorize_intent(
        &revision,
        &request(at(1_500)),
        &spendable(0, 2, 0),
        Some(&amount(UNIT_UID, 3, 1_000)),
    )
    .unwrap();
    assert_eq!(
        exhausted_window.budget_denials,
        vec![BudgetDenial::WindowCapExhausted]
    );

    let exhausted_quantity = authorize_intent(
        &revision,
        &request(at(1_500)),
        &spendable(0, 0, 4_500),
        Some(&amount(UNIT_UID, 3, 1_000)),
    )
    .unwrap();
    assert_eq!(
        exhausted_quantity.budget_denials,
        vec![BudgetDenial::QuantityExhausted]
    );

    // A quantity-limited grant cannot authorize an amountless request, and it
    // never converts between units or scales to make one fit.
    for (amount, expected) in [
        (None, BudgetDenial::QuantityRequired),
        (
            Some(amount(OTHER_UNIT_UID, 3, 1_000)),
            BudgetDenial::QuantityUnitMismatch,
        ),
        (
            Some(amount(UNIT_UID, 2, 100)),
            BudgetDenial::QuantityScaleMismatch,
        ),
        (
            Some(amount(UNIT_UID, 3, -1_000)),
            BudgetDenial::QuantityNotPositive,
        ),
    ] {
        let outcome = authorize_intent(
            &revision,
            &request(at(1_500)),
            &spendable(0, 0, 0),
            amount.as_ref(),
        )
        .unwrap();
        assert_eq!(outcome.budget_denials, vec![expected]);
    }
}

/// The window a request falls in is a pure function of the revision, so a
/// replay of the same instant reserves against the same window.
#[test]
fn windows_tumble_deterministically_from_valid_from() {
    let budget = GrantBudget {
        per_window: Some(GrantWindowLimit {
            count: 1,
            duration_ms: DurationMs::new(1_000),
        }),
        ..GrantBudget::default()
    };
    assert_eq!(budget.window_index(at(0), at(0)), Some(0));
    assert_eq!(budget.window_index(at(0), at(999)), Some(0));
    assert_eq!(budget.window_index(at(0), at(1_000)), Some(1));
    assert_eq!(budget.window_index(at(500), at(1_400)), Some(0));
    assert_eq!(budget.window_index(at(500), at(1_500)), Some(1));
    // Before the grant is valid there is no window to spend from.
    assert_eq!(budget.window_index(at(500), at(0)), None);

    // Usage counted for a different window cannot be spent in this one.
    let revision = grant(budget);
    let outcome = authorize_intent(
        &revision,
        &request(at(1_500)),
        &BudgetUsage {
            intents: 0,
            window_index: Some(0),
            window_intents: 0,
            quantity: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(
        outcome.budget_denials,
        vec![BudgetDenial::WindowUnavailable]
    );
}

/// Authority and budget are reported together, and an intent can never freeze
/// a decision that was not allowed.
#[test]
fn a_refused_request_can_never_become_an_intent() {
    let revision = grant(GrantBudget {
        max_intents: Some(1),
        ..GrantBudget::default()
    });
    let mut wrong = request(at(1_500));
    wrong.capability = Capability::ExternalPayment;
    let outcome = authorize_intent(
        &revision,
        &wrong,
        &BudgetUsage {
            intents: 1,
            ..BudgetUsage::default()
        },
        None,
    )
    .unwrap();
    assert!(!outcome.allowed);
    assert!(!outcome.decision.allowed, "authority objected");
    assert_eq!(
        outcome.budget_denials,
        vec![BudgetDenial::IntentCapExhausted],
        "and so did the budget, in the same answer"
    );

    let denied = KarmaIntent {
        schema: KarmaIntentSchema::V1,
        candidate_hash: hash('c'),
        program_uid: PROGRAM_UID.to_string(),
        program_revision_hash: hash('a'),
        template: slug("record.add-quantity"),
        capability: Capability::RecordAddQuantity,
        target: None,
        fields: BTreeMap::new(),
        quantity: None,
        idempotency_key: "intent-1".to_string(),
        deadline: at(9_000),
        authorization: IntentAuthorization {
            schema: KarmaIntentSchema::V1,
            grant_uid: RECORD_UID.to_string(),
            grant_handle_revision: 2,
            grant_revision_hash: hash('b'),
            request: request(at(1_500)),
            decision: outcome.decision,
            budget: nucleus::karma::BudgetSnapshot {
                budget: GrantBudget::default(),
                window_index: None,
                intents_before: 0,
                intents_after: 1,
                window_intents_before: 0,
                window_intents_after: 1,
                quantity_before: None,
                quantity_after: None,
            },
        },
    };
    assert!(
        denied.intent_hash().is_err(),
        "a denied decision must not be hashable into an intent"
    );
}

/// Budget accounting is stated once, over the whole frozen vocabulary: an
/// intent that could still cause work holds its reservation, and one that never
/// will releases it.
#[test]
fn only_intents_that_could_still_act_hold_their_reservation() {
    for status in [
        IntentStatus::Authorized,
        IntentStatus::Leased,
        IntentStatus::Dispatching,
        IntentStatus::Succeeded,
        IntentStatus::Uncertain,
    ] {
        assert!(status.holds_reservation(), "{status:?} still holds budget");
    }
    for status in [
        IntentStatus::Staged,
        IntentStatus::Failed,
        IntentStatus::Cancelled,
        IntentStatus::Compensated,
        IntentStatus::DeadLetter,
    ] {
        assert!(!status.holds_reservation(), "{status:?} releases budget");
    }
    // K5.2 may only ever write the two states it can actually reach.
    assert_eq!(
        K5_2_INTENT_STATES,
        [IntentStatus::Authorized, IntentStatus::Cancelled]
    );
    assert_eq!(IntentStatus::parse("authorized"), Some(IntentStatus::Authorized));
    assert_eq!(IntentStatus::Cancelled.as_str(), "cancelled");
}

fn grant(budget: GrantBudget) -> DelegationGrantRevision {
    DelegationGrantRevision::new(
        uid(ReferenceKind::Person, PERSON_UID),
        DelegationGrantSpec {
            schema: DelegationGrantSchema::V1,
            purpose: "Bounded pantry restock".to_string(),
            program_uid: uid(ReferenceKind::Program, PROGRAM_UID),
            program_revision: GrantProgramRevisionScope::AnyActive,
            candidate_templates: GrantTemplateScope::Only {
                templates: BTreeSet::from([slug("record.add-quantity")]),
            },
            capabilities: CapabilitySet::new([Capability::RecordAddQuantity]),
            targets: GrantTargetScope::Any,
            budget,
            valid_from: at(0),
            expires_at: at(9_000),
        },
    )
    .unwrap()
}

fn request(logical_at: TimestampMs) -> nucleus::karma::GrantAuthorityRequest {
    nucleus::karma::GrantAuthorityRequest {
        principal_person_uid: uid(ReferenceKind::Person, PERSON_UID),
        program_uid: uid(ReferenceKind::Program, PROGRAM_UID),
        program_revision_hash: hash('a'),
        candidate_template: slug("record.add-quantity"),
        capability: Capability::RecordAddQuantity,
        target: Some(GrantTarget::Record(uid(ReferenceKind::Record, RECORD_UID))),
        logical_at,
    }
}

fn amount(unit: &str, scale: u8, mantissa: i128) -> IntentAmount {
    IntentAmount {
        unit_uid: uid(ReferenceKind::Unit, unit),
        amount: decimal(scale, mantissa),
    }
}

fn decimal(scale: u8, mantissa: i128) -> DecimalValue {
    DecimalValue::from_mantissa(scale, mantissa).unwrap()
}

fn uid(kind: ReferenceKind, value: &str) -> TypedUid {
    TypedUid::new(kind, value).unwrap()
}

fn slug(value: &str) -> Slug {
    Slug::new(value).unwrap()
}

fn hash(value: char) -> CanonicalHash {
    CanonicalHash::parse(format!("sha256:{}", value.to_string().repeat(64))).unwrap()
}

fn at(milliseconds: i64) -> TimestampMs {
    TimestampMs::from_millis(milliseconds).unwrap()
}
