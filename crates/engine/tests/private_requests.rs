use std::collections::BTreeSet;

use engine::private_requests::{
    Command, DIGEST_DOMAIN, FieldChange, QuantityChange, RequestError, RequestLimits,
    ValidatedRequest, decode, ordinary_kind,
};
use nucleus::{DecimalValue, RecordKind};
use protein::authority::{AssertionRole, ExtensionProperty, Operation, Property};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const ORGAN: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FA1";
const PERSON: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FA2";
const RECORD: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FA3";
const OTHER: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FA4";
const CONCEPT: &str = "c_01ARZ3NDEKTSV4RRFFQ69G5FA5";
const UNIT: &str = "c_01ARZ3NDEKTSV4RRFFQ69G5FA6";
const PLACE: &str = "pl_01ARZ3NDEKTSV4RRFFQ69G5FA7";
const ASSERTION: &str = "a_01ARZ3NDEKTSV4RRFFQ69G5FA8";
const SECOND_ASSERTION: &str = "a_01ARZ3NDEKTSV4RRFFQ69G5FA9";
const OPERATION: &str = "op_01ARZ3NDEKTSV4RRFFQ69G5FAB";

fn exact(value: &str, scale: u8) -> Value {
    json!({"scale": scale, "value": value})
}

fn envelope(commands: Vec<Value>, revisions: &[&str]) -> Value {
    json!({
        "version": 1,
        "expected_organ_uid": ORGAN,
        "expected_person_uid": PERSON,
        "operation_uid": OPERATION,
        "expected_revisions": revisions.iter().map(|uid| json!({"record_uid": uid, "revision": 1})).collect::<Vec<_>>(),
        "commands": commands,
    })
}

fn create() -> Value {
    json!({"command": "create_record", "uid": RECORD, "kind": "plain", "head": "Knowledge", "body": "", "quantity": exact("1", 0)})
}

fn update(changes: Vec<Value>) -> Value {
    json!({"command": "update_record", "uid": RECORD, "changes": changes})
}

fn body(value: &str) -> Value {
    json!({"property": "body", "value": value})
}

fn ext(namespace: &str, field: &str, value: Value) -> Value {
    json!({"property": "extension", "namespace": namespace, "field": field, "change": {"change": "set", "value": value}})
}

fn insert() -> Value {
    json!({"command": "insert_assertion", "uid": ASSERTION, "subject_uid": RECORD, "predicate_uid": CONCEPT, "role": "ordinary"})
}

fn existing(command: &str) -> Value {
    json!({"command": command, "uid": ASSERTION, "expected_subject_uid": RECORD})
}

fn parse(value: &Value) -> Result<ValidatedRequest, RequestError> {
    parse_limits(value, &RequestLimits::default())
}

fn parse_limits(value: &Value, limits: &RequestLimits) -> Result<ValidatedRequest, RequestError> {
    decode(&serde_json::to_vec(value).unwrap(), ORGAN, PERSON, limits)
}

fn refusal(value: &Value, error: RequestError) {
    assert_eq!(parse(value).err(), Some(error));
}

fn extension_property(namespace: &str, field: &str) -> Property {
    Property::Extension(ExtensionProperty {
        namespace: namespace.into(),
        field: field.into(),
    })
}

#[test]
fn private_requests_create_has_complete_initial_intent_without_project() {
    let request = parse(&envelope(vec![create()], &[])).unwrap();
    assert_eq!(request.expected_organ_uid(), ORGAN);
    assert_eq!(request.expected_person_uid(), PERSON);
    assert_eq!(request.operation_uid(), OPERATION);
    assert!(request.expected_revisions().is_empty());
    let intent = &request.subjects()[RECORD];
    assert_eq!(intent.operation(), Operation::Create);
    assert_eq!(
        intent.properties(),
        &BTreeSet::from([
            Property::Kind,
            Property::Organ,
            Property::Head,
            Property::Body,
            Property::Quantity
        ])
    );
    assert_eq!(intent.expected_revision(), None);
    assert!(!intent.requires_stored_kind_check());
    assert!(!intent.requires_protein_kind());
    assert!(request.references().records().is_empty());
}

#[test]
fn private_requests_create_with_required_assertions_is_one_subject() {
    let mut record = create();
    record["slug"] = json!("knowledge.first");
    record["unit_uid"] = json!(UNIT);
    record["place_uid"] = json!(PLACE);
    record["extensions"] = json!([{"namespace": "work", "field": "estimate", "change": {"change": "set", "value": 1.25}}]);
    let mut assertion = insert();
    assertion["object_uid"] = json!(OTHER);
    assertion["quantity"] = exact("0.125", 3);
    assertion["unit_uid"] = json!(UNIT);
    let request = parse(&envelope(vec![assertion, record], &[])).unwrap();
    assert_eq!(request.subjects().len(), 1);
    assert_eq!(request.subjects()[RECORD].assertion_commands(), &[0]);
    for property in [
        Property::Slug,
        Property::Unit,
        Property::Place,
        extension_property("work", "estimate"),
    ] {
        assert!(request.subjects()[RECORD].properties().contains(&property));
    }
    assert_eq!(
        request.references().records(),
        &BTreeSet::from([OTHER.to_owned()])
    );
    assert_eq!(
        request.references().concepts(),
        &BTreeSet::from([CONCEPT.to_owned(), UNIT.to_owned()])
    );
    assert_eq!(
        request.references().places(),
        &BTreeSet::from([PLACE.to_owned()])
    );
    assert!(request.assertion_expectations().is_empty());
}

#[test]
fn private_requests_scalar_changes_and_explicit_clears_keep_all_properties() {
    let changes = vec![
        json!({"property": "head", "value": ""}),
        body(""),
        json!({"property": "slug", "change": {"change": "clear"}}),
        json!({"property": "quantity", "value": exact("0.00", 2)}),
        json!({"property": "unit", "change": {"change": "clear"}}),
        json!({"property": "place", "change": {"change": "clear"}}),
        json!({"property": "extension", "namespace": "work", "field": "due", "change": {"change": "clear"}}),
    ];
    let request = parse(&envelope(vec![update(changes)], &[RECORD])).unwrap();
    let intent = &request.subjects()[RECORD];
    assert_eq!(intent.operation(), Operation::Update);
    assert!(intent.requires_stored_kind_check());
    assert_eq!(intent.expected_revision(), Some(1));
    assert_eq!(
        intent.properties(),
        &BTreeSet::from([
            Property::Head,
            Property::Body,
            Property::Slug,
            Property::Quantity,
            Property::Unit,
            Property::Place,
            extension_property("work", "due")
        ])
    );
    assert!(request.references().concepts().is_empty());
}

