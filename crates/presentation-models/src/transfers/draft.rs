// non-lean-yet

use super::{Form, array, display, text, title, transport, workflows};
use serde_json::{Value, json};
use std::collections::HashSet;

pub fn promise(person: &str) -> Value {
    json!({"uid": nucleus::new_uid("p"), "record": "", "party": person,
        "item": nucleus::transfer::disclosure::TransferItem { title: "Item".into(), ..Default::default() },
        "open": person.is_empty(), "delta": -1, "unit": null, "window_start": null,
        "window_end": null, "place": place(), "condition": null,
        "reserve_from": "inherit", "reuse_policy": "duplicate"})
}

pub fn place() -> Value {
    json!({"lat": null, "lon": null, "address": null})
}

pub fn disclosure_preview(data: &Value, viewer: Option<&str>, draft: bool) -> Value {
    let mut row = data.clone();
    if draft {
        row["kind"] = json!("transfer");
        row["parties"] = json!(
            std::iter::once(data["creator"].clone())
                .chain(array(data, "invitees").iter().cloned())
                .map(|person| json!({"actor":person}))
                .collect::<Vec<_>>()
        );
    }
    let creator = if draft {
        data["creator"].as_str()
    } else {
        data.pointer("/revision_evidence/current/terms/parties")
            .and_then(Value::as_array)
            .and_then(|parties| parties.iter().find(|party| party["kind"] == "creator"))
            .and_then(|party| party["person_uid"].as_str())
    };
    nucleus::transfer::disclosure::project_transfer(
        &mut row,
        viewer,
        viewer.is_some() && viewer == creator,
    );
    if let Some(object) = row.as_object_mut() {
        object.retain(|key, _| {
            ["head", "visibility", "promises", "parties", "default_place"].contains(&key.as_str())
        });
    }
    row
}

pub fn review_draft(form: &Form) -> Value {
    let mut data = form.data.clone();
    for promise in data["promises"].as_array_mut().into_iter().flatten() {
        for (group, fields) in [
            ("title", &["/item/title"][..]),
            ("description", &["/item/description"][..]),
            ("source", &["/record", "/condition"][..]),
            ("parties", &["/party", "/open"][..]),
            ("quantity", &["/delta", "/amount", "/unit"][..]),
            ("location", &["/place"][..]),
        ] {
            if promise["disclosed"][group] == false {
                for field in fields {
                    if let Some(value) = promise.pointer_mut(field) {
                        *value = json!("Unavailable; unchanged");
                    }
                }
            }
        }
        if form.mode == "counteroffer-transfer" && !promise["uid"].is_null() {
            promise["record"] = json!("Private binding unchanged");
            promise["item"]["disclosure"] = json!("Private settings unchanged");
        }
    }
    data
}

pub fn dependency() -> Value {
    json!({"uid": null, "scope": "transfer", "promise": null,
        "upstream_kind": "transfer", "upstream": "", "required_state": "kept"})
}

pub fn composer(person: &str, preset: &str) -> Form {
    if let Some(form) = workflows::activity(person, preset) {
        return form;
    }
    let open = matches!(preset, "Service" | "Information");
    let mut first = promise(if open { "" } else { person });
    first["open"] = json!(open);
    if matches!(preset, "Dependency plan" | "Ride" | "Delivery") {
        first["delta"] = json!(1);
    }
    if preset == "Assignment" {
        first["party"] = Value::Null;
    }
    let mut promises = vec![first];
    if matches!(preset, "Trade" | "Sale" | "Group coordination") {
        let mut second = promise(person);
        if matches!(preset, "Trade" | "Sale") {
            second["party"] = Value::Null;
        } else {
            second["party"] = Value::Null;
        }
        promises.push(second);
    }
    let mut form = Form::action(
        "Create transfer",
        json!({
            "creator": person, "head": preset, "slug": null, "agreement": if preset == "Dependency plan" { "dependency" } else { "full" },
            "agreement_pct": 100, "satiation": "none", "parent": null, "source": null,
            "visibility": "hidden", "max_proximity": 1, "reserve_default": "inherit",
            "require_confirmation": true, "default_place": place(), "public_history": null, "invitees": [],
            "promises": promises, "dependencies": if preset == "Dependency plan" { vec![dependency()] } else { Vec::new() }
        }),
    );
    if let Some(data) = form.data.as_object_mut() {
        data.remove("request_id");
    }
    form.mode = "create-transfer-draft".into();
    form.preset = preset.into();
    form.step = Some(0);
    transport::initialize(&mut form);
    form
}

