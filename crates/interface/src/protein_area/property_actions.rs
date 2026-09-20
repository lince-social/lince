use super::*;
use crate::{
    actions::{Action, ActionButton},
    edit_mode::label,
    icons::{Icon, IconButton, Tooltip},
    sand::Square,
};
use bevy::text::EditableText;
use serde_json::json;

#[derive(Component, Clone)]
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
    AddRelation,
    RemoveRelation(String),
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
    let bundle = crate::sand::text_editor(&value, world.resource::<crate::theme::Typography>(), 0);
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
        crate::token_style::border(crate::tokens::Token::Accent),
        crate::sand::Unsaved(false),
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
        ActionButton::new(owner, crate::actions![command]),
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
    let index = index.min(if property == "work_logs" {
        count
    } else {
        count.saturating_sub(1) / 16
    });
    let mut fields = Vec::new();
    if property == "work_logs" {
        let logs = data["work_logs"].as_array().cloned().unwrap_or_default();
        let row = crate::area_panel::row(world, parent);
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
        let log = logs.get(index).cloned().unwrap_or(Value::Null);
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
        let row = crate::area_panel::row(world, parent);
        button(
            world,
            row,
            parent,
            Command::RemoveLog,
            Icon::Delete,
            "Remove the selected work log",
        );
    } else {
        let values = data[property].as_array().cloned().unwrap_or_default();
        for value in values.iter().skip(index * 16).take(16) {
            let row = crate::area_panel::row(world, parent);
            label(world, row, &rows::display(&json!([value])), 14.0);
            let uid = if property == "assignees" {
                value["assertion"].as_str()
            } else {
                value["uid"].as_str()
            };
            if let Some(uid) = uid {
                button(
                    world,
                    row,
                    parent,
                    Command::RemoveRelation(uid.into()),
                    Icon::Close,
                    "Remove this assignment or assertion",
                );
            }
        }
        if values.len() > 16 {
            let row = crate::area_panel::row(world, parent);
            button(
                world,
                row,
                parent,
                Command::Page(false),
                Icon::Previous,
                "Previous assertions",
            );
            label(
                world,
                row,
                &format!("{} / {}", index + 1, values.len().div_ceil(16)),
                14.0,
            );
            button(
                world,
                row,
                parent,
                Command::Page(true),
                Icon::Next,
                "Next assertions",
            );
        }
        if property == "assignees" {
            fields.push(input(
                world,
                parent,
                "Assignee",
                String::new(),
                "Person slug or identity",
            ));
        } else {
            for (title, hint) in [
                ("Assertion", "Assertion name or slug"),
                ("Target", "Optional related Record slug or identity"),
                ("Quantity", "Optional exact quantity"),
                ("Unit", "Optional unit slug or identity"),
            ] {
                fields.push(input(world, parent, title, String::new(), hint));
            }
        }
    }
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

fn current(world: &World, form: &Form) -> Option<Value> {
    world
        .resource::<Runtime>()
        .areas
        .get(&form.binding.area)?
        .data
        .iter()
        .find(|row| row["uid"].as_str() == Some(&form.binding.uid))
        .cloned()
}

fn rebuild(world: &mut World, entity: Entity, form: &Form, data: &Value, index: usize) {
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let focused = form
        .fields
        .iter()
        .position(|(field, _)| Some(*field) == focus);
    let children: Vec<_> = world
        .get::<Children>(entity)
        .into_iter()
        .flatten()
        .copied()
        .collect();
    for child in children {
        world.despawn(child);
    }
    spawn(
        world,
        entity,
        &form.property,
        form.binding.clone(),
        data,
        index,
    );
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
    let Some(form) = world.get::<Form>(entity).cloned() else {
        return false;
    };
    let edited = values(world, &form)
        .iter()
        .zip(&form.fields)
        .any(|(value, (_, initial))| value != initial);
    if !edited && form.pending.is_none() && form.data[&form.property] != data[&form.property] {
        rebuild(world, entity, &form, data, form.index);
    }
    true
}

pub(super) fn save_indicators(
    forms: Query<&Form>,
    mut fields: Query<(&EditableText, &mut crate::sand::Unsaved)>,
) {
    for form in &forms {
        for (entity, confirmed) in &form.fields {
            if let Ok((text, mut unsaved)) = fields.get_mut(*entity) {
                unsaved.set_if_neq(crate::sand::Unsaved(text.value().to_string() != *confirmed));
            }
        }
    }
}

