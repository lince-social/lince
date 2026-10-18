use super::*;
use super::{self as model, FieldKind};
use serde_json::{Value, json};

pub fn choice(values: &[&str]) -> FieldKind {
    FieldKind::Choice(values.iter().map(|value| value.to_string()).collect())
}
pub fn reference(kind: &str) -> FieldKind {
    FieldKind::Reference(kind.into())
}

pub fn audience_fields(form: &mut Form, prefix: &str) {
    if let Some(audience) = form.data.pointer_mut(prefix).and_then(Value::as_object_mut) {
        if audience.get("kind").and_then(Value::as_str) == Some("selected") {
            audience.entry("viewers").or_insert_with(|| json!([]));
        } else {
            audience.remove("viewers");
        }
    }
    form.field(
        &format!("{prefix}/kind"),
        "Receipt audience",
        choice(&["public", "selected"]),
    );
    if form
        .data
        .pointer(&format!("{prefix}/kind"))
        .and_then(Value::as_str)
        == Some("selected")
    {
        form.field(
            &format!("{prefix}/viewers"),
            "Selected People/Organs (person:Record, organ:Record)",
            FieldKind::Identities,
        );
    }
}

fn history_fields(form: &mut Form) {
    if !form.data["public_history"].is_object() {
        return;
    }
    form.field(
        "/public_history/retention_seconds",
        "Public retention (for example 365d or 24h)",
        FieldKind::Duration,
    );
    form.field(
        "/public_history/agreed_quantity",
        "Disclose agreed quantity",
        choice(&["false", "true"]),
    );
    form.field(
        "/public_history/stages",
        "Agreed public report types",
        FieldKind::Selections(
            [
                ("need", "Receiver Need terms"),
                ("giver", "Giver statements"),
                ("receiver", "Receiver confirmations"),
                ("settled", "Canonical settlement"),
                ("settlement_status", "Partial/full settlement status"),
                ("cancelled", "Cancellation amount"),
                (
                    "compensated",
                    "Private compensation status without its effects",
                ),
                ("dispute", "Party disputes"),
            ]
            .into_iter()
            .map(|(value, label)| (value.into(), label.into()))
            .collect(),
        ),
    );
    form.field(
        "/public_history/measurements",
        "Exact public measurement proposals (same format as receipt review)",
        FieldKind::Text,
    );
    audience_fields(form, "/public_history/audience");
}

fn place_fields(form: &mut Form, prefix: &str) {
    if form.data.pointer(prefix).is_none_or(Value::is_null)
        && let Some(place) = form.data.pointer_mut(prefix)
    {
        *place = model::place();
    }
    form.field(
        &format!("{prefix}/address"),
        "Place / address",
        FieldKind::Optional,
    );
    form.field(
        &format!("{prefix}/lat"),
        "Latitude",
        FieldKind::OptionalNumber,
    );
    form.field(
        &format!("{prefix}/lon"),
        "Longitude",
        FieldKind::OptionalNumber,
    );
}