pub fn edit(row: &Value, counteroffer: bool, person: &str) -> Form {
    let mut form = composer(person, "");
    for key in [
        "head",
        "slug",
        "agreement_pct",
        "satiation",
        "parent",
        "source",
        "visibility",
        "max_proximity",
        "reserve_default",
        "require_confirmation",
        "public_history",
        "default_place",
        "dependencies",
    ] {
        if !row[key].is_null() {
            form.data[key] = row[key].clone();
        }
    }
    form.data["agreement"] = row["agreement_type"].clone();
    let creator = array(row, "parties")
        .iter()
        .find(|p| p["role"] == "creator")
        .map(|p| p["actor"].clone())
        .unwrap_or_else(|| json!(if counteroffer { "" } else { person }));
    form.data["creator"] = creator;
    form.data["invitees"] = json!(
        array(row, "invitations")
            .iter()
            .filter(|i| i["status"] == "pending")
            .map(|i| i["addressed_person"].clone())
            .collect::<Vec<_>>()
    );
    form.data["promises"] = json!(
        array(row, "promises")
            .iter()
            .filter(|p| p["state"] != "withdrawn")
            .map(|p| {
                let mut terms = promise("");
                for key in [
                    "uid",
                    "disclosed",
                    "record",
                    "party",
                    "open",
                    "delta",
                    "unit",
                    "window_start",
                    "window_end",
                    "place",
                    "condition",
                    "reserve_from",
                    "reuse_policy",
                ] {
                    if !p[key].is_null() {
                        terms[key] = p[key].clone();
                    }
                }
                if !p["item"].is_null() {
                    terms["item"] = p["item"].clone();
                } else {
                    for key in ["title", "description"] {
                        if let Some(value) = p[key].as_str() {
                            terms["item"][key] = json!(value);
                        }
                    }
                    if p["transport"].is_object() {
                        terms["item"]["transport"] = p["transport"].clone();
                    }
                    if let (Some(exchange), Some(giver), Some(receiver)) = (
                        p["exchange"].as_str(),
                        p["giver"].as_str(),
                        p["receiver"].as_str(),
                    ) {
                        terms["item"]["exchange"] =
                            json!({"uid":exchange,"giver":giver,"receiver":receiver});
                    }
                }
                terms
            })
            .collect::<Vec<_>>()
    );
    if let Some(snapshot) = row
        .pointer("/revision_evidence/current/terms")
        .filter(|value| value.is_object())
    {
        let terms = &snapshot["transfer"];
        for key in [
            "head",
            "slug",
            "agreement_pct",
            "visibility",
            "max_proximity",
            "require_confirmation",
            "public_history",
            "default_place",
        ] {
            form.data[key] = terms[key].clone();
        }
        for (key, source) in [
            ("agreement", "agreement_type"),
            ("parent", "parent_uid"),
            ("source", "source_uid"),
        ] {
            form.data[key] = terms[source].clone();
        }
        for key in ["satiation", "reserve_default"] {
            if !terms[key].is_null() {
                form.data[key] = terms[key].clone();
            }
        }
        if let Some(creator) = array(snapshot, "parties")
            .iter()
            .find(|party| party["kind"] == "creator")
        {
            form.data["creator"] = creator["person_uid"].clone();
        }
        form.data["promises"] = json!(
            array(snapshot, "promises")
                .iter()
                .filter(|p| p["state"] != "withdrawn")
                .map(|p| {
                    let mut terms = promise("");
                    for key in [
                        "uid",
                        "item",
                        "delta",
                        "window_start",
                        "window_end",
                        "condition",
                        "reserve_from",
                    ] {
                        terms[key] = p[key].clone();
                    }
                    for (key, source) in [
                        ("record", "record_uid"),
                        ("party", "person_uid"),
                        ("unit", "unit_uid"),
                        ("place", "location"),
                        ("reuse_policy", "open_reuse_policy"),
                    ] {
                        terms[key] = p[source].clone();
                    }
                    terms["open"] = json!(p["state"] == "open");
                    terms
                })
                .collect::<Vec<_>>()
        );
        form.data["dependencies"] = json!(array(snapshot, "dependencies").iter().map(|d| json!({
            "uid":d["uid"],"scope":d["scope"],"promise":d["promise_uid"],"upstream_kind":d["upstream_kind"],"upstream":d["upstream_uid"],"required_state":d["required_state"]
        })).collect::<Vec<_>>());
    }
    if form.data["default_place"].is_null() {
        form.data["default_place"] = place();
    }
    form.actor = Some(person.into());
    form.mode = if counteroffer {
        "counteroffer-transfer"
    } else if row["revision"] == 0 {
        "adopt-transfer-draft"
    } else {
        "revise-transfer-draft"
    }
    .into();
    form.title = if counteroffer {
        "Counteroffer"
    } else {
        "Edit transfer"
    }
    .into();
    form.transfer = Some(text(row, "uid"));
    form.revision = row["revision"].as_u64();
    form
}