#[test]
fn private_requests_changed_unit_place_and_slug_are_validated() {
    let request = parse(&envelope(
        vec![update(vec![
            json!({"property": "slug", "change": {"change": "set", "value": "work.first"}}),
            json!({"property": "unit", "change": {"change": "set", "value": UNIT}}),
            json!({"property": "place", "change": {"change": "set", "value": PLACE}}),
        ])],
        &[RECORD],
    ))
    .unwrap();
    assert_eq!(
        request.references().concepts(),
        &BTreeSet::from([UNIT.to_owned()])
    );
    assert_eq!(
        request.references().places(),
        &BTreeSet::from([PLACE.to_owned()])
    );
}

#[test]
fn private_requests_set_null_clear_and_absent_are_distinct() {
    let set_null = envelope(
        vec![update(vec![ext("work", "due", Value::Null)])],
        &[RECORD],
    );
    let mut clear = set_null.clone();
    clear["commands"][0]["changes"][0]["change"] = json!({"change": "clear"});
    assert_ne!(
        parse(&set_null).unwrap().digest(),
        parse(&clear).unwrap().digest()
    );
    let mut absent = set_null.clone();
    absent["commands"][0]["changes"][0]
        .as_object_mut()
        .unwrap()
        .remove("change");
    refusal(&absent, RequestError::InvalidRequest);
    let mut missing_value = set_null;
    missing_value["commands"][0]["changes"][0]["change"]
        .as_object_mut()
        .unwrap()
        .remove("value");
    refusal(&missing_value, RequestError::InvalidRequest);
}

#[test]
fn private_requests_delete_and_restore_are_distinct_current_operations() {
    for (command, operation) in [
        ("delete_record", Operation::Delete),
        ("restore_record", Operation::Restore),
    ] {
        let request = parse(&envelope(
            vec![json!({"command": command, "uid": RECORD})],
            &[RECORD],
        ))
        .unwrap();
        assert_eq!(request.subjects()[RECORD].operation(), operation);
        assert!(request.subjects()[RECORD].requires_stored_kind_check());
        assert!(request.subjects()[RECORD].properties().is_empty());
    }
}

#[test]
fn private_requests_assertion_only_writes_require_expected_subject_revision() {
    for command in ["retract_assertion", "promote_identity"] {
        let request = parse(&envelope(vec![existing(command)], &[RECORD])).unwrap();
        let intent = &request.subjects()[RECORD];
        assert_eq!(intent.operation(), Operation::Update);
        assert!(intent.requires_stored_kind_check());
        assert_eq!(intent.assertion_commands(), &[0]);
        assert!(intent.properties().is_empty());
        assert_eq!(request.assertion_expectations()[0].assertion_uid, ASSERTION);
        assert_eq!(
            request.assertion_expectations()[0].expected_subject_uid,
            RECORD
        );
        assert!(request.references().concepts().is_empty());
    }
}

#[test]
fn private_requests_existing_assertion_subject_is_only_an_expectation() {
    let mut command = existing("retract_assertion");
    command["expected_subject_uid"] = json!(OTHER);
    let request = parse(&envelope(vec![command], &[OTHER])).unwrap();
    assert!(request.subjects().contains_key(OTHER));
    assert!(!request.subjects().contains_key(RECORD));
    assert_eq!(
        request.assertion_expectations()[0].expected_subject_uid,
        OTHER
    );
}

#[test]
fn private_requests_assertion_quantity_and_unit_set_or_clear_are_explicit() {
    let mut command = existing("set_assertion_quantity");
    command["quantity"] = json!({"change": "set", "value": exact("9007199254740993.125", 3)});
    command["unit"] = json!({"change": "set", "value": UNIT});
    let request = parse(&envelope(vec![command.clone()], &[RECORD])).unwrap();
    let Command::SetAssertionQuantity {
        quantity: QuantityChange::Set { value },
        unit: FieldChange::Set { value: unit },
        ..
    } = &request.commands()[0]
    else {
        panic!("expected exact quantity command")
    };
    assert_eq!(value.mantissa(), 9007199254740993125);
    assert_eq!(unit, UNIT);
    assert_eq!(
        request.references().concepts(),
        &BTreeSet::from([UNIT.to_owned()])
    );
    command["quantity"] = json!({"change": "clear"});
    refusal(
        &envelope(vec![command.clone()], &[RECORD]),
        RequestError::InvalidRequest,
    );
    command["unit"] = json!({"change": "clear"});
    let request = parse(&envelope(vec![command], &[RECORD])).unwrap();
    assert_eq!(request.subjects()[RECORD].assertion_commands(), &[0]);
}

#[test]
fn private_requests_identity_insert_is_unary_and_unquantified() {
    let mut command = insert();
    command["role"] = json!("identity");
    let request = parse(&envelope(vec![command.clone()], &[RECORD])).unwrap();
    assert!(matches!(
        &request.commands()[0],
        Command::InsertAssertion {
            role: AssertionRole::Identity,
            quantity: None,
            object_uid: None,
            unit_uid: None,
            ..
        }
    ));
    for (field, value) in [
        ("object_uid", json!(OTHER)),
        ("quantity", exact("0", 0)),
        ("unit_uid", json!(UNIT)),
    ] {
        let mut invalid = command.clone();
        invalid[field] = value;
        refusal(
            &envelope(vec![invalid], &[RECORD]),
            RequestError::InvalidRequest,
        );
    }
}

#[test]
fn private_requests_insert_unit_requires_quantity_and_role_is_explicit() {
    let mut command = insert();
    command["unit_uid"] = json!(UNIT);
    refusal(
        &envelope(vec![command], &[RECORD]),
        RequestError::InvalidRequest,
    );
    for role in [Value::Null, json!("admin"), json!(1)] {
        let mut command = insert();
        command["role"] = role;
        refusal(
            &envelope(vec![command], &[RECORD]),
            RequestError::InvalidRequest,
        );
    }
}

#[test]
fn private_requests_update_and_assertions_share_one_revision_and_intent() {
    let request = parse(&envelope(
        vec![update(vec![body("same text")]), insert()],
        &[RECORD],
    ))
    .unwrap();
    assert_eq!(request.subjects().len(), 1);
    assert_eq!(
        request.subjects()[RECORD].properties(),
        &BTreeSet::from([Property::Body])
    );
    assert_eq!(request.subjects()[RECORD].assertion_commands(), &[1]);
}

