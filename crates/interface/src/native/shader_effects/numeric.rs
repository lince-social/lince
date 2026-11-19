use super::*;
use nucleus::shader_effect::bindings::{NumericBinding, NumericSource, NumericValues};
use std::collections::{BTreeMap, BTreeSet};

mod preparation;

#[derive(Component, Clone)]
pub(super) struct Context {
    pub record: Option<String>,
    pub source: Source,
}

struct Batch {
    source: Source,
    requests: BTreeMap<String, protein::shader_inputs::Request>,
    id: String,
    sent: bool,
    ever_sent: bool,
    rows: HashMap<String, Value>,
    retry: Instant,
}

#[derive(Resource, Default)]
pub(super) struct NumericFeeds {
    batches: BTreeMap<String, Batch>,
    retired: Vec<(Source, String)>,
    limited: BTreeSet<(Entity, u8)>,
}

pub(super) fn organs(world: &World) -> std::collections::HashSet<String> {
    world
        .get_resource::<NumericFeeds>()
        .map_or_else(Default::default, |feeds| {
            feeds
                .batches
                .values()
                .map(|batch| &batch.source)
                .chain(feeds.retired.iter().map(|(source, _)| source))
                .filter_map(|source| match source {
                    Source::Organ(uid) => Some(uid.clone()),
                    Source::Local => None,
                })
                .collect()
        })
}

fn source_key(source: &Source) -> String {
    serde_json::to_string(source).unwrap()
}

pub(super) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    if !matches!(
        message,
        ServerMessage::Snapshot { .. }
            | ServerMessage::Update { .. }
            | ServerMessage::Error { .. }
            | ServerMessage::LiveHello {
                login_required: true
            }
            | ServerMessage::SessionAuthenticated { .. }
    ) {
        return;
    }
    let Some(mut feeds) = world.get_resource_mut::<NumericFeeds>() else {
        return;
    };
    if feeds.batches.is_empty() {
        return;
    }
    let Some(batch) = feeds.batches.get_mut(&source_key(source)) else {
        return;
    };
    if matches!(
        message,
        ServerMessage::LiveHello {
            login_required: true
        } | ServerMessage::SessionAuthenticated { .. }
    ) || matches!(message, ServerMessage::Error { id, code, .. } if id == "connection" || id == crate::native::cell_bridge::CONNECTION || code.as_deref() == Some("session_expired"))
    {
        batch.rows.clear();
        batch.sent = false;
        batch.retry = Instant::now() + Duration::from_secs(3);
    }
    match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
            if *id == batch.id && batch.sent =>
        {
            let wanted: BTreeSet<_> = batch
                .requests
                .values()
                .map(|request| request.record.as_str())
                .collect();
            batch.rows = rows
                .iter()
                .take(256)
                .filter_map(|row| {
                    let uid = row["uid"].as_str()?;
                    if !wanted.contains(uid) {
                        return None;
                    }
                    let values = batch
                        .requests
                        .values()
                        .filter(|request| request.record == uid)
                        .map(|request| {
                            let key = protein::shader_inputs::key(&request.field);
                            let value = row["numeric_values"][&key]
                                .as_str()
                                .filter(|value| {
                                    value.len() <= 128
                                        && nucleus::DecimalValue::parse_inferred(value).is_ok()
                                })
                                .map_or(Value::Null, |value| Value::String(value.to_owned()));
                            (key, value)
                        })
                        .collect::<serde_json::Map<_, _>>();
                    Some((uid.to_owned(), serde_json::json!({"numeric_values":values})))
                })
                .collect();
        }
        ServerMessage::Error { id, .. } if *id == batch.id || id == "connection" => {
            batch.rows.clear();
            batch.sent = false;
            batch.retry = Instant::now() + Duration::from_secs(3);
        }
        ServerMessage::LiveHello {
            login_required: true,
            ..
        } => {
            batch.rows.clear();
            batch.sent = false;
        }
        _ => {}
    }
}

pub(super) fn update(world: &mut World) {
    let _update = nucleus::performance::Span::new("shader-numeric-update");
    world.init_resource::<NumericFeeds>();
    world
        .run_system_cached(update_inputs)
        .expect("update current shader numeric inputs");
}

