use super::*;
use model::FieldKind;
use ui::{Command, button};

pub(super) fn keys_form(subject: &str) -> Form {
    let mut form = Form::action(
        "Person signing keys",
        json!({"action":"identity","request":{"kind":"person_keys","subject":subject}}),
    );
    form.data.as_object_mut().unwrap().remove("request_id");
    form.field(
        "/request/subject",
        "Person profile Record",
        FieldKind::Reference("record".into()),
    );
    form.review = json!({"Identity":"Show already recognized signing keys for an existing readable Person profile. No key or profile is accepted automatically."});
    form
}

pub(super) fn preview_form(subject: &str) -> Form {
    let mut form = Form::action(
        "Compare a Person signing key",
        json!({"action":"identity","request":{"kind":"preview_person_key","key":{"subject":subject,"key_id":"","public_key":""}}}),
    );
    form.data.as_object_mut().unwrap().remove("request_id");
    for (path, label, kind) in [
        (
            "/request/key/subject",
            "Existing Person profile",
            FieldKind::Reference("record".into()),
        ),
        (
            "/request/key/key_id",
            "Signing key identifier",
            FieldKind::Text,
        ),
        (
            "/request/key/public_key",
            "Public key (standard base64)",
            FieldKind::Text,
        ),
    ] {
        form.field(path, label, kind);
    }
    form.review = json!({"Comparison":"Obtain this public key independently from the Person. Compare the fingerprint in the next review before accepting. Receive their ordinary Person profile Record first; this does not create profiles or change membership.","Authority":"Accepting a new local identity binding requires Organ administration. Recognized identity and reliability remain separate choices."});
    form
}

pub(super) fn accept_form(preview: &Value) -> Form {
    let mut form = Form::action(
        "Accept the compared Person key",
        json!({"action":"identity","request":{"kind":"accept_person_key","key":preview["key"],"expected_profile_revision":preview["profile_revision"],"expected_review":preview["review_hash"]}}),
    );
    form.data.as_object_mut().unwrap().remove("request_id");
    form.review = json!({"Public identity":preview["key"],"Compare independently":preview["fingerprint"],"Acceptance":"Recognize this exact signing key on this Lince. This does not establish reliability, membership authority or physical delivery. An existing identifier cannot be replaced with a different key."});
    form
}

pub(super) fn controls(world: &mut World, owner: Entity, parent: Entity, person: &str) {
    button(
        world,
        parent,
        owner,
        "Person signing keys…",
        Command::Open(keys_form(person)),
    );
    button(
        world,
        parent,
        owner,
        "Compare Person key…",
        Command::Open(preview_form(person)),
    );
}

pub(super) fn render(world: &mut World, owner: Entity, parent: Entity, document: &Value) -> bool {
    if document["kind"] != "identity" {
        return false;
    }
    let value = &document["value"];
    match value["kind"].as_str() {
        Some("person_key_preview") => {
            forms::review(
                world,
                parent,
                &json!({"Person signing identity":value["key"],"Compare independently":value["fingerprint"],"Meaning":value["meaning"]}),
                0,
            );
            button(
                world,
                parent,
                owner,
                "Accept compared key…",
                Command::Open(accept_form(value)),
            );
        }
        Some("person_key_accepted") => {
            crate::native::edit_mode::label(
                world,
                parent,
                "Person signing identity recognized. Reopen or refresh the receipt to verify it with this key.",
                13.0,
            );
            forms::review(world, parent, &value["key"], 0);
        }
        Some("person_keys") => {
            crate::native::edit_mode::label(world, parent, "Recognized Person signing keys", 15.0);
            for key in array(value, "keys") {
                forms::review(world, parent, key, 0);
                button(
                    world,
                    parent,
                    owner,
                    "Copy public key",
                    Command::CopyText(text(&key["key"], "public_key")),
                );
                button(
                    world,
                    parent,
                    owner,
                    "Copy key identifier",
                    Command::CopyText(text(&key["key"], "key_id")),
                );
            }
            if value["partial"] == true {
                crate::native::edit_mode::label(
                    world,
                    parent,
                    "Showing the first 32 key bindings.",
                    12.0,
                );
            }
            button(
                world,
                parent,
                owner,
                "Copy public key details",
                Command::CopyText(serde_json::to_string(value).unwrap_or_default()),
            );
        }
        _ => return false,
    }
    true
}
