use crate::{
    app::{self, Mobile},
    navigation::Page,
};
use bevy::prelude::*;
use lince_interface::{
    controls,
    transfers::{self as model, FieldKind, Form, array, text, title},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

mod discovery;
mod filters;
#[cfg(test)]
mod tests;

#[derive(Resource, Default)]
pub struct State {
    scope: String,
    pub form: Option<Form>,
    review: Option<String>,
    pending: Option<(String, String)>,
    karma: Value,
    result: Value,
    remote_review: Option<nucleus::trust_analysis::remote::Review>,
    job: Option<String>,
    detail: Option<Value>,
    pages: BTreeMap<String, usize>,
    filter_editor: String,
    measurements: Vec<nucleus::transfer::receipt::Measurement>,
}

#[derive(Clone)]
pub enum Intent {
    New(String),
    Open(Form),
    Choose(usize, String),
    Step(usize),
    Submit,
    ReviewAction,
    TrustFilter(String),
    TrustValue(String, String),
    ResolveMeasurement,
    ClearMeasurement,
    Cancel,
    AddPromise,
    RemovePromise(usize),
    Agreement(u8),
    RefreshAgreement,
    PreviewSettlement,
    AcceptSettlement,
    Analyze,
    PreviewAsk,
    StartAsk,
    PollAsk,
    VerifyAsk,
    CancelAsk,
    Detail(Value),
    ResultPage(String, isize),
}

pub fn ensure_scope(world: &mut World) {
    world.init_resource::<State>();
    let scope = format!(
        "{}:{:?}",
        world.resource::<Mobile>().scope_key(),
        world.resource::<Mobile>().navigation.current
    );
    if world.resource::<State>().scope != scope {
        if let Some((id, kind)) = world.resource::<State>().pending.clone()
            && kind.starts_with("settlement-preview:")
        {
            let _ = app::send(world, cell::ClientMessage::Unsubscribe { id });
        }
        let form = restore_draft(world);
        world.insert_resource(State {
            scope,
            form,
            ..Default::default()
        });
    }
    if world.resource::<State>().form.is_none()
        && world.resource::<State>().pending.is_none()
        && let Some(form) = restore_draft(world)
    {
        world.resource_mut::<State>().form = Some(form);
    }
}

fn draft_allowed(form: &Form) -> bool {
    form.step.is_some()
        && matches!(
            form.mode.as_str(),
            "create-transfer-draft"
                | "counteroffer-transfer"
                | "adopt-transfer-draft"
                | "revise-transfer-draft"
        )
}

fn restore_draft(world: &World) -> Option<Form> {
    let mobile = world.resource::<Mobile>();
    if !mobile.ready || mobile.identity == crate::session::Identity::Locked {
        return None;
    }
    let raw = mobile.drafts.get("workflow-draft/current")?;
    if raw.len() > 262_144 {
        return None;
    }
    let mut form: Form = serde_json::from_str(raw).ok()?;
    if !draft_allowed(&form) || form.actor.as_deref() != Some(&person(world)) {
        return None;
    }
    let matches = match &mobile.navigation.current {
        Page::Transfers => form.transfer.is_none(),
        Page::Transfer(uid) => form.transfer.as_deref() == Some(uid),
        _ => false,
    };
    if !matches {
        return None;
    }
    form.review = Value::Null;
    form.step = form.step.map(|step| step.min(3));
    Some(form)
}

pub fn persist_draft(world: &mut World) {
    let Some(mut form) = world
        .get_resource::<State>()
        .and_then(|state| state.form.clone())
        .filter(draft_allowed)
    else {
        return;
    };
    let field_scope = format!("workflow-{}", form.request_id);
    for field in &mut form.fields {
        if !matches!(
            field.kind,
            FieldKind::Choice(_) | FieldKind::OptionalChoice(_)
        ) {
            field.value = world
                .resource::<Mobile>()
                .draft(&field_scope, &field.path, &field.value);
        }
    }
    form.review = Value::Null;
    let Ok(value) = serde_json::to_string(&form) else {
        return;
    };
    if value.len() <= 262_144 {
        world
            .resource_mut::<Mobile>()
            .drafts
            .insert("workflow-draft/current".into(), value);
    }
}

pub fn person(world: &World) -> String {
    if let crate::session::Identity::Person(person) = &world.resource::<Mobile>().identity {
        return person.clone();
    }
    world
        .resource::<Mobile>()
        .rows
        .values()
        .flatten()
        .find(|row| row["kind"] == "transfer_context")
        .and_then(|row| row["acting_person"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| world.resource::<Mobile>().draft("workflow", "person", ""))
}

fn current(world: &World) -> Option<Value> {
    let Page::Transfer(uid) = &world.resource::<Mobile>().navigation.current else {
        return None;
    };
    app::row(world, "transfer", uid)
}

fn send(world: &mut World, action: Value, kind: &str) -> Result<(), String> {
    ensure_scope(world);
    if world.resource::<State>().pending.is_some() || !app::action_idle(world) {
        return Err("Wait for the current request to finish".into());
    }
    let action: engine::actions::Action =
        serde_json::from_value(action).map_err(|error| error.to_string())?;
    let id = nucleus::new_uid("mobile-workflow");
    app::send(
        world,
        cell::ClientMessage::Act {
            id: id.clone(),
            action,
        },
    )?;
    world.resource_mut::<State>().pending = Some((id, kind.into()));
    Ok(())
}

fn commit_fields(world: &mut World) -> Result<(), String> {
    app::capture(world);
    let Some(mut form) = world.resource_mut::<State>().form.take() else {
        return Ok(());
    };
    let scope = format!("workflow-{}", form.request_id);
    for field in &mut form.fields {
        if !matches!(
            field.kind,
            FieldKind::Choice(_) | FieldKind::OptionalChoice(_)
        ) {
            field.value = world
                .resource::<Mobile>()
                .draft(&scope, &field.path, &field.value);
        }
    }
    let result = form.commit_fields();
    if result.is_ok() {
        model::bind_preset(&mut form);
    }
    world.resource_mut::<State>().form = Some(form);
    result
}

fn fingerprint(world: &World, form: &Form) -> Result<String, String> {
    Ok(nucleus::fact::sha256_hex(
        &serde_json::to_vec(&(world.resource::<Mobile>().scope_key(), form.payload()?))
            .map_err(|error| error.to_string())?,
    ))
}

fn open(world: &mut World, mut form: Form) -> Result<(), String> {
    let person = person(world);
    if form.actor.as_deref().is_some_and(|actor| actor != person) {
        return Err("This form belongs to another Person; open it again".into());
    }
    form.actor = Some(person);
    if form.step.is_some() {
        model::fields::composer_fields(&mut form);
    }
    let reviewed = if form.step.is_none() && form.fields.is_empty() {
        Some(fingerprint(world, &form)?)
    } else {
        None
    };
    let mut state = world.resource_mut::<State>();
    state.form = Some(form);
    state.review = reviewed;
    state.result = Value::Null;
    Ok(())
}

fn trust_query(world: &World) -> Result<nucleus::trust_analysis::query::Query, String> {
    use nucleus::trust_analysis::{
        Condition, Identity,
        paths::Policy,
        query::{EvidenceLevel, Query},
    };
    let Page::Trust(kind, uid) = &world.resource::<Mobile>().navigation.current else {
        return Err("Open a Person or Organ experience view".into());
    };
    let mobile = world.resource::<Mobile>();
    let value = |field: &str, default: &str| mobile.draft("trust", field, default);
    let identities = |field: &str| -> Result<Vec<Identity>, String> {
        value(field, "")
            .split([',', '\n'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                let identity = Identity::Organ(value.into());
                identity.validate()?;
                Ok(identity)
            })
            .collect()
    };
    let mut query = Query::for_subject(if kind == "organ" {
        Identity::Organ(uid.clone())
    } else {
        Identity::Person(uid.clone())
    });
    query.role = if mobile.draft("trust-policy", "role", "giver") == "receiver" {
        nucleus::transfer::outcome::Role::Receiver
    } else {
        nucleus::transfer::outcome::Role::Giver
    };
    query.counterparty_basis = if mobile.draft("trust-policy", "basis", "organ") == "person" {
        nucleus::trust_analysis::query::CounterpartyBasis::AttributedPerson
    } else {
        nucleus::trust_analysis::query::CounterpartyBasis::Organ
    };
    let time = |key: &str| -> Result<Option<i64>, String> {
        let raw = value(key, "");
        if raw.trim().is_empty() {
            return Ok(None);
        }
        chrono::DateTime::parse_from_rfc3339(raw.trim())
            .map(|time| Some(time.timestamp()))
            .map_err(|_| "Use a date and time with a timezone".into())
    };
    query.from = time("from")?;
    query.until = time("until")?;
    query.measurements = world.resource::<State>().measurements.clone();
    let issuers = identities("issuers")?;
    if !issuers.is_empty() {
        query.issuers.include.push(Condition {
            identities: issuers,
            ..Default::default()
        });
    }
    for name in ["endpoints", "issuers", "counterparties"] {
        let filter = filters::read(world, name)?;
        match name {
            "endpoints" => query.endpoints = filter,
            "issuers" => {
                query.issuers.include.extend(filter.include);
                query.issuers.exclude.extend(filter.exclude);
            }
            _ => query.counterparties = filter,
        }
    }
    query.exclude_everywhere = identities("excluded")?;
    let anchor = value("anchor", &person(world));
    let mut contacts = Policy::for_anchor(Identity::Person(anchor));
    contacts.max_depth = value("depth", "3")
        .parse()
        .map_err(|_| "Use contact depth 0–6")?;
    contacts.exclude_everywhere = query.exclude_everywhere.clone();
    contacts.intermediaries = filters::read(world, "intermediaries")?;
    contacts.targets = filters::read(world, "targets")?;
    query.contacts = Some(contacts);
    query.levels = [
        ("need", EvidenceLevel::NeedLinkedReceiver),
        ("receiver", EvidenceLevel::Receiver),
        ("giver", EvidenceLevel::Giver),
    ]
    .into_iter()
    .filter(|(name, _)| {
        mobile.draft(
            "trust-policy",
            &format!("level-{name}"),
            if *name == "need" { "true" } else { "false" },
        ) == "true"
    })
    .map(|(_, level)| level)
    .collect();
    query.weights.need_linked_receiver =
        nucleus::DecimalValue::parse_inferred(&value("need_weight", "1"))
            .map_err(|error| error.to_string())?;
    query.weights.receiver =
        nucleus::DecimalValue::parse_inferred(&value("receiver_weight", "0.5"))
            .map_err(|error| error.to_string())?;
    query.weights.giver = nucleus::DecimalValue::parse_inferred(&value("giver_weight", "0.1"))
        .map_err(|error| error.to_string())?;
    query.weights.distance_decay =
        nucleus::DecimalValue::parse_inferred(&value("distance_weight", "0.5"))
            .map_err(|error| error.to_string())?;
    query.validate()?;
    Ok(query)
}

fn ask_draft(world: &World) -> Result<nucleus::trust_analysis::remote::Draft, String> {
    Ok(nucleus::trust_analysis::remote::Draft {
        query: trust_query(world)?,
        as_organ: false,
        linked_claims: Vec::new(),
        include_contacts: true,
        receipt_sources: Vec::new(),
        contact_subjects: Vec::new(),
    })
}

pub fn apply(world: &mut World, intent: Intent) -> Result<(), String> {
    ensure_scope(world);
    if world
        .resource::<State>()
        .pending
        .as_ref()
        .is_some_and(|(_, kind)| kind == "mutation")
    {
        return Err("Wait for the submitted change before editing this form".into());
    }
    let person = person(world);
    match intent {
        Intent::ResolveMeasurement => {
            app::capture(world);
            let concept = world.resource::<Mobile>().draft("trust", "concept", "");
            let unit = world.resource::<Mobile>().draft("trust", "unit", "");
            let key = format!("measurement:{concept}:{unit}");
            send(
                world,
                json!({"action":"vocabulary","request":{"command":"inspect_measurement","concept":concept,"unit":if unit.is_empty() { None } else { Some(unit) }}}),
                &key,
            )?;
        }
        Intent::ClearMeasurement => {
            world.resource_mut::<State>().measurements.clear();
            world.resource_mut::<State>().remote_review = None;
        }
        Intent::TrustFilter(name) => {
            app::capture(world);
            world.resource_mut::<State>().filter_editor = name;
        }
        Intent::TrustValue(key, value) => {
            app::capture(world);
            world
                .resource_mut::<Mobile>()
                .drafts
                .insert(format!("trust-policy/{key}"), value);
            world.resource_mut::<State>().remote_review = None;
        }
        Intent::New(preset) => {
            if !nucleus::valid_uid(&person, "r") {
                return Err("Choose your acting Person first".into());
            }
            open(world, model::composer(&person, &preset))?;
        }
        Intent::Open(form) => open(world, form)?,
        Intent::Choose(index, value) => {
            commit_fields(world)?;
            let state = &mut *world.resource_mut::<State>();
            let form = state.form.as_mut().ok_or("This form was closed")?;
            form.fields
                .get_mut(index)
                .ok_or("This field changed")?
                .value = value;
            form.commit_fields()?;
            model::bind_preset(form);
            state.review = None;
        }
        Intent::Step(step) => {
            commit_fields(world)?;
            let mut form = world
                .resource_mut::<State>()
                .form
                .take()
                .ok_or("This form was closed")?;
            form.step = Some(step.min(4));
            model::fields::composer_fields(&mut form);
            let review = if step == 4 {
                Some(fingerprint(world, &form)?)
            } else {
                None
            };
            let mut state = world.resource_mut::<State>();
            state.form = Some(form);
            state.review = review;
        }
        Intent::Submit => {
            commit_fields(world)?;
            let form = world
                .resource::<State>()
                .form
                .clone()
                .ok_or("This form was closed")?;
            if world.resource::<State>().review.as_ref() != Some(&fingerprint(world, &form)?) {
                return Err("Review the current terms before confirming".into());
            }
            if let Some(row) = current(world)
                && form.transfer.as_deref() == row["uid"].as_str()
                && form
                    .revision
                    .is_some_and(|revision| row["revision"].as_u64() != Some(revision))
            {
                return Err("Transfer terms changed; refresh and review them again".into());
            }
            if form.mode == "needs-settlement-review" {
                return Err("Review the exact settlement before confirming".into());
            }
            send(world, form.payload()?, "mutation")?;
        }
        Intent::ReviewAction => {
            commit_fields(world)?;
            let form = world
                .resource::<State>()
                .form
                .clone()
                .ok_or("Open a form first")?;
            if form.step.is_some() || form.mode == "needs-settlement-review" {
                return Err("Use the draft or settlement review controls".into());
            }
            let key = fingerprint(world, &form)?;
            let mut fields = serde_json::Map::new();
            for field in &form.fields {
                fields.insert(
                    field.label.clone(),
                    form.data
                        .pointer(&field.path)
                        .cloned()
                        .unwrap_or(Value::Null),
                );
            }
            let state = &mut *world.resource_mut::<State>();
            state.form.as_mut().unwrap().review["Current fields"] = Value::Object(fields);
            state.review = Some(key);
        }
        Intent::Cancel => {
            if world
                .resource::<State>()
                .form
                .as_ref()
                .is_some_and(draft_allowed)
            {
                world
                    .resource_mut::<Mobile>()
                    .drafts
                    .remove("workflow-draft/current");
            }
            let mut state = world.resource_mut::<State>();
            state.form = None;
            state.review = None;
        }
        Intent::AddPromise => {
            commit_fields(world)?;
            let state = &mut *world.resource_mut::<State>();
            let form = state.form.as_mut().ok_or("Open a draft first")?;
            form.data["promises"]
                .as_array_mut()
                .ok_or("This form has no items")?
                .push(model::promise(&person));
            state.review = None;
        }
        Intent::RemovePromise(index) => {
            commit_fields(world)?;
            let state = &mut *world.resource_mut::<State>();
            let form = state.form.as_mut().ok_or("Open a draft first")?;
            let promises = form.data["promises"]
                .as_array_mut()
                .ok_or("This form has no items")?;
            if promises.len() > 1 && index < promises.len() {
                promises.remove(index);
            }
            state.review = None;
        }
        Intent::RefreshAgreement => {
            let row = current(world).ok_or("Open a Transfer first")?;
            send(
                world,
                json!({"action":"inspect-transfer-karma","transfer":row["uid"],"person":person}),
                "karma",
            )?;
        }
        Intent::Agreement(level) => {
            let row = current(world).ok_or("Open a Transfer first")?;
            let snapshot = lince_interface::karma::transfers::snapshot(&row)
                .or_else(|| {
                    serde_json::from_value(world.resource::<State>().karma["state"].clone()).ok()
                })
                .ok_or("Refresh the agreement review first")?;
            if row["uid"] != snapshot.transfer
                || row["revision"].as_u64() != Some(snapshot.revision)
            {
                return Err("Refresh the agreement review for these terms".into());
            }
            let action = model::actions::agreement(&snapshot, &person, level)
                .ok_or("This Person is not a participant")?;
            let mut form = Form::action(
                if level == 2 {
                    "Agree to these terms"
                } else {
                    "Update my agreement level"
                },
                action,
            );
            form.actor = Some(person);
            form.transfer = Some(snapshot.transfer);
            form.revision = Some(snapshot.revision);
            form.review = json!({"Agreement level":level,"Terms revision":snapshot.revision,"Sharing":"Agreement does not start live location or record delivery."});
            open(world, form)?;
        }
        Intent::PreviewSettlement => {
            commit_fields(world)?;
            let form = world
                .resource::<State>()
                .form
                .clone()
                .ok_or("Open a settlement first")?;
            if form.mode != "needs-settlement-review" || world.resource::<State>().pending.is_some()
            {
                return Err("Wait for the current review or reopen settlement".into());
            }
            let quantity = form.data["canonical_quantity"]
                .as_f64()
                .filter(|value| value.is_finite() && *value > 0.0)
                .ok_or("Enter a positive settlement quantity")?;
            let query = serde_json::from_value(json!({"source":"transfer_settlement_preview","where":[{"uid_eq":form.data["occurrence"]},{"quantity_eq":quantity}]})).map_err(|error| error.to_string())?;
            let id = nucleus::new_uid("mobile-workflow");
            let key = fingerprint(world, &form)?;
            app::send(
                world,
                cell::ClientMessage::Subscribe {
                    id: id.clone(),
                    protein: query,
                },
            )?;
            let mut state = world.resource_mut::<State>();
            state.pending = Some((id, format!("settlement-preview:{key}")));
            state.result = Value::Null;
        }
        Intent::AcceptSettlement => {
            commit_fields(world)?;
            let state = world.resource::<State>();
            let form = state.form.clone().ok_or("Open a settlement first")?;
            if state.review.as_deref() != Some(&fingerprint(world, &form)?)
                || state.pending.is_some()
            {
                return Err("The amount or Person changed; request a fresh review".into());
            }
            let preview = if state.result["settlement_preview"].is_object() {
                &state.result["settlement_preview"]
            } else {
                &state.result
            };
            let action = model::actions::reviewed_payload(&form, preview, &person)?;
            let mut reviewed = Form::action("Confirm reviewed settlement", action);
            reviewed.actor = Some(person);
            reviewed.mode = "reviewed".into();
            reviewed.review = preview.clone();
            open(world, reviewed)?;
        }
        Intent::Analyze => {
            app::capture(world);
            let query = trust_query(world)?;
            send(
                world,
                json!({"action":"trust-analysis","person":person,"request_id":nucleus::new_uid("trust-mobile"),"request":{"kind":"analyze_query","query":query,"as_organ":false}}),
                "analysis",
            )?;
        }
        Intent::PreviewAsk => {
            app::capture(world);
            let draft = ask_draft(world)?;
            world.resource_mut::<State>().remote_review = None;
            send(
                world,
                json!({"action":"trust-analysis","person":person,"request_id":nucleus::new_uid("trust-mobile"),"request":{"kind":"remote","request":{"kind":"preview","draft":draft}}}),
                "preview",
            )?;
        }
        Intent::StartAsk => {
            app::capture(world);
            let review = world
                .resource::<State>()
                .remote_review
                .clone()
                .ok_or("Review the questions and recipients first")?;
            if review.person != person || review.draft != ask_draft(world)? {
                return Err("The subject, reader or filters changed; preview again".into());
            }
            let hash = review.hash()?;
            send(
                world,
                json!({"action":"trust-analysis","person":person,"request_id":nucleus::new_uid("trust-mobile"),"request":{"kind":"remote","request":{"kind":"start","review":review,"expected_review":hash}}}),
                "start",
            )?;
        }
        Intent::PollAsk | Intent::VerifyAsk | Intent::CancelAsk => {
            let job = world
                .resource::<State>()
                .job
                .clone()
                .ok_or("No ask-around is active")?;
            let kind = if matches!(intent, Intent::PollAsk) {
                "poll"
            } else if matches!(intent, Intent::VerifyAsk) {
                "verify"
            } else {
                "cancel"
            };
            send(
                world,
                json!({"action":"trust-analysis","person":person,"request_id":nucleus::new_uid("trust-mobile"),"request":{"kind":"remote","request":{"kind":kind,"job":job}}}),
                kind,
            )?;
        }
        Intent::Detail(value) => {
            let mut state = world.resource_mut::<State>();
            state.detail = (!value.is_null()).then_some(value);
            state.pages.clear();
        }
        Intent::ResultPage(path, delta) => {
            let state = &mut *world.resource_mut::<State>();
            let page = state.pages.entry(path).or_default();
            *page = page.saturating_add_signed(delta);
        }
    }
    world.resource_mut::<Mobile>().dirty = true;
    Ok(())
}

pub fn receive(world: &mut World, message: &cell::ServerMessage) -> bool {
    let (id, data, error, created) = match message {
        cell::ServerMessage::ActionOk {
            id, data, created, ..
        } if id.starts_with("mobile-workflow_") => (
            id,
            data.clone().unwrap_or(Value::Null),
            None,
            created.clone(),
        ),
        cell::ServerMessage::Error { id, message, .. } if id.starts_with("mobile-workflow_") => {
            (id, Value::Null, Some(message.clone()), None)
        }
        cell::ServerMessage::Snapshot { id, rows } if id.starts_with("mobile-workflow_") => {
            let _ = app::send(world, cell::ClientMessage::Unsubscribe { id: id.clone() });
            ensure_scope(world);
            let Some((_, kind)) = world
                .resource::<State>()
                .pending
                .clone()
                .filter(|(pending, _)| pending == id)
            else {
                return true;
            };
            let expected = kind.strip_prefix("settlement-preview:");
            let form = world.resource::<State>().form.clone();
            let matches = form
                .as_ref()
                .and_then(|form| fingerprint(world, form).ok())
                .is_some_and(|key| Some(key.as_str()) == expected);
            let state = &mut *world.resource_mut::<State>();
            state.pending = None;
            state.result = if matches {
                rows.first().cloned().unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            state.review = if matches {
                expected.map(str::to_owned)
            } else {
                None
            };
            world.resource_mut::<Mobile>().dirty = true;
            return true;
        }
        _ => return false,
    };
    ensure_scope(world);
    let Some((pending, kind)) = world
        .resource::<State>()
        .pending
        .clone()
        .filter(|(pending, _)| pending == id)
    else {
        return true;
    };
    let _ = pending;
    let completed_draft = kind == "mutation"
        && world
            .resource::<State>()
            .form
            .as_ref()
            .is_some_and(draft_allowed);
    let measurement_key = format!(
        "measurement:{}:{}",
        world.resource::<Mobile>().draft("trust", "concept", ""),
        world.resource::<Mobile>().draft("trust", "unit", "")
    );
    let state = &mut *world.resource_mut::<State>();
    state.pending = None;
    if let Some(error) = error {
        world.resource_mut::<Mobile>().status = error;
    } else {
        if kind == "karma" {
            state.karma = data.clone();
        }
        if kind == measurement_key
            && let Ok(measurement) = serde_json::from_value(data["measurement"].clone())
        {
            state.measurements = vec![measurement];
            state.remote_review = None;
        }
        if data["kind"] == "remote_review" {
            state.remote_review = serde_json::from_value(data["review"].clone()).ok();
        }
        if data["kind"] == "remote_started" {
            state.job = data["job"].as_str().map(str::to_owned);
            state.remote_review = None;
        }
        if kind == "mutation" {
            state.form = None;
            state.review = None;
        }
        state.result = data;
        state.detail = None;
        state.pages.clear();
        if completed_draft {
            world
                .resource_mut::<Mobile>()
                .drafts
                .remove("workflow-draft/current");
        }
        if kind == "mutation"
            && let Some(uid) = created.filter(|uid| nucleus::valid_uid(uid, "r"))
        {
            world
                .resource_mut::<Mobile>()
                .navigation
                .open(Page::Transfer(uid));
        }
        world.resource_mut::<Mobile>().status = "Request completed".into();
    }
    world.resource_mut::<Mobile>().dirty = true;
    true
}

fn button(world: &mut World, parent: Entity, caption: &str, intent: Intent) {
    app::button(world, parent, caption, app::Intent::Workflow(intent));
}

pub fn render(world: &mut World, parent: Entity) {
    ensure_scope(world);
    let person = person(world);
    if !matches!(
        world.resource::<Mobile>().identity,
        crate::session::Identity::Person(_)
    ) {
        app::input(
            world,
            parent,
            "workflow",
            "person",
            "Acting Person Record",
            &person,
            false,
        );
    }
    if let Some(detail) = world.resource::<State>().detail.clone() {
        button(world, parent, "Back to view", Intent::Detail(Value::Null));
        details(world, parent, &detail, "/details", 0);
        return;
    }
    if world.resource::<State>().form.is_some() {
        render_form(world, parent);
        return;
    }
    match world.resource::<Mobile>().navigation.current.clone() {
        Page::Transfers => {
            discovery::entry(world, parent, None);
            let row = controls::row(world, parent);
            for preset in model::PRESETS {
                button(world, row, preset, Intent::New(preset.into()));
            }
            let rows = world
                .resource::<Mobile>()
                .rows
                .get("records")
                .cloned()
                .unwrap_or_default();
            for transfer in rows.iter().take(crate::views::PAGE_SIZE) {
                app::button(
                    world,
                    parent,
                    &title(transfer),
                    app::Intent::Open(Page::Transfer(text(transfer, "uid"))),
                );
            }
            let row = controls::row(world, parent);
            app::button(world, row, "Previous", app::Intent::Previous);
            app::button(world, row, "More", app::Intent::More);
        }
        Page::Transfer(uid) => {
            let Some(transfer) = current(world) else {
                app::label(
                    world,
                    parent,
                    "Transfer unavailable; refresh or check access.",
                    16.0,
                );
                return;
            };
            app::label(world, parent, &title(&transfer), 22.0);
            discovery::entry(world, parent, Some(&transfer));
            button(
                world,
                parent,
                "Refresh agreement review",
                Intent::RefreshAgreement,
            );
            for (level, caption) in [
                (0, "Withdraw my agreement"),
                (1, "Review these terms"),
                (2, "Agree to these terms"),
            ] {
                button(world, parent, caption, Intent::Agreement(level));
            }
            if transfer["capabilities"]["edit_terms"] == true {
                button(
                    world,
                    parent,
                    "Edit terms",
                    Intent::Open(model::edit(&transfer, false, &person)),
                );
            }
            for invitation in array(&transfer, "invitations") {
                for (cap, caption, verb) in [
                    ("accept", "Accept invitation", "accept-transfer-invitation"),
                    ("reject", "Decline invitation", "reject-transfer-invitation"),
                ] {
                    if invitation["capabilities"][cap] != true {
                        continue;
                    }
                    let mut action = model::actions::action_base(&transfer, verb, &person);
                    action["invitation"] = invitation["uid"].clone();
                    action["person"] = invitation["addressed_person"].clone();
                    let mut form = Form::action(caption, action);
                    form.review = json!({"Invitation":invitation["addressed_person_head"],"Transfer":title(&transfer)});
                    button(world, parent, caption, Intent::Open(form));
                }
            }
            for form in model::transport::journey_forms(&transfer, &person) {
                let caption = form.title.clone();
                button(world, parent, &caption, Intent::Open(form));
            }
            for form in model::actions::outcome_forms(&transfer, &person) {
                let caption = form.title.clone();
                button(world, parent, &caption, Intent::Open(form));
            }
            let record = model::transport::sharing_record(&transfer, &person);
            app::button(
                world,
                parent,
                "Review my live location sharing",
                app::Intent::OpenLocation(record, Some(uid.clone())),
            );
            let mut records = vec![uid];
            for promise in array(&transfer, "promises") {
                for field in ["origin", "destination"] {
                    if let Some(uid) = promise["transport"][field].as_str() {
                        records.push(uid.into());
                    }
                }
                if let Some(uid) = promise["record"].as_str() {
                    records.push(uid.into());
                }
            }
            records.extend(
                array(&transfer, "parties")
                    .iter()
                    .filter_map(|party| party["actor"].as_str())
                    .map(str::to_owned),
            );
            app::button(
                world,
                parent,
                "Map permitted locations",
                app::Intent::MapRecords(records),
            );
            for party in array(&transfer, "parties") {
                if let Some(uid) = party["actor"].as_str() {
                    app::button(
                        world,
                        parent,
                        &format!("Experience: {}", title(party)),
                        app::Intent::Open(Page::Trust("person".into(), uid.into())),
                    );
                }
            }
            for (name, field) in [
                ("Items", "promises"),
                ("Participants", "parties"),
                ("Journey", "journey"),
                ("Capacity", "transport_capacity"),
                ("Fulfilment", "observed_fulfilment"),
                ("Occurrences and settlement", "occurrences"),
            ] {
                app::label(world, parent, name, 18.0);
                details(
                    world,
                    parent,
                    &transfer[field],
                    &format!("/transfer/{field}"),
                    0,
                );
            }
        }
        Page::Trust(_, _) => render_trust(world, parent),
        _ => {}
    }
    let result = world.resource::<State>().result.clone();
    if !result.is_null() {
        discovery::results(world, parent, &result);
        details(world, parent, &result, "/result", 0);
    }
}

fn render_form(world: &mut World, parent: Entity) {
    let mut form = world.resource::<State>().form.clone().unwrap();
    if form.step.is_some() {
        model::fields::composer_fields(&mut form);
    }
    world.resource_mut::<State>().form = Some(form.clone());
    app::label(world, parent, &form.title, 22.0);
    if world
        .resource::<State>()
        .pending
        .as_ref()
        .is_some_and(|(_, kind)| kind == "mutation")
    {
        app::label(world, parent, "Saving the reviewed change…", 16.0);
        return;
    }
    let scope = format!("workflow-{}", form.request_id);
    for (index, field) in form.fields.iter().enumerate() {
        match &field.kind {
            FieldKind::Choice(options) => {
                app::label(world, parent, &field.label, 16.0);
                for option in options {
                    button(
                        world,
                        parent,
                        &model::human(option),
                        Intent::Choose(index, option.clone()),
                    );
                }
            }
            FieldKind::OptionalChoice(options) => {
                app::label(world, parent, &field.label, 16.0);
                for (value, label) in options {
                    button(world, parent, label, Intent::Choose(index, value.clone()));
                }
            }
            _ => {
                app::input(
                    world,
                    parent,
                    &scope,
                    &field.path,
                    &field.label,
                    &field.value,
                    false,
                );
            }
        }
    }
    if form.step == Some(2) {
        button(world, parent, "Add item", Intent::AddPromise);
        for index in 1..array(&form.data, "promises").len() {
            button(
                world,
                parent,
                &format!("Remove item {}", index + 1),
                Intent::RemovePromise(index),
            );
        }
    }
    if let Some(step) = form.step {
        if step > 0 {
            button(world, parent, "Previous", Intent::Step(step - 1));
        }
        if step < 4 {
            button(
                world,
                parent,
                if step == 3 { "Review draft" } else { "Next" },
                Intent::Step(step + 1),
            );
        } else {
            details(world, parent, &model::review_draft(&form), "/review", 0);
            button(world, parent, "Confirm reviewed draft", Intent::Submit);
        }
    } else if form.mode == "needs-settlement-review" {
        button(
            world,
            parent,
            "Request exact settlement review",
            Intent::PreviewSettlement,
        );
        let result = world.resource::<State>().result.clone();
        details(world, parent, &result, "/settlement-review", 0);
        if !result.is_null() {
            button(
                world,
                parent,
                "Use this settlement review",
                Intent::AcceptSettlement,
            );
        }
    } else {
        details(world, parent, &form.review, "/review", 0);
        if !form.fields.is_empty() {
            button(world, parent, "Review these changes", Intent::ReviewAction);
        }
        if world.resource::<State>().review.is_some() {
            button(world, parent, "Confirm reviewed action", Intent::Submit);
        }
    }
    button(world, parent, "Cancel", Intent::Cancel);
}

fn render_trust(world: &mut World, parent: Entity) {
    app::label(world, parent, "Visible transfer experience", 22.0);
    app::label(
        world,
        parent,
        "Amounts stay grouped by meaning and unit. Signatures establish authorship; reports are not proof of physical truth. Opening this view sends no ask-around.",
        15.0,
    );
    for (field, label, default) in [
        (
            "issuers",
            "Reporting Organs I rely on (comma-separated)",
            "",
        ),
        ("excluded", "Exclude these Organs everywhere", ""),
        ("anchor", "Contact-path starting Person", ""),
        ("depth", "Maximum contact distance", "3"),
        ("from", "From (date, time and timezone; optional)", ""),
        ("until", "Until (date, time and timezone; optional)", ""),
        ("concept", "Vocabulary Concept Record (optional)", ""),
        ("unit", "Unit Concept Record (optional)", ""),
        ("need_weight", "Need-linked receiver weight", "1"),
        ("receiver_weight", "Receiver-only weight", "0.5"),
        ("giver_weight", "Giver-only weight", "0.1"),
        ("distance_weight", "Weight per further contact hop", "0.5"),
    ] {
        app::input(world, parent, "trust", field, label, default, false);
    }
    filters::render(world, parent);
    button(
        world,
        parent,
        "Resolve this exact meaning and unit",
        Intent::ResolveMeasurement,
    );
    button(
        world,
        parent,
        "Group all visible meanings separately",
        Intent::ClearMeasurement,
    );
    let meanings = world.resource::<State>().measurements.clone();
    details(world, parent, &json!(meanings), "/measurements", 0);
    button(
        world,
        parent,
        "Analyze visible local evidence",
        Intent::Analyze,
    );
    button(
        world,
        parent,
        "Review ask-around questions and recipients",
        Intent::PreviewAsk,
    );
    if let Some(review) = world.resource::<State>().remote_review.clone() {
        details(
            world,
            parent,
            &json!({"Questions":review.draft,"Recipients":review.endpoints}),
            "/ask-review",
            0,
        );
        button(
            world,
            parent,
            "Start this reviewed ask-around",
            Intent::StartAsk,
        );
    }
    if world.resource::<State>().job.is_some() {
        button(world, parent, "Refresh ask-around", Intent::PollAsk);
        button(world, parent, "Verify gathered evidence", Intent::VerifyAsk);
        button(world, parent, "Cancel ask-around", Intent::CancelAsk);
    }
}

fn details(world: &mut World, parent: Entity, value: &Value, path: &str, depth: usize) {
    if value.is_null() {
        return;
    }
    if depth >= 4 && (value.is_array() || value.is_object()) {
        button(world, parent, "Open details", Intent::Detail(value.clone()));
        return;
    }
    match value {
        Value::Array(values) => {
            let page = world
                .resource::<State>()
                .pages
                .get(path)
                .copied()
                .unwrap_or_default()
                .min(values.len().saturating_sub(1) / 50);
            if page > 0 {
                button(
                    world,
                    parent,
                    "Previous results",
                    Intent::ResultPage(path.into(), -1),
                );
            }
            for (index, value) in values.iter().enumerate().skip(page * 50).take(50) {
                details(world, parent, value, &format!("{path}/{index}"), depth + 1);
            }
            if (page + 1) * 50 < values.len() {
                button(
                    world,
                    parent,
                    "More results",
                    Intent::ResultPage(path.into(), 1),
                );
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if !value.is_null() {
                    app::label(world, parent, &model::human(key), 15.0);
                    details(
                        world,
                        parent,
                        value,
                        &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                        depth + 1,
                    );
                }
            }
        }
        Value::String(uid) if nucleus::valid_uid(uid, "r") => {
            app::button(
                world,
                parent,
                uid,
                app::Intent::Open(Page::Record(uid.clone())),
            );
        }
        _ => {
            app::label(world, parent, &model::display(value), 15.0);
        }
    }
}
