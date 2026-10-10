use bevy::{math::DVec2, prelude::*};
use lince_interface::visibility as ui;
use nucleus::visibility::Data;

pub struct VisibilityCastlePlugin;

impl Plugin for VisibilityCastlePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ui::VisibilityUiPlugin).add_systems(
            Update,
            (receive.after(crate::cell_bridge::ReceiveCell), send).chain(),
        );
    }
}

pub(crate) fn populate(world: &mut World, _root: Entity, sand: Entity) -> Entity {
    let body = crate::sand_panel::frame(world, sand, "Visibility Castle");
    world.get_mut::<Node>(body).unwrap().overflow = Overflow::scroll_y();
    crate::scroll_sand::attach(world, body);
    ui::mount(world, body, "", Data::Record)
}

#[derive(Clone)]
struct Open {
    record: String,
    data: Data,
}

impl crate::actions::Action for Open {
    fn apply(&self, world: &mut World, target: Entity) {
        let mut ancestor = target;
        let mut placement = DVec2::ZERO;
        let root = loop {
            if let Some(item) = world.get::<crate::canvas::CanvasItem>(ancestor) {
                placement = item.position + DVec2::splat(32.0);
            }
            if let Some(root) = world.get::<crate::sand::InBox>(ancestor) {
                break root.0;
            }
            if world.get::<crate::container::BoxRoot>(ancestor).is_some() {
                break ancestor;
            }
            let Some(parent) = world.get::<ChildOf>(ancestor) else {
                return;
            };
            ancestor = parent.parent();
        };
        let workspace = world
            .get::<crate::workspace::Workspaces>(root)
            .map_or(1, |spaces| spaces.active);
        let sand = crate::sand_store::spawn_sand(
            world,
            root,
            workspace,
            crate::sand_store::SandKind::Visibility,
            "Visibility",
            placement,
        );
        if let Some(panel) = world
            .get::<crate::sand_store::StoredSand>(sand)
            .and_then(|sand| sand.content)
        {
            ui::set_target(world, panel, &self.record, self.data);
        }
    }
}

pub fn record_controls(world: &mut World, parent: Entity, record: &str) {
    let row = crate::sand_panel::row(world, parent);
    for (data, label) in [
        (Data::Record, "Record visibility"),
        (Data::Place, "Saved place visibility"),
        (Data::LiveLocation, "Live location visibility"),
    ] {
        crate::sand_panel::button(
            world,
            row,
            parent,
            label,
            Open {
                record: record.into(),
                data,
            },
        );
    }
}

fn send(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<ui::Request>>) {
    let requests: Vec<_> = cursor
        .read(world.resource::<Messages<ui::Request>>())
        .cloned()
        .collect();
    for request in requests {
        if let Err(error) = crate::sand_panel::send(
            world,
            cell::ClientMessage::Act {
                id: request.id.clone(),
                action: engine::actions::Action::Visibility {
                    request: request.command,
                },
            },
        ) {
            ui::receive(world, &request.id, Err(error));
        }
    }
}

fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|messages| cursor.read(messages).cloned().collect())
        .unwrap_or_default();
    for message in messages {
        match message.0 {
            cell::ServerMessage::ActionOk { id, data, .. } => {
                ui::receive(world, &id, Ok(data.unwrap_or_default()))
            }
            cell::ServerMessage::Error { id, message, .. } => ui::receive(world, &id, Err(message)),
            _ => {}
        }
    }
}