#[test]
fn private_requests_noop_attempts_do_not_erase_required_properties_or_assertions() {
    let mut clear = ext("custom.data", "missing", Value::Null);
    clear["change"] = json!({"change": "clear"});
    let request = parse(&envelope(
        vec![update(vec![body(""), clear]), existing("promote_identity")],
        &[RECORD],
    ))
    .unwrap();
    assert_eq!(
        request.subjects()[RECORD].properties(),
        &BTreeSet::from([Property::Body, extension_property("custom.data", "missing")])
    );
    assert_eq!(request.subjects()[RECORD].assertion_commands(), &[1]);
}

#[test]
fn private_requests_plain_and_protein_are_the_only_ordinary_new_kinds() {
    for kind in [RecordKind::Plain, RecordKind::Protein] {
        assert!(ordinary_kind(kind));
        let mut command = create();
        command["kind"] = json!(kind);
        assert!(parse(&envelope(vec![command], &[])).is_ok());
    }
    for kind in [
        RecordKind::Person,
        RecordKind::Organ,
        RecordKind::Device,
        RecordKind::Message,
        RecordKind::MessageDraft,
        RecordKind::Conversation,
        RecordKind::Thread,
        RecordKind::ThreadInvite,
        RecordKind::Rule,
        RecordKind::Signal,
        RecordKind::Transfer,
        RecordKind::Decision,
        RecordKind::Sand,
        RecordKind::Program,
        RecordKind::Frequency,
        RecordKind::Grant,
        RecordKind::CallSession,
    ] {
        assert!(!ordinary_kind(kind));
        let mut command = create();
        command["kind"] = json!(kind);
        refusal(&envelope(vec![command], &[]), RequestError::UnsupportedKind);
    }
}

#[test]
fn private_requests_protein_fields_require_protein_kind_without_body_rewrite() {
    for field in [
        "source",
        "where",
        "fields",
        "include",
        "aggregate",
        "order",
        "limit",
    ] {
        let request = parse(&envelope(
            vec![update(vec![ext("lince.protein", field, Value::Null)])],
            &[RECORD],
        ))
        .unwrap();
        assert!(request.subjects()[RECORD].requires_protein_kind());
        assert_eq!(
            request.subjects()[RECORD].properties(),
            &BTreeSet::from([extension_property("lince.protein", field)])
        );
    }
    let mut record = create();
    record["extensions"] = json!([{"namespace": "lince.protein", "field": "source", "change": {"change": "set", "value": "record"}}]);
    refusal(
        &envelope(vec![record.clone()], &[]),
        RequestError::UnsupportedKind,
    );
    record["kind"] = json!("protein");
    assert!(
        parse(&envelope(vec![record], &[])).unwrap().subjects()[RECORD].requires_protein_kind()
    );
}

#[test]
fn private_requests_protected_extension_families_and_unknown_protein_fields_refuse() {
    for namespace in [
        "lince",
        "lince.person",
        "lince.pairing",
        "lince.protein.policy",
        "lince.message",
        "lince.message-draft",
        "lince.invite",
        "lince.roster",
        "lince.schedule.executor",
        "communication",
        "communication.v1",
        "communication.session.v1",
    ] {
        refusal(
            &envelope(
                vec![update(vec![ext(namespace, "active", json!(true))])],
                &[RECORD],
            ),
            RequestError::ProtectedProperty,
        );
    }
    refusal(
        &envelope(
            vec![update(vec![ext("lince.protein", "grants", json!([]))])],
            &[RECORD],
        ),
        RequestError::ProtectedProperty,
    );
}

#[test]
fn private_requests_extension_names_and_slugs_are_bounded_unambiguous() {
    for namespace in [
        "",
        " lince.person",
        "lince.person ",
        "Lince.person",
        "work..x",
        "work/field",
    ] {
        refusal(
            &envelope(vec![update(vec![ext(namespace, "x", json!(1))])], &[RECORD]),
            RequestError::InvalidRequest,
        );
    }
    for field in ["", "body.hidden", " x", "x ", "x\n"] {
        refusal(
            &envelope(vec![update(vec![ext("work", field, json!(1))])], &[RECORD]),
            RequestError::InvalidRequest,
        );
    }
    for slug in ["", "Upper", "a..b", "a/b"] {
        let mut record = create();
        record["slug"] = json!(slug);
        refusal(&envelope(vec![record], &[]), RequestError::InvalidRequest);
    }
    let name = "x".repeat(200);
    assert!(
        parse(&envelope(
            vec![update(vec![ext(&name, &name, json!(1))])],
            &[RECORD]
        ))
        .is_ok()
    );
    refusal(
        &envelope(
            vec![update(vec![ext(&format!("{name}x"), "x", json!(1))])],
            &[RECORD],
        ),
        RequestError::InvalidRequest,
    );
}

#[test]
fn private_requests_no_authority_or_arbitrary_patch_fields_are_accepted() {
    for field in [
        "actor",
        "organ_uid",
        "replica_root",
        "created_at",
        "updated_at",
        "revision",
        "facts",
        "policy",
        "allowed",
        "target_uids",
        "password",
        "signing_key",
    ] {
        let mut value = envelope(vec![create()], &[]);
        value[field] = json!("forged");
        refusal(&value, RequestError::InvalidRequest);
        let mut command = create();
        command[field] = json!("forged");
        refusal(&envelope(vec![command], &[]), RequestError::InvalidRequest);
    }
    for property in [
        "kind",
        "organ",
        "replica_root",
        "visibility",
        "role",
        "policy",
        "patch",
        "actor",
        "identity",
    ] {
        refusal(
            &envelope(
                vec![update(vec![
                    json!({"property": property, "value": "forged"}),
                ])],
                &[RECORD],
            ),
            RequestError::InvalidRequest,
        );
    }
    for command in [
        "act",
        "patch",
        "create_user",
        "create_message",
        "collab_update",
        "push_ops",
        "set_visibility",
    ] {
        refusal(
            &envelope(vec![json!({"command": command, "uid": RECORD})], &[RECORD]),
            RequestError::InvalidRequest,
        );
    }
}

