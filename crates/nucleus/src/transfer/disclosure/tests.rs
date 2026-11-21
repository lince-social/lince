use serde_json::json;

use super::{AudienceScope, FieldAudience, TransferItem, project_transfer};

#[test]
fn invalid_item_replaces_the_whole_projection_before_redaction() {
    let mut row = json!({
        "uid": "transfer",
        "proof": { "private": "signature" },
        "promises": [false, { "item": false }],
    });

    project_transfer(&mut row, None, false);

    assert_eq!(
        row,
        json!({
            "kind": "transfer", "uid": "transfer", "disclosure_error": "invalid_item",
        })
    );
}

#[test]
fn malformed_projection_rows_are_discarded_before_field_redaction() {
    let mut row = json!({"uid": "transfer", "promises": [true], "proof": "private"});
    project_transfer(&mut row, None, false);
    assert_eq!(
        row,
        json!({"kind": "transfer", "uid": "transfer", "disclosure_error": "invalid_item"}),
    );
}

#[test]
fn malformed_nested_progress_and_readiness_cannot_expose_details() {
    let mut item = TransferItem::default();
    item.disclosure.parties.scope = AudienceScope::Owner;
    let mut row = json!({
        "uid": "transfer", "promises": [{"uid": "p", "item": item}],
        "readiness": "private readiness", "settlement_progress": "private source",
    });
    project_transfer(&mut row, None, false);
    assert_eq!(row["readiness"], json!({"blockers": null}));
    assert_eq!(row["settlement_progress"], json!({"by_resource": null}));
}

#[test]
fn shared_simulations_keep_only_typed_authorized_information() {
    let body = json!({"kind":"transfer-simulation", "id":"share-1", "transfer":"transfer", "revision":1, "source_at_ms":10, "duration_ms":100,
        "assumptions":[{"source":{"transfer":"transfer","revision":1,"promise":"promise","exchange":"route","occurrence":null},"after_ms":50,"quantity":{"scale":0,"value":"4"}}],"result":null});
    let mut item = TransferItem {
        title: "Apples".into(),
        ..Default::default()
    };
    item.disclosure.quantity.scope = AudienceScope::Selected;
    item.disclosure.quantity.people = vec!["beto".into()];
    let row = json!({"uid":"transfer","revision":1,"promises":[{"uid":"promise","party":"ana","item":item}],
        "threads":[{"uid":"thread","head":"secret-title","messages":[
            {"uid":"message","body":body.to_string(),"references":["secret-record"]},
            {"uid":"other","body":"secret-text"}]}]});
    let mut beto = row.clone();
    project_transfer(&mut beto, Some("beto"), false);
    assert_eq!(beto["threads"][0]["messages"].as_array().unwrap().len(), 1);
    assert!(!beto.to_string().contains("secret-"));
    assert!(
        crate::simulation::sharing::Shared::parse(
            beto["threads"][0]["messages"][0]["body"].as_str().unwrap()
        )
        .is_some()
    );
    let mut stranger = row;
    project_transfer(&mut stranger, Some("dora"), false);
    assert_eq!(stranger["threads"], json!([]));
    let mut arbitrary = body;
    arbitrary["database"] = json!("secret-db");
    assert!(crate::simulation::sharing::Shared::parse(&arbitrary.to_string()).is_none());
}

#[test]
fn private_source_and_selected_fields_have_distinct_audiences() {
    let mut item = TransferItem {
        title: "City bike".into(),
        ..Default::default()
    };
    assert!(item.disclosure.title.allows(None, Some("ana"), false));
    assert!(
        !item
            .disclosure
            .source
            .allows(Some("beto"), Some("ana"), true)
    );
    assert!(
        item.disclosure
            .source
            .allows(Some("ana"), Some("ana"), false)
    );
    assert!(!item.disclosure.source.allows(None, None, false));
    item.disclosure.location = FieldAudience {
        scope: AudienceScope::Selected,
        people: vec!["courier".into()],
    };
    assert!(
        item.disclosure
            .location
            .allows(Some("courier"), Some("ana"), false)
    );
    assert!(
        !item
            .disclosure
            .location
            .allows(Some("beto"), Some("ana"), true)
    );
    assert!(item.validate().is_ok());
    item.disclosure.location.scope = AudienceScope::Everyone;
    assert!(item.validate().is_err());
}

#[test]
fn sharing_source_identity_never_shares_private_accounting() {
    let mut item = TransferItem {
        title: "Apples".into(),
        ..Default::default()
    };
    item.disclosure.source.scope = AudienceScope::Everyone;
    let mut row = json!({"promises":[{"uid":"p","item":item,"party":"beto",
        "record":"inventory","record_quantity":99,"availability":{"reserved":20},
        "application":{"formula":"private formula"}}]});
    project_transfer(&mut row, Some("beto"), false);
    assert_eq!(row["promises"][0]["record"], "inventory");
    assert!(row["promises"][0]["record_quantity"].is_null());
    assert!(row["promises"][0]["availability"].is_null());
    assert!(!row.to_string().contains("private formula"));
}

