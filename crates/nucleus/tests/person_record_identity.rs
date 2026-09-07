use nucleus::karma::{ReferenceKind, TypedUid};
use serde_json::json;

#[test]
fn person_record_identity_constructor_accepts_ordinary_record_ids() {
    for _ in 0..16 {
        let uid = nucleus::new_uid("r");
        assert!(nucleus::valid_uid(&uid, "r"));
        let person = TypedUid::new(ReferenceKind::Person, uid.clone()).unwrap();
        assert_eq!(person.kind(), ReferenceKind::Person);
        assert_eq!(person.as_str(), uid);
    }
}

#[test]
fn person_record_identity_serialization_preserves_person_kind_and_record_uid() {
    let uid = nucleus::new_uid("r");
    let person = TypedUid::new(ReferenceKind::Person, uid.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&person).unwrap(),
        json!({ "kind": "person", "uid": uid })
    );
    assert_eq!(
        serde_json::from_slice::<TypedUid>(&serde_json::to_vec(&person).unwrap()).unwrap(),
        person
    );
}

#[test]
fn person_record_identity_deserialization_accepts_record_uid_directly() {
    let uid = nucleus::new_uid("r");
    let person: TypedUid = serde_json::from_value(json!({ "kind": "person", "uid": uid })).unwrap();
    assert_eq!(person.kind(), ReferenceKind::Person);
    assert_eq!(person.as_str(), uid);
}

#[test]
fn person_record_identity_obsolete_person_prefix_is_not_an_alias() {
    let uid = nucleus::new_uid("p");
    assert!(TypedUid::new(ReferenceKind::Person, uid.clone()).is_err());
    assert!(serde_json::from_value::<TypedUid>(json!({ "kind": "person", "uid": uid })).is_err());
}

#[test]
fn person_record_identity_malformed_uids_refuse_construction_and_deserialization() {
    for uid in [
        "",
        "r_",
        "r_01APS3NDEKTSV4RRFFQ69G5FA",
        "r_01APS3NDEKTSV4RRFFQ69G5FAV0",
        "r_01aps3ndektsv4rrffq69g5fav",
        "r_01APS3NDEKTSV4RRFFQ69G5FAI",
        "r_01APS3NDEKTSV4RRFFQ69G5FAO",
        "r_01APS3NDEKTSV4RRFFQ69G5FAU",
        "r_01APS3NDEKTSV4RRFFQ69G5FAé",
        "r__01APS3NDEKTSV4RRFFQ69G5FAV",
        " r_01APS3NDEKTSV4RRFFQ69G5FAV",
        "r_01APS3NDEKTSV4RRFFQ69G5FAV ",
        "person_01APS3NDEKTSV4RRFFQ69G5FAV",
        "c_01APS3NDEKTSV4RRFFQ69G5FAV",
    ] {
        assert!(TypedUid::new(ReferenceKind::Person, uid).is_err(), "{uid}");
        assert!(
            serde_json::from_value::<TypedUid>(json!({ "kind": "person", "uid": uid })).is_err(),
            "{uid}"
        );
    }
}

#[test]
fn person_record_identity_promise_retains_its_separate_prefix() {
    let uid = nucleus::new_uid("p");
    let promise = TypedUid::new(ReferenceKind::Promise, uid.clone()).unwrap();
    assert_eq!(promise.kind(), ReferenceKind::Promise);
    assert_eq!(promise.as_str(), uid);
    assert_eq!(
        serde_json::from_value::<TypedUid>(json!({ "kind": "promise", "uid": uid })).unwrap(),
        promise
    );
    let record_uid = nucleus::new_uid("r");
    assert!(TypedUid::new(ReferenceKind::Promise, record_uid.clone()).is_err());
    assert!(
        serde_json::from_value::<TypedUid>(json!({ "kind": "promise", "uid": record_uid }))
            .is_err()
    );
}

#[test]
fn person_record_identity_other_reference_kinds_retain_their_prefixes() {
    for (kind, prefix) in [
        (ReferenceKind::Record, "r"),
        (ReferenceKind::Fact, "f"),
        (ReferenceKind::Organ, "r"),
        (ReferenceKind::Place, "pl"),
        (ReferenceKind::Concept, "c"),
        (ReferenceKind::Unit, "c"),
        (ReferenceKind::Link, "l"),
        (ReferenceKind::Promise, "p"),
        (ReferenceKind::Transfer, "r"),
        (ReferenceKind::Program, "r"),
        (ReferenceKind::ProgramRevision, "r"),
        (ReferenceKind::Node, "r"),
        (ReferenceKind::Signal, "r"),
        (ReferenceKind::Frequency, "r"),
        (ReferenceKind::Sense, "r"),
        (ReferenceKind::View, "r"),
        (ReferenceKind::Model, "r"),
        (ReferenceKind::Objective, "r"),
        (ReferenceKind::Workflow, "r"),
        (ReferenceKind::Grant, "r"),
        (ReferenceKind::TrustScope, "r"),
        (ReferenceKind::Run, "r"),
        (ReferenceKind::Candidate, "r"),
        (ReferenceKind::Decision, "r"),
        (ReferenceKind::Intent, "r"),
        (ReferenceKind::Receipt, "r"),
        (ReferenceKind::Simulation, "r"),
    ] {
        let uid = nucleus::new_uid(prefix);
        let typed = TypedUid::new(kind, uid.clone()).unwrap();
        assert_eq!(typed.kind(), kind);
        assert_eq!(typed.as_str(), uid);
        assert_eq!(
            serde_json::from_value::<TypedUid>(serde_json::to_value(&typed).unwrap()).unwrap(),
            typed
        );
        let wrong = nucleus::new_uid(if prefix == "r" { "f" } else { "r" });
        assert!(TypedUid::new(kind, wrong.clone()).is_err());
        assert!(serde_json::from_value::<TypedUid>(json!({ "kind": kind, "uid": wrong })).is_err());
    }
}

#[test]
fn person_record_identity_record_and_person_tags_share_identity_not_type() {
    let uid = nucleus::new_uid("r");
    let record = TypedUid::new(ReferenceKind::Record, uid.clone()).unwrap();
    let person = TypedUid::new(ReferenceKind::Person, uid).unwrap();
    assert_eq!(record.as_str(), person.as_str());
    assert_ne!(record, person);
}
