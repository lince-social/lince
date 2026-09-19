use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    app::interface_app,
    container::BoxRoot,
    edit_mode::{EditAction, EditMode},
    sand_store::{SandKind, SandPreview, StoreEntry, StoredSand},
};

#[derive(Resource)]
struct Fixture {
    path: String,
    captures: usize,
}

fn rect(world: &World, entity: Entity) -> Rect {
    let node = world.get::<ComputedNode>(entity).unwrap();
    Rect::from_center_size(
        world.get::<UiGlobalTransform>(entity).unwrap().translation * node.inverse_scale_factor(),
        node.size() * node.inverse_scale_factor(),
    )
}

fn capture(world: &mut World, suffix: &str) {
    let path = format!("{}.{suffix}.png", world.resource::<Fixture>().path);
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(|_: On<ScreenshotCaptured>, mut fixture: ResMut<Fixture>| fixture.captures += 1);
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    if frame < 20 {
        return;
    }
    let panel = world.get::<EditMode>(root).unwrap().panel;
    match frame {
        20 => {
            EditAction::Open.apply(world, root);
            EditAction::Store.apply(world, root);
        }
        40 => {
            let previews: Vec<_> = world
                .query_filtered::<Entity, With<SandPreview>>()
                .iter(world)
                .collect();
            assert!(previews.len() >= 13);
            for preview in previews {
                let row = world.get::<ChildOf>(preview).unwrap().parent();
                assert!(rect(world, preview).min.x - rect(world, row).min.x >= 10.0);
                assert_eq!(rect(world, preview).size(), Vec2::new(72.0, 64.0));
            }
            capture(world, "sands");
        }
        50 => {
            let heading = world
                .query::<(Entity, &Text)>()
                .iter(world)
                .find(|(_, text)| text.0 == "Castles")
                .unwrap()
                .0;
            world.get_mut::<ScrollPosition>(panel).unwrap().0.y =
                rect(world, heading).min.y - rect(world, panel).min.y - 16.0;
        }
        60 => capture(world, "castles"),
        70 => world.get_mut::<ScrollPosition>(panel).unwrap().0.y = 100000.0,
        80 => {
            let label = world
                .query::<(Entity, &Text)>()
                .iter(world)
                .find(|(_, text)| text.0 == "Cancel imports")
                .unwrap()
                .0;
            let button = world.get::<ChildOf>(label).unwrap().parent();
            assert!(rect(world, button).max.y < rect(world, panel).max.y - 12.0);
            capture(world, "bottom");
        }
        90 => {
            let filter = world
                .query::<(Entity, &EditableText)>()
                .iter(world)
                .find(|(_, input)| input.max_characters == Some(200))
                .unwrap()
                .0;
            world
                .get_mut::<EditableText>(filter)
                .unwrap()
                .editor
                .set_text("Operation");
            world.get_mut::<ScrollPosition>(panel).unwrap().0.y = 0.0;
        }
        100 => {
            let row = world
                .query::<(Entity, &StoreEntry)>()
                .iter(world)
                .find(|(_, entry)| entry.0 == SandKind::Operation)
                .unwrap()
                .0;
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            world.write_message(WindowEvent::CursorMoved(CursorMoved {
                window,
                position: rect(world, row).center(),
                delta: None,
            }));
        }
        104 | 106 => {
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
                window,
                button: MouseButton::Left,
                state: if frame == 104 {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
            }));
        }
        120 => {
            assert_eq!(
                world
                    .query::<&StoredSand>()
                    .iter(world)
                    .filter(|sand| sand.kind == SandKind::Operation)
                    .count(),
                1
            );
            assert_eq!(world.resource::<Fixture>().captures, 3);
            println!(
                "Store cards passed: real miniatures, card padding, bottom spacing, filtering and click-to-add."
            );
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
}

#[tokio::main]
async fn main() {
    interface_app()
        .insert_resource(Fixture {
            path: std::env::args().nth(1).expect("provide screenshot path"),
            captures: 0,
        })
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