fn update_inputs(
    world: &mut World,
    previews: &mut QueryState<(Entity, &'static shader::EffectPreview, &'static Context)>,
    mut prepared: Local<preparation::Preparation>,
) {
    prepared.refresh(world, previews);
    for entity in prepared.inactive.drain(..) {
        let mut preview = world.get_mut::<shader::EffectPreview>(entity).unwrap();
        let changed =
            preview.numeric != NumericValues::default() || !preview.numeric_bindings.is_empty();
        if preview.numeric != NumericValues::default() {
            preview.numeric = default();
        }
        if !preview.numeric_bindings.is_empty() {
            preview.numeric_bindings.clear();
        }
        if changed {
            nucleus::performance::add("shader-numeric-preview-updates", 1);
        }
    }
    let feeds = world.resource::<NumericFeeds>();
    if prepared.demands.is_empty()
        && feeds.batches.is_empty()
        && feeds.retired.is_empty()
        && feeds.limited.is_empty()
    {
        return;
    }
    let (desired, limited) = prepared.admit();
    let feed_maintenance = nucleus::performance::Span::new("shader-numeric-feed-maintenance");
    let mut feeds = world.remove_resource::<NumericFeeds>().unwrap();
    if feeds.limited != limited {
        feeds.limited = limited;
    }
    let obsolete: Vec<_> = feeds
        .batches
        .iter()
        .filter(|(key, batch)| {
            desired
                .get(key.as_str())
                .is_none_or(|desired| !desired.matches(&batch.requests))
        })
        .map(|(key, _)| key.clone())
        .collect();
    for key in obsolete {
        if feeds.retired.len() >= 32 {
            break;
        }
        let batch = feeds.batches.remove(&key).unwrap();
        if batch.ever_sent {
            feeds.retired.push((batch.source, batch.id));
        }
    }
    feeds.retired.retain(|(source, id)| {
        let Some(sender) = crate::native::protein_area::auxiliary_sender(world, source) else {
            let message = ClientMessage::Unsubscribe { id: id.clone() };
            crate::native::protein_area::auxiliary_subscribed(world, source, &message);
            return false;
        };
        match sender.try_reserve() {
            Ok(permit) => {
                let message = ClientMessage::Unsubscribe { id: id.clone() };
                crate::native::protein_area::send_auxiliary(world, source, permit, message);
                false
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => true,
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                let message = ClientMessage::Unsubscribe { id: id.clone() };
                crate::native::protein_area::auxiliary_subscribed(world, source, &message);
                false
            }
        }
    });
    for (key, desired) in desired {
        if feeds.batches.contains_key(key) || feeds.retired.len() >= 32 || feeds.batches.len() >= 16
        {
            continue;
        }
        feeds.batches.insert(
            key.to_owned(),
            Batch {
                source: desired.source.clone(),
                requests: desired
                    .requests
                    .into_iter()
                    .map(|(key, request)| (key.to_owned(), request.clone()))
                    .collect(),
                id: nucleus::new_uid("numeric-feed"),
                sent: false,
                ever_sent: false,
                rows: HashMap::new(),
                retry: Instant::now(),
            },
        );
    }
    let retiring = !feeds.retired.is_empty();
    for batch in feeds.batches.values_mut() {
        crate::native::protein_area::ensure_auxiliary(world, &batch.source);
        let sender = crate::native::protein_area::auxiliary_sender(world, &batch.source)
            .filter(|sender| !sender.is_closed());
        if sender.is_none() {
            batch.rows.clear();
            batch.sent = false;
        }
        if !retiring
            && !batch.sent
            && batch.retry <= Instant::now()
            && crate::native::laboratory::normal(world)
            && let Some(sender) = sender
        {
            if let Ok(permit) = sender.try_reserve() {
                let records: BTreeSet<_> = batch
                    .requests
                    .values()
                    .map(|request| request.record.as_str())
                    .collect();
                let message = ClientMessage::Subscribe {
                    id: batch.id.clone(),
                    protein: protein::Protein {
                        source: protein::Source::Record,
                        filter: vec![protein::Predicate::Any(
                            records
                                .iter()
                                .map(|uid| protein::Predicate::UidEq((*uid).to_owned()))
                                .collect(),
                        )],
                        fields: Some(["uid", "numeric_values"].map(str::to_owned).into()),
                        include: protein::Include {
                            numeric_fields: batch.requests.values().cloned().collect(),
                            ..default()
                        },
                        aggregate: None,
                        order: vec![],
                        limit: Some(records.len()),
                    },
                };
                crate::native::protein_area::send_auxiliary(world, &batch.source, permit, message);
                batch.sent = true;
                batch.ever_sent = true;
                nucleus::performance::add("shader-numeric-subscriptions-prepared", 1);
            }
            batch.retry = Instant::now() + Duration::from_secs(3);
        }
    }
    drop(feed_maintenance);
    {
        let _publication = nucleus::performance::Span::new("shader-numeric-value-publication");
        for (entity, _) in &prepared.demands {
            let input = &prepared.inputs[entity];
            let preview = world.get::<shader::EffectPreview>(*entity).unwrap();
            let values = input.values(&feeds, *entity);
            let unchanged = values
                .iter()
                .enumerate()
                .all(|(slot, value)| preview.numeric.get(slot as u8) == *value);
            let values = if unchanged {
                None
            } else {
                nucleus::performance::add("shader-numeric-value-resolutions", 1);
                Some(input.resolve(&feeds, *entity))
            };
            let bindings_changed = preview.numeric_bindings != input.bindings;
            if values
                .as_ref()
                .is_some_and(|values| &preview.numeric != values)
                || bindings_changed
            {
                let mut preview = world.get_mut::<shader::EffectPreview>(*entity).unwrap();
                if let Some(values) = values
                    && preview.numeric != values
                {
                    preview.numeric = values;
                }
                if bindings_changed {
                    preview.numeric_bindings.clone_from(&input.bindings);
                }
                nucleus::performance::add("shader-numeric-preview-updates", 1);
            }
        }
    }
    let active = !feeds.batches.is_empty() || !feeds.retired.is_empty();
    world.insert_resource(feeds);
    if active && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.after(Duration::from_secs(3));
    }
}

