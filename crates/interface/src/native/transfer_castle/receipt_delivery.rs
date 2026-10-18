use super::*;
use model::FieldKind;
use receipts::action_form;
use ui::{Command, button};

pub(super) fn inbox_form(person: &str, before: Option<&str>) -> Form {
    action_form(
        person,
        "Receipt inbox",
        json!({"kind":"delivery","request":{"kind":"inbox","before":before}}),
    )
}

pub(super) fn inspect_form(person: &str) -> Form {
    let mut form = action_form(
        person,
        "Inspect a receipt Message",
        json!({"kind":"delivery","request":{"kind":"inspect","message":""}}),
    );
    form.field(
        "/request/request/message",
        "Message Record",
        FieldKind::Reference("record".into()),
    );
    form
}

pub(super) fn preview_form(person: &str, source: &Value, thread: &Value, artifact: &Value) -> Form {
    let expires = artifact
        .pointer("/value/value/publication/scope/expires_at")
        .or_else(|| artifact.pointer("/value/value/scope/expires_at"))
        .or_else(|| artifact.pointer("/value/value/expires_at"))
        .and_then(Value::as_i64)
        .unwrap_or(i64::MAX)
        .min(nucleus::execution::now().timestamp() + 86400);
    let mut form = action_form(
        person,
        "Preview artifact delivery",
        json!({"kind":"delivery","request":{"kind":"preview","request":{"source":source,"thread":thread["thread"],"recipients":thread["recipients"],"expires_at":chrono::DateTime::from_timestamp(expires,0).unwrap().to_rfc3339()}}}),
    );
    form.field(
        "/request/request/request/expires_at",
        "Delivery expiry (date and time with timezone)",
        FieldKind::Text,
    );
    form.review["Recipients"] = thread["recipients"].clone();
    form.review["Privacy"] = json!(
        "Review all accepted Organs in this conversation. Use a different conversation to change recipients. Private requests and reviews cannot go to unrelated Organs."
    );
    form
}

pub(super) fn send_form(person: &str, preview: &Value) -> Form {
    let mut form = action_form(
        person,
        "Send reviewed artifact",
        json!({"kind":"delivery","request":{"kind":"send","request":preview["request"],"reviewed_at":preview["reviewed_at"],"expected_projection":preview["expected_projection"]}}),
    );
    form.review = json!({"Exact outgoing artifact":preview["artifact"],"Privacy":"Send this attachment through normal Organ conversation exchange. It contains no local receipt Record or private Need mapping. A participation request deliberately includes its private role-opening references for its named approver.","Delivery":"Queued or delivered bytes do not confirm a physical contribution, settle a Transfer or approve sharing terms."});
    form
}

pub(super) fn options(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    person: &str,
    source: Value,
) {
    button(
        world,
        parent,
        owner,
        "Choose conversation for delivery…",
        Command::Open(action_form(
            person,
            "Choose artifact recipients",
            json!({"kind":"delivery","request":{"kind":"options","source":source}}),
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
        Some("delivery_inbox") => {
            for message in array(value, "messages") {
                button(
                    world,
                    parent,
                    owner,
                    &format!(
                        "{} · {}",
                        if message["outgoing"] == true {
                            "Outgoing"
                        } else {
                            "Incoming"
                        },
                        text(message, "label")
                    ),
                    Command::Open(action_form(
                        person,
                        "Inspect receipt Message",
                        json!({"kind":"delivery","request":{"kind":"inspect","message":message["message"]}}),
                    )),
                );
            }
            if array(value, "messages").is_empty() {
                crate::native::edit_mode::label(
                    world,
                    parent,
                    "No receipt Messages in this permitted inbox view.",
                    12.0,
                );
            }
            if value["partial"] == true {
                crate::native::edit_mode::label(
                    world,
                    parent,
                    "Partial inbox: showing at most 64 permitted Messages within this bounded scan.",
                    12.0,
                );
            }
            if let Some(before) = value["next_before"].as_str() {
                button(
                    world,
                    parent,
                    owner,
                    "Older receipt Messages",
                    Command::Open(inbox_form(person, Some(before))),
                );
            }
            button(
                world,
                parent,
                owner,
                "Newest receipt Messages",
                Command::Open(inbox_form(person, None)),
            );
        }
        Some("delivery_options") => {
            crate::native::edit_mode::label(
                world,
                parent,
                "Choose an accepted Organ conversation. Its entire current recipient list will be reviewed before sending.",
                13.0,
            );
            for thread in array(value, "threads") {
                button(
                    world,
                    parent,
                    owner,
                    &format!(
                        "{} · {} recipients",
                        text(thread, "label"),
                        array(thread, "recipients").len()
                    ),
                    Command::Open(preview_form(
                        person,
                        &value["source"],
                        thread,
                        &value["artifact"],
                    )),
                );
            }
            if array(value, "threads").is_empty() {
                crate::native::edit_mode::label(
                    world,
                    parent,
                    "No eligible conversation. Create or accept a conversation with the intended Organ first.",
                    12.0,
                );
            }
            if value["partial"] == true {
                crate::native::edit_mode::label(
                    world,
                    parent,
                    "Partial choices: the latest 64 conversation Threads were checked.",
                    12.0,
                );
            }
        }
        Some("delivery_preview") => {
            forms::review(world, parent, &value["artifact"], 0);
            button(
                world,
                parent,
                owner,
                "Send this exact artifact…",
                Command::Open(send_form(person, value)),
            );
        }
        Some("delivery") => {
            forms::review(world, parent, &value["status"], 0);
            forms::review(world, parent, &value["artifact"], 0);
            button(
                world,
                parent,
                owner,
                "Open Message",
                Command::Record(text(value, "message")),
            );
            let artifact = &value["artifact"]["artifact"];
            if value["status"]["expired"] != true && value["status"]["cancelled"] != true {
                if artifact["kind"] == "document"
                    && value["status"]["verification"]["state"] == "verified"
                {
                    button(
                        world,
                        parent,
                        owner,
                        "Import reviewed document…",
                        Command::Open(action_form(
                            person,
                            "Import received receipt",
                            json!({"kind":"delivery","request":{"kind":"import","message":value["message"],"expected_projection":value["hash"]}}),
                        )),
                    );
                } else if artifact["kind"] == "participation"
                    && artifact["value"]["terms"]["organ"]
                        == world.get::<View>(owner).unwrap().context["organ"]
                {
                    button(
                        world,
                        parent,
                        owner,
                        "Review original participation approval…",
                        Command::Open(receipts::participation_form(person, &artifact["value"])),
                    );
                }
            }
            if value["status"]["outgoing"] == true || value["status"]["queued"] == true {
                for (label, kind) in [
                    ("Refresh delivery status", "inspect"),
                    ("Retry permitted delivery…", "retry"),
                    ("Cancel further delivery…", "cancel"),
                ] {
                    button(
                        world,
                        parent,
                        owner,
                        label,
                        Command::Open(action_form(
                            person,
                            label,
                            json!({"kind":"delivery","request":{"kind":kind,"message":value["message"]}}),
                        )),
                    );
                }
            }
        }
        _ => return false,
    }
    true
}