pub fn composer_fields(form: &mut Form) {
    form.fields.clear();
    match form.step.unwrap_or(0) {
        0 => {
            form.field("/head", "Title", FieldKind::Text);
            transport::fields(form);
            form.field("/slug", "Slug (optional)", FieldKind::Optional);
            form.field(
                "/agreement",
                "Agreement",
                choice(&["full", "individual", "percentage", "dependency"]),
            );
            if form.data["agreement"] == "percentage" {
                form.field("/agreement_pct", "Required percentage", FieldKind::Number);
            }
        }
        1 => {
            form.field("/creator", "Creator", reference("person"));
            form.field(
                "/invitees",
                "Invited People (comma-separated references)",
                FieldKind::People,
            );
        }
        2 => {
            for index in 0..array(&form.data, "promises").len() {
                let prefix = format!("/promises/{index}");
                if form.data["promises"][index]["item"].is_null() {
                    form.data["promises"][index]["item"] =
                        json!(nucleus::transfer::disclosure::TransferItem {
                            title: "Item".into(),
                            ..Default::default()
                        });
                }
                form.field(
                    &format!("{prefix}/item/title"),
                    "Item title",
                    FieldKind::Text,
                );
                form.field(
                    &format!("{prefix}/item/description"),
                    "Item description",
                    FieldKind::Text,
                );
                form.field(
                    &format!("{prefix}/record"),
                    &format!("Promise {} · Private source Record (optional)", index + 1),
                    reference("record"),
                );
                form.field(
                    &format!("{prefix}/open"),
                    "OPEN offer",
                    choice(&["false", "true"]),
                );
                if form.data["promises"][index]["open"] != true {
                    let private_commitment = form.data["agreement"] == "dependency"
                        && form.data["promises"][index]["item"]["exchange"].is_null();
                    let responsible = text(&form.data["promises"][index], "party");
                    let others = std::iter::once(&form.data["creator"])
                        .chain(array(&form.data, "invitees"))
                        .filter_map(Value::as_str)
                        .filter(|person| !person.is_empty() && *person != responsible)
                        .collect::<std::collections::BTreeSet<_>>();
                    let counterparty = if others.len() == 1 {
                        others.into_iter().next().unwrap().to_owned()
                    } else {
                        String::new()
                    };
                    let promise = &mut form.data["promises"][index];
                    if !private_commitment
                        && promise.pointer("/disclosed/parties") != Some(&Value::Bool(false))
                    {
                        if promise["item"]["exchange"].is_null() {
                            let outgoing = promise["delta"].as_f64().unwrap_or(-1.0) < 0.0;
                            let person = text(promise, "party");
                            promise["item"]["exchange"] = json!({"uid":nucleus::new_uid("exchange"),
                                "giver":if outgoing {person.as_str()} else {&counterparty}, "receiver":if outgoing {&counterparty} else {person.as_str()}});
                        }
                        form.field(
                            &format!("{prefix}/item/exchange/giver"),
                            "From",
                            reference("person"),
                        );
                        form.field(
                            &format!("{prefix}/item/exchange/receiver"),
                            "To",
                            reference("person"),
                        );
                    }
                    if private_commitment {
                        form.field(
                            &format!("{prefix}/party"),
                            "Responsible Person",
                            reference("person"),
                        );
                    }
                }
                let routed = form.data["promises"][index]
                    .pointer("/item/exchange")
                    .is_some_and(Value::is_object)
                    && form.data["promises"][index]["open"] != true;
                if routed && form.data["promises"][index]["amount"].is_null() {
                    form.data["promises"][index]["amount"] = json!(
                        form.data["promises"][index]["delta"]
                            .as_f64()
                            .unwrap_or(-1.0)
                            .abs()
                    );
                }
                form.field(
                    &format!("{prefix}/{}", if routed { "amount" } else { "delta" }),
                    if routed {
                        "Amount"
                    } else {
                        "Quantity (negative gives, positive receives)"
                    },
                    FieldKind::Number,
                );
                form.field(
                    &format!("{prefix}/unit"),
                    "Unit (optional)",
                    reference("unit"),
                );
                if form.data["promises"][index]["temporary"].is_null() {
                    form.data["promises"][index]["temporary"] =
                        json!(form.data["promises"][index]["item"]["loan"].is_object());
                }
                if form.data["promises"][index]["item"]["return_of"].is_null()
                    && form.data["promises"][index]["item"]["future_need_for"].is_null()
                {
                    form.field(
                        &format!("{prefix}/temporary"),
                        "Temporary loan",
                        choice(&["false", "true"]),
                    );
                    if form.data["promises"][index]["temporary"] == true {
                        if form.data["promises"][index]["item"]["loan"].is_null() {
                            form.data["promises"][index]["item"]["loan"] =
                                json!({"from":"", "until":""});
                        }
                        form.field(
                            &format!("{prefix}/item/loan/from"),
                            "Loan starts (date, time, timezone)",
                            FieldKind::Text,
                        );
                        form.field(
                            &format!("{prefix}/item/loan/until"),
                            "Loan ends (exclusive, with timezone)",
                            FieldKind::Text,
                        );
                    } else {
                        form.data["promises"][index]["item"]["loan"] = Value::Null;
                    }
                }
                form.field(
                    &format!("{prefix}/window_start"),
                    "Start (date, time, timezone)",
                    FieldKind::Optional,
                );
                form.field(
                    &format!("{prefix}/window_end"),
                    "Deadline (date, time, timezone)",
                    FieldKind::Optional,
                );
                form.field(
                    &format!("{prefix}/condition"),
                    "Condition",
                    FieldKind::Optional,
                );
                form.field(
                    &format!("{prefix}/reserve_from"),
                    "Reserve from",
                    choice(&["inherit", "none", "proposed", "agreed", "active"]),
                );
                form.field(
                    &format!("{prefix}/reuse_policy"),
                    "OPEN reuse",
                    choice(&["duplicate", "consume"]),
                );
                place_fields(form, &format!("{prefix}/place"));
            }
        }
        3 => {
            history_fields(form);
            for index in 0..array(&form.data, "promises").len() {
                for field in [
                    "title",
                    "description",
                    "source",
                    "parties",
                    "quantity",
                    "location",
                ] {
                    let path = format!("/promises/{index}/item/disclosure/{field}");
                    if form.data.pointer(&path).is_none() {
                        continue;
                    }
                    form.field(
                        &format!("{path}/scope"),
                        &format!("Item {} · Who can see {field}", index + 1),
                        choice(&["everyone", "participants", "owner", "selected"]),
                    );
                    if form
                        .data
                        .pointer(&format!("{path}/scope"))
                        .and_then(Value::as_str)
                        == Some("selected")
                    {
                        form.field(
                            &format!("{path}/people"),
                            "Allowed People (comma-separated references)",
                            FieldKind::People,
                        );
                    } else if let Some(people) = form.data.pointer_mut(&format!("{path}/people")) {
                        *people = json!([]);
                    }
                }
            }
            form.field(
                "/visibility",
                "Who can discover this transfer",
                choice(&["hidden", "public", "proximity"]),
            );
            if form.data["visibility"] == "proximity" {
                form.field("/max_proximity", "Maximum proximity", FieldKind::Number);
            }
            form.field(
                "/reserve_default",
                "Default reservation",
                choice(&["inherit", "none", "proposed", "agreed", "active"]),
            );
            form.field(
                "/require_confirmation",
                "Require delivery and receipt confirmation",
                choice(&["true", "false"]),
            );
            form.field(
                "/satiation",
                "Completion policy",
                choice(&["none", "first_completes"]),
            );
            form.field("/parent", "Parent transfer", reference("transfer"));
            form.field("/source", "Shared source", reference("record"));
            place_fields(form, "/default_place");
            for index in 0..array(&form.data, "dependencies").len() {
                let prefix = format!("/dependencies/{index}");
                form.field(
                    &format!("{prefix}/scope"),
                    &format!("Dependency {} · Scope", index + 1),
                    choice(&["transfer", "promise"]),
                );
                form.field(
                    &format!("{prefix}/promise"),
                    "Dependent promise (for promise scope)",
                    FieldKind::Optional,
                );
                form.field(
                    &format!("{prefix}/upstream_kind"),
                    "Upstream kind",
                    choice(&["transfer", "promise"]),
                );
                form.field(
                    &format!("{prefix}/upstream"),
                    "Upstream transfer or promise",
                    reference("transfer"),
                );
                let transfer_outcome =
                    form.data["dependencies"][index]["upstream_kind"] == "transfer";
                if transfer_outcome && form.data["dependencies"][index]["required_state"] == "kept"
                {
                    form.data["dependencies"][index]["required_state"] = json!("settled");
                }
                form.field(
                    &format!("{prefix}/required_state"),
                    "Required outcome",
                    if transfer_outcome {
                        choice(&["agreed", "settled"])
                    } else {
                        choice(&[
                            "kept",
                            "active",
                            "agreed",
                            "open",
                            "proposed",
                            "broken",
                            "withdrawn",
                        ])
                    },
                );
            }
        }
        _ => {}
    }
}