#[test]
fn redaction_removes_nested_history_and_keeps_independent_item_text() {
    let mut item = TransferItem {
        title: "City bike".into(),
        description: "Blue frame".into(),
        ..Default::default()
    };
    item.disclosure.parties.scope = AudienceScope::Owner;
    item.disclosure.quantity.scope = AudienceScope::Owner;
    item.disclosure.location.scope = AudienceScope::Selected;
    item.disclosure.location.people.push("courier".into());
    let original = json!({
        "uid":"transfer", "head":"A bike", "visibility":"public",
        "parties":[{"actor":"secret-person","actor_head":"secret-name"}],
        "promises":[{"uid":"promise","item":item,"party":"secret-person","record":"secret-record","record_head":"secret-head","record_quantity":99,"place":{"address":"Collection point"},"delta":17,"unit":"secret-unit","condition":"secret-condition","claim_pairs":["secret-pair"],"availability":{"quantity":99}}],
        "occurrences":[{"uid":"occurrence","promise":"promise","giver":"secret-person","receiver":"secret-receiver","record":"secret-record","quantity":17,"claim_history":["secret-person"],"settlement_progress":{"slices":["secret-record"],"remaining_quantity":17}}],
        "revision_evidence":{"terms":"secret-record"},"timeline":["secret-person"],"proof":{"action_intent":"secret-encoded-action"},
        "source":"secret-record", "balance":{"secret-unit":17}, "invitations":["secret-person"], "threads":["secret-thread"],
        "agreement":{"history":["secret-person"]}, "settlement_progress":{"by_resource":["secret-record"]}, "default_place":{"address":"Collection point"}
    });
    let mut public = original.clone();
    project_transfer(&mut public, None, false);
    assert_eq!(public["promises"][0]["title"], "City bike");
    assert_eq!(public["promises"][0]["description"], "Blue frame");
    assert!(!public.to_string().contains("secret-"), "{public}");
    assert!(!public.to_string().contains("Collection point"));
    assert!(public["promises"][0]["delta"].is_null());
    let mut courier = original.clone();
    project_transfer(&mut courier, Some("courier"), false);
    assert_eq!(
        courier["promises"][0]["place"]["address"],
        "Collection point"
    );
    assert!(!courier.to_string().contains("secret-"), "{courier}");
    let mut owner = original;
    project_transfer(&mut owner, Some("secret-person"), true);
    assert_eq!(owner["promises"][0]["record"], "secret-record");
}
#[test]
fn cancellation_terms_stay_reviewable_without_private_sources() {
    let mut item = TransferItem {
        title: "Apples".into(),
        ..Default::default()
    };
    let original = json!({"uid":"transfer","revision":3,"parties":[{"actor":"ana"},{"actor":"beto"}],
        "promises":[{"uid":"promise","party":"ana","record":"secret-stock","item":item}],
        "occurrences":[{"uid":"occurrence","promise":"promise","record":"secret-stock",
            "cancellations":[{"uid":"cancel","revision":3,"quantity":{"scale":0,"value":"6"},"status":"pending","private":"secret-stock","capabilities":{"apply_cancellation":true},
                "action_payloads":{"apply_cancellation":{"action":"apply-transfer-cancellation","person":"beto","transfer":"transfer","cancellation":"cancel","expected_revision":3,"record":"secret-stock"}}}],
            "action_payloads":{"propose_cancellation":{"action":"propose-transfer-cancellation","transfer":"transfer","occurrence":"occurrence","person":"beto","expected_revision":3,"expected_remaining_quantity":{"scale":0,"value":"6"},"record":"secret-stock"}}}]
    });
    let mut recipient = original.clone();
    project_transfer(&mut recipient, Some("beto"), false);
    assert_eq!(
        recipient["occurrences"][0]["cancellations"][0]["quantity"]["value"],
        "6"
    );
    assert_eq!(
        recipient["occurrences"][0]["cancellations"][0]["action_payloads"]["apply_cancellation"]["person"],
        "beto"
    );
    assert_eq!(
        recipient["occurrences"][0]["action_payloads"]["propose_cancellation"]["person"],
        "beto"
    );
    assert!(!recipient.to_string().contains("secret-stock"));
    item.disclosure.quantity.scope = AudienceScope::Owner;
    let mut public = original;
    public["promises"][0]["item"] = json!(item);
    project_transfer(&mut public, None, false);
    assert!(public["occurrences"][0]["cancellations"].is_null());
    assert!(public["occurrences"][0]["action_payloads"].is_null());
}
