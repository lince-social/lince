use lince_interface::karma as model;
mod persistence;
#[cfg(test)]
mod tests;
mod ui;

use bevy::{math::DVec2, prelude::*};
use cell::{ClientMessage, ServerMessage};
use model::{Draft, Rule};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::{
    actions::Action,
    cell_bridge::{CellBridge, CellMessage, ReceiveCell},
    workspace::WorkspaceMember,
};
pub(crate) use persistence::{SavedKarmaCastle, snapshot};

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct KarmaCastle {
    pub draft: Option<Draft>,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub edits: Vec<Draft>,
}

#[derive(Component)]
struct View {
    controls: Entity,
    editing: Option<(String, usize)>,
    deleting: Vec<String>,
    deleting_pending: Option<String>,
    saving: bool,
    form: Entity,
    list: Entity,
    status: Entity,
    rules: Vec<Rule>,
    records: Vec<Value>,
    frequencies: Vec<Value>,
    record_lookup: HashMap<String, usize>,
    frequency_lookup: HashMap<String, usize>,
    pending: Option<String>,
    submitted: Option<Draft>,
    ready: bool,
}

#[derive(Resource, Default)]
struct Requests {
    subscriptions: HashMap<String, (Entity, usize)>,
    readings: HashMap<String, Entity>,
}

pub struct KarmaCastlePlugin;

impl Plugin for KarmaCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<CellMessage>()
            .add_observer(ui::hover_on)
            .add_observer(ui::hover_off)
            .add_systems(
                Update,
                (receive.after(ReceiveCell), maintain, ui::tick).chain(),
            )
            .add_systems(
                PostUpdate,
                ui::inputs
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    castle: KarmaCastle,
) -> Entity {
    world.init_resource::<Requests>();
    let owner = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(980.0, 620.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            castle,
        ))
        .id();
    let header = ui::row(world, owner);
    crate::edit_mode::label(world, header, "Karma", 22.0);
    world.get_mut::<Node>(header).unwrap().align_items = AlignItems::Center;
    let controls = ui::row(world, header);
    world.get_mut::<Node>(controls).unwrap().width = Val::Auto;
    ui::search(world, header, owner);
    ui::headings(world, owner);
    let scroll = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            ChildOf(owner),
        ))
        .id();
    crate::scroll_sand::attach(world, scroll);
    let form = ui::stack(world, scroll);
    let list = ui::stack(world, scroll);
    let status = crate::edit_mode::label(world, owner, "", 12.0);
    world.get_mut::<Node>(status).unwrap().display = Display::None;
    world.entity_mut(owner).insert(View {
        controls,
        editing: None,
        deleting: Vec::new(),
        deleting_pending: None,
        saving: false,
        form,
        list,
        status,
        rules: Vec::new(),
        records: Vec::new(),
        frequencies: Vec::new(),
        record_lookup: HashMap::new(),
        frequency_lookup: HashMap::new(),
        pending: None,
        submitted: None,
        ready: false,
    });
    ui::render_form(world, owner);
    owner
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Karma Castle",
        "Condition, threshold, consequence. Reuse fields and watch live readings.",
        ui::Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, KarmaCastle::default()),
    );
}

fn status(world: &mut World, owner: Entity, text: impl Into<String>) {
    if let Some(view) = world.get::<View>(owner) {
        let entity = view.status;
        let text = text.into();
        world.get_mut::<Node>(entity).unwrap().display = if text.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        world.get_mut::<Text>(entity).unwrap().0 = text;
    }
}

fn send(world: &World, message: ClientMessage) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Saving is unavailable in the Laboratory".into());
    }
    world
        .get_non_send::<CellBridge>()
        .ok_or("No Cell connection")?
        .outgoing
        .try_send(message)
        .map_err(|error| format!("Could not reach the Cell: {error}"))
}

