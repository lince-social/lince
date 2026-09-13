use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    canvas_selection::SandSelection,
    container::BoxRoot,
    edit_mode::{EditAction, EditMode},
    icons::IconButton,
    sand_store::{SandKind, spawn_sand},
};

fn setup(world: &mut World) {
    world.spawn(BoxRoot);
    let mut window = world.query::<&mut Window>().single_mut(world).unwrap();
    window.resolution.set(1100.0, 900.0);
}

fn activate(world: &mut World, label: &str) {
    let entity = world
        .query::<(Entity, &IconButton)>()
        .iter(world)
        .find(|(_, icon)| icon.label == label)
        .unwrap()
        .0;
    world.trigger(bevy::ui_widgets::Activate { entity });
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame < 15 {
        return;
    }
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    match frame {
        15 => {
            let note = spawn_sand(
                world,
                root,
                1,
                SandKind::EditableText,
                "Weekly plan",
                DVec2::new(-250.0, 0.0),
            );
            let calendar = lince_interface::calendar::spawn(
                world,
                root,
                1,
                DVec2::new(300.0, 0.0),
                Default::default(),
            );
            world
                .entity_mut(root)
                .insert(SandSelection(vec![note, calendar]));
            EditAction::Open.apply(world, root);
            EditAction::Store.apply(world, root);
        }
        25 => {
            let mut fields = world.query::<&mut EditableText>();
            fields
                .iter_mut(world)
                .find(|e| e.max_characters == Some(80))
                .unwrap()
                .editor
                .set_text("Weekly planning");
            activate(world, "Save selected group as a custom Castle");
        }
        35 => activate(world, "Add Weekly planning at the camera"),
        45 => {
            assert_eq!(world.get::<SandSelection>(root).unwrap().0.len(), 2);
            let panel = world.get::<EditMode>(root).unwrap().panel;
            world
                .entity_mut(panel)
                .insert(ScrollPosition(Vec2::new(0.0, 450.0)));
        }
        60 => {
            let button = world
                .query::<(&IconButton, &ComputedNode)>()
                .iter(world)
                .find(|(i, _)| i.label == "Add Weekly planning at the camera")
                .unwrap();
            assert!(button.1.size().min_element() >= 24.0);
            let path = std::env::args().nth(1).expect("provide output path");
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
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
    let directory = tempfile::tempdir().unwrap();
    lince_interface::app::interface_app()
        .insert_resource(lince_interface::workspace::WorkspaceFile::new(
            directory.path().join("interface.json"),
        ))
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, setup)
        .add_systems(Last, exercise)
        .run();
}
