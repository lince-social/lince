use super::*;
use bevy::ecs::message::Messages;
use cell::{ClientMessage, ServerMessage};

fn sender(world: &World) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    crate::native::protein_area::editor_sender(world, &RecordBinding { area: Entity::PLACEHOLDER, uid: String::new(), source: Source::Local })
}

fn query(ids: &BTreeSet<String>, namespace: &str) -> protein::Protein {
    protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::Any(ids.iter().cloned().map(protein::Predicate::UidEq).collect())],
        fields: Some(vec!["uid".into(), "head".into(), "extension".into()]),
        limit: Some(512),
        include: protein::Include { extension: Some(protein::ExtensionInclude { namespace: namespace.into() }), ..Default::default() },
        aggregate: None,
        order: Vec::new(),
    }
}

fn read_entry(namespace: &str, row: Value) -> Option<(String, Entry)> {
    let uid = row["uid"].as_str()?.to_owned();
    if !nucleus::valid_uid(&uid, "r") { return None; }
    let title = row["head"].as_str().unwrap_or("Opening preferences").chars().take(160).collect();
    let value = row.get("extension").cloned().unwrap_or(Value::Null);
    let raw = value.clone();
    let payload = (|| {
        if value.is_null() && namespace == WINDOW_NAMESPACE {
            return Ok(Payload::Window(Window { revision: 0, properties: Properties::default() }));
        }
        if serde_json::to_vec(&value).map_err(|error| error.to_string())?.len() > nucleus::window_opening::MAX_BYTES {
            return Err("Opening preferences exceed 64 KiB.".into());
        }
        let payload = if namespace == RULE_NAMESPACE {
            Payload::Rule(serde_json::from_value(value).map_err(|error| error.to_string())?)
        } else {
            Payload::Window(serde_json::from_value(value).map_err(|error| error.to_string())?)
        };
        payload.validate()?;
        Ok(payload)
    })();
    Some((uid, Entry { title, exists: true, value: raw, payload }))
}

fn retire(feed: &mut Feed, sender: Option<&tokio::sync::mpsc::Sender<ClientMessage>>) {
    if let Some(id) = feed.id.take() && let Some(sender) = sender {
        if matches!(sender.try_send(ClientMessage::Unsubscribe { id: id.clone() }), Err(tokio::sync::mpsc::error::TrySendError::Full(_))) {
            feed.retiring = Some(Retirement { id, sender: sender.clone() });
        }
    }
    feed.ready = false;
    feed.entries.clear();
    feed.deadline = None;
    feed.retry = None;
    feed.blocked = false;
}

fn subscribe(feed: &mut Feed, selected: BTreeSet<String>, sender: Option<&tokio::sync::mpsc::Sender<ClientMessage>>) {
    let now = Instant::now();
    if feed.selected != selected {
        let retained = std::mem::take(&mut feed.entries);
        retire(feed, sender);
        feed.selected = selected;
        feed.entries = retained.into_iter().filter(|(uid, _)| feed.selected.contains(uid)).collect();
        feed.error = None;
    }
    if feed.selected.len() > 512 {
        feed.blocked = true;
        feed.error = Some("Too many opening-preference Records are selected; close unused editors or remove a reference.".into());
    }
    if feed.retry.is_some_and(|retry| retry > now) { return; }
    if let Some(retiring) = &feed.retiring {
        match retiring.sender.try_send(ClientMessage::Unsubscribe { id: retiring.id.clone() }) {
            Ok(()) | Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => { feed.retiring = None; feed.retry = None; }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                if !feed.blocked { feed.deadline.get_or_insert(now + Duration::from_secs(10)); feed.retry = Some(now + Duration::from_millis(500)); }
                return;
            }
        }
    }
    if feed.selected.is_empty() {
        feed.ready = true;
        feed.error = None;
        feed.deadline = None;
        feed.retry = None;
        return;
    }
    if feed.blocked || feed.id.is_some() || feed.retry.is_some_and(|retry| retry > now) { return; }
    let Some(sender) = sender else {
        feed.error = Some("Connect to the local Cell to load opening preferences.".into());
        return;
    };
    let id = nucleus::new_uid("park-opening");
    match sender.try_send(ClientMessage::Subscribe { id: id.clone(), protein: query(&feed.selected, feed.namespace) }) {
        Ok(()) => { feed.id = Some(id); feed.deadline.get_or_insert(now + Duration::from_secs(10)); feed.retry = None; }
        Err(error) => { feed.error = Some(format!("Could not load opening preferences: {error}")); feed.deadline.get_or_insert(now + Duration::from_secs(10)); feed.retry = Some(now + Duration::from_millis(500)); }
    }
}

