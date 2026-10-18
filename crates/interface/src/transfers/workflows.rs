use super::{self as model, Form};
use serde_json::{Value, json};

pub const ACTIVITIES: [&str; 4] = [
    "Contribution",
    "Paid contribution",
    "Need to receive",
    "Need to contribute",
];

pub fn activity(person: &str, preset: &str) -> Option<Form> {
    if !ACTIVITIES.contains(&preset) {
        return None;
    }
    let mut form = model::composer(
        person,
        if preset == "Paid contribution" {
            "Sale"
        } else {
            "Donation"
        },
    );
    form.preset = preset.into();
    form.title = format!("Create {preset}");
    form.data["head"] = json!(preset);
    form.data["promises"][0]["item"]["title"] = json!(if preset == "Need to contribute" {
        "Attention or participation I need"
    } else if preset == "Need to receive" {
        "Contribution I need"
    } else {
        "Contribution I offer"
    });
    if preset == "Paid contribution" {
        form.data["promises"][1]["item"]["title"] = json!("Return contribution");
    }
    bind(&mut form);
    Some(form)
}

pub fn bind(form: &mut Form) -> bool {
    if !ACTIVITIES.contains(&form.preset.as_str()) {
        return false;
    }
    let requesting = matches!(
        form.preset.as_str(),
        "Need to receive" | "Need to contribute"
    );
    let creator = form.data["creator"].clone();
    let other = model::array(&form.data, "invitees")
        .first()
        .cloned()
        .unwrap_or(Value::Null);
    for (index, promise) in form.data["promises"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .enumerate()
        .take(2)
    {
        let (giver, receiver) = if requesting && index == 0 || !requesting && index == 1 {
            (&other, &creator)
        } else {
            (&creator, &other)
        };
        promise["party"] = giver.clone();
        promise["open"] = json!(giver.is_null());
        if giver.is_null() || receiver.is_null() {
            promise["item"]["exchange"] = Value::Null;
        } else {
            let uid = promise["item"]["exchange"]["uid"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| nucleus::new_uid("exchange"));
            promise["item"]["exchange"] = json!({"uid":uid,"giver":giver,"receiver":receiver});
        }
    }
    true
}

pub fn alternative(person: &str, request: &Value) -> Form {
    let receiving = model::array(request, "promises").iter().any(|promise| {
        promise
            .pointer("/item/exchange/receiver")
            .and_then(Value::as_str)
            == Some(person)
    });
    let mut form = activity(
        person,
        if receiving {
            "Need to receive"
        } else {
            "Contribution"
        },
    )
    .expect("Activity composer");
    form.title = "Propose another solution".into();
    form.data["head"] = json!(format!("Alternative for {}", model::title(request)));
    form.data["source"] = request["uid"].clone();
    form
}

pub fn smaller_need(person: &str, request: &Value) -> Form {
    let mut form = activity(person, "Need to receive").expect("Need composer");
    form.title = "Create a smaller linked Need".into();
    form.data["head"] = json!(format!("Part of {}", model::title(request)));
    form.data["source"] = request["uid"].clone();
    form.data["parent"] = request["uid"].clone();
    form
}
