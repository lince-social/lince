mod model;
mod persistence;
#[cfg(test)]
mod tests;
mod ui;

use bevy::{math::DVec2, prelude::*};
use cell::{ClientMessage, ServerMessage};
use model::{Draft, Frequency};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    actions::Action,
    cell_bridge::{CellBridge, CellMessage, ReceiveCell},
    workspace::WorkspaceMember,
};
pub(crate) use persistence::{SavedFrequencyCastle, snapshot};

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct FrequencyCastle {
    pub draft: Option<Draft>,
    pub search: String,
}

#[derive(Component)]
struct View {
    form: Entity,
    list: Entity,
    status: Entity,
    rows: Vec<Frequency>,
    pending: Option<Pending>,
    deleting: Option<String>,
    page: usize,
    ready: bool,
}

struct Pending {
    id: String,
    draft: Option<Draft>,
    message: &'static str,
}

#[derive(Resource, Default)]
struct Requests(HashMap<String, Entity>);

pub struct FrequencyCastlePlugin;

impl Plugin for FrequencyCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<CellMessage>()
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
    castle: FrequencyCastle,
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
                size: Vec2::new(940.0, 700.0),
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
    crate::edit_mode::label(world, header, "Frequency", 22.0);
    ui::button(world, header, owner, "+", ui::Command::New);
    let search = world.get::<FrequencyCastle>(owner).unwrap().search.clone();
    ui::input(world, header, owner, 4, "Filter frequencies", &search);
    let scroll = ui::stack(world, owner);
    {
        let mut node = world.get_mut::<Node>(scroll).unwrap();
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
        node.min_height = px(0);
        node.overflow = Overflow::scroll_y();
    }
    crate::scroll_sand::attach(world, scroll);
    let form = ui::stack(world, scroll);
    let list = ui::stack(world, scroll);
    let status = crate::edit_mode::label(world, owner, "", 12.0);
    world.get_mut::<Node>(status).unwrap().display = Display::None;
    world.entity_mut(owner).insert(View {
        form,
        list,
        status,
        rows: Vec::new(),
        pending: None,
        deleting: None,
        page: 0,
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
        "Frequency Castle",
        "Create, edit, and delete shared frequencies.",
        ui::Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, FrequencyCastle::default()),
    );
}

fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    if let Some(view) = world.get::<View>(owner) {
        let entity = view.status;
        let message = message.into();
        world.get_mut::<Node>(entity).unwrap().display = if message.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        world.get_mut::<Text>(entity).unwrap().0 = message;
    }
}

fn send(world: &World, message: ClientMessage) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    world
        .get_non_send::<CellBridge>()
        .ok_or("No Cell connection")?
        .outgoing
        .try_send(message)
        .map_err(|error| format!("Could not reach the Cell: {error}"))
}

fn submit(
    world: &mut World,
    owner: Entity,
    action: engine::actions::Action,
    draft: Option<Draft>,
    message: &'static str,
) {
    if world
        .get::<View>(owner)
        .is_none_or(|view| view.pending.is_some())
        || crate::laboratory::suspended(world, owner)
    {
        return;
    }
    let id = nucleus::new_uid("frequency-castle");
    match send(
        world,
        ClientMessage::Act {
            id: id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            world.get_mut::<View>(owner).unwrap().pending = Some(Pending { id, draft, message });
            status(world, owner, "Saving…");
        }
        Err(error) => status(world, owner, error),
    }
}

fn save(world: &mut World, owner: Entity) {
    ui::capture(world, owner);
    let Some(draft) = world
        .get::<FrequencyCastle>(owner)
        .and_then(|castle| castle.draft.clone())
    else {
        return;
    };
    let frequency = match draft.definition() {
        Ok(frequency) => frequency,
        Err(error) => {
            status(world, owner, error);
            return;
        }
    };
    let request_id = nucleus::new_uid("frequency-edit");
    let action = engine::actions::Action::SaveKarmaFrequency {
        request_id,
        frequency_uid: draft.uid.clone(),
        expected_handle_revision: draft.revision,
        frequency,
        restart: draft
            .original_fields
            .as_ref()
            .is_none_or(|fields| fields[2..] != draft.fields[2..])
            || draft.original_unit != Some(draft.unit),
    };
    submit(world, owner, action, Some(draft), "");
}