fn accept(feed: &mut Feed, rows: Vec<Value>) -> bool {
    let previous = feed.entries.clone();
    feed.deadline = None;
    if rows.len() > 512 || serde_json::to_vec(&rows).map_or(true, |bytes| bytes.len() > 4 * 1024 * 1024) {
        feed.ready = false;
        feed.error = Some("Opening-preference snapshot exceeds its limit.".into());
        feed.entries.clear();
        return previous != feed.entries;
    }
    feed.entries = rows.into_iter().filter_map(|row| read_entry(feed.namespace, row))
        .filter(|(uid, _)| feed.selected.contains(uid)).collect();
    for uid in &feed.selected {
        feed.entries.entry(uid.clone()).or_insert_with(|| Entry { title: uid.clone(), exists: false, value: Value::Null, payload: Err("The Record is missing or is not readable.".into()) });
    }
    feed.error = None;
    feed.ready = true;
    previous != feed.entries
}

pub(super) fn refresh(world: &mut World) {
    if let Some(mut catalog) = world.get_resource_mut::<Catalog>() {
        let sender = catalog.sender.clone();
        retire(&mut catalog.rules, sender.as_ref());
        retire(&mut catalog.windows, sender.as_ref());
        catalog.advance();
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
}

pub(super) struct Completion {
    pub uid: String,
    pub payload: Payload,
    pub created: bool,
    pub result: Result<(), String>,
}

pub(super) fn save(world: &mut World, owner: Entity, uid: Option<String>, title: String, payload: Payload, creating: bool) -> Result<String, String> {
    payload.validate()?;
    if crate::native::laboratory::blocked(world, owner) { return Err("Opening preferences cannot be saved from the Laboratory.".into()); }
    if title.trim().is_empty() || title.chars().count() > 160 || title.chars().any(char::is_control) { return Err("Give the rule a name of 1–160 characters.".into()); }
    let sender = sender(world).ok_or("Connect to the local Cell before saving opening preferences.")?;
    world.init_resource::<Catalog>();
    if world.resource::<Catalog>().pending.len() >= 16 { return Err("Opening-preference editor is busy; wait for a pending save.".into()); }
    if creating && !matches!(payload, Payload::Rule(_)) { return Err("Select an existing window Record.".into()); }
    let uid = uid.unwrap_or_else(|| nucleus::new_uid("r"));
    if !nucleus::valid_uid(&uid, "r") { return Err("The Record identity is invalid.".into()); }
    let action = if creating {
        engine::actions::Action::CreateRecordDraft { draft: engine::record_creation::Draft { uid: uid.clone(), head: title.clone(), body: "Application opening rule".into(), ..Default::default() } }
    } else {
        engine::actions::Action::SetExtension { target: uid.clone(), namespace: payload.namespace().into(), fds: payload.value() }
    };
    let id = nucleus::new_uid("park-opening-save");
    sender.try_send(ClientMessage::Act { id: id.clone(), action }).map_err(|error| error.to_string())?;
    world.resource_mut::<Catalog>().pending.insert(id, Pending { owner, uid: uid.clone(), title, payload, creating, created: false, deadline: Instant::now() + Duration::from_secs(10) });
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
    Ok(uid)
}

fn finish(world: &mut World, catalog: &mut Catalog, pending: Pending, result: Result<(), String>) {
    if result.is_ok() {
        let feed = if pending.payload.namespace() == RULE_NAMESPACE { &mut catalog.rules } else { &mut catalog.windows };
        let newer = feed.entries.get(&pending.uid).and_then(|entry| entry.payload.as_ref().ok()).is_some_and(|payload| payload.revision() > pending.payload.revision());
        if !newer {
            feed.entries.insert(pending.uid.clone(), Entry { title: pending.title.clone(), exists: true, value: pending.payload.value(), payload: Ok(pending.payload.clone()) });
            catalog.advance();
        }
    }
    ui::completed(world, pending.owner, Completion { uid: pending.uid, payload: pending.payload, created: pending.created, result });
}

fn clock(world: &World, catalog: &mut Catalog) {
    let next = [catalog.rules.deadline, catalog.rules.retry, catalog.windows.deadline, catalog.windows.retry].into_iter().flatten()
        .chain(catalog.pending.values().map(|pending| pending.deadline)).min();
    if catalog.clock.as_ref().map(|(deadline, _)| *deadline) == next { return; }
    if let Some((_, task)) = catalog.clock.take() { task.abort(); }
    if let Some(deadline) = next
        && let Some(wake) = world.get_resource::<crate::host::WakeSignal>().cloned()
        && let Ok(runtime) = tokio::runtime::Handle::try_current()
    {
        catalog.clock = Some((deadline, runtime.spawn(async move {
            tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
            wake.ring();
        })));
    }
}

pub(super) fn update(world: &mut World) {
    let mut catalog = world.remove_resource::<Catalog>().unwrap_or_default();
    let messages: Vec<_> = world.get_resource::<Messages<CellMessage>>().map(|messages| catalog.cursor.read(messages).map(|message| message.0.clone()).collect()).unwrap_or_default();
    let next_sender = sender(world);
    let changed = match (&catalog.sender, &next_sender) { (Some(old), Some(new)) => !old.same_channel(new), (None, None) => false, _ => true };
    let authentication = messages.iter().any(|message| matches!(message, ServerMessage::SessionChallenge { .. } | ServerMessage::SessionAuthenticated { .. }));
    if changed || authentication {
        catalog.advance();
        ui::context_changed(world);
        retire(&mut catalog.rules, catalog.sender.as_ref());
        retire(&mut catalog.windows, catalog.sender.as_ref());
        for (_, pending) in std::mem::take(&mut catalog.pending) {
            finish(world, &mut catalog, pending, Err("The Cell connection or login changed. The draft is kept; reload its Record before saving again.".into()));
        }
    }
    catalog.sender = next_sender;
    for message in messages {
        match message {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
                let changed = if catalog.rules.id.as_ref() == Some(&id) { accept(&mut catalog.rules, rows) }
                    else if catalog.windows.id.as_ref() == Some(&id) { accept(&mut catalog.windows, rows) } else { false };
                if changed { catalog.advance(); }
            }
            ServerMessage::Error { id, message, .. } => {
                if let Some(pending) = catalog.pending.remove(&id) { finish(world, &mut catalog, pending, Err(message)); }
                else {
                    for feed in [&mut catalog.rules, &mut catalog.windows] {
                        if feed.id.as_ref() == Some(&id) {
                            retire(feed, catalog.sender.as_ref());
                            feed.error = Some(message.clone());
                            feed.blocked = true;
                        }
                    }
                    catalog.advance();
                }
            }
            ServerMessage::ActionOk { id, created, .. } => {
                if let Some(mut pending) = catalog.pending.remove(&id) {
                    if pending.creating {
                        if created.as_deref() != Some(pending.uid.as_str()) {
                            finish(world, &mut catalog, pending, Err("Record creation returned a different identity.".into()));
                            continue;
                        }
                        pending.created = true;
                        pending.creating = false;
                        pending.deadline = Instant::now() + Duration::from_secs(10);
                        let id = nucleus::new_uid("park-opening-save");
                        let request = ClientMessage::Act { id: id.clone(), action: engine::actions::Action::SetExtension { target: pending.uid.clone(), namespace: pending.payload.namespace().into(), fds: pending.payload.value() } };
                        if catalog.sender.as_ref().is_some_and(|sender| sender.try_send(request).is_ok()) { catalog.pending.insert(id, pending); }
                        else { finish(world, &mut catalog, pending, Err("The rule Record was created, but saving its preferences could not be queued. The same Record can be retried.".into())); }
                    } else { finish(world, &mut catalog, pending, Ok(())); }
                }
            }
            _ => {}
        }
    }
    let now = Instant::now();
    let expired: Vec<_> = catalog.pending.iter().filter(|(_, pending)| pending.deadline <= now).map(|(id, _)| id.clone()).collect();
    for id in expired {
        if let Some(pending) = catalog.pending.remove(&id) { finish(world, &mut catalog, pending, Err("Saving timed out. The draft is kept; reload before retrying to check whether the Record was saved.".into())); }
    }
    let timed_out = [catalog.rules.deadline, catalog.windows.deadline].into_iter().flatten().any(|deadline| deadline <= now);
    for feed in [&mut catalog.rules, &mut catalog.windows] {
        if feed.deadline.is_some_and(|deadline| deadline <= now) {
            retire(feed, catalog.sender.as_ref());
            feed.error = Some("Loading opening preferences timed out. Refresh to try again.".into());
            feed.blocked = true;
        }
    }
    if timed_out { catalog.advance(); }
    let (extra_rules, extra_windows) = ui::references(world);
    let mut rules: BTreeSet<_> = world.get_resource::<LayoutSettings>().map(|settings| settings.opening_rules.iter().cloned().collect()).unwrap_or_default();
    rules.extend(extra_rules);
    let mut windows = catalog.window_refs.clone();
    windows.extend(extra_windows);
    subscribe(&mut catalog.rules, rules, catalog.sender.as_ref());
    subscribe(&mut catalog.windows, windows, catalog.sender.as_ref());
    clock(world, &mut catalog);
    world.insert_resource(catalog);
}

#[cfg(test)]
mod tests;