fn save(world: &mut World, owner: Entity) {
    if world
        .get::<View>(owner)
        .is_none_or(|view| view.pending.is_some())
    {
        return;
    }
    ui::capture(world, owner);
    let castle = world.get::<KarmaCastle>(owner).unwrap();
    let Some(draft) = castle
        .draft
        .clone()
        .or_else(|| castle.edits.first().cloned())
    else {
        world.get_mut::<View>(owner).unwrap().saving = false;
        return;
    };
    if draft.name.trim().is_empty() || !nucleus::valid_slug(draft.slug.trim()) || !draft.valid() {
        world.get_mut::<View>(owner).unwrap().saving = false;
        status(
            world,
            owner,
            "Enter a name and a lowercase slug using letters, numbers, hyphens or dots",
        );
        return;
    }
    let request_id = nucleus::new_uid("karma-edit");
    let action = engine::actions::Action::SaveKarmaRule {
        identity: Some(nucleus::karma::rule_field::RuleIdentity {
            name: draft.name.clone(),
            slug: draft.slug.clone(),
        }),
        rule: draft.rule.clone(),
        expected_revision: draft.revision,
        fields: draft.fields.clone().map(|field| field.input()),
        request_id: request_id.clone(),
    };
    match send(
        world,
        ClientMessage::Act {
            id: request_id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.pending = Some(request_id);
            view.submitted = Some(draft);
            view.saving = true;
            status(world, owner, "Saving…");
            ui::render_form(world, owner);
            ui::render_list(world, owner);
        }
        Err(error) => {
            world.get_mut::<View>(owner).unwrap().saving = false;
            status(world, owner, error);
        }
    }
}

fn delete_next(world: &mut World, owner: Entity) {
    let Some(uid) = world.get::<View>(owner).unwrap().deleting.first().cloned() else {
        return;
    };
    let id = nucleus::new_uid("karma-delete");
    match send(
        world,
        ClientMessage::Act {
            id: id.clone(),
            action: engine::actions::Action::DeleteRecurrence {
                recurrence: uid.clone(),
            },
        },
    ) {
        Ok(()) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.pending = Some(id);
            view.deleting_pending = Some(uid);
            ui::render_controls(world, owner);
        }
        Err(error) => status(world, owner, error),
    }
}

fn query(index: usize) -> protein::Protein {
    let source = ["karma_rule", "record", "frequency"][index];
    let mut query: protein::Protein =
        serde_json::from_value(serde_json::json!({"source": source})).unwrap();
    if index == 1 {
        query.fields = Some(
            ["uid", "slug", "head", "quantity"]
                .map(str::to_owned)
                .into(),
        );
    }
    query
}

fn lookup(rows: &[Value]) -> HashMap<String, usize> {
    rows.iter()
        .enumerate()
        .flat_map(|(index, row)| {
            ["uid", "slug"]
                .into_iter()
                .filter_map(move |key| row[key].as_str().map(|value| (value.to_owned(), index)))
        })
        .collect()
}