fn maintain(world: &mut World) {
    let stale: Vec<_> = world
        .resource::<Requests>()
        .0
        .iter()
        .filter(|(_, owner)| world.get::<FrequencyCastle>(**owner).is_none())
        .map(|(id, _)| id.clone())
        .collect();
    for id in stale {
        if send(world, ClientMessage::Unsubscribe { id: id.clone() }).is_ok() {
            world.resource_mut::<Requests>().0.remove(&id);
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<FrequencyCastle>>()
        .iter(world)
        .collect();
    for owner in owners {
        if crate::laboratory::suspended(world, owner) {
            continue;
        }
        let id = format!("frequency-castle-{}", owner.to_bits());
        if world.resource::<Requests>().0.contains_key(&id) {
            continue;
        }
        let protein = serde_json::from_value(serde_json::json!({"source":"frequency"})).unwrap();
        match send(
            world,
            ClientMessage::Subscribe {
                id: id.clone(),
                protein,
            },
        ) {
            Ok(()) => {
                world.resource_mut::<Requests>().0.insert(id, owner);
            }
            Err(error) => status(world, owner, error),
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
                let Some(owner) = world.resource::<Requests>().0.get(&id).copied() else {
                    continue;
                };
                let rows = match rows
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<Vec<Frequency>, _>>()
                {
                    Ok(rows) => rows,
                    Err(error) => {
                        status(world, owner, format!("Could not read frequencies: {error}"));
                        continue;
                    }
                };
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    continue;
                };
                if view.ready && view.rows == rows {
                    continue;
                }
                let first = !view.ready;
                let redraw = first
                    || view.rows.len() != rows.len()
                    || view
                        .rows
                        .iter()
                        .zip(&rows)
                        .any(|(before, after)| !before.same_layout(after));
                view.ready = true;
                view.rows = rows;
                ui::refresh_next_date(world, owner);
                if redraw {
                    ui::render_list(world, owner);
                }
                if first {
                    status(world, owner, "");
                }
            }
            ServerMessage::ActionOk { id, created, .. } => {
                let owners: Vec<_> = world
                    .query::<(Entity, &View)>()
                    .iter(world)
                    .filter(|(_, view)| {
                        view.pending
                            .as_ref()
                            .is_some_and(|pending| pending.id == id)
                    })
                    .map(|(owner, _)| owner)
                    .collect();
                for owner in owners {
                    ui::capture(world, owner);
                    let pending = world
                        .get_mut::<View>(owner)
                        .unwrap()
                        .pending
                        .take()
                        .unwrap();
                    let mut castle = world.get_mut::<FrequencyCastle>(owner).unwrap();
                    if let Some(submitted) = pending.draft {
                        if castle.draft.as_ref() == Some(&submitted) {
                            castle.draft = None;
                        } else if let Some(draft) = &mut castle.draft {
                            draft.uid = created.clone().or_else(|| submitted.uid.clone());
                            draft.revision = Some(
                                submitted
                                    .revision
                                    .map_or(2, |revision| revision.saturating_add(1)),
                            );
                            draft.original = submitted.definition().ok();
                            draft.original_fields = Some(submitted.fields);
                            draft.original_unit = Some(submitted.unit);
                        }
                    }
                    world.get_mut::<View>(owner).unwrap().deleting = None;
                    ui::render_form(world, owner);
                    ui::render_list(world, owner);
                    status(world, owner, pending.message);
                }
            }
            ServerMessage::Error { id, message, .. } => {
                let owners: Vec<_> = world
                    .query::<(Entity, &View)>()
                    .iter(world)
                    .filter(|(owner, view)| {
                        view.pending
                            .as_ref()
                            .is_some_and(|pending| pending.id == id)
                            || world.resource::<Requests>().0.get(&id) == Some(owner)
                            || id == crate::cell_bridge::CONNECTION
                    })
                    .map(|(owner, _)| owner)
                    .collect();
                for owner in owners {
                    world.get_mut::<View>(owner).unwrap().pending = None;
                    status(world, owner, &message);
                }
            }
            _ => {}
        }
    }
}
