use serde_json::{Value, json};

pub fn delivery_label(cell: &Value) -> String {
    let name = cell["label"].as_str().unwrap_or("Device");
    let delivery = &cell["delivery"];
    if delivery.is_null() {
        return format!("{name}: delivery has not been checked. Changes are saved on this device.");
    }
    let last = delivery["succeeded_at"].as_str().unwrap_or("never");
    if let Some(error) = delivery["error"].as_str() {
        return format!("{name}: {error}\nLast successful exchange: {last}");
    }
    let pending = delivery["pending"].as_i64().unwrap_or(0);
    let state = if pending == 0 {
        "Delivery confirmed at last check".into()
    } else {
        format!("{pending} operation(s) awaiting confirmation")
    };
    format!("{name}: {state}\nLast successful exchange: {last}")
}

pub fn peer_network_label(network: &Value) -> String {
    if network.is_null() {
        return "Peer connection unavailable. Check whether another app is using this port.".into();
    }
    let addresses = network["addresses"]
        .as_array()
        .filter(|values| !values.is_empty())
        .or_else(|| network["listening"].as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let reach = if network["relay_only"] == true {
        "Internet relay only. Enable direct connections to listen on this port.".to_owned()
    } else {
        format!("Peer UDP addresses: {addresses}")
    };
    let urls = |field: &str| {
        network[field].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")
    };
    let selection = match network["relay_selection"].as_str() {
        Some("disabled") => "Connection relays disabled for local-only reach".to_owned(),
        Some("custom") => format!("Selected connection relays: {}", urls("selected_relays")),
        _ => "Connection relays: Iroh preset".to_owned(),
    };
    let advertised = urls("advertised_relays");
    let advertised = if advertised.is_empty() {
        "No connection relay advertised yet".to_owned()
    } else {
        format!("Currently advertised connection relay: {advertised}")
    };
    format!("{reach}\n{selection}\n{advertised}")
}

#[derive(Clone)]
pub enum FieldKind {
    Text,
    OptionalText,
    TextList,
    Number,
    Filter,
    Scope,
    Choice(Vec<(String, Value)>),
}

impl FieldKind {
    pub fn parse(&self, value: &Value, text: &str) -> Result<Value, String> {
        Ok(match self {
            Self::Text => json!(text.trim()),
            Self::OptionalText => if text.trim().is_empty() { Value::Null } else { json!(text.trim()) },
            Self::TextList => json!(text.split([',', '\n']).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>()),
            Self::Number => json!(
                text.trim()
                    .parse::<u32>()
                    .map_err(|_| "Enter a non-negative whole number")?
            ),
            Self::Choice(_) => value.clone(),
            Self::Scope => scope(value, text)?,
            Self::Filter => {
                if !text.trim().is_empty() {
                    let predicate: protein::Predicate = serde_json::from_str(text.trim())
                        .map_err(|e| format!("Invalid filter: {e}"))?;
                    let query: protein::Protein =
                        serde_json::from_value(json!({"source":"record", "where":[predicate]}))
                            .map_err(|e| e.to_string())?;
                    protein::validate(&query).map_err(|e| e.to_string())?;
                }
                json!(text.trim())
            }
        })
    }
}

pub fn scope_text(value: &Value) -> String {
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

pub fn scope(mode: &Value, text: &str) -> Result<Value, String> {
    match mode.as_str() {
        Some("all") => Ok(Value::Null),
        Some("none") => Ok(json!([])),
        Some("some") => {
            let mut fields: Vec<_> = text
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            fields.sort_unstable();
            fields.dedup();
            if fields.is_empty() {
                return Err("Name at least one field, or choose Minimum only".into());
            }
            Ok(json!(fields))
        }
        _ => Err("Choose a sharing limit".into()),
    }
}
