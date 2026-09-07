use protein::authority::RolePolicy;
use protein::{LinkDirection, Predicate, Protein};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

fn round_trip<T>(raw: Value) -> T
where
    T: DeserializeOwned + Serialize,
{
    let value: T = serde_json::from_value(raw).unwrap();
    let encoded = serde_json::to_value(value).unwrap();
    serde_json::from_value(encoded).unwrap()
}

fn rejects_protein(raw: Value) {
    assert!(serde_json::from_value::<Protein>(raw).is_err());
}

#[test]
fn every_source_aggregate_and_order_spelling_round_trips() {
    for source in [
        "record",
        "promise",
        "decision",
        "fact",
        "concept",
        "lingua",
        "assertion",
        "transfer",
        "transfer_settlement_preview",
        "transfer_bulk_completion_preview",
        "auth",
        "karma",
        "timeline",
        "entry",
        "frequency",
        "recurrence",
        "nearby",
    ] {
        let protein: Protein = round_trip(json!({"source": source}));
        assert_eq!(serde_json::to_value(protein).unwrap()["source"], source);
    }

    for op in ["sum", "count"] {
        for by in [
            "total",
            "concept",
            "classification",
            "kind",
            "cause_kind",
            "day",
            "month",
        ] {
            let _: Protein = round_trip(json!({
                "source": "record",
                "aggregate": {"op": op, "by": by}
            }));
        }
    }

    for order in [
        json!({"asc": "head"}),
        json!({"desc": "quantity"}),
        json!({"link": {"kind": "part-of", "higher": "from"}}),
        json!({"link": {"kind": "assigned", "higher": "to"}}),
    ] {
        let _: Protein = round_trip(json!({"source": "record", "order": [order]}));
    }
}

#[test]
fn every_predicate_spelling_round_trips() {
    let mut predicates = vec![
        json!({"all": []}),
        json!({"any": []}),
        json!({"not": {"uid_eq": "r_subject"}}),
        json!({"quantity_lt": "1.25"}),
        json!({"quantity_lte": "1.25"}),
        json!({"quantity_gt": "1.25"}),
        json!({"quantity_gte": "1.25"}),
        json!({"quantity_eq": "1.25"}),
        json!({"uid_eq": "r_subject"}),
        json!({"occurrence_in": ["open"]}),
        json!({"kind_eq": "plain"}),
        json!({"slug_eq": "task"}),
        json!({"concept_in": "c_work"}),
        json!({"text_contains": "needle"}),
        json!({"state_in": ["open"]}),
        json!({"revision_eq": 1}),
        json!({"revision_lt": 2}),
        json!({"revision_lte": 2}),
        json!({"revision_gt": 2}),
        json!({"revision_gte": 2}),
        json!({"status_in": ["pending"]}),
        json!({"viewer_role_in": ["operator"]}),
        json!({"invitation_state_in": ["open"]}),
        json!({"person_eq": "r_person"}),
        json!({"unit_eq": "c_hour"}),
        json!({"window_end_before": "2026-09-08T00:00:00Z"}),
        json!({"window_end_after": "2026-09-08T00:00:00Z"}),
        json!({"at_since": "2026-09-07T00:00:00Z"}),
        json!({"at_before": "2026-09-08T00:00:00Z"}),
        json!({"classified_in": "c_class"}),
        json!({"cause_kind_eq": "user_edit"}),
        json!({"record_eq": "r_subject"}),
        json!({"near": {"of": "p_place", "meters": 125.5}}),
        json!({"organ_eq": "r_organ"}),
        json!({"organ_in": ["r_organ"]}),
    ];
    for direction in ["both", "out", "in"] {
        predicates.push(json!({
            "relation": {"kind": "assigned", "direction": direction, "other": "r_other"}
        }));
    }
    predicates.push(json!({"relation": {"kind": "assigned"}}));
    predicates.push(json!({
        "under": {"record": "r_root", "kind": "contains", "include_self": true}
    }));
    predicates.push(json!({"under": {"record": "r_root"}}));
    for field in ["start", "due"] {
        for op in ["eq", "lt", "lte", "gt", "gte", "exists"] {
            let value = if op == "exists" {
                Value::Null
            } else {
                json!("2026-09-07")
            };
            predicates.push(json!({
                "work_date": {"field": field, "op": op, "value": value}
            }));
        }
    }

    for predicate in predicates {
        let _: Predicate = round_trip(predicate);
    }
}