#[test]
fn private_requests_nested_unknown_fields_are_not_silently_discarded() {
    let mut revision = envelope(vec![update(vec![body("x")])], &[RECORD]);
    revision["expected_revisions"][0]["extra"] = json!(true);
    refusal(&revision, RequestError::InvalidRequest);
    let mut decimal = create();
    decimal["quantity"]["extra"] = json!(true);
    refusal(&envelope(vec![decimal], &[]), RequestError::InvalidRequest);
    let change = json!({"property": "unit", "change": {"change": "clear", "value": UNIT}});
    refusal(
        &envelope(vec![update(vec![change])], &[RECORD]),
        RequestError::InvalidRequest,
    );
    let mut extension = ext("work", "estimate", json!(1));
    extension["extra"] = json!(true);
    refusal(
        &envelope(vec![update(vec![extension])], &[RECORD]),
        RequestError::InvalidRequest,
    );
    let mut command = existing("set_assertion_quantity");
    command["quantity"] = json!({"change": "set", "value": {"scale": 0, "value": "1", "extra": 1}});
    command["unit"] = json!({"change": "clear"});
    refusal(
        &envelope(vec![command], &[RECORD]),
        RequestError::InvalidRequest,
    );
}

#[test]
fn private_requests_field_clear_rejects_unknown_members_in_every_command_position() {
    let mut cases = Vec::new();
    for property in ["slug", "unit", "place", "extension"] {
        let mut change = json!({"property": property, "change": {"change": "clear"}});
        if property == "extension" {
            change["namespace"] = json!("work");
            change["field"] = json!("due");
        }
        cases.push((
            envelope(vec![update(vec![change])], &[RECORD]),
            "/commands/0/changes/0/change",
            None,
        ));
    }
    let mut creation = create();
    creation["extensions"] = json!([
        {"namespace": "work", "field": "due", "change": {"change": "clear"}}
    ]);
    cases.push((
        envelope(vec![creation], &[]),
        "/commands/0/extensions/0/change",
        Some(RequestError::ContradictoryCommands),
    ));
    let mut assertion = existing("set_assertion_quantity");
    assertion["quantity"] = json!({"change": "set", "value": exact("1", 0)});
    assertion["unit"] = json!({"change": "clear"});
    cases.push((
        envelope(vec![assertion], &[RECORD]),
        "/commands/0/unit",
        None,
    ));
    for (value, pointer, expected_error) in cases {
        assert_eq!(parse(&value).err(), expected_error, "{pointer}");
        if expected_error.is_none() {
            let request = parse(&value).unwrap();
            let roundtrip = decode(
                request.canonical_bytes(),
                ORGAN,
                PERSON,
                &RequestLimits::default(),
            )
            .unwrap();
            assert_eq!(request.canonical_bytes(), roundtrip.canonical_bytes());
            assert_eq!(request.digest(), roundtrip.digest());
        }
        for (key, extra) in [("value", Value::Null), ("extra", json!({"nested": true}))] {
            let mut malformed = value.clone();
            malformed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(key.into(), extra);
            refusal(&malformed, RequestError::InvalidRequest);
        }
    }
}

#[test]
fn private_requests_quantity_clear_rejects_unknown_members_without_changing_valid_roundtrip() {
    let mut assertion = existing("set_assertion_quantity");
    assertion["quantity"] = json!({"change": "clear"});
    assertion["unit"] = json!({"change": "clear"});
    let value = envelope(vec![assertion], &[RECORD]);
    let request = parse(&value).unwrap();
    let roundtrip = decode(
        request.canonical_bytes(),
        ORGAN,
        PERSON,
        &RequestLimits::default(),
    )
    .unwrap();
    assert_eq!(request.canonical_bytes(), roundtrip.canonical_bytes());
    assert_eq!(request.digest(), roundtrip.digest());
    assert!(matches!(
        &request.commands()[0],
        Command::SetAssertionQuantity {
            quantity: QuantityChange::Clear {},
            unit: FieldChange::Clear {},
            ..
        }
    ));
    for (key, extra) in [("value", exact("1", 0)), ("extra", json!({"nested": true}))] {
        let mut malformed = value.clone();
        malformed["commands"][0]["quantity"]
            .as_object_mut()
            .unwrap()
            .insert(key.into(), extra);
        refusal(&malformed, RequestError::InvalidRequest);
    }
}

#[test]
fn private_requests_empty_clear_variants_keep_the_exact_existing_wire_shape() {
    let wire = r#"{"change":"clear"}"#;
    let field: FieldChange<String> = serde_json::from_str(wire).unwrap();
    let extension: FieldChange<Value> = serde_json::from_str(wire).unwrap();
    let quantity: QuantityChange = serde_json::from_str(wire).unwrap();
    assert_eq!(field, FieldChange::Clear {});
    assert_eq!(extension, FieldChange::Clear {});
    assert_eq!(quantity, QuantityChange::Clear {});
    assert_eq!(serde_json::to_string(&field).unwrap(), wire);
    assert_eq!(serde_json::to_string(&extension).unwrap(), wire);
    assert_eq!(serde_json::to_string(&quantity).unwrap(), wire);
}

#[test]
fn private_requests_duplicate_wire_fields_fail_at_every_depth() {
    let value = envelope(
        vec![update(vec![ext(
            "work",
            "x",
            json!({"nested": {"unique": 1}}),
        )])],
        &[RECORD],
    );
    let raw = serde_json::to_string(&value).unwrap();
    for (needle, replacement) in [
        ("\"version\":1", "\"version\":1,\"version\":1"),
        ("\"revision\":1", "\"revision\":1,\"revision\":1"),
        (
            "\"command\":\"update_record\"",
            "\"command\":\"update_record\",\"command\":\"update_record\"",
        ),
        (
            "\"namespace\":\"work\"",
            "\"namespace\":\"work\",\"namespace\":\"work\"",
        ),
        ("\"unique\":1", "\"unique\":1,\"unique\":2"),
        ("\"unique\":1", "\"unique\":1,\"uniqu\\u0065\":2"),
    ] {
        assert!(raw.contains(needle));
        let malformed = raw.replacen(needle, replacement, 1);
        assert_eq!(
            decode(
                malformed.as_bytes(),
                ORGAN,
                PERSON,
                &RequestLimits::default()
            )
            .err(),
            Some(RequestError::InvalidJson)
        );
    }
}

#[test]
fn private_requests_duplicate_decimal_fields_fail_before_typed_parsing() {
    let raw = serde_json::to_string(&envelope(vec![create()], &[]))
        .unwrap()
        .replace("\"scale\":0", "\"scale\":0,\"scale\":0");
    assert_eq!(
        decode(raw.as_bytes(), ORGAN, PERSON, &RequestLimits::default()).err(),
        Some(RequestError::InvalidJson)
    );
}

