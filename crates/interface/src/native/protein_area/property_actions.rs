use super::*;
use crate::host::performance::{self, Count, Stage};
use crate::native::{
    actions::{Action, ActionButton},
    edit_mode::label,
    icons::{Icon, IconButton, Tooltip},
    sand::Square,
};
use bevy::text::EditableText;
use serde_json::json;
use std::sync::Arc;

#[derive(Component)]
pub(super) struct Form {
    binding: RecordBinding,
    pub(super) property: String,
    data: Value,
    index: usize,
    pub(super) fields: Vec<(Entity, String)>,
    pending: Option<Vec<String>>,
    attempted: Option<Vec<String>>,
    saving_log: bool,
}

#[derive(Clone)]
pub(super) enum Command {
    SaveLog,
    AddLog,
    RemoveLog,
    Page(bool),
}

fn input(
    world: &mut World,
    parent: Entity,
    title: &str,
    value: String,
    hint: &str,
) -> (Entity, String) {
    label(world, parent, title, 14.0);
    let bundle = crate::native::sand::text_editor(
        &value,
        world.resource::<crate::native::theme::Typography>(),
        0,
    );
    let entity = world.spawn(bundle).id();
    let mut text = world.get_mut::<EditableText>(entity).unwrap();
    text.max_characters = Some(4096);
    text.allow_newlines = false;
    text.visible_lines = Some(1.0);
    world.entity_mut(entity).insert((
        Node {
            width: percent(100),
            min_height: px(28),
            border: UiRect::all(px(1)),
            flex_shrink: 0.0,
            ..default()
        },
        Tooltip(hint.into()),
        crate::native::token_style::border(crate::native::tokens::Token::Accent),
        crate::native::sand::Unsaved(false),
        ChildOf(parent),
    ));
    super::history::attach_text(world, entity);
    (entity, value)
}

fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    command: Command,
    icon: Icon,
    title: &str,
) {
    world.spawn((
        Square,
        IconButton::new(icon, title),
        ActionButton::new(owner, crate::native::actions![command]),
        ChildOf(parent),
    ));
}

pub(super) fn spawn(
    world: &mut World,
    parent: Entity,
    property: &str,
    binding: RecordBinding,
    data: &Value,
    index: usize,
) {
    let count = data[property].as_array().map_or(0, Vec::len);
    let index = index.min(count);
    let mut fields = Vec::new();
    let logs = data["work_logs"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let row = crate::native::area_panel::row(world, parent);
    button(
        world,
        row,
        parent,
        Command::Page(false),
        Icon::Previous,
        "Previous work log",
    );
    label(
        world,
        row,
        &if index == logs.len() {
            "New work log".into()
        } else {
            format!("{} / {}", index + 1, logs.len())
        },
        14.0,
    );
    button(
        world,
        row,
        parent,
        Command::Page(true),
        Icon::Next,
        "Next work log",
    );
    let log = logs.get(index).unwrap_or(&Value::Null);
    fields.push(input(
        world,
        parent,
        "Start",
        log["start"].as_str().unwrap_or_default().into(),
        "Timestamp with timezone, for example 2026-09-13T09:00:00-03:00",
    ));
    fields.push(input(
        world,
        parent,
        "End",
        log["end"].as_str().unwrap_or_default().into(),
        "Timestamp with timezone; blank keeps this log running",
    ));
    let row = crate::native::area_panel::row(world, parent);
    button(
        world,
        row,
        parent,
        Command::RemoveLog,
        Icon::Delete,
        "Remove the selected work log",
    );
    world.entity_mut(parent).insert(Form {
        binding,
        property: property.into(),
        data: json!({property: data[property]}),
        index,
        fields,
        pending: None,
        attempted: None,
        saving_log: false,
    });
}

fn values(world: &World, form: &Form) -> Vec<String> {
    performance::add(Count::PropertyFieldValuesCopied, form.fields.len() as u64);
    form.fields
        .iter()
        .map(|(entity, _)| {
            world
                .get::<EditableText>(*entity)
                .map(|text| text.value().to_string())
                .unwrap_or_default()
        })
        .collect()
}

fn edited(world: &World, form: &Form) -> bool {
    form.fields.iter().any(|(entity, initial)| {
        world
            .get::<EditableText>(*entity)
            .map_or(!initial.is_empty(), |text| text.value() != initial.as_str())
    })
}

fn values_match(world: &World, form: &Form, values: &[String]) -> bool {
    form.fields.len() == values.len()
        && form.fields.iter().zip(values).all(|((entity, _), value)| {
            world
                .get::<EditableText>(*entity)
                .map_or(value.is_empty(), |text| text.value() == value.as_str())
        })
}

pub(super) fn selection_pending(world: &mut World, owner: Entity) -> bool {
    world.query::<&Form>().iter(world).any(|form| {
        form.binding.area == owner
            && (form.pending.is_some()
                || form.fields.iter().any(|(entity, initial)| {
                    world.get::<EditableText>(*entity).is_some_and(|text| {
                        text.value() != initial
                            || text.is_composing()
                            || text.pending_paste.is_some()
                    })
                }))
    })
}

fn current<'a>(world: &'a World, form: &Form) -> Option<(&'a Arc<Vec<Value>>, usize)> {
    let data = &world
        .resource::<Runtime>()
        .areas
        .get(&form.binding.area)?
        .data;
    let row = data
        .iter()
        .position(|row| row["uid"].as_str() == Some(&form.binding.uid))?;
    Some((data, row))
}

