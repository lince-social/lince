use crate::{
    actions::{Action, ActionButton},
    protein_area::{RecordBinding, Source},
};
use bevy::{prelude::*, text::EditableText};
use cell::{ClientMessage, ServerMessage};
use serde_json::Value;

#[derive(Component)]
pub struct WorkTimer {
    binding: Option<RecordBinding>,
    input: Option<Entity>,
    query: String,
    subscription: String,
    label: Entity,
    control: Entity,
    status: Entity,
    running: Option<chrono::DateTime<chrono::FixedOffset>>,
    total: i64,
    pending: Option<String>,
}

pub struct WorkTimerPlugin;

impl Plugin for WorkTimerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update
                .after(crate::cell_bridge::ReceiveCell)
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
    if let Some(input) = input {
        if let Some(mut area) = world.get_mut::<crate::sand_text::SandText>(input) {
            area.offset = [8.0, 8.0];
            area.size = [232.0, 32.0];
        }
        if let Some(mut node) = world.get_mut::<Node>(input) {
            node.left = px(8);
            node.top = px(8);
            node.width = px(232);
            node.height = px(32);
        }
        if let Some(mut text) = world.get_mut::<EditableText>(input) {
            text.visible_lines = Some(1.0);
        }
        world
            .entity_mut(input)
            .insert(TextLayout::linebreak(bevy::text::LineBreak::NoWrap));
        if let Some(mut node) = world.get_mut::<Node>(parent) {
            node.padding = UiRect {
                top: px(48),
                left: px(8),
                right: px(8),
                bottom: px(8),
            };
            node.flex_direction = FlexDirection::Column;
            node.row_gap = px(6);
        }
    }
    let label = crate::edit_mode::label(world, parent, "00:00:00", 24.0);
    let button = world
        .spawn((
            crate::sand::Square,
            Node {
                width: px(112),
                height: px(32),
                min_height: px(32),
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
            "Enter a Record slug or identity above"
        },
        12.0,
    );
    world.entity_mut(parent).insert(WorkTimer {
        binding,
        input,
        query: String::new(),
        subscription: format!("work-timer-{}", parent.to_bits()),
        label,
        control,
        status,
        running: None,
        total: 0,
        pending: None,
    });
    refresh(world, parent, data);
}

pub(crate) fn refresh(world: &mut World, entity: Entity, data: &Value) -> bool {
    let Some(mut timer) = world.get_mut::<WorkTimer>(entity) else {
        return false;
    };
    timer.running = data["running_since"]
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());
    timer.total = data["spent_seconds"].as_i64().unwrap_or(0).max(0);
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

#[derive(Clone)]
struct Toggle;