pub fn observer(person: &str, upstream: &Value) -> Form {
    let mut form = composer(person, "Dependency plan");
    form.title = "Use this outcome privately".into();
    form.data["head"] = json!(format!("After {}", title(upstream)));
    form.data["visibility"] = json!("hidden");
    form.data["dependencies"][0]["upstream"] = upstream["uid"].clone();
    form.data["dependencies"][0]["required_state"] = json!("kept");
    form
}

pub fn bind_preset(form: &mut Form) {
    if form.step.is_none() {
        return;
    }
    if transport::bind(form) {
        return;
    }
    if workflows::bind(form) {
        return;
    }
    let creator = form.data["creator"].clone();
    let invitee = array(&form.data, "invitees")
        .first()
        .cloned()
        .unwrap_or(Value::Null);
    for (index, promise) in form.data["promises"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if promise["open"] == true {
            continue;
        }
        if text(promise, "party").is_empty() {
            promise["party"] = if form.preset == "Assignment"
                || (matches!(
                    form.preset.as_str(),
                    "Trade" | "Sale" | "Group coordination"
                ) && index == 1)
            {
                invitee.clone()
            } else {
                creator.clone()
            };
        }
        if matches!(form.preset.as_str(), "Donation" | "Trade" | "Sale")
            && !invitee.is_null()
            && promise["item"]["exchange"].is_null()
        {
            let (giver, receiver) = if index == 0 {
                (&creator, &invitee)
            } else {
                (&invitee, &creator)
            };
            promise["delta"] = json!(-promise["delta"].as_f64().unwrap_or(1.0).abs());
            promise["party"] = giver.clone();
            promise["item"]["exchange"] =
                json!({"uid":nucleus::new_uid("exchange"),"giver":giver,"receiver":receiver});
        }
    }
}