#[test]
fn private_requests_trailing_invalid_utf8_and_non_json_inputs_refuse() {
    let raw = serde_json::to_vec(&envelope(vec![create()], &[])).unwrap();
    for suffix in [b"{}".as_slice(), b" true", &[0xff]] {
        let mut input = raw.clone();
        input.extend_from_slice(suffix);
        assert_eq!(
            decode(&input, ORGAN, PERSON, &RequestLimits::default()).err(),
            Some(RequestError::InvalidJson)
        );
    }
    for input in [
        b"".as_slice(),
        b"null",
        b"[]",
        b"{",
        b"{\"x\":NaN}",
        b"{\"x\":Infinity}",
    ] {
        assert!(decode(input, ORGAN, PERSON, &RequestLimits::default()).is_err());
    }
}

#[test]
fn private_requests_expected_scope_is_equality_not_an_identity_override() {
    let value = envelope(vec![create()], &[]);
    let raw = serde_json::to_vec(&value).unwrap();
    assert_eq!(
        decode(&raw, OTHER, PERSON, &RequestLimits::default()).err(),
        Some(RequestError::ScopeMismatch)
    );
    assert_eq!(
        decode(&raw, ORGAN, OTHER, &RequestLimits::default()).err(),
        Some(RequestError::ScopeMismatch)
    );
    let request = parse(&value).unwrap();
    assert_eq!(
        request.require_scope(OTHER, PERSON),
        Err(RequestError::ScopeMismatch)
    );
    assert!(request.require_scope(ORGAN, PERSON).is_ok());
}

#[test]
fn private_requests_all_wire_identity_classes_are_canonical() {
    for invalid in [
        "",
        "r_bad",
        "r_81ARZ3NDEKTSV4RRFFQ69G5FA3",
        "r_01arz3ndektsv4rrffq69g5fa3",
        "r_01ARZ3NDEKTSV4RRFFQ69G5FAI",
        "r_01ARZ3NDEKTSV4RRFFQ69G5FA3 ",
    ] {
        let mut command = create();
        command["uid"] = json!(invalid);
        refusal(&envelope(vec![command], &[]), RequestError::InvalidIdentity);
    }
    for (field, invalid) in [
        ("expected_person_uid", PERSON.replacen("r_", "p_", 1)),
        ("expected_organ_uid", ORGAN.replacen("r_", "o_", 1)),
        ("operation_uid", OPERATION.replacen("op_", "r_", 1)),
    ] {
        let mut value = envelope(vec![create()], &[]);
        value[field] = json!(invalid);
        refusal(&value, RequestError::InvalidIdentity);
    }
    for (field, invalid) in [
        ("uid", RECORD),
        ("subject_uid", ASSERTION),
        ("predicate_uid", RECORD),
        ("object_uid", CONCEPT),
        ("unit_uid", RECORD),
    ] {
        let mut command = insert();
        command["quantity"] = exact("1", 0);
        command[field] = json!(invalid);
        refusal(
            &envelope(vec![command], &[RECORD]),
            RequestError::InvalidIdentity,
        );
    }
    let mut value = create();
    value["place_uid"] = json!(CONCEPT);
    refusal(&envelope(vec![value], &[]), RequestError::InvalidIdentity);
}

#[test]
fn private_requests_accept_actual_generated_identity_prefixes() {
    let organ = nucleus::new_uid("r");
    let person = nucleus::new_uid("r");
    let mut value = envelope(vec![create()], &[]);
    value["expected_organ_uid"] = json!(organ);
    value["expected_person_uid"] = json!(person);
    value["operation_uid"] = json!(nucleus::new_uid("op"));
    value["commands"][0]["uid"] = json!(nucleus::new_uid("r"));
    assert!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            &organ,
            &person,
            &RequestLimits::default()
        )
        .is_ok()
    );
}

#[test]
fn private_requests_revision_coverage_is_exact_and_positive() {
    let command = update(vec![body("x")]);
    refusal(
        &envelope(vec![command.clone()], &[]),
        RequestError::RevisionCoverage,
    );
    refusal(
        &envelope(vec![command.clone()], &[RECORD, OTHER]),
        RequestError::RevisionCoverage,
    );
    refusal(
        &envelope(vec![command.clone()], &[RECORD, RECORD]),
        RequestError::RevisionCoverage,
    );
    refusal(
        &envelope(vec![create()], &[RECORD]),
        RequestError::RevisionCoverage,
    );
    for revision in [json!(0), json!(-1)] {
        let mut value = envelope(vec![command.clone()], &[RECORD]);
        value["expected_revisions"][0]["revision"] = revision;
        refusal(&value, RequestError::RevisionCoverage);
    }
    for revision in [json!(1.0), json!("1"), json!(u64::MAX)] {
        let mut value = envelope(vec![command.clone()], &[RECORD]);
        value["expected_revisions"][0]["revision"] = revision;
        refusal(&value, RequestError::InvalidRequest);
    }
}

#[test]
fn private_requests_repeated_record_lifecycle_and_properties_refuse() {
    for second in [
        create(),
        update(vec![body("x")]),
        json!({"command": "delete_record", "uid": RECORD}),
        json!({"command": "restore_record", "uid": RECORD}),
    ] {
        refusal(
            &envelope(vec![create(), second], &[]),
            RequestError::ContradictoryCommands,
        );
    }
    refusal(
        &envelope(vec![update(vec![body("same"), body("same")])], &[RECORD]),
        RequestError::ContradictoryCommands,
    );
    refusal(
        &envelope(
            vec![update(vec![
                ext("work", "x", json!(1)),
                ext("work", "x", json!(2)),
            ])],
            &[RECORD],
        ),
        RequestError::ContradictoryCommands,
    );
    let mut command = create();
    command["extensions"] =
        json!([{"namespace": "work", "field": "x", "change": {"change": "clear"}}]);
    refusal(
        &envelope(vec![command], &[]),
        RequestError::ContradictoryCommands,
    );
}

#[test]
fn private_requests_duplicate_assertion_ids_tuples_and_identity_commands_refuse() {
    refusal(
        &envelope(vec![insert(), existing("retract_assertion")], &[RECORD]),
        RequestError::ContradictoryCommands,
    );
    let mut second = insert();
    second["uid"] = json!(SECOND_ASSERTION);
    refusal(
        &envelope(vec![insert(), second.clone()], &[RECORD]),
        RequestError::ContradictoryCommands,
    );
    let mut first = insert();
    first["role"] = json!("identity");
    second["role"] = json!("identity");
    second["predicate_uid"] = json!(UNIT);
    refusal(
        &envelope(vec![first, second], &[RECORD]),
        RequestError::ContradictoryCommands,
    );
    let mut promotion = existing("promote_identity");
    promotion["uid"] = json!(SECOND_ASSERTION);
    refusal(
        &envelope(vec![existing("promote_identity"), promotion], &[RECORD]),
        RequestError::ContradictoryCommands,
    );
}

