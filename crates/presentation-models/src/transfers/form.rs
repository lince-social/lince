// non-lean-yet

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{STEPS, array, display, text, transport, validate_draft};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Form {
    pub title: String,
    pub data: Value,
    pub fields: Vec<Field>,
    pub step: Option<usize>,
    pub transfer: Option<String>,
    pub revision: Option<u64>,
    pub mode: String,
    pub request_id: String,
    pub actor: Option<String>,
    pub preset: String,
    #[serde(default)]
    pub review: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Field {
    pub path: String,
    pub label: String,
    pub kind: FieldKind,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FieldKind {
    Text,
    Optional,
    Number,
    OptionalNumber,
    People,
    Identities,
    Duration,
    Selections(Vec<(String, String)>),
    Choice(Vec<String>),
    DataChoice(Vec<(Value, String)>),
    OptionalChoice(Vec<(String, String)>),
    Reference(String),
}

impl Form {
    pub fn action(title: &str, mut data: Value) -> Self {
        let request_id = nucleus::new_uid("transfer-ui");
        data["request_id"] = json!(request_id);
        Self {
            review: Value::Null,
            title: title.into(),
            data,
            fields: Vec::new(),
            step: None,
            transfer: None,
            revision: None,
            mode: String::new(),
            request_id,
            actor: None,
            preset: String::new(),
        }
    }

    pub fn field(&mut self, path: &str, label: &str, kind: FieldKind) {
        if self.mode == "counteroffer-transfer"
            && (["/creator", "/invitees", "/source", "/parent"].contains(&path)
                || (path.starts_with("/dependencies/") && self.data["dependencies"].is_null()))
        {
            return;
        }
        if let Some(suffix) = path.strip_prefix("/promises/")
            && let Some((index, field)) = suffix.split_once('/')
        {
            let promise = self.data.pointer(&format!("/promises/{index}"));
            let disclosure = match field {
                "item/title" => Some("title"),
                "item/description" => Some("description"),
                "record" | "condition" => Some("source"),
                "party" | "open" | "item/exchange/giver" | "item/exchange/receiver" => {
                    Some("parties")
                }
                "delta" | "amount" | "unit" => Some("quantity"),
                _ if field.starts_with("place/") => Some("location"),
                _ => None,
            };
            if disclosure.is_some_and(|key| promise.is_some_and(|p| p["disclosed"][key] == false))
                || (self.mode == "counteroffer-transfer"
                    && promise.is_some_and(|p| !p["uid"].is_null())
                    && (field.starts_with("item/disclosure/") || field == "record"))
            {
                return;
            }
        }
        let value = self.data.pointer(path).unwrap_or(&Value::Null);
        let value = if matches!(kind, FieldKind::Duration) {
            value
                .as_u64()
                .map(|seconds| {
                    [(86400, "d"), (3600, "h"), (60, "m"), (1, "s")]
                        .into_iter()
                        .find(|(unit, _)| seconds % unit == 0)
                        .map(|(unit, label)| format!("{}{label}", seconds / unit))
                        .unwrap_or_default()
                })
                .unwrap_or_default()
        } else if matches!(kind, FieldKind::Identities) {
            value
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| {
                            Some(format!(
                                "{}:{}",
                                value["kind"].as_str()?,
                                value["uid"].as_str()?
                            ))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default()
        } else if matches!(kind, FieldKind::People | FieldKind::Selections(_)) {
            value
                .as_array()
                .map(|values| values.iter().map(display).collect::<Vec<_>>().join(", "))
                .unwrap_or_default()
        } else {
            display(value)
        };
        self.fields.push(Field {
            path: path.into(),
            label: label.into(),
            kind,
            value,
        });
    }

    pub fn commit_fields(&mut self) -> Result<(), String> {
        let _preparation = nucleus::performance::Span::new("transfer-form-preparation");
        let mut data = Cow::Borrowed(&self.data);
        for field in &self.fields {
            let raw = field.value.trim();
            let value = match &field.kind {
                FieldKind::Optional | FieldKind::Reference(_) | FieldKind::OptionalChoice(_)
                    if raw.is_empty() =>
                {
                    Value::Null
                }
                FieldKind::OptionalNumber if raw.is_empty() => Value::Null,
                FieldKind::Number | FieldKind::OptionalNumber => {
                    let value: Value = serde_json::from_str(raw)
                        .map_err(|_| format!("{} must be a number", field.label))?;
                    if !value.is_number() {
                        return Err(format!("{} must be a finite number", field.label));
                    }
                    value
                }
                FieldKind::Duration => {
                    let split = raw.char_indices().last().map_or(0, |(index, _)| index);
                    let (number, unit) = raw.split_at(split);
                    let multiplier = match unit {
                        "d" => 86400_u32,
                        "h" => 3600,
                        "m" => 60,
                        "s" => 1,
                        _ => return Err("Use a duration such as 365d, 24h, 30m or 60s".into()),
                    };
                    let seconds = number
                        .parse::<u32>()
                        .ok()
                        .and_then(|value| value.checked_mul(multiplier))
                        .filter(|seconds| *seconds > 0)
                        .ok_or("Choose a positive duration within its supported range")?;
                    json!(seconds)
                }
                FieldKind::Identities => {
                    let mut identities = Vec::new();
                    for value in raw
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        let (kind, uid) = value
                            .split_once(':')
                            .ok_or("Use person:Record or organ:Record for each audience member")?;
                        let identity = match kind {
                            "person" => nucleus::trust_analysis::Identity::Person(uid.into()),
                            "organ" => nucleus::trust_analysis::Identity::Organ(uid.into()),
                            _ => return Err("Choose a Person or Organ audience member".into()),
                        };
                        identity.validate()?;
                        identities.push(identity);
                    }
                    identities.sort();
                    if identities.len() > 64 || identities.windows(2).any(|pair| pair[0] == pair[1])
                    {
                        return Err("Choose at most 64 distinct audience members".into());
                    }
                    json!(identities)
                }
                FieldKind::People | FieldKind::Selections(_) => json!(
                    raw.split(',')
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .collect::<Vec<_>>()
                ),
                FieldKind::DataChoice(choices) => choices
                    .iter()
                    .find(|(value, _)| value.to_string() == raw)
                    .map(|(value, _)| value.clone())
                    .ok_or_else(|| format!("Choose {}", field.label))?,
                FieldKind::Choice(choices) => {
                    if !choices.iter().any(|choice| choice == raw) {
                        return Err(format!("Choose {}", field.label));
                    }
                    if raw == "true" || raw == "false" {
                        json!(raw == "true")
                    } else {
                        json!(raw)
                    }
                }
                FieldKind::OptionalChoice(choices) => {
                    if !choices.iter().any(|(value, _)| value == raw) {
                        return Err(format!("Choose {}", field.label));
                    }
                    json!(raw)
                }
                _ => json!(raw),
            };
            let target = data
                .pointer(&field.path)
                .ok_or("This field is no longer available")?;
            if !nucleus::json::same_value_encoding(target, &value) {
                *data
                    .to_mut()
                    .pointer_mut(&field.path)
                    .ok_or("This field is no longer available")? = value;
            }
        }
        for path in ["/public_history/audience", "/request/scope/audience"] {
            let Some(audience) = data.pointer(path).and_then(Value::as_object) else {
                continue;
            };
            let add_viewers = match (
                audience.get("kind").and_then(Value::as_str),
                audience.contains_key("viewers"),
            ) {
                (Some("public"), true) => false,
                (Some("selected"), false) => true,
                _ => continue,
            };
            let audience = data
                .to_mut()
                .pointer_mut(path)
                .and_then(Value::as_object_mut)
                .ok_or("This audience is no longer available")?;
            if add_viewers {
                audience.insert("viewers".into(), json!([]));
            } else {
                audience.remove("viewers");
            }
        }
        if data["public_history"].is_object() {
            if let Some(raw) = data["public_history"]["measurements"].as_str() {
                let measurements = serde_json::from_str(raw)
                    .map_err(|_| "Use the reviewed public measurement proposal array")?;
                data.to_mut()["public_history"]["measurements"] = measurements;
            }
            let terms: nucleus::transfer::public_history::Terms =
                Deserialize::deserialize(&data["public_history"])
                    .map_err(|error| error.to_string())?;
            terms.validate()?;
        }
        if self.data != *data {
            nucleus::performance::add("transfer-form-updates", 1);
            if data["action"] == "transfer-receipt"
                && data["request"]["kind"] == "authorize_participation"
            {
                self.mode = "action".into();
                if let Some(review) = self.review.as_object_mut() {
                    review.remove("Reviewed request");
                    review.insert("Private request".into(), Value::Null);
                }
            }
            self.request_id = nucleus::new_uid("transfer-ui");
            if self.step.is_none() && data.get("request_id").is_some() {
                data.to_mut()["request_id"] = json!(self.request_id);
            }
        }
        if let Cow::Owned(data) = data {
            self.data = data;
        }
        Ok(())
    }

    pub fn payload(&self) -> Result<Value, String> {
        if self.step.is_none() {
            let mut data = self.data.clone();
            if data["action"] == "transfer-receipt"
                && data["request"]["kind"] == "authorize_participation"
                && (self.mode != "participation-reviewed"
                    || self.review["Reviewed request"] != data["request"]["request"])
            {
                return Err(
                    "Review the current private request before approving original participation"
                        .into(),
                );
            }
            if data["action"] == "transfer-receipt" && data["request"]["kind"] == "prepare" {
                let expiry = data
                    .pointer("/request/scope/expires_at")
                    .and_then(Value::as_str)
                    .ok_or("Choose the receipt's retention date and UTC time")?;
                let expiry = chrono::DateTime::parse_from_rfc3339(expiry)
                    .map_err(|_| "Use an ISO date and time with a timezone for retention")?;
                data["request"]["scope"]["expires_at"] = json!(expiry.timestamp());
                if let Some(raw) = data["request"]["scope"]["public_measurements"].as_str() {
                    let measurements: Vec<nucleus::transfer::receipt::Measurement> =
                        serde_json::from_str(raw)
                            .map_err(|_| "Paste a measurement proposal array")?;
                    if measurements.len() > nucleus::transfer::receipt::MAX_MEASURES {
                        return Err("Select at most 32 public measurements".into());
                    }
                    for measurement in &measurements {
                        measurement.validate()?;
                    }
                    data["request"]["scope"]["public_measurements"] = json!(measurements);
                }
            }
            if data["action"] == "transfer-receipt"
                && data["request"]["kind"] == "delivery"
                && data["request"]["request"]["kind"] == "preview"
            {
                let expiry = data
                    .pointer("/request/request/request/expires_at")
                    .and_then(Value::as_str)
                    .ok_or("Choose a delivery expiry with a timezone")?;
                let expiry = chrono::DateTime::parse_from_rfc3339(expiry)
                    .map_err(|_| "Use an ISO date and time with a timezone for delivery")?;
                data["request"]["request"]["request"]["expires_at"] = json!(expiry.timestamp());
            }
            if data["action"] == "set-transfer-private-application-policy" {
                let effects = data["effects"]
                    .as_array_mut()
                    .ok_or("Choose the Records this outcome changes")?;
                if effects.is_empty() || effects.len() > 32 {
                    return Err("Choose between one and 32 effects".into());
                }
                for effect in effects {
                    let ratio = effect
                        .as_object_mut()
                        .and_then(|object| object.remove("private_ratio"));
                    if let Some(ratio) = ratio
                        .as_ref()
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                    {
                        let ratio = nucleus::DecimalValue::parse_inferred(ratio.trim())
                            .map_err(|_| "The ratio must be a decimal with at most 18 places")?;
                        effect["formula"] = json!(format!("incoming() * {ratio}"));
                    }
                    nucleus::transfer::application::validate(
                        effect["formula"].as_str().unwrap_or_default(),
                    )
                    .map_err(|error| error.to_string())?;
                }
            }
            return Ok(data);
        }
        if !self.data.is_object() {
            return Err("Transfer draft must be an object".into());
        }
        let mut data = self.data.clone();
        for promise in data["promises"].as_array_mut().into_iter().flatten() {
            if promise["open"] == true {
                if promise["item"].is_object() {
                    promise["item"]["exchange"] = Value::Null;
                }
            } else if promise
                .pointer("/item/exchange")
                .is_some_and(Value::is_object)
            {
                let delta = promise["delta"].as_f64().unwrap_or(-1.0);
                let amount = promise["amount"].as_f64().unwrap_or(delta.abs());
                if !amount.is_finite() || amount <= 0.0 {
                    return Err("Exchange amount must be positive".into());
                }
                promise["delta"] = json!(if delta < 0.0 { -amount } else { amount });
                promise["party"] = promise["item"]["exchange"]
                    [if delta < 0.0 { "giver" } else { "receiver" }]
                .clone();
            }
        }
        transport::normalize(&mut data)?;
        validate_draft(&data, self.mode == "counteroffer-transfer")?;
        if data["agreement"] != "percentage" {
            data["agreement_pct"] = Value::Null;
        }
        if data["visibility"] != "proximity" {
            data["max_proximity"] = Value::Null;
        }
        for promise in data["promises"].as_array_mut().into_iter().flatten() {
            if let Some(object) = promise.as_object_mut() {
                object.remove("disclosed");
                object.remove("amount");
                object.remove("temporary");
            }
            if promise["record"].is_null() {
                promise["record"] = json!("");
            }
            if promise["open"] == true {
                promise["party"] = Value::Null;
            }
            clean_place(&mut promise["place"])?;
        }
        clean_place(&mut data["default_place"])?;
        let mut action = serde_json::Map::from_iter([
            ("action".into(), json!(self.mode)),
            ("request_id".into(), json!(self.request_id)),
        ]);
        if let Some(transfer) = &self.transfer {
            action.insert("transfer".into(), json!(transfer));
            if self.mode != "adopt-transfer-draft" {
                action.insert("expected_revision".into(), json!(self.revision));
            }
            if self.mode == "counteroffer-transfer" {
                action.insert("person".into(), json!(self.actor));
            }
            action.insert("draft".into(), data);
        } else {
            let Value::Object(data) = data else {
                return Err("Transfer draft must be an object".into());
            };
            action.extend(data);
        }
        Ok(Value::Object(action))
    }

    pub fn valid(&self) -> bool {
        self.data.is_object()
            && self.data.to_string().len() < 262_144
            && self.fields.len() <= 1024
            && self.fields.iter().all(|field| {
                field.value.len() <= 16_384 && self.data.pointer(&field.path).is_some()
            })
            && self.step.is_none_or(|step| step < STEPS.len())
            && self.step.is_none_or(|_| {
                ["promises", "dependencies", "invitees"]
                    .into_iter()
                    .all(|key| self.data[key].is_array())
                    && array(&self.data, "promises").len() <= 64
                    && array(&self.data, "dependencies").len() <= 64
                    && array(&self.data, "promises").iter().all(Value::is_object)
                    && array(&self.data, "dependencies")
                        .iter()
                        .all(Value::is_object)
            })
    }
}

fn clean_place(value: &mut Value) -> Result<(), String> {
    if value.is_null() {
        return Ok(());
    }
    if value["lat"].is_null() && value["lon"].is_null() {
        if text(value, "address").trim().is_empty() {
            *value = Value::Null;
        }
        return Ok(());
    }
    if !value["lat"]
        .as_f64()
        .is_some_and(|v| (-90.0..=90.0).contains(&v))
        || !value["lon"]
            .as_f64()
            .is_some_and(|v| (-180.0..=180.0).contains(&v))
    {
        return Err("Enter valid latitude and longitude together".into());
    }
    Ok(())
}
