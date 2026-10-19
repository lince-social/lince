use bevy::prelude::*;
use lince_interface::visibility as ui;
use nucleus::visibility::Data;

pub struct VisibilityPlugin;

impl Plugin for VisibilityPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ui::VisibilityUiPlugin)
            .add_systems(Update, send);
    }
}

#[derive(Component)]
struct Overlay;

pub fn close(world: &mut World) -> bool {
    let panels: Vec<_> = world
        .query_filtered::<Entity, With<Overlay>>()
        .iter(world)
        .collect();
    let closed = !panels.is_empty();
    for panel in panels {
        world.despawn(panel);
    }
    closed
}

pub fn open(world: &mut World, record: &str, data: Data) {
    crate::location::close(world);
    close(world);
    let parent = world
        .spawn((
            Overlay,
            crate::app::Modal,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(px(18)),
                row_gap: px(12),
                ..default()
            },
            GlobalZIndex(100),
            BackgroundColor(lince_interface::theme::PAPER),
        ))
        .id();
    crate::app::button(
        world,
        parent,
        "Close visibility",
        crate::app::Intent::CloseVisibility,
    );
    ui::mount(world, parent, record, data);
}

fn send(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<ui::Request>>) {
    let requests: Vec<_> = cursor
        .read(world.resource::<Messages<ui::Request>>())
        .cloned()
        .collect();
    for request in requests {
        if let Err(error) = crate::app::send(
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

pub fn receive(world: &mut World, message: &cell::ServerMessage) -> bool {
    match message {
        cell::ServerMessage::ActionOk { id, data, .. } if id.starts_with("visibility-ui_") => {
            ui::receive(world, id, Ok(data.clone().unwrap_or_default()));
            true
        }
        cell::ServerMessage::Error { id, message, .. } if id.starts_with("visibility-ui_") => {
            ui::receive(world, id, Err(message.clone()));
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::text::EditableText;
    use nucleus::visibility::{Command, Context};

    fn activate(world: &mut World, caption: &str) {
        let entity = world
            .query::<(Entity, &Children, &bevy::ui_widgets::Button)>()
            .iter(world)
            .find(|(_, children, _)| {
                children.iter().any(|child| {
                    world
                        .get::<Text>(child)
                        .is_some_and(|text| text.0 == caption)
                })
            })
            .unwrap()
            .0;
        world.trigger(bevy::ui_widgets::Activate { entity });
        world.flush();
    }

    #[test]
    fn record_visibility_uses_shared_radius_rules_and_discards_closed_editor_replies() {
        let mut world = World::new();
        world.insert_resource(lince_interface::theme::Typography(Handle::default()));
        world.init_resource::<Messages<ui::Request>>();
        let record = nucleus::new_uid("r");
        open(&mut world, &record, Data::Place);
        let request = world
            .resource_mut::<Messages<ui::Request>>()
            .drain()
            .next()
            .unwrap();
        assert!(
            matches!(&request.command, Command::Context { record_uid, data } if record_uid == &record && *data == Data::Place)
        );
        ui::receive(
            &mut world,
            &request.id,
            Ok(serde_json::to_value(Context {
                record_uid: record.clone(),
                data: Data::Place,
                saved: None,
                organs: vec![],
                persons: vec![],
                records: vec![],
            })
            .unwrap()),
        );
        activate(&mut world, "Add inclusion");
        let editor = world
            .query::<(Entity, &lince_interface::controls::Editor)>()
            .iter(&world)
            .find(|(_, editor)| editor.title.contains("radius"))
            .unwrap()
            .0;
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text("3000");
        activate(&mut world, "Save visibility");
        let saved = world
            .resource_mut::<Messages<ui::Request>>()
            .drain()
            .next()
            .unwrap();
        assert!(
            matches!(&saved.command, Command::Save { record_uid, data, policy, expected_revision } if record_uid == &record && *data == Data::Place && *expected_revision == 0 && policy.include.len() == 1 && policy.include[0].radius_metres == Some(3000) && policy.exclude.is_empty())
        );
        assert!(close(&mut world));
        assert!(
            world
                .get::<lince_interface::controls::Editor>(editor)
                .is_none()
        );
        let other = nucleus::new_uid("r");
        open(&mut world, &other, Data::LiveLocation);
        assert!(receive(
            &mut world,
            &cell::ServerMessage::Error {
                id: saved.id,
                message: "late reply for the closed Record".into(),
                code: None,
            }
        ));
        let panel = world
            .query_filtered::<Entity, With<ui::Panel>>()
            .single(&world)
            .unwrap();
        let selected = ui::selection(&world, panel).unwrap();
        assert_eq!(selected.record_uid, other);
        assert_eq!(selected.data, Data::LiveLocation);
        assert!(
            world
                .query::<&lince_interface::controls::Editor>()
                .iter(&world)
                .next()
                .is_none()
        );
    }
}