fn rebuild(world: &mut World, entity: Entity, data: &Value, index: usize) {
    let Some(form) = world.get::<Form>(entity) else {
        return;
    };
    performance::count(Count::PropertyFormsRebuilt);
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let focused = form
        .fields
        .iter()
        .position(|(field, _)| Some(*field) == focus);
    let property = form.property.clone();
    let binding = form.binding.clone();
    let children: Vec<_> = world
        .get::<Children>(entity)
        .into_iter()
        .flatten()
        .copied()
        .collect();
    for child in children {
        world.despawn(child);
    }
    spawn(world, entity, &property, binding, data, index);
    if let Some(focused) = focused
        && let Some(field) = world
            .get::<Form>(entity)
            .and_then(|form| form.fields.get(focused))
            .map(|(field, _)| *field)
        && let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>()
    {
        focus.set(field, bevy::input_focus::FocusCause::Pressed);
    }
}

pub(super) fn refresh(world: &mut World, entity: Entity, data: &Value) -> bool {
    let Some(form) = world.get::<Form>(entity) else {
        return false;
    };
    if form.pending.is_some() || form.data[&form.property] == data[&form.property] {
        return true;
    }
    if !edited(world, form) {
        let index = form.index;
        rebuild(world, entity, data, index);
    }
    true
}

pub(super) fn save_indicators(
    forms: Query<&Form>,
    mut fields: Query<(&EditableText, &mut crate::native::sand::Unsaved)>,
) {
    for form in &forms {
        for (entity, confirmed) in &form.fields {
            if let Ok((text, mut unsaved)) = fields.get_mut(*entity) {
                unsaved.set_if_neq(crate::native::sand::Unsaved(
                    text.value() != confirmed.as_str(),
                ));
            }
        }
    }
}

pub(super) fn finished(world: &mut World, entity: Entity, error: Option<String>) {
    let _reply = performance::Span::new(Stage::PropertyFormReply);
    let pending = {
        let Some(mut form) = world.get_mut::<Form>(entity) else {
            return;
        };
        let pending = form.pending.take();
        if form.saving_log {
            if error.is_none()
                && let Some(sent) = pending
            {
                let index = form.index;
                form.data["work_logs"][index]["start"] = json!(sent[0].trim());
                form.data["work_logs"][index]["end"] = if sent[1].trim().is_empty() {
                    Value::Null
                } else {
                    json!(sent[1].trim())
                };
                for ((_, initial), value) in form.fields.iter_mut().zip(sent) {
                    *initial = value;
                }
            }
            return;
        }
        pending
    };
    let form = world.get::<Form>(entity).unwrap();
    if error.is_none()
        && pending
            .as_ref()
            .is_some_and(|sent| values_match(world, form, sent))
        && let Some((data, row)) = current(world, form)
    {
        let index = form.index;
        let data = Arc::clone(data);
        rebuild(world, entity, &data[row], index);
    }
}