#[test]
fn private_requests_no_assertion_side_effects_share_delete_or_restore() {
    for lifecycle in ["delete_record", "restore_record"] {
        refusal(
            &envelope(
                vec![json!({"command": lifecycle, "uid": RECORD}), insert()],
                &[RECORD],
            ),
            RequestError::ContradictoryCommands,
        );
    }
    for command in ["retract_assertion", "promote_identity"] {
        refusal(
            &envelope(vec![create(), existing(command)], &[]),
            RequestError::ContradictoryCommands,
        );
    }
}

#[test]
fn private_requests_exact_decimals_roundtrip_without_floating_point() {
    for (scale, value) in [
        (0, "170141183460469231731687303715884105727"),
        (0, "-170141183460469231731687303715884105728"),
        (3, "9007199254740993.125"),
        (9, "0.000000001"),
        (2, "0.00"),
    ] {
        let mut record = create();
        record["quantity"] = exact(value, scale);
        let request = parse(&envelope(vec![record], &[])).unwrap();
        let Command::CreateRecord { quantity, .. } = &request.commands()[0] else {
            panic!("expected create")
        };
        assert_eq!(
            *quantity,
            DecimalValue::parse_canonical(scale, value).unwrap()
        );
        let roundtrip = decode(
            request.canonical_bytes(),
            ORGAN,
            PERSON,
            &RequestLimits::default(),
        )
        .unwrap();
        assert_eq!(request.commands(), roundtrip.commands());
        assert_eq!(request.digest(), roundtrip.digest());
    }
}

#[test]
fn private_requests_malformed_or_lossy_quantities_refuse() {
    for quantity in [
        json!(1.25),
        json!("1.25"),
        exact("1e3", 0),
        exact("01", 0),
        exact("-0", 0),
        exact("1.0", 0),
        exact("1", 19),
        exact("170141183460469231731687303715884105728", 0),
        json!({"value": "1"}),
        json!({"scale": 0, "value": 1}),
    ] {
        let mut record = create();
        record["quantity"] = quantity;
        refusal(&envelope(vec![record], &[]), RequestError::InvalidRequest);
    }
}

#[test]
fn private_requests_general_extension_numbers_retain_integer_and_finite_float_data() {
    let value = json!({"integer": 9007199254740993_u64, "unsigned": u64::MAX, "signed": i64::MIN, "fraction": 1.25, "small": 1e-20, "negative_zero": -0.0});
    let request = parse(&envelope(
        vec![update(vec![ext("work", "estimate", value.clone())])],
        &[RECORD],
    ))
    .unwrap();
    let Command::UpdateRecord { changes, .. } = &request.commands()[0] else {
        panic!("expected update")
    };
    let engine::private_requests::RecordChange::Extension(change) = &changes[0] else {
        panic!("expected extension")
    };
    assert_eq!(change.change, FieldChange::Set { value });
    let roundtrip = decode(
        request.canonical_bytes(),
        ORGAN,
        PERSON,
        &RequestLimits::default(),
    )
    .unwrap();
    assert_eq!(request.digest(), roundtrip.digest());
}

#[test]
fn private_requests_float_canonical_roundtrip_is_independent_of_serde_roundtrip_feature() {
    let mut bits = 0x42db_e123_9876_5432_u64;
    let mut numbers = vec![f64::MAX, f64::MIN_POSITIVE, f64::from_bits(1), -0.0];
    for _ in 0..128 {
        bits = bits.wrapping_mul(6364136223846793005).wrapping_add(1);
        let number = f64::from_bits(bits);
        if number.is_finite() {
            numbers.push(number);
        }
    }
    for number in numbers {
        let request = parse(&envelope(
            vec![update(vec![ext("work", "number", json!(number))])],
            &[RECORD],
        ))
        .unwrap();
        let Command::UpdateRecord { changes, .. } = &request.commands()[0] else {
            panic!("expected update")
        };
        let engine::private_requests::RecordChange::Extension(change) = &changes[0] else {
            panic!("expected extension")
        };
        let FieldChange::Set { value } = &change.change else {
            panic!("expected numeric value")
        };
        assert_eq!(value.as_f64().unwrap().to_bits(), number.to_bits());
        let roundtrip = decode(
            request.canonical_bytes(),
            ORGAN,
            PERSON,
            &RequestLimits::default(),
        )
        .unwrap();
        assert_eq!(request.canonical_bytes(), roundtrip.canonical_bytes());
    }
}

#[test]
fn private_requests_numeric_token_order_includes_nested_arrays_and_exponent_numbers() {
    let value = envelope(
        vec![update(vec![ext(
            "work",
            "mixed",
            json!([3, {"text": "123", "decimal": 0.1, "integer": -9}, 4.5]),
        )])],
        &[RECORD],
    );
    let request = parse(&value).unwrap();
    let canonical: Value = serde_json::from_slice(request.canonical_bytes()).unwrap();
    assert_eq!(
        canonical["commands"][0]["changes"][0]["change"]["value"],
        value["commands"][0]["changes"][0]["change"]["value"]
    );
}

#[test]
fn private_requests_numeric_preflight_preserves_strict_json_number_grammar() {
    let raw = serde_json::to_string(&envelope(
        vec![update(vec![ext("work", "x", json!("TOKEN"))])],
        &[RECORD],
    ))
    .unwrap();
    for token in [
        "01", "-01", "+1", ".1", "1.", "1e", "1e+", "1e-", "1-2", "--1", "1.2.3", "1e2e3", "1 2",
    ] {
        let input = raw.replace("\"TOKEN\"", token);
        assert_eq!(
            decode(input.as_bytes(), ORGAN, PERSON, &RequestLimits::default()).err(),
            Some(RequestError::InvalidJson)
        );
    }
    let input = raw.replace("\"TOKEN\"", "1.7976931348623158e308");
    let request = decode(input.as_bytes(), ORGAN, PERSON, &RequestLimits::default()).unwrap();
    let roundtrip = decode(
        request.canonical_bytes(),
        ORGAN,
        PERSON,
        &RequestLimits::default(),
    )
    .unwrap();
    assert_eq!(request.digest(), roundtrip.digest());
    let input = raw.replace("\"TOKEN\"", &format!("0.{}", "0".repeat(127)));
    assert_eq!(
        decode(input.as_bytes(), ORGAN, PERSON, &RequestLimits::default()).err(),
        Some(RequestError::LimitExceeded)
    );
}