impl Action for Toggle {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(timer) = world.get::<WorkTimer>(entity) else {
            return;
        };
        let Some(binding) = timer.binding.clone() else {
            return;
        };
        if timer.pending.is_some() {
            return;
        }
        let request = engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: binding.uid.clone(),
            mutation: engine::record_change::Mutation::Timer {
                running: timer.running.is_none(),
            },
        };
        let id = request.id.clone();
        let status = timer.status;
        match crate::record_binding::submit(world, &binding, request) {
            Ok(()) => world.get_mut::<WorkTimer>(entity).unwrap().pending = Some(id),
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

pub(crate) fn receive(world: &mut World, message: &ServerMessage) {
    let timers: Vec<_> = world
        .query::<(Entity, &WorkTimer)>()
        .iter(world)
        .map(|(entity, _)| entity)
        .collect();
    for entity in timers {
        let timer = world.get::<WorkTimer>(entity).unwrap();
        match message {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                if id == &timer.subscription =>
            {
                let status = timer.status;
                if let Some(data) = rows.first() {
                    if let Some(uid) = data["uid"].as_str() {
                        world.get_mut::<WorkTimer>(entity).unwrap().binding = Some(RecordBinding {
                            area: entity,
                            uid: uid.into(),
                            source: Source::Local,
                        });
                        refresh(world, entity, data);
                        world.get_mut::<Text>(status).unwrap().0.clear();
                    }
                } else {
                    world.get_mut::<WorkTimer>(entity).unwrap().binding = None;
                    world.get_mut::<Text>(status).unwrap().0 = "Record not found".into();
                }
            }
            ServerMessage::ActionOk { id, .. } if timer.pending.as_ref() == Some(id) => {
                world.get_mut::<WorkTimer>(entity).unwrap().pending = None
            }
            ServerMessage::Error { id, message, .. }
                if timer.pending.as_ref() == Some(id) || id == &timer.subscription =>
            {
                let status = timer.status;
                world.get_mut::<WorkTimer>(entity).unwrap().pending = None;
                world.get_mut::<Text>(status).unwrap().0 = message.clone();
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
        .query::<(Entity, &WorkTimer)>()
        .iter(world)
        .map(|(entity, _)| entity)
        .collect();
    let mut running = false;
    for entity in timers {
        let timer = world.get::<WorkTimer>(entity).unwrap();
        let value = timer
            .input
            .and_then(|input| world.get::<EditableText>(input))
            .filter(|text| !text.is_composing())
            .map(|text| text.value().to_string());
        if let Some(value) = value {
            let value = value.trim().trim_start_matches('#');
            if value.is_empty() && !timer.query.is_empty() {
                let id = timer.subscription.clone();
                let status = timer.status;
                if world
                    .get_non_send::<crate::cell_bridge::CellBridge>()
                    .is_some_and(|bridge| {
                        bridge
                            .outgoing
                            .try_send(ClientMessage::Unsubscribe { id })
                            .is_ok()
                    })
                {
                    let mut timer = world.get_mut::<WorkTimer>(entity).unwrap();
                    timer.query.clear();
                    timer.binding = None;
                    timer.running = None;
                    timer.total = 0;
                    timer.pending = None;
                    subscriptions.remove(&entity);
                    world.get_mut::<Text>(status).unwrap().0 =
                        "Enter a Record slug or identity above".into();
                }
            } else if !value.is_empty() && value != timer.query {
                let mut query = protein::Protein {
                    source: protein::Source::Record,
                    filter: Vec::new(),
                    fields: None,
                    include: Default::default(),
                    aggregate: None,
                    order: Vec::new(),
                    limit: Some(1),
                };
                query.filter = vec![if nucleus::valid_uid(value, "r") {
                    protein::Predicate::UidEq(value.into())
                } else {
                    protein::Predicate::SlugEq(value.into())
                }];
                query.fields = Some(
                    ["uid", "spent_seconds", "running_since"]
                        .map(str::to_string)
                        .into(),
                );
                query.limit = Some(1);
                let message = ClientMessage::Subscribe {
                    id: timer.subscription.clone(),
                    protein: query,
                };
                if world
                    .get_non_send::<crate::cell_bridge::CellBridge>()
                    .is_some_and(|bridge| bridge.outgoing.try_send(message).is_ok())
                {
                    let mut timer = world.get_mut::<WorkTimer>(entity).unwrap();
                    timer.query = value.into();
                    timer.binding = None;
                    timer.running = None;
                    timer.total = 0;
                    timer.pending = None;
                    subscriptions.insert(entity, timer.subscription.clone());
                }
            }
        }
        let timer = world.get::<WorkTimer>(entity).unwrap();
        let seconds = timer
            .running
            .map(|at| (chrono::Utc::now() - at.with_timezone(&chrono::Utc)).num_seconds())
            .unwrap_or(timer.total);
        running |= timer.running.is_some();
        let (label, control, status) = (timer.label, timer.control, timer.status);
        let caption = if timer.running.is_some() {
            "Pause"
        } else {
            "Start"
        };
        let caption = caption.to_owned();
        let state = timer
            .binding
            .as_ref()
            .and_then(|binding| crate::record_binding::status(world, binding));
        let text = formatted(seconds);
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
            world.get_mut::<Text>(control).unwrap().0 = caption;
        }
        if let Some(state) = state {
            if world
                .get::<Text>(status)
                .is_some_and(|label| label.0 != state)
            {
                world.get_mut::<Text>(status).unwrap().0 = state;
            }
        }
    }
    if running && wake_at.is_none_or(|at| at.elapsed().as_secs() >= 1) {
        *wake_at = Some(std::time::Instant::now());
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned() {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                wake.ring();
            });
        }
    }
}
