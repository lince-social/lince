#![cfg(feature = "models")]

use lince_interface::{calendar, frequency, karma, organ, queries::ProteinDraft, records};
use nucleus::karma::rule_field::{RuleFieldInput, RuleFieldKind};
use serde_json::{Value, json};

#[test]
fn query_drafts_preserve_sort_precedence_and_exact_quantities_across_hosts() {
    let mut draft = ProteinDraft::default();
    draft.query["where"] = json!([{"all":[{"quantity_eq":"9007199254740993.125"}]}]);
    draft.query["order"] = json!([{"asc":"due_date"},{"desc":"quantity"}]);
    draft.query["limit"] = json!("200");
    let encoded = serde_json::to_vec(&draft).unwrap();
    let restored: ProteinDraft = serde_json::from_slice(&encoded).unwrap();
    let compiled = restored.compile().unwrap();
    assert_eq!(compiled.limit, Some(200));
    let restored = ProteinDraft::from_protein(draft.name, draft.slug, compiled);
    assert_eq!(restored.query["where"], draft.query["where"]);
    assert_eq!(restored.query["order"], draft.query["order"]);
}

#[test]
fn malformed_queries_remain_drafts_and_cannot_be_subscribed() {
    let mut draft = ProteinDraft::default();
    for limit in [json!("unfinished"), json!(0), json!(-1)] {
        draft.query["limit"] = limit;
        assert!(draft.valid_storage());
        assert!(draft.compile().is_err());
    }
    draft.query["limit"] = Value::Null;
    draft.query["where"] = json!([{"all":[{"any":[]}]}]);
    assert!(draft.compile().is_err());
    draft.query["where"] = json!([{"text_contains":"x".repeat(131_073)}]);
    assert!(!draft.valid_storage());
    assert!(draft.compile().is_err());
}

#[test]
fn sharing_limits_do_not_turn_empty_named_fields_into_unrestricted_access() {
    assert_eq!(organ::scope(&json!("all"), "").unwrap(), Value::Null);
    assert_eq!(organ::scope(&json!("none"), "").unwrap(), json!([]));
    assert!(organ::scope(&json!("some"), " , ").is_err());
    assert!(organ::scope(&Value::Null, "body").is_err());
    let fields = organ::scope(&json!("some"), " head, body, head ").unwrap();
    assert_eq!(fields, json!(["body", "head"]));
    assert_eq!(organ::scope_text(&fields), "body, head");
    assert!(
        organ::FieldKind::Filter
            .parse(&Value::Null, "{broken")
            .is_err()
    );
    assert!(organ::FieldKind::Number.parse(&Value::Null, "-1").is_err());
}

#[test]
fn record_binding_uses_the_full_schema_and_protects_computed_properties() {
    for property in [
        "head",
        "body",
        "slug",
        "quantity",
        "assertions",
        "assignees",
        "start_date",
        "due_date",
        "estimate_min",
        "work_logs",
        "work_timer",
        "threads",
    ] {
        let mut binding = records::Binding::new(property);
        binding.editable = true;
        assert!(binding.valid(), "{property}");
    }
    for property in [
        "created_at",
        "updated_at",
        "kind",
        "spent_seconds",
        "running_since",
    ] {
        let mut binding = records::Binding::new(property);
        assert!(binding.valid(), "{property}");
        binding.editable = true;
        assert!(!binding.valid(), "{property}");
    }
    assert!(!records::Binding::new("password").valid());
    let source = records::Source::Organ("organ-a".into());
    let restored: records::Source =
        serde_json::from_value(serde_json::to_value(&source).unwrap()).unwrap();
    assert_eq!(restored, source);
    assert_ne!(restored, records::Source::Local);
}

#[test]
fn record_conflict_baselines_keep_extension_state_for_work_properties() {
    let row = json!({"head":"Olá", "quantity":"9007199254740993.125", "due_date":"2026-09-26", "extension":{"work":{"due":"2026-09-26"}}});
    assert_eq!(records::display(&row["quantity"]), "9007199254740993.125");
    assert_eq!(
        records::baseline(&row, "due_date")["extension"],
        row["extension"]
    );
    assert_eq!(records::baseline(&row, "head"), json!({"head":"Olá"}));
}

