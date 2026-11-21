#![cfg(feature = "models")]

use lince_interface::{calendar, frequency, karma, organ, queries::ProteinDraft, records};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, from_slice, from_str, from_value, json, to_value, to_vec};

const EXACT_QUANTITY: &str = "9007199254740993.125";

fn round_trip<T: Serialize + DeserializeOwned>(value: &T) -> T {
    from_slice(&to_vec(value).expect("serialize model fixture")).expect("deserialize model fixture")
}

#[test]
fn schedule_drafts_keep_the_window_outside_filter_groups() {
    let window = json!({"past_days":0,"future_days":30,"timezone":"America/Sao_Paulo"});
    let query = from_value(json!({
        "source": "schedule",
        "where": [{"rolling_window": window}, {"uid_eq": "shower"}],
        "order": [{"desc": "#task"}],
    }))
    .expect("read the rolling Schedule query");
    let draft = ProteinDraft::from_protein("Personal schedule".into(), String::new(), query);
    assert_eq!(draft.query["where"][0]["rolling_window"], window);
    assert!(draft.query["where"][1].get("all").is_some());
    let compiled = to_value(draft.compile().expect("compile the rolling Schedule query"))
        .expect("encode the rolling Schedule query");
    assert_eq!(compiled["where"][0]["rolling_window"], window);
    let fixed = from_str(
        r#"{"source":"schedule","where":[{"projection_window":{
            "from_ms":1800000000000,"until_ms":1800086400000,"timezone":"UTC"
        }}]}"#,
    )
    .expect("read the fixed Schedule query");
    ProteinDraft::from_protein("Fixed".into(), String::new(), fixed)
        .compile()
        .expect("compile the fixed Schedule query");
}

#[test]
fn query_drafts_preserve_sort_precedence_and_exact_quantities_across_hosts() {
    let mut draft = ProteinDraft::default();
    draft.query["where"] = json!([{"all": [{"quantity_eq": EXACT_QUANTITY}]}]);
    draft.query["order"] = json!([{"asc":"due_date"},{"desc":"quantity"}]);
    draft.query["limit"] = json!("200");
    let compiled = round_trip(&draft)
        .compile()
        .expect("compile the restored draft");
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
    assert_eq!(organ::scope(&json!("all"), ""), Ok(Value::Null));
    assert_eq!(organ::scope(&json!("none"), ""), Ok(json!([])));
    assert!(organ::scope(&json!("some"), " , ").is_err());
    assert!(organ::scope(&Value::Null, "body").is_err());
    let fields = organ::scope(&json!("some"), " head, body, head ").expect("read the named scope");
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
    let restored = round_trip(&source);
    assert_eq!(restored, source);
    assert_ne!(restored, records::Source::Local);
}

#[test]
fn record_conflict_baselines_keep_extension_state_for_work_properties() {
    let row = json!({
        "head": "Olá",
        "quantity": EXACT_QUANTITY,
        "due_date": "2026-09-26",
        "extension": {"work": {"due": "2026-09-26"}},
    });
    assert_eq!(records::display(&row["quantity"]), EXACT_QUANTITY);
    assert_eq!(
        records::baseline(&row, "due_date")["extension"],
        row["extension"]
    );
    assert_eq!(records::baseline(&row, "head"), json!({"head":"Olá"}));
}

#[test]
fn linked_karma_fields_keep_their_identity_and_revision_when_drafts_are_saved() {
    let fields = ["condition", "threshold", "consequence"].map(|kind| karma::SharedField {
        uid: format!("field-{kind}"),
        kind: from_value(Value::from(kind)).expect("read the Karma field kind"),
        source: "quantity(@daily)".into(),
        revision: 7,
    });
    let rule = karma::Rule {
        bindings: Vec::new(),
        record: String::new(),
        uid: "rule-a".into(),
        name: "Daily".into(),
        slug: "daily".into(),
        fields: fields.into(),
        revision: 9,
        state: "active".into(),
    };
    let restored = round_trip(&karma::Draft::from_rule(&rule));
    assert!(restored.valid());
    assert_eq!(restored.revision, Some(9));
    for (field, original) in restored.fields.iter().zip(&rule.fields) {
        assert_eq!(
            to_value(field.input()).expect("encode the shared field reference"),
            json!({"kind": "reference", "uid": original.uid, "revision": 7}),
        );
    }
    let condition = rule.fields.first().expect("the condition field").kind;
    assert_eq!(karma::insert_at("ação", 2..2, "@daily", condition), "ação");
}

#[test]
fn frequency_drafts_preserve_definitions_and_refuse_incomplete_time_input() {
    let mut draft = frequency::Draft {
        fields: [
            "daily",
            "Daily work",
            "1 day + 100ms",
            "2026-09-26T09:00:00-03:00",
        ]
        .map(str::to_owned),
        ..Default::default()
    };
    let definition = draft.definition().expect("compile the original frequency");
    assert_eq!(round_trip(&draft).definition(), Ok(definition));
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
    calendar
        .select("2026-09-26")
        .expect("select the start date");
    calendar.selecting_end = true;
    assert!(calendar.select("2026-09-25").is_err());
    assert!(calendar.end.is_none());
    calendar.select("2026-09-27").expect("select the end date");
    assert!(calendar.valid());
}

#[test]
fn calendar_places_timed_work_on_dates_in_its_own_timezone() {
    let timed = json!({
        "start_date": "2026-10-03T01:30:00Z",
        "due_date": "2026-10-04T00:30:00Z"
    });
    let (start, end) = calendar::span_in(&timed, "America/Sao_Paulo").expect("local date span");
    assert_eq!(start.to_string(), "2026-10-02");
    assert_eq!(end.to_string(), "2026-10-03");
    let (start, end) = calendar::span(&timed).expect("UTC date span");
    assert_eq!(start.to_string(), "2026-10-03");
    assert_eq!(end.to_string(), "2026-10-04");
    let all_day = json!({"due_date": "2026-10-03"});
    assert_eq!(
        calendar::span_in(&all_day, "America/Sao_Paulo"),
        calendar::span(&all_day)
    );
    assert!(calendar::span_in(&json!({"start_date": "2026-10-03T01:30:00"}), "UTC").is_none());
}