pub(super) fn finished(world: &mut World, entity: Entity, error: Option<String>) {
    let Some(form) = world.get::<Form>(entity).cloned() else {
        return;
    };
    world.get_mut::<Form>(entity).unwrap().pending = None;
    if form.saving_log {
        if error.is_none() {
            let mut current = world.get_mut::<Form>(entity).unwrap();
            if let Some(sent) = form.pending {
                for ((_, initial), value) in current.fields.iter_mut().zip(&sent) {
                    *initial = value.clone();
                }
                current.data["work_logs"][form.index]["start"] = json!(sent[0].trim());
                current.data["work_logs"][form.index]["end"] = if sent[1].trim().is_empty() {
                    Value::Null
                } else {
                    json!(sent[1].trim())
                };
            }
        }
        return;
    }
    if error.is_none()
        && form
            .pending
            .as_ref()
            .is_some_and(|sent| sent == &values(world, &form))
    {
        if let Some(data) = current(world, &form) {
            rebuild(world, entity, &form, &data, form.index);
        }
    }
}

pub(super) fn commit_edits(world: &mut World, mut previous_focus: Local<Option<Entity>>) {
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let enter = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| {
            keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter)
        });
    let forms: Vec<_> = world
        .query::<(Entity, &Form)>()
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
            let values = values(world, form);
            let changed = form.attempted.as_ref() != Some(&values)
                && values
                    .iter()
                    .zip(&form.fields)
                    .any(|(value, (_, initial))| value != initial);
            let focused = form.fields.iter().any(|(entity, _)| Some(*entity) == focus);
            let blurred = form
                .fields
                .iter()
                .any(|(entity, _)| Some(*entity) == *previous_focus)
                && !focused;
            let existing_log = form.property == "work_logs"
                && form.data["work_logs"]
                    .as_array()
                    .is_some_and(|logs| logs.get(form.index).is_some());
            (changed && (existing_log || blurred || (focused && enter))).then_some((
                entity,
                values,
                form.property.clone(),
                existing_log,
            ))
        })
        .collect();
    *previous_focus = focus;
    for (entity, values, property, existing_log) in forms {
        if property == "work_logs"
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
        } else if property == "work_logs" {
            Command::AddLog
        } else {
            Command::AddRelation
        };
        command.apply(world, entity);
    }
}

impl Action for Command {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(form) = world.get::<Form>(entity).cloned() else {
            return;
        };
        if form.pending.is_some() {
            return;
        }
        let Some(data) = current(world, &form) else {
            status(world, form.binding.area, "Record is no longer available");
            return;
        };
        if matches!(self, Self::Page(_)) {
            let index = if let Self::Page(next) = self {
                if values(world, &form)
                    .iter()
                    .zip(&form.fields)
                    .any(|(value, (_, initial))| value != initial)
                {
                    status(
                        world,
                        form.binding.area,
                        "Wait for edits to save before changing pages",
                    );
                    return;
                }
                let count = data[&form.property].as_array().map_or(0, Vec::len);
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
            rebuild(world, entity, &form, &data, index);
            return;
        }
        if form.fields.iter().any(|(entity, _)| {
            world
                .get::<EditableText>(*entity)
                .is_some_and(|text| text.is_composing() || text.pending_paste.is_some())
        }) {
            return;
        }
        let values = values(world, &form);
        let action = match self {
            Self::AddRelation => {
                let assignee = form.property == "assignees";
                if values.first().is_none_or(|value| value.trim().is_empty()) {
                    status(world, form.binding.area, "Enter an assignee or assertion");
                    return;
                }
                let optional = |index: usize| {
                    values
                        .get(index)
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                };
                engine::actions::Action::ChangeRecord {
                    request: engine::record_change::Request {
                        id: nucleus::new_uid("op"),
                        record_uid: form.binding.uid.clone(),
                        mutation: engine::record_change::Mutation::Assertion {
                            predicate: if assignee {
                                "assigned-to".into()
                            } else {
                                values[0].trim().trim_start_matches('#').into()
                            },
                            object: optional(if assignee { 0 } else { 1 }),
                            quantity: if assignee { None } else { optional(2) },
                            unit: if assignee { None } else { optional(3) },
                        },
                    },
                }
            }
            Self::RemoveRelation(uid) => engine::actions::Action::ChangeRecord {
                request: engine::record_change::Request {
                    id: nucleus::new_uid("op"),
                    record_uid: form.binding.uid.clone(),
                    mutation: engine::record_change::Mutation::RetractAssertion {
                        assertion: uid.clone(),
                    },
                },
            },
            Self::SaveLog | Self::AddLog | Self::RemoveLog => {
                let log_id = if matches!(self, Self::AddLog) {
                    format!("work.log:{}", nucleus::new_uid("op"))
                } else {
                    let Some(id) = form.data["work_logs"]
                        .as_array()
                        .and_then(|logs| logs.get(form.index))
                        .and_then(|log| log["id"].as_str())
                    else {
                        status(world, form.binding.area, "Choose a work log");
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
        match execute(world, &form.binding, entity, action) {
            Ok(()) => {
                let mut form = world.get_mut::<Form>(entity).unwrap();
                form.pending = Some(values);
                form.saving_log = matches!(self, Self::SaveLog);
            }
            Err(error) => status(world, form.binding.area, error),
        }
    }
}
