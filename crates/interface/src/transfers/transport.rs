use super::{FieldKind, Form, array};

pub fn fields(form: &mut Form) {
    if !form.data["promises"][0]["item"]["transport"].is_object() {
        return;
    }
    let prefix = "/promises/0/item/transport";
    form.field(
        &format!("{prefix}/direction"),
        "Transport direction",
        FieldKind::Choice(vec!["need".into(), "offer".into()]),
    );
    form.field(
        &format!("{prefix}/origin"),
        "Origin Record / #address",
        FieldKind::Reference("record".into()),
    );
    form.field(
        &format!("{prefix}/destination"),
        "Destination Record / #address",
        FieldKind::Reference("record".into()),
    );
    form.field(
        &format!("{prefix}/via"),
        "Intermediate stop Records / #addresses, in order, comma separated",
        FieldKind::Text,
    );
    if let Some(field) = form.fields.last_mut() {
        field.value = form
            .data
            .pointer(&format!("{prefix}/via"))
            .and_then(Value::as_array)
            .map(|stops| {
                stops
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
    }
    form.field(
        "/promises/0/window_start",
        "Pickup window starts (date, time, timezone)",
        FieldKind::Text,
    );
    form.field(
        "/promises/0/window_end",
        "Pickup window ends (date, time, timezone)",
        FieldKind::Text,
    );
    form.field(
        &format!("{prefix}/count"),
        "Passenger / delivery count",
        FieldKind::Number,
    );
    form.field(
        &format!("{prefix}/capacity"),
        "Reviewed capacity",
        FieldKind::Number,
    );
}
use nucleus::transfer::transport::{MAX_CAPACITY, Terms};
use serde_json::{Value, json};

pub fn sharing_record(transfer: &Value, person: &str) -> String {
    array(transfer, "promises")
        .iter()
        .find(|promise| promise["party"] == person)
        .and_then(|promise| promise["record"].as_str())
        .filter(|uid| nucleus::valid_uid(uid, "r"))
        .unwrap_or(person)
        .into()
}

pub fn journey_forms(transfer: &Value, person: &str) -> Vec<Form> {
    use nucleus::transfer::transport::{Journey, Step};
    let Ok(state) = serde_json::from_value::<Journey>(transfer["journey"].clone()) else {
        return Vec::new();
    };
    let Some(own) = state
        .participants
        .iter()
        .find(|progress| progress.person == person)
    else {
        return Vec::new();
    };
    if !state.agreed {
        return Vec::new();
    }
    [Step::Arrived, Step::Boarded, Step::Departed, Step::Destination, Step::Finished, Step::Cancelled].into_iter().filter(|step| {
        !own.step.is_some_and(|previous| previous.terminal() || *step <= previous) && (!(state.expired || state.capacity_released) || step.terminal())
    }).map(|step| {
        let mut form = Form::action(step.caption(), json!({"action":"record-journey-progress","transfer":state.transfer,"expected_revision":state.revision,"person":person,"step":step}));
        form.actor = Some(person.into());
        form.transfer = Some(state.transfer.clone());
        form.revision = Some(state.revision);
        form.review = json!({"Progress":step.caption(),"Person":person,"Journey":super::title(transfer),"Sharing":if step.terminal() {"Stop only my journey-linked live sharing. Other participants control their own sharing."} else {"My sharing settings stay under my control."},"Transfer obligations":"Use receipt and settlement controls for remaining obligations."});
        form
    }).collect()
}

pub fn initialize(form: &mut Form) {
    if !matches!(form.preset.as_str(), "Ride" | "Delivery") {
        return;
    }
    form.data["promises"][0]["item"]["transport"] = json!({"activity":if form.preset=="Ride" {"ride"} else {"delivery"},"direction":"offer","origin":"","destination":"","count":1,"capacity":1});
    for field in ["location", "quantity"] {
        form.data["promises"][0]["item"]["disclosure"][field] =
            json!({"scope":"participants","people":[]});
    }
    form.data["reserve_default"] = json!("agreed");
    bind(form);
}

pub fn bind(form: &mut Form) -> bool {
    if form.mode != "create-transfer-draft" || !matches!(form.preset.as_str(), "Ride" | "Delivery")
    {
        return false;
    }
    if !form.data["promises"][0]["item"]["transport"].is_object() {
        return false;
    }
    let creator = form.data["creator"].clone();
    let other = array(&form.data, "invitees")
        .first()
        .cloned()
        .unwrap_or(Value::Null);
    let promise = &mut form.data["promises"][0];
    let need = promise["item"]["transport"]["direction"] == "need";
    let (giver, receiver) = if need {
        (&other, &creator)
    } else {
        (&creator, &other)
    };
    promise["party"] = giver.clone();
    promise["open"] = json!(giver.is_null());
    if let Some(count) = promise["item"]["transport"]["count"].as_f64() {
        promise["delta"] = json!(-count);
        promise["amount"] = json!(count);
    }
    if giver.is_null() || receiver.is_null() {
        promise["item"]["exchange"] = Value::Null;
    } else {
        let uid = promise["item"]["exchange"]["uid"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| nucleus::new_uid("exchange"));
        promise["item"]["exchange"] = json!({"uid":uid,"giver":giver,"receiver":receiver});
    }
    true
}

pub fn normalize(data: &mut Value) -> Result<(), String> {
    for promise in data["promises"].as_array_mut().into_iter().flatten() {
        if !promise["item"]["transport"].is_object() {
            continue;
        }
        if let Some(stops) = promise["item"]["transport"]["via"].as_str() {
            promise["item"]["transport"]["via"] = json!(
                stops
                    .split(',')
                    .map(str::trim)
                    .filter(|stop| !stop.is_empty())
                    .collect::<Vec<_>>()
            );
        }
        for field in ["count", "capacity"] {
            let value = promise["item"]["transport"][field]
                .as_f64()
                .filter(|value| {
                    value.is_finite()
                        && value.fract() == 0.0
                        && *value > 0.0
                        && *value <= f64::from(MAX_CAPACITY)
                })
                .ok_or(
                    "Choose a positive whole passenger/delivery count and capacity, at most 10000",
                )?;
            promise["item"]["transport"][field] = json!(value as u32);
        }
        let terms: Terms = serde_json::from_value(promise["item"]["transport"].clone())
            .map_err(|error| error.to_string())?;
        terms.validate().map_err(str::to_owned)?;
        if promise["delta"]
            .as_f64()
            .is_none_or(|delta| delta.abs() != f64::from(terms.count))
        {
            return Err("Transport quantity must match its passenger/delivery count".into());
        }
        let from = promise["window_start"]
            .as_str()
            .ok_or("Choose the pickup-window start")?;
        let until = promise["window_end"]
            .as_str()
            .ok_or("Choose the pickup-window end")?;
        let from = chrono::DateTime::parse_from_rfc3339(from)
            .map_err(|_| "Use a pickup start with date, time and timezone")?;
        let until = chrono::DateTime::parse_from_rfc3339(until)
            .map_err(|_| "Use a pickup end with date, time and timezone")?;
        if from >= until {
            return Err("Pickup-window end must be after its start".into());
        }
    }
    Ok(())
}

pub fn parts(transfer: &Value, person: &str) -> Vec<Form> {
    let Some(promise) = array(transfer, "promises")
        .iter()
        .find(|promise| promise["transport"].is_object())
    else {
        return Vec::new();
    };
    let Ok(terms) = serde_json::from_value::<Terms>(promise["transport"].clone()) else {
        return Vec::new();
    };
    if terms.via.is_empty() {
        return Vec::new();
    }
    terms
        .stops()
        .windows(2)
        .enumerate()
        .map(|(index, pair)| {
            let mut form = super::composer(
                person,
                if terms.activity == nucleus::transfer::transport::Activity::Ride {
                    "Ride"
                } else {
                    "Delivery"
                },
            );
            form.title = format!("Contribute transport on part {}", index + 1);
            form.data["head"] = json!(format!("{} · part {}", super::title(transfer), index + 1));
            form.data["parent"] = transfer["uid"].clone();
            form.data["source"] = transfer["uid"].clone();
            let part = &mut form.data["promises"][0];
            part["item"]["transport"]["origin"] = json!(pair[0]);
            part["item"]["transport"]["destination"] = json!(pair[1]);
            part["item"]["transport"]["capacity"] = json!(terms.capacity);
            part["window_start"] = promise["window_start"].clone();
            part["window_end"] = promise["window_end"].clone();
            form
        })
        .collect()
}

#[cfg(test)]
mod parts_tests {
    use super::*;
    #[test]
    fn each_stop_part_is_an_independent_ordinary_transfer_draft() {
        let person = nucleus::new_uid("r");
        let parent = nucleus::new_uid("r");
        let a = nucleus::new_uid("r");
        let b = nucleus::new_uid("r");
        let c = nucleus::new_uid("r");
        let transfer = json!({"uid":parent,"head":"Journey","promises":[{"transport":{"activity":"ride","direction":"offer","origin":a,"destination":c,"via":[b],"count":1,"capacity":1},"window_start":"2030-01-01T00:00:00Z","window_end":"2030-01-01T01:00:00Z"}]});
        let parts = parts(&transfer, &person);
        assert_eq!(parts.len(), 2);
        for form in &parts {
            assert_eq!(form.mode, "create-transfer-draft");
            assert_eq!(form.data["parent"], parent);
            assert_eq!(form.data["source"], parent);
            assert_eq!(form.data["require_confirmation"], true);
            assert!(form.data["promises"][0]["item"]["exchange"].is_null());
        }
        assert_ne!(parts[0].request_id, parts[1].request_id);
        assert_eq!(
            parts[0].data["promises"][0]["item"]["transport"]["destination"],
            b
        );
        assert_eq!(
            parts[1].data["promises"][0]["item"]["transport"]["origin"],
            b
        );
    }
}