pub(super) fn status_fields(world: &World, entity: Entity) -> impl Iterator<Item = (&str, &str)> {
    let preview = world.get::<shader::EffectPreview>(entity);
    let effect = preview.and_then(shader::EffectPreview::working);
    let current = effect
        .zip(preview)
        .is_some_and(|(effect, preview)| effect.bindings == preview.numeric_bindings);
    let feeds = world.get_resource::<NumericFeeds>();
    effect.into_iter().flat_map(move |effect| {
        effect.bindings.iter().map(move |binding| {
            let value = current
                .then(|| preview.unwrap().numeric.get(binding.slot))
                .flatten()
                .unwrap_or_else(|| {
                    if feeds.is_some_and(|feeds| feeds.limited.contains(&(entity, binding.slot))) {
                        "reading limit reached"
                    } else {
                        "unavailable"
                    }
                });
            (binding.label.as_str(), value)
        })
    })
}

pub(super) fn status(world: &World, entity: Entity) -> String {
    nucleus::performance::add("shader-numeric-status-formats", 1);
    let mut status = String::new();
    for (label, value) in status_fields(world, entity) {
        if !status.is_empty() {
            status.push_str(" · ");
        }
        status.push_str(label);
        status.push_str(": ");
        status.push_str(value);
    }
    status
}

pub(super) fn controls(world: &mut World, owner: Entity, panel: Entity, effect: &Effect) {
    crate::native::sand_panel::button(
        world,
        panel,
        owner,
        "Add numeric input",
        Command::AddBinding,
    );
    if !effect.bindings.is_empty() {
        crate::native::edit_mode::label(
            world,
            panel,
            "Rules compare exact stored decimals. WGSL receives f32 approximations; a presence flag of 0 means the reading is unavailable.",
            12.0,
        );
    }
    for binding in &effect.bindings {
        let slot = binding.slot;
        let group = slot / 4;
        let axis = ["x", "y", "z", "w"][usize::from(slot % 4)];
        crate::native::edit_mode::label(
            world,
            panel,
            &format!(
                "Numeric slot {slot}: inputs.numeric{group}.{axis}; check inputs.present{group}.{axis} before use"
            ),
            13.0,
        );
        input(
            world,
            panel,
            owner,
            InputKey::BindingLabel(slot),
            "Input label",
            &binding.label,
            false,
        );
        input(
            world,
            panel,
            owner,
            InputKey::BindingRecord(slot),
            "Record UID (empty uses this Record or the Sand's reference)",
            binding.source.record.as_deref().unwrap_or(""),
            false,
        );
        input(
            world,
            panel,
            owner,
            InputKey::BindingOrgan(slot),
            "Organ UID (empty inherits the source)",
            binding.source.organ.as_deref().unwrap_or(""),
            false,
        );
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            match &binding.source.field {
                model::bindings::NumericField::Quantity => "Input kind: quantity (change)",
                model::bindings::NumericField::Extension { .. } => {
                    "Input kind: extension number (change)"
                }
            },
            Command::BindingField(slot),
        );
        if let model::bindings::NumericField::Extension { namespace, path } = &binding.source.field
        {
            input(
                world,
                panel,
                owner,
                InputKey::BindingNamespace(slot),
                "Extension namespace",
                namespace,
                false,
            );
            let pointer = path
                .iter()
                .map(|part| format!("/{}", part.replace('~', "~0").replace('/', "~1")))
                .collect::<String>();
            input(
                world,
                panel,
                owner,
                InputKey::BindingPath(slot),
                "Field path (for example /daily/protein)",
                &pointer,
                false,
            );
        }
        crate::native::sand_panel::button(
            world,
            panel,
            owner,
            "Remove numeric input",
            Command::RemoveBinding(slot),
        );
    }
}

