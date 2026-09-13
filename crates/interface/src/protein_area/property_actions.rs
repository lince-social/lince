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
}

#[derive(Clone)]
pub(super) enum Command {
    AddRelation,
    RemoveRelation(String),
    SaveLog,
    AddLog,
    RemoveLog,
    Page(bool),
    Reset,
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
        ChildOf(parent),
    ));
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
        count.saturating_sub(1)
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
            &format!(
                "{} / {}",
                if logs.is_empty() { 0 } else { index + 1 },
                logs.len()
            ),
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
            Command::SaveLog,
            Icon::Save,
            "Save the selected work log",
        );
        button(
            world,
            row,
            parent,
            Command::AddLog,
            Icon::Plus,
            "Add a work log using these times",
        );
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
        button(
            world,
            parent,
            parent,
            Command::AddRelation,
            Icon::Plus,
            "Add this assignment or assertion",
        );
    }
    button(
        world,
        parent,
        parent,
        Command::Reset,
        Icon::Reset,
        "Reload current values and discard this form's edits",
    );
    world.entity_mut(parent).insert(Form {
        binding,
        property: property.into(),
        data: json!({property: data[property]}),
        index,
        fields,
        pending: None,
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

pub(super) fn finished(world: &mut World, entity: Entity, error: Option<String>) {
    let Some(form) = world.get::<Form>(entity).cloned() else {
        return;
    };
    world.get_mut::<Form>(entity).unwrap().pending = None;
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
        if matches!(self, Self::Reset | Self::Page(_)) {
            let index = if let Self::Page(next) = self {
                if values(world, &form)
                    .iter()
                    .zip(&form.fields)
                    .any(|(value, (_, initial))| value != initial)
                {
                    status(
                        world,
                        form.binding.area,
                        "Save or reset the form before changing pages",
                    );
                    return;
                }
                let count = data[&form.property].as_array().map_or(0, Vec::len);
                let last = if form.property == "work_logs" {
                    count.saturating_sub(1)
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
                engine::actions::Action::AssertRecord {
                    subject: form.binding.uid.clone(),
                    predicate: if assignee {
                        "assigned-to".into()
                    } else {
                        values[0].trim().trim_start_matches('#').into()
                    },
                    object: optional(if assignee { 0 } else { 1 }),
                    quantity: if assignee { None } else { optional(2) },
                    unit: if assignee { None } else { optional(3) },
                }
            }
            Self::RemoveRelation(uid) => engine::actions::Action::RetractAssertion {
                assertion: uid.clone(),
            },
            Self::SaveLog | Self::AddLog | Self::RemoveLog => {
                if data["work_logs"] != form.data["work_logs"] {
                    status(
                        world,
                        form.binding.area,
                        "Work logs changed elsewhere. Reload the form first.",
                    );
                    return;
                }
                let mut logs = data["work_logs"].as_array().cloned().unwrap_or_default();
                let log = json!({"start":values[0].trim(),"end":if values[1].trim().is_empty() { Value::Null } else { json!(values[1].trim()) }});
                match self {
                    Self::AddLog => logs.push(log),
                    Self::SaveLog if form.index < logs.len() => logs[form.index] = log,
                    Self::RemoveLog if form.index < logs.len() => {
                        logs.remove(form.index);
                    }
                    _ => {
                        status(world, form.binding.area, "Choose a work log");
                        return;
                    }
                }
                let mut fds = data["extension"].clone();
                if !fds.is_object() {
                    fds = json!({});
                }
                fds["logs"] = json!(logs);
                if let Err(error) = engine::private_work::WorkMetadata::parse(&fds) {
                    status(world, form.binding.area, error.to_string());
                    return;
                }
                engine::actions::Action::SetExtension {
                    target: form.binding.uid.clone(),
                    namespace: "work".into(),
                    fds,
                }
            }
            _ => return,
        };
        match execute(world, &form.binding, entity, action) {
            Ok(()) => world.get_mut::<Form>(entity).unwrap().pending = Some(values),
            Err(error) => status(world, form.binding.area, error),
        }
    }
}