pub fn validate_draft(data: &Value, counteroffer: bool) -> Result<(), String> {
    if text(data, "head").trim().is_empty() {
        return Err("Enter a transfer title".into());
    }
    let creator = text(data, "creator");
    if creator.is_empty() && !counteroffer {
        return Err("Choose the Person creating this transfer".into());
    }
    let mut people = HashSet::from([creator]);
    for person in array(data, "invitees") {
        if !people.insert(display(person)) {
            return Err(
                "Each invitee must be different from the creator and other invitees".into(),
            );
        }
    }
    if data["agreement"] == "percentage"
        && !data["agreement_pct"]
            .as_u64()
            .is_some_and(|v| (1..=100).contains(&v))
    {
        return Err("Agreement percentage must be between 1 and 100".into());
    }
    if data["visibility"] == "proximity" && !data["max_proximity"].as_u64().is_some_and(|v| v > 0) {
        return Err("Maximum proximity must be a positive whole number".into());
    }
    if data["satiation"] == "first_completes" && text(data, "source").is_empty() {
        return Err("Choose a shared source for first-sibling completion".into());
    }
    for (index, promise) in array(data, "promises").iter().enumerate() {
        if (text(promise, "record").is_empty() && promise["item"].is_null())
            || !promise["delta"]
                .as_f64()
                .is_some_and(|v| v.is_finite() && v != 0.0)
        {
            return Err(format!(
                "Promise {} needs a record and a nonzero quantity",
                index + 1
            ));
        }
        if !promise["item"].is_null() {
            let item: nucleus::transfer::disclosure::TransferItem =
                serde_json::from_value(promise["item"].clone())
                    .map_err(|error| error.to_string())?;
            item.validate().map_err(str::to_string)?;
        }
        if promise["open"] != true
            && text(promise, "party").is_empty()
            && promise.pointer("/disclosed/parties") != Some(&Value::Bool(false))
        {
            return Err(format!("Choose a Person or OPEN for promise {}", index + 1));
        }
        let dates: Vec<_> = ["window_start", "window_end"].into_iter().map(|key| {
            if text(promise, key).is_empty() { Ok(None) } else {
                chrono::DateTime::parse_from_rfc3339(&text(promise, key)).map(Some)
                    .map_err(|_| format!("Promise {} needs a date with a timezone, such as 2026-10-01T10:00:00-03:00", index + 1))
            }
        }).collect::<Result<_, _>>()?;
        if let [Some(start), Some(end)] = dates.as_slice()
            && start >= end
        {
            return Err("The start must come before the deadline".into());
        }
    }
    if !counteroffer && data["agreement"] == "dependency" && array(data, "dependencies").is_empty()
    {
        return Err("Add a dependency for this agreement".into());
    }
    for dependency in array(data, "dependencies") {
        if text(dependency, "upstream").is_empty() {
            return Err("Choose the upstream item for each dependency".into());
        }
    }
    Ok(())
}

pub fn loan_proposal(transfer: &Value, item: &Value, person: &str, future: bool) -> Form {
    let mut form = composer(person, if future { "Future Need" } else { "Return" });
    let borrower = text(item, "receiver");
    let lender = text(item, "giver");
    let reference = json!({"origin":item["accepted_loan"]["origin"],"transfer":transfer["uid"],"exchange":item["exchange"]});
    let promise = &mut form.data["promises"][0];
    promise["item"] = json!(nucleus::transfer::disclosure::TransferItem {
        title: title(item),
        ..Default::default()
    });
    promise["record"] = json!("");
    promise["delta"] = json!(if future {
        item["delta"].as_f64().unwrap_or(1.0).abs()
    } else {
        -item["delta"].as_f64().unwrap_or(1.0).abs()
    });
    promise["unit"] = item["unit"].clone();
    if future {
        promise["open"] = json!(true);
        promise["party"] = Value::Null;
        promise["item"]["future_need_for"] = reference;
        promise["window_start"] = json!(
            item["accepted_loan"]["until_ms"]
                .as_i64()
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|time| time.to_rfc3339())
        );
        form.data["visibility"] = json!("public");
    } else {
        promise["party"] = json!(borrower);
        promise["item"]["exchange"] =
            json!({"uid":nucleus::new_uid("exchange"),"giver":borrower,"receiver":lender});
        promise["item"]["return_of"] = reference;
        form.data["invitees"] = json!([if person == borrower { lender } else { borrower }]);
    }
    form.data["head"] = json!(format!(
        "{} · {}",
        if future { "Future Need" } else { "Return" },
        title(item)
    ));
    form
}