fn decoded_path_part(value: &str) -> impl Iterator<Item = char> + '_ {
    let mut chars = value.chars().peekable();
    std::iter::from_fn(move || {
        let current = chars.next()?;
        Some(if current == '~' {
            match chars.peek() {
                Some('0') => {
                    chars.next();
                    '~'
                }
                Some('1') => {
                    chars.next();
                    '/'
                }
                _ => current,
            }
        } else {
            current
        })
    })
}

pub(super) fn input_changed(editor: &Editor, key: &InputKey, input: &EditableText) -> bool {
    let slot = match key {
        InputKey::BindingLabel(slot)
        | InputKey::BindingRecord(slot)
        | InputKey::BindingOrgan(slot)
        | InputKey::BindingNamespace(slot)
        | InputKey::BindingPath(slot) => *slot,
        _ => return false,
    };
    let Some(binding) = editor
        .effect
        .bindings
        .iter()
        .find(|binding| binding.slot == slot)
    else {
        return false;
    };
    match key {
        InputKey::BindingLabel(_) => input.value() != binding.label.as_str(),
        InputKey::BindingNamespace(_) => matches!(&binding.source.field,
            model::bindings::NumericField::Extension { namespace, .. } if input.value() != namespace.as_str()),
        InputKey::BindingRecord(_) | InputKey::BindingOrgan(_) => {
            let text = input.value().to_cow();
            let text = text.trim();
            let current = if matches!(key, InputKey::BindingRecord(_)) {
                &binding.source.record
            } else {
                &binding.source.organ
            };
            current.as_deref() != (!text.is_empty()).then_some(text)
        }
        InputKey::BindingPath(_) => {
            let model::bindings::NumericField::Extension { path, .. } = &binding.source.field
            else {
                return false;
            };
            let text = input.value().to_cow();
            let Some(pointer) = text.strip_prefix('/') else {
                return !path.is_empty();
            };
            let mut parts = pointer.split('/');
            for current in path {
                if !parts
                    .next()
                    .is_some_and(|part| decoded_path_part(part).eq(current.chars()))
                {
                    return true;
                }
            }
            parts.next().is_some()
        }
        _ => false,
    }
}