#[test]
fn optional_fields_and_include_defaults_remain_unchanged() {
    let minimal: Protein = round_trip(json!({"source": "record"}));
    assert!(minimal.filter.is_empty());
    assert!(minimal.fields.is_none());
    assert!(minimal.aggregate.is_none());
    assert!(minimal.order.is_empty());
    assert!(minimal.limit.is_none());
    assert!(minimal.include.facts.is_none());
    assert!(minimal.include.promises.is_none());
    assert!(minimal.include.links.is_none());
    assert!(minimal.include.threads.is_none());
    assert!(!minimal.include.availability);
    assert!(minimal.include.extension.is_none());
    assert!(minimal.include.projection.is_none());
    assert!(!minimal.include.contact);
    assert!(!minimal.include.conversations);
    assert!(!minimal.include.reference_reads);

    let protein: Protein = round_trip(json!({
        "source": "record",
        "where": [
            {"relation": {"kind": "assigned"}},
            {"under": {"record": "r_root"}},
            {"work_date": {"field": "due", "op": "exists"}},
            {"near": {"of": "p_office", "meters": 42.5}}
        ],
        "include": {
            "facts": {},
            "promises": {},
            "links": {},
            "threads": {},
            "availability": true,
            "extension": {"namespace": "work"},
            "projection": {"at": "2026-09-07T00:00:00Z"},
            "contact": true,
            "conversations": true,
            "reference_reads": true
        }
    }));
    assert_eq!(protein.filter.len(), 4);
    let Predicate::Relation {
        direction, other, ..
    } = &protein.filter[0]
    else {
        panic!()
    };
    assert_eq!(*direction, LinkDirection::Both);
    assert!(other.is_none());
    let Predicate::Under {
        kind, include_self, ..
    } = &protein.filter[1]
    else {
        panic!()
    };
    assert_eq!(kind, "part-of");
    assert!(!include_self);
    let Predicate::WorkDate { value, .. } = &protein.filter[2] else {
        panic!()
    };
    assert!(value.is_none());
    let Predicate::Near { meters, .. } = &protein.filter[3] else {
        panic!()
    };
    assert_eq!(*meters, 42.5);
    assert_eq!(protein.include.facts.unwrap().limit, 10);
    assert!(protein.include.promises.unwrap().state.is_empty());
    let links = protein.include.links.unwrap();
    assert!(links.kinds.is_empty());
    assert_eq!(links.direction, LinkDirection::Both);
    assert_eq!(links.depth, 0);
    assert_eq!(protein.include.threads.unwrap().messages_limit, 50);
    assert!(protein.include.availability);
    assert_eq!(protein.include.extension.unwrap().namespace, "work");
    assert_eq!(
        protein.include.projection.unwrap().at,
        "2026-09-07T00:00:00Z"
    );
    assert!(protein.include.contact);
    assert!(protein.include.conversations);
    assert!(protein.include.reference_reads);

    let explicit: Protein = round_trip(json!({
        "source": "fact",
        "include": {"facts": {"limit": 37}, "threads": {"messages_limit": 73}}
    }));
    assert_eq!(explicit.include.facts.unwrap().limit, 37);
    assert_eq!(explicit.include.threads.unwrap().messages_limit, 73);
}

#[test]
fn unknown_fields_are_rejected_at_every_structural_layer() {
    rejects_protein(json!({"source": "record", "unexpected": true}));
    rejects_protein(json!({
        "source": "record",
        "aggregate": {"op": "count", "by": "total", "unexpected": true}
    }));
    for include in [
        json!({"unexpected": true}),
        json!({"facts": {"limit": 1, "unexpected": true}}),
        json!({"promises": {"state": [], "unexpected": true}}),
        json!({"links": {"kinds": [], "unexpected": true}}),
        json!({"threads": {"messages_limit": 1, "unexpected": true}}),
        json!({"extension": {"namespace": "work", "unexpected": true}}),
        json!({"projection": {"at": "2026-09-07T00:00:00Z", "unexpected": true}}),
    ] {
        rejects_protein(json!({"source": "record", "include": include}));
    }
    rejects_protein(json!({
        "source": "record",
        "order": [{"link": {"kind": "part-of", "higher": "from", "unexpected": true}}]
    }));
}

#[test]
fn every_struct_predicate_rejects_unknown_fields_directly_and_when_nested() {
    for predicate in [
        json!({
            "relation": {"kind": "assigned", "direction": "out", "unexpected": true}
        }),
        json!({
            "under": {"record": "r_root", "include_self": true, "unexpected": true}
        }),
        json!({
            "work_date": {"field": "start", "op": "eq", "value": "2026-09-07", "unexpected": true}
        }),
        json!({"near": {"of": "p_office", "meters": 10.0, "unexpected": true}}),
    ] {
        assert!(serde_json::from_value::<Predicate>(predicate).is_err());
    }

    rejects_protein(json!({
        "source": "record",
        "where": [{
            "all": [{
                "any": [{
                    "not": {"relation": {"kind": "assigned", "unexpected": true}}
                }]
            }]
        }]
    }));
}

#[test]
fn role_policy_uses_the_same_strict_nested_predicate_grammar() {
    let valid: RolePolicy = round_trip(json!({
        "read": {
            "all": [
                {"relation": {"kind": "assigned", "direction": "both"}},
                {"not": {"under": {"record": "r_private", "include_self": true}}}
            ]
        },
        "grants": []
    }));
    assert!(matches!(valid.read, Predicate::All(_)));

    assert!(
        serde_json::from_value::<RolePolicy>(json!({
            "read": {
                "all": [{
                    "any": [{
                        "not": {"near": {"of": "p_office", "meters": 10.0, "unexpected": true}}
                    }]
                }]
            },
            "grants": []
        }))
        .is_err()
    );
}