#[test]
fn linked_karma_fields_keep_their_identity_and_revision_when_drafts_are_saved() {
    let fields = RuleFieldKind::ALL.map(|kind| karma::SharedField {
        uid: format!("field-{kind:?}"),
        kind,
        source: "quantity(@daily)".into(),
        revision: 7,
    });
    let rule = karma::Rule {
        bindings: Vec::new(),
        record: String::new(),
        uid: "rule-a".into(),
        name: "Daily".into(),
        slug: "daily".into(),
        fields: fields.to_vec(),
        revision: 9,
        state: "active".into(),
    };
    let draft = karma::Draft::from_rule(&rule);
    let restored: karma::Draft =
        serde_json::from_slice(&serde_json::to_vec(&draft).unwrap()).unwrap();
    assert!(restored.valid());
    assert_eq!(restored.revision, Some(9));
    for (field, original) in restored.fields.iter().zip(fields) {
        let RuleFieldInput::Reference { uid, revision } = field.input() else {
            panic!("Shared field was copied");
        };
        assert_eq!(uid, original.uid);
        assert_eq!(revision, 7);
    }
    assert_eq!(
        karma::insert_at("ação", 2..2, "@daily", RuleFieldKind::Condition),
        "ação"
    );
}

#[test]
fn frequency_drafts_preserve_definitions_and_refuse_incomplete_time_input() {
    let mut draft = frequency::Draft::default();
    draft.fields = [
        "daily".into(),
        "Daily work".into(),
        "1 day + 100ms".into(),
        "2026-09-26T09:00:00-03:00".into(),
    ];
    let definition = draft.definition().unwrap();
    let restored: frequency::Draft =
        serde_json::from_slice(&serde_json::to_vec(&draft).unwrap()).unwrap();
    assert_eq!(restored.definition().unwrap(), definition);
    for input in ["0 days", "1 day +", "4294967295ms + 1ms", "1 day + 🐈"] {
        draft.fields[2] = input.into();
        assert!(draft.definition().is_err(), "{input}");
    }
    draft.fields[2] = "1 day".into();
    draft.fields[3] = "2026-09-26T09:00:00".into();
    assert!(draft.definition().is_err());
}

#[test]
fn calendar_input_rejects_invalid_dates_and_reversed_ranges() {
    assert!(calendar::parse("2026-02-29").is_none());
    assert!(calendar::parse("2024-02-29").is_some());
    let mut calendar = calendar::Calendar::default();
    calendar.select("2026-09-26").unwrap();
    calendar.selecting_end = true;
    assert!(calendar.select("2026-09-25").is_err());
    assert!(calendar.end.is_none());
    calendar.select("2026-09-27").unwrap();
    assert!(calendar.valid());
}

#[test]
fn calendar_places_timed_work_on_dates_in_its_own_timezone() {
    let timed = json!({
        "start_date": "2026-10-03T01:30:00Z",
        "due_date": "2026-10-04T00:30:00Z"
    });
    assert_eq!(
        calendar::span_in(&timed, "America/Sao_Paulo"),
        Some((
            calendar::parse("2026-10-02").unwrap(),
            calendar::parse("2026-10-03").unwrap()
        ))
    );
    assert_eq!(
        calendar::span(&timed),
        Some((
            calendar::parse("2026-10-03").unwrap(),
            calendar::parse("2026-10-04").unwrap()
        ))
    );
    let all_day = json!({"due_date": "2026-10-03"});
    assert_eq!(
        calendar::span_in(&all_day, "America/Sao_Paulo"),
        calendar::span(&all_day)
    );
    assert!(
        calendar::span_in(&json!({"start_date": "2026-10-03T01:30:00"}), "UTC").is_none()
    );
}