pub(super) fn edit(world: &mut World, owner: Entity, key: &InputKey, value: String) {
    let slot = match key {
        InputKey::BindingLabel(slot)
        | InputKey::BindingRecord(slot)
        | InputKey::BindingOrgan(slot)
        | InputKey::BindingNamespace(slot)
        | InputKey::BindingPath(slot) => *slot,
        _ => return,
    };
    let mut editor = world.get_mut::<Editor>(owner).unwrap();
    let Some(binding) = editor
        .effect
        .bindings
        .iter_mut()
        .find(|binding| binding.slot == slot)
    else {
        return;
    };
    match key {
        InputKey::BindingLabel(_) => binding.label = value,
        InputKey::BindingRecord(_) => {
            binding.source.record = (!value.trim().is_empty()).then(|| value.trim().to_owned())
        }
        InputKey::BindingOrgan(_) => {
            binding.source.organ = (!value.trim().is_empty()).then(|| value.trim().to_owned())
        }
        InputKey::BindingNamespace(_) => {
            if let model::bindings::NumericField::Extension { namespace, .. } =
                &mut binding.source.field
            {
                *namespace = value;
            }
        }
        InputKey::BindingPath(_) => {
            if let model::bindings::NumericField::Extension { path, .. } = &mut binding.source.field
            {
                *path = value
                    .strip_prefix('/')
                    .map(|pointer| {
                        pointer
                            .split('/')
                            .map(|part| part.replace("~1", "/").replace("~0", "~"))
                            .collect()
                    })
                    .unwrap_or_default();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protein::shader_inputs::key;
    use std::sync::Arc;

    #[test]
    fn numeric_messages_stay_scoped_and_clear_withdrawn_or_expired_readings() {
        use model::bindings::NumericField;
        let mut world = World::new();
        world.init_resource::<NumericFeeds>();
        let uid = nucleus::new_uid("r");
        let field = NumericField::Quantity;
        world.resource_mut::<NumericFeeds>().batches.insert(
            source_key(&Source::Local),
            Batch {
                source: Source::Local,
                requests: [(
                    "one".into(),
                    protein::shader_inputs::Request {
                        record: uid.clone(),
                        field: field.clone(),
                    },
                )]
                .into(),
                id: "numbers".into(),
                sent: true,
                ever_sent: true,
                rows: default(),
                retry: Instant::now(),
            },
        );
        let message = ServerMessage::Snapshot {
            id: "numbers".into(),
            rows: vec![serde_json::json!({
            "uid":uid, "secret":"unrequested metadata", "numeric_values":{(key(&field)):"20", "other":"123"}
            })].into(),
        };
        receive(&mut world, &Source::Organ(nucleus::new_uid("r")), &message);
        assert!(
            world
                .resource::<NumericFeeds>()
                .batches
                .values()
                .next()
                .unwrap()
                .rows
                .is_empty()
        );
        receive(&mut world, &Source::Local, &message);
        let rows = &world
            .resource::<NumericFeeds>()
            .batches
            .values()
            .next()
            .unwrap()
            .rows;
        assert_eq!(rows[&uid]["numeric_values"][key(&field)], "20");
        assert_eq!(rows[&uid]["numeric_values"].as_object().unwrap().len(), 1);
        assert!(
            !serde_json::to_string(rows)
                .unwrap()
                .contains("unrequested metadata")
        );
        receive(
            &mut world,
            &Source::Local,
            &ServerMessage::Update {
                id: "numbers".into(),
                rows: vec![].into(),
            },
        );
        assert!(
            world
                .resource::<NumericFeeds>()
                .batches
                .values()
                .next()
                .unwrap()
                .rows
                .is_empty()
        );
        receive(&mut world, &Source::Local, &message);
        receive(
            &mut world,
            &Source::Local,
            &ServerMessage::SessionAuthenticated {
                id: "auth".into(),
                session_id: "renewed".into(),
                person: nucleus::new_uid("r"),
                key_id: "key".into(),
            },
        );
        receive(&mut world, &Source::Local, &message);
        assert!(
            world
                .resource::<NumericFeeds>()
                .batches
                .values()
                .next()
                .unwrap()
                .rows
                .is_empty()
        );
        world
            .resource_mut::<NumericFeeds>()
            .batches
            .values_mut()
            .next()
            .unwrap()
            .sent = true;
        receive(&mut world, &Source::Local, &message);
        assert_eq!(
            world
                .resource::<NumericFeeds>()
                .batches
                .values()
                .next()
                .unwrap()
                .rows[&uid]["numeric_values"][key(&field)],
            "20"
        );
        receive(
            &mut world,
            &Source::Local,
            &ServerMessage::Error {
                id: "session".into(),
                message: "Expired".into(),
                code: Some("session_expired".into()),
            },
        );
        assert!(
            world
                .resource::<NumericFeeds>()
                .batches
                .values()
                .next()
                .unwrap()
                .rows
                .is_empty()
        );
        assert!(
            !world
                .resource::<NumericFeeds>()
                .batches
                .values()
                .next()
                .unwrap()
                .sent
        );
        receive(&mut world, &Source::Local, &message);
        assert!(
            world
                .resource::<NumericFeeds>()
                .batches
                .values()
                .next()
                .unwrap()
                .rows
                .is_empty()
        );
        update(&mut world);
        assert!(world.resource::<NumericFeeds>().batches.is_empty());
        assert!(world.resource::<NumericFeeds>().retired.is_empty());
    }

    #[tokio::test]
    async fn queued_unsubscribes_survive_backpressure_until_transport_recovers() {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let runtime = cell::CellRuntime {
            commands: default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: default(),
            fiote: None,
            speech: None,
            information: None,
        };
        let bridge = crate::native::cell_bridge::connect(
            runtime,
            crate::native::wake::WakeSignal::new(|| {}),
        );
        while bridge
            .outgoing
            .try_send(ClientMessage::Unsubscribe { id: "fill".into() })
            .is_ok()
        {}
        let mut world = World::new();
        world.insert_non_send(bridge);
        world.init_resource::<NumericFeeds>();
        world
            .resource_mut::<NumericFeeds>()
            .retired
            .push((Source::Local, "retired".into()));
        update(&mut world);
        assert_eq!(world.resource::<NumericFeeds>().retired.len(), 1);
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::time::sleep(Duration::from_millis(10)).await;
                update(&mut world);
                if world.resource::<NumericFeeds>().retired.is_empty() {
                    break;
                }
            }
        })
        .await
        .unwrap();
    }

    async fn until(app: &mut App, condition: impl Fn(&mut World) -> bool) {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                shader::prepare_test_previews(app.world_mut());
                app.update();
                if condition(app.world_mut()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    fn press(world: &mut World, owner: Entity, caption: &str) {
        let button = world
            .query::<(
                &crate::native::actions::ActionButton,
                &crate::native::icons::Tooltip,
            )>()
            .iter(world)
            .find(|(button, tooltip)| button.target == owner && tooltip.0 == caption)
            .unwrap()
            .0
            .clone();
        button.actions.run(world, button.target);
    }

    fn fill(world: &mut World, owner: Entity, label: &str, value: &str) {
        let entity = world
            .query::<(Entity, &Input, &bevy::a11y::AccessibilityNode)>()
            .iter(world)
            .find(|(_, input, node)| input.editor == owner && node.label() == Some(label))
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(value);
    }

    fn value(world: &World, preview: Entity) -> Option<&str> {
        world
            .get::<shader::EffectPreview>(preview)
            .unwrap()
            .numeric
            .get(0)
    }

    #[tokio::test]
    async fn changing_preview_source_releases_old_readings_before_resubscribing() {
        let mut app = App::new();
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let bridge = crate::native::cell_bridge::connect(
            cell::CellRuntime {
                commands: default(),
                engine: engine.clone(),
                store: engine.store.clone(),
                lanes: Arc::new(cell::LaneHub::new()),
                wire: default(),
                fiote: None,
                speech: None,
                information: None,
            },
            crate::native::wake::WakeSignal::new(|| {}),
        );
        app.world_mut().insert_non_send(bridge);
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<crate::native::tokens::ThemeSettings>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<bevy::picking::hover::HoverMap>()
            .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
            .add_plugins((
                crate::native::protein_area::ProteinAreaPlugin,
                crate::native::description::DescriptionPlugin,
                ShaderEffectsPlugin,
            ));
        let root = app.world_mut().spawn(Node::default()).id();
        let uid = nucleus::new_uid("r");
        let mut effect = Effect::default();
        effect.bindings = vec![NumericBinding {
            slot: 0,
            label: "Context quantity".into(),
            source: default(),
        }];
        let preview = shader::effect(app.world_mut(), root, effect, None);
        app.world_mut().entity_mut(preview).insert(Context {
            record: Some(uid.clone()),
            source: Source::Local,
        });
        until(&mut app, |world| {
            !world.resource::<NumericFeeds>().batches.is_empty()
        })
        .await;
        let id = app.world().resource::<NumericFeeds>().batches[&source_key(&Source::Local)]
            .id
            .clone();
        receive(
            app.world_mut(),
            &Source::Local,
            &ServerMessage::Snapshot {
                id,
                rows: vec![
                    serde_json::json!({"uid":uid,"numeric_values":{(key(&model::bindings::NumericField::Quantity)):"20"}}),
                ].into(),
            },
        );
        update(app.world_mut());
        assert_eq!(value(app.world(), preview), Some("20"));
        let source = Source::Organ(nucleus::new_uid("r"));
        let replacement = nucleus::new_uid("r");
        app.world_mut().entity_mut(preview).insert(Context {
            record: Some(replacement.clone()),
            source: source.clone(),
        });
        update(app.world_mut());
        assert!(value(app.world(), preview).is_none());
        let feeds = app.world().resource::<NumericFeeds>();
        assert_eq!(feeds.batches.len(), 1);
        assert_eq!(
            feeds.batches[&source_key(&source)]
                .requests
                .values()
                .next()
                .unwrap()
                .record,
            replacement
        );
        app.world_mut()
            .get_mut::<shader::EffectPreview>(preview)
            .unwrap()
            .disabled = true;
        update(app.world_mut());
        assert!(value(app.world(), preview).is_none());
        assert!(app.world().resource::<NumericFeeds>().batches.is_empty());
    }

    #[tokio::test]
    async fn numeric_source_target_and_field_limits_report_unavailable_and_release_sources() {
        for source_count in [1, 32] {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .init_resource::<Assets<Font>>()
                .init_resource::<crate::native::theme::Typography>()
                .init_resource::<crate::native::tokens::ThemeSettings>()
                .init_resource::<bevy::input_focus::InputFocus>()
                .init_resource::<bevy::picking::hover::HoverMap>()
                .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
                .add_plugins((
                    crate::native::protein_area::ProteinAreaPlugin,
                    crate::native::description::DescriptionPlugin,
                    ShaderEffectsPlugin,
                ));
            let root = app.world_mut().spawn(Node::default()).id();
            let sources: Vec<_> = (0..source_count).map(|_| nucleus::new_uid("r")).collect();
            let mut previews = vec![];
            for index in 0..64 {
                let mut effect = Effect::default();
                effect.bindings = (0..8)
                    .map(|slot| NumericBinding {
                        slot,
                        label: format!("Value {slot}"),
                        source: NumericSource {
                            record: Some(nucleus::new_uid("r")),
                            ..default()
                        },
                    })
                    .collect();
                let preview = shader::effect_with_priority(
                    app.world_mut(),
                    root,
                    effect,
                    Some("0".into()),
                    index >= 56,
                );
                app.world_mut().entity_mut(preview).insert(Context {
                    record: None,
                    source: if source_count == 1 {
                        Source::Local
                    } else {
                        Source::Organ(sources[index % source_count].clone())
                    },
                });
                previews.push(preview);
            }
            until(&mut app, |world| {
                previews
                    .iter()
                    .all(|preview| shader::working(world, *preview).is_some())
            })
            .await;
            app.update();
            let feeds = app.world().resource::<NumericFeeds>();
            assert_eq!(feeds.batches.len(), if source_count == 1 { 1 } else { 16 });
            assert_eq!(
                feeds
                    .batches
                    .values()
                    .map(|batch| batch.requests.len())
                    .sum::<usize>(),
                if source_count == 1 { 128 } else { 256 }
            );
            assert!(
                feeds
                    .batches
                    .values()
                    .all(|batch| batch.requests.len() <= 128)
            );
            assert!(feeds.retired.len() <= 32);
            assert!(
                previews
                    .iter()
                    .any(|preview| status(app.world(), *preview).contains("reading limit reached"))
            );
            if source_count > 1 {
                assert_eq!(crate::native::shader_effects::organs(app.world()).len(), 16);
            }
            for preview in previews {
                app.world_mut().despawn(preview);
            }
            until(&mut app, |world| {
                world.resource::<NumericFeeds>().batches.is_empty()
            })
            .await;
            assert!(crate::native::shader_effects::organs(app.world()).is_empty());
        }
    }

    #[tokio::test]
    async fn native_slots_batch_related_quantities_and_extension_fields_and_release_queries() {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let organ = store::organs::ensure_local(&engine.store.pool, "http://numeric-shaders")
            .await
            .unwrap()
            .uid;
        engine
            .set_signer(engine::trust::Signer::generate(&organ, "key"))
            .await
            .unwrap();
        let mut records = vec![];
        for quantity in [3.0, 10.0] {
            records.push(
                engine
                    .act(
                        engine::actions::Action::CreateRecord {
                            slug: None,
                            kind: nucleus::RecordKind::Plain,
                            head: "Numeric shader".into(),
                            body: "Keep this description".into(),
                            quantity,
                        },
                        None,
                    )
                    .await
                    .unwrap()
                    .created
                    .unwrap(),
            );
        }
        engine
            .act(
                engine::actions::Action::SetExtension {
                    target: records[0].clone(),
                    namespace: model::NAMESPACE.into(),
                    fds: serde_json::to_value(Extension::single(1, Effect::default())).unwrap(),
                },
                None,
            )
            .await
            .unwrap();
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<crate::native::tokens::ThemeSettings>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<bevy::picking::hover::HoverMap>()
            .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
            .insert_resource(crate::native::app::CellHandle(cell::CellRuntime {
                commands: default(),
                engine: engine.clone(),
                store: engine.store.clone(),
                lanes: Arc::new(cell::LaneHub::new()),
                wire: default(),
                fiote: None,
                speech: None,
                information: None,
            }))
            .add_plugins((
                crate::native::cell_bridge::CellBridgePlugin,
                crate::native::protein_area::ProteinAreaPlugin,
                crate::native::record_binding::RecordBindingPlugin,
                crate::native::description::DescriptionPlugin,
                ShaderEffectsPlugin,
            ));
        let root = app
            .world_mut()
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
            ))
            .id();
        let owner = open(
            app.world_mut(),
            root,
            Target::Record(RecordBinding {
                area: Entity::PLACEHOLDER,
                uid: records[0].clone(),
                source: Source::Local,
            }),
            Effect::default(),
            0,
            None,
        );
        until(&mut app, |world| example_visible(world, owner)).await;
        press(app.world_mut(), owner, "Add numeric input");
        fill(
            app.world_mut(),
            owner,
            "Record UID (empty uses this Record or the Sand's reference)",
            &records[1],
        );
        let preview = app.world().get::<Editor>(owner).unwrap().preview;
        until(&mut app, |world| value(world, preview) == Some("10")).await;
        let effect = app.world().get::<Editor>(owner).unwrap().effect.clone();
        let surface = app
            .world_mut()
            .spawn((
                Node::default(),
                Surface {
                    effect,
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        until(&mut app, |world| {
            world
                .get::<Background>(surface)
                .is_some_and(|background| value(world, background.preview) == Some("10"))
        })
        .await;
        let sand_preview = app.world().get::<Background>(surface).unwrap().preview;
        assert_eq!(app.world().resource::<NumericFeeds>().batches.len(), 1);
        assert_eq!(
            app.world()
                .resource::<NumericFeeds>()
                .batches
                .values()
                .next()
                .unwrap()
                .requests
                .len(),
            1
        );
        press(app.world_mut(), owner, "Save / Apply");
        until(&mut app, |world| {
            world.get::<Editor>(owner).unwrap().revision == 2
        })
        .await;
        engine
            .act(
                engine::actions::Action::SetQuantity {
                    target: records[1].clone(),
                    value: 20.0,
                },
                None,
            )
            .await
            .unwrap();
        until(&mut app, |world| {
            value(world, preview) == Some("20") && value(world, sand_preview) == Some("20")
        })
        .await;
        press(app.world_mut(), owner, "Input kind: quantity (change)");
        fill(
            app.world_mut(),
            owner,
            "Field path (for example /daily/protein)",
            "/daily/protein",
        );
        engine
            .act(
                engine::actions::Action::SetExtension {
                    target: records[1].clone(),
                    namespace: "nutrition".into(),
                    fds: serde_json::json!({"daily":{"protein":25,"note":"do not project this"}}),
                },
                None,
            )
            .await
            .unwrap();
        until(&mut app, |world| {
            value(world, preview) == Some("25") && value(world, sand_preview) == Some("20")
        })
        .await;
        assert_eq!(app.world().resource::<NumericFeeds>().batches.len(), 1);
        assert_eq!(
            app.world()
                .resource::<NumericFeeds>()
                .batches
                .values()
                .next()
                .unwrap()
                .requests
                .len(),
            2
        );
        assert!(
            !serde_json::to_string(
                &app.world()
                    .resource::<NumericFeeds>()
                    .batches
                    .values()
                    .next()
                    .unwrap()
                    .rows
            )
            .unwrap()
            .contains("do not project this")
        );
        engine
            .act(
                engine::actions::Action::SetExtension {
                    target: records[1].clone(),
                    namespace: "nutrition".into(),
                    fds: serde_json::json!({"daily":{"protein":0}}),
                },
                None,
            )
            .await
            .unwrap();
        until(&mut app, |world| value(world, preview) == Some("0")).await;
        assert_eq!(
            app.world()
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .numeric
                .gpu()
                .1[0],
            1
        );
        fill(
            app.world_mut(),
            owner,
            "Record UID (empty uses this Record or the Sand's reference)",
            &nucleus::new_uid("r"),
        );
        until(&mut app, |world| {
            value(world, preview).is_none()
                && status(world, preview).contains("unavailable")
                && value(world, sand_preview) == Some("20")
        })
        .await;
        assert_eq!(
            app.world()
                .get::<shader::EffectPreview>(preview)
                .unwrap()
                .numeric
                .gpu()
                .1[0],
            0
        );
        assert_eq!(value(app.world(), sand_preview), Some("20"));
        app.world_mut().despawn(owner);
        app.world_mut().despawn(surface);
        until(&mut app, |world| {
            world.resource::<NumericFeeds>().batches.is_empty()
                && world.resource::<NumericFeeds>().retired.is_empty()
        })
        .await;
        assert_eq!(
            engine.doc_text(&records[0]).await.unwrap().1,
            "Keep this description"
        );
    }
}
