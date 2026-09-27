mod model;
#[cfg(test)]
mod tests;
mod ui;

use crate::{
    actions::{Action, ActionButton},
    protein_area::{RecordBinding, Source},
};
use bevy::{prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use model::Entry;
pub(crate) use model::LocalTimer;
use serde_json::{Value, json};

struct Pending {
    id: String,
    form: Option<(Entity, [String; 2])>,
}

#[derive(Component)]
pub struct WorkTimer {
    binding: Option<RecordBinding>,
    input: Option<Entity>,
    query: String,
    subscription: String,
    label: Entity,
    control: Entity,
    status: Entity,
    list: Option<Entity>,
    logs: Vec<Entry>,
    pending: Option<Pending>,
}

pub struct WorkTimerPlugin;
impl Plugin for WorkTimerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update
                .after(crate::cell_bridge::ReceiveCell)
                .run_if(crate::laboratory::normal),
        )
        .add_systems(
            PostUpdate,
            ui::edits
                .after(bevy::text::EditableTextSystems)
                .before(crate::actions::ApplyActions)
                .run_if(crate::laboratory::normal),
        );
    }
}

pub fn populate(
    world: &mut World,
    parent: Entity,
    binding: Option<RecordBinding>,
    data: &Value,
    input: Option<Entity>,
) {
    if binding.is_none() && world.get::<LocalTimer>(parent).is_none() {
        world.entity_mut(parent).insert(LocalTimer::default());
    }
    if let Some(input) = input {
        let caption = crate::edit_mode::label(world, parent, "Record (optional)", 12.0);
        world.entity_mut(caption).insert(Node {
            position_type: PositionType::Absolute,
            left: px(8),
            top: px(6),
            ..default()
        });
        if let Some(mut area) = world.get_mut::<crate::sand_text::SandText>(input) {
            area.offset = [8.0, 24.0];
            area.size = [344.0, 32.0];
        }
        if let Some(mut node) = world.get_mut::<Node>(input) {
            node.left = px(8);
            node.top = px(24);
            node.width = percent(95);
            node.height = px(32);
        }
        if let Some(mut text) = world.get_mut::<EditableText>(input) {
            text.visible_lines = Some(1.0);
            text.allow_newlines = false;
        }
        world.entity_mut(input).insert((
            TextLayout::no_wrap(),
            crate::icons::Tooltip(
                "Optional Record slug or identity. Leave blank for a standalone stopwatch.".into(),
            ),
        ));
        if let Some(mut node) = world.get_mut::<Node>(parent) {
            node.padding = UiRect {
                top: px(64),
                left: px(8),
                right: px(8),
                bottom: px(8),
            };
            node.flex_direction = FlexDirection::Column;
            node.row_gap = px(6);
        }
    }
    let label = crate::edit_mode::label(world, parent, "Total 00:00:00", 24.0);
    let button = world
        .spawn((
            crate::sand::Square,
            Node {
                width: px(112),
                height: px(32),
                min_height: px(32),
                flex_shrink: 0.0,
                align_self: AlignSelf::Start,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ActionButton::new(parent, crate::actions![Toggle]),
            ChildOf(parent),
        ))
        .id();
    let control = crate::edit_mode::label(world, button, "Start", 16.0);
    let status = crate::edit_mode::label(
        world,
        parent,
        if binding.is_some() {
            ""
        } else {
            "Standalone stopwatch"
        },
        12.0,
    );
    let list = input.map(|_| {
        let entity = world
            .spawn((
                Node {
                    width: percent(100),
                    min_height: px(0),
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition::default(),
                ChildOf(parent),
            ))
            .id();
        crate::scroll_sand::attach(world, entity);
        entity
    });
    world.entity_mut(parent).insert(WorkTimer {
        binding,
        input,
        query: String::new(),
        subscription: nucleus::new_uid("timer"),
        label,
        control,
        status,
        list,
        logs: Vec::new(),
        pending: None,
    });
    refresh(world, parent, data);
    ui::reconcile(world, parent);
}

pub(crate) fn refresh(world: &mut World, entity: Entity, data: &Value) -> bool {
    let Some(timer) = world.get::<WorkTimer>(entity) else {
        return false;
    };
    let logs = if timer.binding.is_none() && timer.query.is_empty() {
        world
            .get::<LocalTimer>(entity)
            .map(|local| local.logs.clone())
            .unwrap_or_default()
    } else {
        data["work_logs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| serde_json::from_value::<Entry>(value.clone()).ok())
            .collect()
    };
    world.get_mut::<WorkTimer>(entity).unwrap().logs = logs;
    ui::reconcile(world, entity);
    true
}

pub fn formatted(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

fn status(world: &mut World, entity: Entity, message: &str) {
    if let Some(timer) = world.get::<WorkTimer>(entity) {
        let status = timer.status;
        if world.get::<Text>(status).is_some_and(|text| text.0 != message) {
            world.get_mut::<Text>(status).unwrap().0 = message.into();
        }
    }
}

fn reference_matches(world: &World, timer: &WorkTimer) -> bool {
    timer
        .input
        .and_then(|input| world.get::<EditableText>(input))
        .is_none_or(|input| {
            !input.is_composing()
                && input
                    .value()
                    .to_string()
                    .trim()
                    .trim_start_matches('#')
                    .trim_start_matches('@')
                    == timer.query
        })
}

fn submit(
    world: &mut World,
    entity: Entity,
    mutation: engine::record_change::Mutation,
    form: Option<(Entity, [String; 2])>,
) -> Result<(), String> {
    let timer = world
        .get::<WorkTimer>(entity)
        .ok_or("Time Castle is closed")?;
    if !reference_matches(world, timer) {
        return Err("Wait for the selected Record to load".into());
    }
    if timer.pending.is_some() {
        return Err("Wait for the pending time change".into());
    }
    let binding = timer
        .binding
        .clone()
        .ok_or("Choose a valid Record or clear the Record field for standalone timing")?;
    let request = engine::record_change::Request {
        id: nucleus::new_uid("op"),
        record_uid: binding.uid.clone(),
        mutation,
    };
    let id = request.id.clone();
    crate::record_binding::submit(world, &binding, request)?;
    world.get_mut::<WorkTimer>(entity).unwrap().pending = Some(Pending { id, form });
    status(world, entity, "Saving");
    Ok(())
}

fn change(
    world: &mut World,
    entity: Entity,
    id: &str,
    value: Option<Entry>,
    form: Option<(Entity, [String; 2])>,
) -> Result<(), String> {
    if let Some(value) = &value {
        engine::private_work::WorkMetadata::parse(&json!({"logs":[value.value()]}))
            .map_err(|error| error.to_string())?;
    }
    let timer = world
        .get::<WorkTimer>(entity)
        .ok_or("Time Castle is closed")?;
    if !reference_matches(world, timer) {
        return Err("Wait for the selected Record to load".into());
    }
    if timer.binding.is_some() {
        return submit(
            world,
            entity,
            engine::record_change::Mutation::WorkLog {
                log_id: id.into(),
                value: value.as_ref().map(Entry::value),
            },
            form,
        );
    }
    if !timer.query.is_empty() {
        return Err("Record not found; clear its reference to edit standalone time".into());
    }
    world
        .get_mut::<LocalTimer>(entity)
        .ok_or("Standalone timer is unavailable")?
        .change(id, value)?;
    if let Some((form, values)) = form {
        ui::finished(world, form, values, true);
    }
    refresh(world, entity, &Value::Null);
    status(world, entity, "Saved locally");
    Ok(())
}

#[derive(Clone)]
struct Toggle;
impl Action for Toggle {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(timer) = world.get::<WorkTimer>(entity) else {
            return;
        };
        if !reference_matches(world, timer) {
            status(world, entity, "Wait for the selected Record to load");
            return;
        }
        let running = timer.logs.iter().any(|entry| entry.end.is_none());
        let result = if timer.binding.is_some() {
            submit(
                world,
                entity,
                engine::record_change::Mutation::Timer { running: !running },
                None,
            )
        } else if timer.query.is_empty() {
            let result = world
                .get_mut::<LocalTimer>(entity)
                .ok_or_else(|| "Standalone timer is unavailable".to_string())
                .and_then(|mut local| local.toggle(chrono::Utc::now()));
            if result.is_ok() {
                refresh(world, entity, &Value::Null);
                status(world, entity, "Saved locally");
            }
            result
        } else {
            Err("Record not found; clear its reference for standalone timing".into())
        };
        if let Err(error) = result {
            status(world, entity, &error);
        }
    }
}

pub(crate) fn receive(world: &mut World, message: &ServerMessage) {
    let timers: Vec<_> = world
        .query_filtered::<Entity, With<WorkTimer>>()
        .iter(world)
        .collect();
    for entity in timers {
        let timer = world.get::<WorkTimer>(entity).unwrap();
        match message {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                if id == &timer.subscription =>
            {
                if let Some(data) = rows.first()
                    && let Some(uid) = data["uid"].as_str()
                {
                    world.get_mut::<WorkTimer>(entity).unwrap().binding = Some(RecordBinding {
                        area: entity,
                        uid: uid.into(),
                        source: Source::Local,
                    });
                    refresh(world, entity, data);
                    status(world, entity, "Record work log");
                } else {
                    let mut timer = world.get_mut::<WorkTimer>(entity).unwrap();
                    timer.binding = None;
                    timer.logs.clear();
                    ui::reconcile(world, entity);
                    status(
                        world,
                        entity,
                        "Record not found; clear its reference for standalone timing",
                    );
                }
            }
            ServerMessage::ActionOk { id, .. }
                if timer
                    .pending
                    .as_ref()
                    .is_some_and(|pending| &pending.id == id) =>
            {
                let pending = world
                    .get_mut::<WorkTimer>(entity)
                    .unwrap()
                    .pending
                    .take()
                    .unwrap();
                if let Some((form, values)) = pending.form {
                    ui::finished(world, form, values, true);
                }
                status(world, entity, "Saved");
            }
            ServerMessage::Error { id, message, .. }
                if timer
                    .pending
                    .as_ref()
                    .is_some_and(|pending| &pending.id == id)
                    || id == &timer.subscription =>
            {
                world.get_mut::<WorkTimer>(entity).unwrap().pending = None;
                status(world, entity, message);
            }
            _ => {}
        }
    }
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
    mut wake_at: Local<Option<std::time::Instant>>,
    mut subscriptions: Local<std::collections::HashMap<Entity, String>>,
) {
    subscriptions.retain(|entity, id| {
        world.get::<WorkTimer>(*entity).is_some()
            || !world
                .get_non_send::<crate::cell_bridge::CellBridge>()
                .is_some_and(|bridge| {
                    bridge
                        .outgoing
                        .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                        .is_ok()
                })
    });
    let unbuilt: Vec<_> = world
        .query_filtered::<(Entity, &crate::sand_store::StoredSand), Without<WorkTimer>>()
        .iter(world)
        .filter(|(_, sand)| sand.kind == crate::sand_store::SandKind::WorkTimer)
        .map(|(entity, sand)| (entity, sand.content))
        .collect();
    for (entity, input) in unbuilt {
        populate(world, entity, None, &Value::Null, input);
    }
    if let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() {
        let messages: Vec<_> = cursor
            .read(messages)
            .map(|message| message.0.clone())
            .collect();
        for message in messages {
            receive(world, &message);
        }
    }
    let timers: Vec<_> = world
        .query_filtered::<Entity, With<WorkTimer>>()
        .iter(world)
        .collect();
    let now = chrono::Utc::now();
    let mut running = false;
    for entity in timers {
        let timer = world.get::<WorkTimer>(entity).unwrap();
        let value = timer
            .input
            .and_then(|input| world.get::<EditableText>(input))
            .filter(|text| !text.is_composing())
            .map(|text| text.value().to_string());
        if let Some(value) = value {
            let value = value.trim().trim_start_matches('#').trim_start_matches('@');
            if value != timer.query && timer.pending.is_none() {
                let previous = timer.subscription.clone();
                let subscription = nucleus::new_uid("timer");
                let message = if value.is_empty() {
                    ClientMessage::Unsubscribe {
                        id: previous.clone(),
                    }
                } else {
                    let predicate = if nucleus::valid_uid(value, "r") {
                        json!({"uid_eq":value})
                    } else {
                        json!({"slug_eq":value})
                    };
                    ClientMessage::Subscribe { id: subscription.clone(), protein: serde_json::from_value(json!({"source":"record", "where":[predicate], "fields":["uid","work_logs"], "limit":1})).unwrap() }
                };
                let sent = world
                    .get_non_send::<crate::cell_bridge::CellBridge>()
                    .is_some_and(|bridge| {
                        if !value.is_empty()
                            && !timer.query.is_empty()
                            && bridge
                                .outgoing
                                .try_send(ClientMessage::Unsubscribe { id: previous })
                                .is_err()
                        {
                            return false;
                        }
                        bridge.outgoing.try_send(message).is_ok()
                    });
                if sent || value.is_empty() {
                    let mut timer = world.get_mut::<WorkTimer>(entity).unwrap();
                    timer.query = value.into();
                    timer.subscription = subscription.clone();
                    timer.binding = None;
                    timer.logs.clear();
                    ui::reset(world, entity);
                    if value.is_empty() {
                        subscriptions.remove(&entity);
                        refresh(world, entity, &Value::Null);
                        status(world, entity, "Standalone stopwatch");
                    } else {
                        subscriptions.insert(entity, subscription);
                        ui::reconcile(world, entity);
                        status(world, entity, "Loading Record");
                    }
                } else {
                    status(world, entity, "Waiting for the local Organ");
                }
            }
        }
        let timer = world.get::<WorkTimer>(entity).unwrap();
        let active = timer.logs.iter().any(|entry| entry.end.is_none());
        running |= active
            || world
                .get::<LocalTimer>(entity)
                .is_some_and(|local| local.logs.iter().any(|entry| entry.end.is_none()));
        let seconds = timer.logs.iter().fold(0i64, |total, entry| {
            total.saturating_add(entry.seconds(now))
        });
        let (label, control) = (timer.label, timer.control);
        let state = timer
            .pending
            .as_ref()
            .and_then(|_| timer.binding.as_ref())
            .and_then(|binding| crate::record_binding::status(world, binding));
        let caption = if active { "Pause" } else { "Start" };
        let text = format!("Total {}", formatted(seconds));
        if world
            .get::<Text>(label)
            .is_some_and(|label| label.0 != text)
        {
            world.get_mut::<Text>(label).unwrap().0 = text;
        }
        if world
            .get::<Text>(control)
            .is_some_and(|label| label.0 != caption)
        {
            world.get_mut::<Text>(control).unwrap().0 = caption.into();
        }
        if let Some(state) = state {
            status(world, entity, &state);
        }
    }
    ui::tick(world, now);
    if running && wake_at.is_none_or(|at| at.elapsed().as_secs() >= 1) {
        *wake_at = Some(std::time::Instant::now());
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                wake.ring();
            });
        }
    }
}