#[test]
fn private_requests_nonfinite_and_out_of_range_integer_tokens_refuse() {
    let raw = serde_json::to_string(&envelope(
        vec![update(vec![ext("work", "x", json!("NUMBER_TOKEN"))])],
        &[RECORD],
    ))
    .unwrap();
    for token in [
        "1e309",
        "-1e309",
        "18446744073709551616",
        "-9223372036854775809",
        "NaN",
        "Infinity",
    ] {
        let input = raw.replace("\"NUMBER_TOKEN\"", token);
        assert_eq!(
            decode(input.as_bytes(), ORGAN, PERSON, &RequestLimits::default()).err(),
            Some(RequestError::InvalidJson)
        );
    }
    let request = parse(&envelope(
        vec![update(vec![body(
            "18446744073709551616 \"escaped\" \\ 1e309",
        )])],
        &[RECORD],
    ));
    assert!(request.is_ok());
}

#[test]
fn private_requests_canonical_ordering_and_float_spelling_are_stable() {
    let value = envelope(
        vec![update(vec![ext(
            "work",
            "x",
            json!({"z": 1.25, "a": {"last": 2, "first": 1}}),
        )])],
        &[RECORD],
    );
    let raw = serde_json::to_string(&value).unwrap();
    let reordered = raw
        .replace("\"first\":1,\"last\":2", "\"last\":2,\"first\":1")
        .replace("1.25", "125e-2");
    let request = parse(&value).unwrap();
    let other = decode(
        reordered.as_bytes(),
        ORGAN,
        PERSON,
        &RequestLimits::default(),
    )
    .unwrap();
    assert_eq!(request.canonical_bytes(), other.canonical_bytes());
    assert_eq!(request.digest(), other.digest());
    let hash: [u8; 32] = Sha256::new()
        .chain_update(DIGEST_DOMAIN)
        .chain_update(request.canonical_bytes())
        .finalize()
        .into();
    assert_eq!(*request.digest(), hash);
}

#[test]
fn private_requests_integer_and_float_representations_have_distinct_canonical_outcomes() {
    let raw = serde_json::to_string(&envelope(
        vec![update(vec![ext("work", "x", json!("TOKEN"))])],
        &[RECORD],
    ))
    .unwrap();
    let decode_token = |token: &str| {
        decode(
            raw.replace("\"TOKEN\"", token).as_bytes(),
            ORGAN,
            PERSON,
            &RequestLimits::default(),
        )
        .unwrap()
    };
    let integer = decode_token("1");
    let decimal = decode_token("1.0");
    let exponent = decode_token("1e0");
    assert_ne!(integer.canonical_bytes(), decimal.canonical_bytes());
    assert_ne!(integer.digest(), decimal.digest());
    assert_eq!(decimal.canonical_bytes(), exponent.canonical_bytes());
    assert_eq!(decode_token("-0").digest(), decode_token("0").digest());
    assert_ne!(decode_token("-0.0").digest(), decode_token("0.0").digest());
}

#[test]
fn private_requests_meaningful_array_and_command_order_remain_in_digest() {
    let first = envelope(
        vec![update(vec![ext("work", "sequence", json!([1, 2]))])],
        &[RECORD],
    );
    let second = envelope(
        vec![update(vec![ext("work", "sequence", json!([2, 1]))])],
        &[RECORD],
    );
    assert_ne!(
        parse(&first).unwrap().digest(),
        parse(&second).unwrap().digest()
    );
    let first = envelope(vec![update(vec![body("x")]), insert()], &[RECORD]);
    let second = envelope(vec![insert(), update(vec![body("x")])], &[RECORD]);
    assert_ne!(
        parse(&first).unwrap().digest(),
        parse(&second).unwrap().digest()
    );
}

