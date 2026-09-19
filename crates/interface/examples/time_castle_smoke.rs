use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    winit::WinitSettings,
};
use lince_interface::{
    actions::ActionButton,
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
    work_timer::WorkTimer,
};

fn toggle(world: &mut World) {
    let button = world
        .query::<(&ActionButton, &Children)>()
        .iter(world)
        .find(|(button, children)| {
            world.get::<WorkTimer>(button.target).is_some()
                && children.iter().any(|child| {
                    world
                        .get::<Text>(child)
                        .is_some_and(|text| text.0 == "Start" || text.0 == "Pause")
                })
        })
        .unwrap()
        .0
        .clone();
    button.actions.run(world, button.target);
}

fn exercise(world: &mut World) {
    match world.resource::<FrameCount>().0 {
        20 => {
            let root = world
                .query_filtered::<Entity, With<BoxRoot>>()
                .single(world)
                .unwrap();
            spawn_sand(
                world,
                root,
                1,
                SandKind::WorkTimer,
                "",
                DVec2::new(-180.0, -250.0),
            );
            let font = world
                .resource::<lince_interface::theme::Typography>()
                .text(24.0);
            world.spawn((
                Text::new("▾ ▦ ✓ ○ ☐ ☑"),
                font,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(30),
                    top: px(24),
                    ..default()
                },
            ));
        }
        30 => toggle(world),
        40 => toggle(world),
        50 => {
            let fields: Vec<_> = world
                .query::<(Entity, &EditableText)>()
                .iter(world)
                .filter(|(_, text)| text.value().to_string().contains('T'))
                .map(|(entity, _)| entity)
                .collect();
            assert_eq!(fields.len(), 2);
            for (field, value) in fields
                .into_iter()
                .zip(["2026-09-19T10:00:00Z", "2026-09-19T10:05:00Z"])
            {
                world
                    .get_mut::<EditableText>(field)
                    .unwrap()
                    .editor
                    .set_text(value);
            }
        }
        60 => toggle(world),
        80 => {
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0.starts_with("Total 00:05:"))
            );
            let mut visible_fields = 0;
            for (text, node) in world.query::<(&EditableText, &ComputedNode)>().iter(world) {
                if text.value().to_string().contains('T') {
                    assert!(
                        node.size.x > 200.0 && node.size.x < 360.0,
                        "Timestamp field size: {:?}",
                        node.size
                    );
                    assert!(
                        node.size.y >= 30.0,
                        "Timestamp field height: {:?}",
                        node.size
                    );
                    visible_fields += 1;
                }
            }
            assert_eq!(visible_fields, 3);
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("/tmp/lince-time-castle.png"))
                .observe(
                    |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        exit.write(AppExit::Success);
                    },
                );
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    lince_interface::app::interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1000.0, 800.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .run();
}
