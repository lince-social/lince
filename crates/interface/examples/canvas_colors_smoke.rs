use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::{EditableText, TextEdit},
    ui_widgets::Activate,
    winit::WinitSettings,
};
use lince_interface::{
    app::interface_app,
    canvas::{CanvasItem, CanvasView},
    canvas_background::CanvasColors,
    container::BoxRoot,
    edit_mode::{ColorField, EditAction, EditControl},
    sand::Square,
    theme::PURPLE,
    workspace::{WorkspaceFile, Workspaces},
};

#[derive(Resource)]
struct CapturePath(String);

fn activate(world: &mut World, action: EditAction) {
    if action == EditAction::Canvas {
        use lince_interface::actions::Action;
        let root = world.query_filtered::<Entity, With<BoxRoot>>().single(world).unwrap();
        action.apply(world, root);
        return;
    }
    let entity = world
        .query::<(Entity, &EditControl)>()
        .iter(world)
        .find(|(_, control)| control.action == action)
        .unwrap()
        .0;
    world.trigger(Activate { entity });
}

fn exercise(world: &mut World) {
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    match world.resource::<FrameCount>().0 {
        10 => activate(world, EditAction::Toggle),
        12 => activate(world, EditAction::Canvas),
        16 => {
            for (field, value) in [
                (ColorField::Background, "#102030"),
                (ColorField::Grid, "#8090A0"),
            ] {
                let entity = world
                    .query::<(Entity, &ColorField)>()
                    .iter(world)
                    .find(|(_, candidate)| **candidate == field)
                    .unwrap()
                    .0;
                let mut text = world.get_mut::<EditableText>(entity).unwrap();
                text.queue_edit(TextEdit::SelectAll);
                text.queue_edit(TextEdit::Insert(value.into()));
            }
        }
        20 => {
            assert_eq!(
                world.get::<Workspaces>(root).unwrap().entries[0].colors,
                CanvasColors {
                    background: [16, 32, 48],
                    grid: [128, 144, 160]
                }
            );
            let path = world.resource::<CapturePath>().0.clone();
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{path}.editor.png")));
        }
        24 => activate(world, EditAction::Close),
        26 => world.get_mut::<CanvasView>(root).unwrap().center = DVec2::new(7.0, 11.0),
        32 => {
            let path = world.resource::<CapturePath>().0.clone();
            world.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(
                |event: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    for (x, y, expected) in [(2,2,[16,32,48]), (9,3,[128,144,160])] {
                        let pixel = event.image.get_color_at(x,y).unwrap().to_srgba();
                        for (value, expected) in [pixel.red,pixel.green,pixel.blue].into_iter().zip(expected) {
                            assert!((value * 255.0 - expected as f32).abs() < 2.0, "canvas color was not rendered at {x},{y}");
                        }
                    }
                    println!("Canvas colors smoke passed: edit controls, same-frame text edits and save, background pixels, grid pixels and pan alignment.");
                    exit.write(AppExit::Success);
                },
            );
        }
        1800 => panic!("canvas colors smoke timed out"),
        _ => {}
    }
}

fn main() {
    let directory = tempfile::tempdir().unwrap();
    interface_app()
        .insert_resource(WorkspaceFile::new(directory.path().join("interface.json")))
        .insert_resource(CapturePath(
            std::env::args().nth(1).expect("provide screenshot path"),
        ))
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands| {
            let root = commands.spawn(BoxRoot).id();
            commands.spawn((
                Square,
                CanvasItem {
                    position: DVec2::new(180.0, 0.0),
                    size: Vec2::splat(180.0),
                },
                BackgroundColor(PURPLE),
                ChildOf(root),
            ));
        })
        .add_systems(Update, exercise)
        .run();
}
