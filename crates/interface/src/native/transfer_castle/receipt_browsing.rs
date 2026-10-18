use super::*;
use ui::{Command, button};

pub(super) fn versions_form(person: &str, record: &Value, before: Value) -> Form {
    let mut form = receipts::action_form(
        person,
        "Saved receipt versions",
        json!({"kind":"versions","record":record,"before":before}),
    );
    form.review = json!({"History":"List immutable saved versions. Each version is checked against its original document and current signing authority when opened."});
    form
}

pub(super) fn proof_form(person: &str, record: &Value, version: &Value, before: Value) -> Form {
    let mut form = receipts::action_form(
        person,
        "Original signed proof",
        json!({"kind":"proof","record":record,"version":version,"before":before}),
    );
    form.review = json!({"Evidence":"Read a bounded page from the selected original version. This view is not a portable receipt. Importing or exporting remains a separate reviewed action."});
    form
}

pub(super) fn controls(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    person: &str,
    receipt: &Value,
) {
    button(
        world,
        parent,
        owner,
        "Saved versions",
        Command::Open(versions_form(person, &receipt["record"], Value::Null)),
    );
    button(
        world,
        parent,
        owner,
        "Browse original signed proof",
        Command::Open(proof_form(
            person,
            &receipt["record"],
            &receipt["version"],
            Value::Null,
        )),
    );
}

pub(super) fn render(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    person: &str,
    document: &Value,
) -> bool {
    let value = &document["value"];
    match document["kind"].as_str() {
        Some("receipt_versions") => {
            crate::native::edit_mode::label(world, parent, "Saved receipt versions", 15.0);
            crate::native::edit_mode::label(world, parent, &text(value, "coverage"), 12.0);
            for version in array(value, "versions") {
                button(
                    world,
                    parent,
                    owner,
                    &format!(
                        "Version {} · {}",
                        text(version, "version"),
                        text(version, "saved_at")
                    ),
                    Command::Open(receipts::action_form(
                        person,
                        "Inspect original version",
                        json!({"kind":"inspect_version","record":value["record"],"version":version["version"]}),
                    )),
                );
            }
            if value["next_before"].is_string() {
                button(
                    world,
                    parent,
                    owner,
                    "Older versions",
                    Command::Open(versions_form(
                        person,
                        &value["record"],
                        value["next_before"].clone(),
                    )),
                );
            }
            button(
                world,
                parent,
                owner,
                "Newest versions",
                Command::Open(versions_form(person, &value["record"], Value::Null)),
            );
        }
        Some("receipt_proof") => {
            crate::native::edit_mode::label(
                world,
                parent,
                &format!(
                    "Original version {} · {}",
                    text(value, "version"),
                    model::human(&text(value, "document_state"))
                ),
                15.0,
            );
            crate::native::edit_mode::label(world, parent, &text(value, "coverage"), 12.0);
            detail::evidence(
                world,
                owner,
                parent,
                "receipt-proof-header",
                "Scope, approvals and signing authority",
                &value["header"],
            );
            for signed in array(value, "events") {
                let key = nucleus::karma::canonical_hash("lince.receipt.proof-view.v1", signed)
                    .map(|hash| hash.as_str().to_owned())
                    .unwrap_or_default();
                detail::evidence(world, owner, parent, &key, "Signed event", signed);
            }
            if value["next_before"].is_string() {
                button(
                    world,
                    parent,
                    owner,
                    "Next proof page",
                    Command::Open(proof_form(
                        person,
                        &value["record"],
                        &value["version"],
                        value["next_before"].clone(),
                    )),
                );
            }
            button(
                world,
                parent,
                owner,
                "First proof page",
                Command::Open(proof_form(
                    person,
                    &value["record"],
                    &value["version"],
                    Value::Null,
                )),
            );
            button(
                world,
                parent,
                owner,
                "Open this exact version",
                Command::Open(receipts::action_form(
                    person,
                    "Inspect original version",
                    json!({"kind":"inspect_version","record":value["record"],"version":value["version"]}),
                )),
            );
            button(
                world,
                parent,
                owner,
                "Saved versions",
                Command::Open(versions_form(person, &value["record"], Value::Null)),
            );
        }
        _ => return false,
    }
    true
}