fn maintain(world: &mut World) {
    let stale: Vec<_> = world
        .resource::<Requests>()
        .subscriptions
        .iter()
        .filter(|(_, (owner, _))| world.get::<KarmaCastle>(*owner).is_none())
        .map(|(id, _)| id.clone())
        .collect();
    for id in stale {
        if send(world, ClientMessage::Unsubscribe { id: id.clone() }).is_ok() {
            world.resource_mut::<Requests>().subscriptions.remove(&id);
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<KarmaCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        for index in 0..3 {
            let id = format!("karma-castle-{}-{index}", owner.to_bits());
            if world.resource::<Requests>().subscriptions.contains_key(&id) {
                continue;
            }
            match send(
                world,
                ClientMessage::Subscribe {
                    id: id.clone(),
                    protein: query(index),
                },
            ) {
                Ok(()) => {
                    world
                        .resource_mut::<Requests>()
                        .subscriptions
                        .insert(id, (owner, index));
                }
                Err(error) => status(world, owner, error),
            }
        }
    }
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        match message {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
                let Some((owner, index)) =
                    world.resource::<Requests>().subscriptions.get(&id).copied()
                else {
                    continue;
                };
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    continue;
                };
                match index {
                    0 => {
                        match rows
                            .into_iter()
                            .map(serde_json::from_value)
                            .collect::<Result<Vec<Rule>, _>>()
                        {
                            Ok(rules) => {
                                if view.ready && view.rules == rules {
                                    continue;
                                }
                                view.ready = true;
                                view.rules = rules;
                                ui::capture(world, owner);
                                ui::render_list(world, owner);
                            }
                            Err(error) => {
                                status(world, owner, format!("Could not read rules: {error}"));
                                continue;
                            }
                        }
                        if world.get::<View>(owner).unwrap().pending.is_none() {
                            status(world, owner, "");
                        }
                    }
                    1 => {
                        if view.records == rows {
                            continue;
                        }
                        view.record_lookup = lookup(&rows);
                        view.records = rows;
                    }
                    _ => {
                        if view.frequencies == rows {
                            continue;
                        }
                        view.frequency_lookup = lookup(&rows);
                        view.frequencies = rows;
                    }
                }
                ui::refresh_links(world, owner);
            }
            ServerMessage::ActionOk { id, data, .. } => {
                let reading = world.resource_mut::<Requests>().readings.remove(&id);
                if let Some(entity) = reading {
                    ui::reading_reply(
                        world,
                        entity,
                        data.as_ref()
                            .and_then(|data| data["value"].as_str())
                            .unwrap_or("No reading")
                            .into(),
                    );
                    continue;
                }
                let owners: Vec<_> = world
                    .query::<(Entity, &View)>()
                    .iter(world)
                    .filter(|(_, view)| view.pending.as_ref() == Some(&id))
                    .map(|(owner, _)| owner)
                    .collect();
                for owner in owners {
                    ui::capture(world, owner);
                    world.get_mut::<View>(owner).unwrap().pending = None;
                    let submitted = world.get_mut::<View>(owner).unwrap().submitted.take();
                    if let Some(submitted) = submitted {
                        let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
                        if castle.draft.as_ref() == Some(&submitted) {
                            castle.draft = None;
                        }
                        castle.edits.retain(|draft| draft != &submitted);
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        if view
                            .editing
                            .as_ref()
                            .is_some_and(|(uid, _)| submitted.rule.as_ref() == Some(uid))
                        {
                            view.editing = None;
                        }
                    }
                    let deleted = world
                        .get_mut::<View>(owner)
                        .unwrap()
                        .deleting_pending
                        .take();
                    if let Some(deleted) = deleted {
                        world
                            .get_mut::<KarmaCastle>(owner)
                            .unwrap()
                            .edits
                            .retain(|draft| draft.rule.as_ref() != Some(&deleted));
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        if view
                            .editing
                            .as_ref()
                            .is_some_and(|(uid, _)| uid == &deleted)
                        {
                            view.editing = None;
                        }
                        view.deleting.retain(|uid| uid != &deleted);
                        view.rules.retain(|rule| rule.uid != deleted);
                        delete_next(world, owner);
                    }
                    ui::render_form(world, owner);
                    ui::render_list(world, owner);
                    if world.get::<View>(owner).unwrap().saving {
                        save(world, owner);
                    }
                    if world.get::<View>(owner).unwrap().pending.is_none() {
                        status(world, owner, "");
                    }
                }
            }
            ServerMessage::Error { id, message, .. } => {
                let reading = world.resource_mut::<Requests>().readings.remove(&id);
                if let Some(entity) = reading {
                    ui::reading_reply(world, entity, message);
                    continue;
                }
                let owners: Vec<_> = world
                    .query::<(Entity, &View)>()
                    .iter(world)
                    .filter(|(owner, view)| {
                        view.pending.as_ref() == Some(&id)
                            || world
                                .resource::<Requests>()
                                .subscriptions
                                .get(&id)
                                .is_some_and(|(entity, _)| entity == owner)
                            || id == crate::cell_bridge::CONNECTION
                    })
                    .map(|(owner, _)| owner)
                    .collect();
                for owner in owners {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.pending = None;
                    view.submitted = None;
                    view.saving = false;
                    view.deleting_pending = None;
                    ui::render_form(world, owner);
                    ui::render_list(world, owner);
                    status(world, owner, &message);
                }
            }
            _ => {}
        }
    }
}