pub(super) fn commit_edits(
    world: &mut World,
    mut previous_focus: Local<Option<Entity>>,
    forms: &mut QueryState<(Entity, &Form)>,
) {
    let _commit = performance::Span::new(Stage::PropertyFormCommit);
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let enter = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| {
            keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter)
        });
    let forms: Vec<_> = forms
        .iter(world)
        .filter(|(_, form)| form.pending.is_none())
        .filter(|(_, form)| {
            form.fields.iter().all(|(entity, _)| {
                world
                    .get::<EditableText>(*entity)
                    .is_some_and(|text| !text.is_composing() && text.pending_paste.is_none())
            })
        })
        .filter_map(|(entity, form)| {
            let focused = form.fields.iter().any(|(entity, _)| Some(*entity) == focus);
            let blurred = form
                .fields
                .iter()
                .any(|(entity, _)| Some(*entity) == *previous_focus)
                && !focused;
            let is_log = form.property == "work_logs";
            let existing_log = is_log
                && form.data["work_logs"]
                    .as_array()
                    .is_some_and(|logs| logs.get(form.index).is_some());
            if !(existing_log || blurred || (focused && enter))
                || !edited(world, form)
                || form
                    .attempted
                    .as_ref()
                    .is_some_and(|attempted| values_match(world, form, attempted))
            {
                return None;
            }
            Some((entity, values(world, form), is_log, existing_log))
        })
        .collect();
    *previous_focus = focus;
    for (entity, values, is_log, existing_log) in forms {
        if is_log
            && !existing_log
            && !values
                .first()
                .is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value.trim()).is_ok())
        {
            continue;
        }
        world.get_mut::<Form>(entity).unwrap().attempted = Some(values);
        let command = if existing_log {
            Command::SaveLog
        } else {
            Command::AddLog
        };
        command.apply(world, entity);
    }
}

impl Action for Command {
    fn apply(&self, world: &mut World, entity: Entity) {
        let _command = performance::Span::new(Stage::PropertyFormCommand);
        let Some(form) = world.get::<Form>(entity) else {
            return;
        };
        if form.pending.is_some() {
            return;
        }
        let owner = form.binding.area;
        let Some((data, row)) = current(world, form) else {
            status(world, owner, "Record is no longer available");
            return;
        };
        if matches!(self, Self::Page(_)) {
            let index = if let Self::Page(next) = self {
                if edited(world, form) {
                    status(world, owner, "Wait for edits to save before changing pages");
                    return;
                }
                let count = data[row][&form.property].as_array().map_or(0, Vec::len);
                let last = if form.property == "work_logs" {
                    count
                } else {
                    count.saturating_sub(1) / 16
                };
                if *next {
                    (form.index + 1).min(last)
                } else {
                    form.index.saturating_sub(1)
                }
            } else {
                form.index
            };
            let data = Arc::clone(data);
            rebuild(world, entity, &data[row], index);
            return;
        }
        if form.fields.iter().any(|(entity, _)| {
            world
                .get::<EditableText>(*entity)
                .is_some_and(|text| text.is_composing() || text.pending_paste.is_some())
        }) {
            return;
        }
        let values = values(world, form);
        let action = match self {
            Self::SaveLog | Self::AddLog | Self::RemoveLog => {
                let log_id = if matches!(self, Self::AddLog) {
                    format!("work.log:{}", nucleus::new_uid("op"))
                } else {
                    let Some(id) = form.data["work_logs"]
                        .as_array()
                        .and_then(|logs| logs.get(form.index))
                        .and_then(|log| log["id"].as_str())
                    else {
                        status(world, owner, "Choose a work log");
                        return;
                    };
                    id.to_owned()
                };
                let value = (!matches!(self, Self::RemoveLog)).then(|| json!({
                    "start": values[0].trim(),
                    "end": if values[1].trim().is_empty() { Value::Null } else { json!(values[1].trim()) }
                }));
                engine::actions::Action::ChangeRecord {
                    request: engine::record_change::Request {
                        id: nucleus::new_uid("op"),
                        record_uid: form.binding.uid.clone(),
                        mutation: engine::record_change::Mutation::WorkLog { log_id, value },
                    },
                }
            }
            _ => return,
        };
        let binding = form.binding.clone();
        match execute(world, &binding, entity, action) {
            Ok(()) => {
                let mut form = world.get_mut::<Form>(entity).unwrap();
                form.pending = Some(values);
                form.saving_log = matches!(self, Self::SaveLog);
            }
            Err(error) => status(world, owner, error),
        }
    }
}