#[test]
fn private_requests_revisions_are_canonicalized_as_an_unordered_unique_set() {
    let mut second = update(vec![body("other")]);
    second["uid"] = json!(OTHER);
    let first = envelope(vec![update(vec![body("first")]), second], &[RECORD, OTHER]);
    let mut reordered = first.clone();
    reordered["expected_revisions"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(
        parse(&first).unwrap().canonical_bytes(),
        parse(&reordered).unwrap().canonical_bytes()
    );
}

#[test]
fn private_requests_digest_binds_operation_scope_revisions_and_payload() {
    let original = envelope(vec![update(vec![body("text")])], &[RECORD]);
    let original_digest = *parse(&original).unwrap().digest();
    let mut variants = Vec::new();
    let mut changed = original.clone();
    changed["operation_uid"] = json!(OPERATION.replace("FAB", "FAC"));
    variants.push(changed);
    let mut changed = original.clone();
    changed["expected_revisions"][0]["revision"] = json!(2);
    variants.push(changed);
    let mut changed = original.clone();
    changed["commands"][0]["changes"][0]["value"] = json!("other");
    variants.push(changed);
    for changed in variants {
        assert_ne!(original_digest, *parse(&changed).unwrap().digest());
    }
    let mut changed = original.clone();
    changed["expected_person_uid"] = json!(OTHER);
    let request = decode(
        &serde_json::to_vec(&changed).unwrap(),
        ORGAN,
        OTHER,
        &RequestLimits::default(),
    )
    .unwrap();
    assert_ne!(original_digest, *request.digest());
    let mut changed = original;
    changed["expected_organ_uid"] = json!(OTHER);
    let request = decode(
        &serde_json::to_vec(&changed).unwrap(),
        OTHER,
        PERSON,
        &RequestLimits::default(),
    )
    .unwrap();
    assert_ne!(original_digest, *request.digest());
}

#[test]
fn private_requests_unicode_and_escaped_string_spellings_are_stable() {
    let value = envelope(vec![update(vec![body("é🦀")])], &[RECORD]);
    let raw = serde_json::to_string(&value)
        .unwrap()
        .replace("é🦀", "\\u00e9\\ud83e\\udd80");
    let first = parse(&value).unwrap();
    let second = decode(raw.as_bytes(), ORGAN, PERSON, &RequestLimits::default()).unwrap();
    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
}

#[test]
fn private_requests_input_and_canonical_byte_limits_are_exact() {
    let value = envelope(vec![create()], &[]);
    let bytes = serde_json::to_vec(&value).unwrap();
    let mut limits = RequestLimits {
        bytes: bytes.len(),
        ..RequestLimits::default()
    };
    assert!(parse_limits(&value, &limits).is_ok());
    limits.bytes -= 1;
    assert_eq!(
        parse_limits(&value, &limits).err(),
        Some(RequestError::LimitExceeded)
    );
    let length = parse(&value).unwrap().canonical_bytes().len();
    limits = RequestLimits {
        canonical_bytes: length,
        ..RequestLimits::default()
    };
    assert!(parse_limits(&value, &limits).is_ok());
    limits.canonical_bytes -= 1;
    assert_eq!(
        parse_limits(&value, &limits).err(),
        Some(RequestError::LimitExceeded)
    );
    assert_eq!(
        decode(
            &vec![b' '; RequestLimits::default().bytes + 1],
            ORGAN,
            PERSON,
            &RequestLimits::default()
        )
        .err(),
        Some(RequestError::LimitExceeded)
    );
}

fn json_size(value: &Value) -> (usize, usize) {
    let children: Vec<&Value> = match value {
        Value::Array(values) => values.iter().collect(),
        Value::Object(values) => values.values().collect(),
        _ => Vec::new(),
    };
    let sizes: Vec<_> = children.into_iter().map(json_size).collect();
    (
        1 + sizes.iter().map(|(nodes, _)| nodes).sum::<usize>(),
        1 + sizes
            .iter()
            .map(|(_, depth)| depth)
            .max()
            .copied()
            .unwrap_or(0),
    )
}

#[test]
fn private_requests_shared_json_node_and_depth_limits_are_exact() {
    let value = envelope(
        vec![update(vec![ext(
            "work",
            "nested",
            json!({"one": [{"two": true}]}),
        )])],
        &[RECORD],
    );
    let (nodes, depth) = json_size(&value);
    let mut limits = RequestLimits {
        nodes,
        depth,
        ..RequestLimits::default()
    };
    assert!(parse_limits(&value, &limits).is_ok());
    limits.nodes -= 1;
    assert_eq!(
        parse_limits(&value, &limits).err(),
        Some(RequestError::LimitExceeded)
    );
    limits.nodes = nodes;
    limits.depth -= 1;
    assert_eq!(
        parse_limits(&value, &limits).err(),
        Some(RequestError::LimitExceeded)
    );
}

#[test]
fn private_requests_deep_untrusted_objects_refuse_before_unbounded_recursion() {
    let value = envelope(
        vec![update(vec![ext("work", "nested", json!("DEEP_TOKEN"))])],
        &[RECORD],
    );
    let raw = serde_json::to_string(&value).unwrap().replace(
        "\"DEEP_TOKEN\"",
        &format!("{}null{}", "[".repeat(10000), "]".repeat(10000)),
    );
    assert_eq!(
        decode(raw.as_bytes(), ORGAN, PERSON, &RequestLimits::default()).err(),
        Some(RequestError::LimitExceeded)
    );
}

#[test]
fn private_requests_string_limits_count_utf8_bytes_and_object_keys() {
    let value = envelope(vec![update(vec![body(&"é".repeat(20))])], &[RECORD]);
    let mut limits = RequestLimits {
        string_bytes: 40,
        ..RequestLimits::default()
    };
    assert!(parse_limits(&value, &limits).is_ok());
    limits.string_bytes = 39;
    assert_eq!(
        parse_limits(&value, &limits).err(),
        Some(RequestError::LimitExceeded)
    );
    let mut nested = serde_json::Map::new();
    nested.insert("x".repeat(41), Value::Null);
    let value = envelope(
        vec![update(vec![ext("work", "x", Value::Object(nested))])],
        &[RECORD],
    );
    assert_eq!(
        parse_limits(&value, &limits).err(),
        Some(RequestError::LimitExceeded)
    );
}

#[test]
fn private_requests_command_subject_and_property_limits_apply_to_the_whole_batch() {
    let mut second = update(vec![body("other")]);
    second["uid"] = json!(OTHER);
    let value = envelope(vec![update(vec![body("first")]), second], &[RECORD, OTHER]);
    let mut limits = RequestLimits {
        commands: 2,
        subjects: 2,
        ..RequestLimits::default()
    };
    assert!(parse_limits(&value, &limits).is_ok());
    limits.commands = 1;
    assert_eq!(
        parse_limits(&value, &limits).err(),
        Some(RequestError::LimitExceeded)
    );
    limits.commands = 2;
    limits.subjects = 1;
    assert_eq!(
        parse_limits(&value, &limits).err(),
        Some(RequestError::LimitExceeded)
    );
    let value = envelope(
        vec![update(vec![
            body("x"),
            json!({"property": "head", "value": "x"}),
        ])],
        &[RECORD],
    );
    limits = RequestLimits {
        changes_per_record: 1,
        ..RequestLimits::default()
    };
    assert_eq!(
        parse_limits(&value, &limits).err(),
        Some(RequestError::LimitExceeded)
    );
    limits.changes_per_record = 2;
    assert!(parse_limits(&value, &limits).is_ok());
}

#[test]
fn private_requests_invalid_limits_do_not_disable_hard_caps() {
    let setters: [fn(&mut RequestLimits, usize); 8] = [
        |limits, n| limits.bytes = n,
        |limits, n| limits.canonical_bytes = n,
        |limits, n| limits.depth = n,
        |limits, n| limits.nodes = n,
        |limits, n| limits.string_bytes = n,
        |limits, n| limits.commands = n,
        |limits, n| limits.subjects = n,
        |limits, n| limits.changes_per_record = n,
    ];
    for setter in setters {
        for invalid in [0, usize::MAX] {
            let mut limits = RequestLimits::default();
            setter(&mut limits, invalid);
            assert_eq!(
                parse_limits(&envelope(vec![create()], &[]), &limits).err(),
                Some(RequestError::InvalidLimits)
            );
        }
    }
}

#[test]
fn private_requests_empty_batches_updates_and_unsupported_versions_refuse() {
    refusal(&envelope(vec![], &[]), RequestError::InvalidRequest);
    refusal(
        &envelope(vec![update(vec![])], &[RECORD]),
        RequestError::InvalidRequest,
    );
    let mut value = envelope(vec![create()], &[]);
    value["version"] = json!(2);
    refusal(&value, RequestError::UnsupportedVersion);
}

#[test]
fn private_requests_refusals_do_not_echo_private_input() {
    let mut value = envelope(vec![create()], &[]);
    value["password"] = json!("SECRET_DO_NOT_ECHO");
    let error = parse(&value).err().unwrap();
    assert!(!error.to_string().contains("SECRET_DO_NOT_ECHO"));
    assert!(!format!("{error:?}").contains("SECRET_DO_NOT_ECHO"));
}
