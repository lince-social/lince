use bevy::{
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::i18n::{Language, Token, catalog, initialize, native_text};
use lince_sandbox::{
    actions::Action,
    app::interface_app,
    canvas::CanvasItem,
    container::BoxRoot,
    laboratory::{Laboratory, LaboratoryAction},
    sand_store::{SandKind, spawn_sand},
    theme::Typography,
    workspace::Workspaces,
};
use std::{path::PathBuf, time::Instant};

const SAMPLES: [Token; 6] = [
    Token::Name,
    Token::Title,
    Token::Description,
    Token::Message,
    Token::Information,
    Token::PlainText,
];

#[derive(Resource)]
struct Demo {
    output: PathBuf,
    started: Instant,
    stage: u8,
    ready_frame: u32,
}

fn label(world: &mut World, parent: Entity, value: &str, size: f32) {
    let font = world.resource::<Typography>().text(size);
    world.spawn((
        Text::new(value),
        font,
        lince_interface::style::text(lince_interface::tokens::Token::Ink),
        Node {
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(parent),
    ));
}

fn compose(world: &mut World) {
    let root = world.resource::<Laboratory>().root.unwrap();
    let configuration = spawn_sand(
        world,
        root,
        1,
        SandKind::Configuration,
        "Language preview",
        DVec2::new(-310.0, 175.0),
    );
    world.get_mut::<CanvasItem>(configuration).unwrap().size = Vec2::new(620.0, 680.0);
    for mut text in world.query::<&mut Text>().iter_mut(world) {
        if text.0 == Token::LoadingConfiguration.text() {
            text.0 = "Read-only Laboratory preview".into();
        }
    }
    let samples = spawn_sand(
        world,
        root,
        1,
        SandKind::Square,
        "",
        DVec2::new(330.0, 125.0),
    );
    world.get_mut::<CanvasItem>(samples).unwrap().size = Vec2::new(510.0, 580.0);
    world.entity_mut(samples).insert(Node {
        padding: UiRect::all(px(24)),
        flex_direction: FlexDirection::Column,
        row_gap: px(12),
        ..default()
    });
    label(world, samples, "New Ithkuil", 30.0);
    label(world, samples, "Active at startup · art-x-ithkuil", 15.0);
    label(
        world,
        samples,
        "Native script from the translation catalog",
        14.0,
    );
    for token in SAMPLES {
        let row = world
            .spawn((
                Node {
                    column_gap: px(24),
                    align_items: AlignItems::Center,
                    min_height: px(42),
                    ..default()
                },
                ChildOf(samples),
            ))
            .id();
        label(world, row, token.text(), 32.0);
        label(world, row, Language::English.catalog().get(token), 20.0);
    }
    label(world, samples, "Missing translations use English:", 14.0);
    label(world, samples, Token::RestartRequired.text(), 14.0);
    assert_eq!(catalog().language, Language::Ithkuil);
}

fn update(world: &mut World) {
    assert!(
        world.resource::<Demo>().started.elapsed().as_secs() < 120,
        "Language demo timed out"
    );
    let frame = world.resource::<FrameCount>().0;
    match world.resource::<Demo>().stage {
        0 if frame > 15 => {
            let root = world
                .query_filtered::<Entity, With<BoxRoot>>()
                .single(world)
                .unwrap();
            LaboratoryAction::Open.apply(world, root);
            compose(world);
            let mut demo = world.resource_mut::<Demo>();
            demo.stage = 1;
            demo.ready_frame = frame + 100;
        }
        1 if frame >= world.resource::<Demo>().ready_frame => {
            for token in SAMPLES {
                let script = native_text(token.text()).unwrap();
                assert!(
                    world
                        .query::<&Text>()
                        .iter(world)
                        .any(|text| text.0 == script.encoding())
                );
            }
            let output = world.resource::<Demo>().output.clone();
            world.resource_mut::<Demo>().stage = 2;
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(output))
                .observe(
                    |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        println!("New Ithkuil Laboratory capture saved");
                        exit.write(AppExit::Success);
                    },
                );
        }
        _ => {}
    }
}

fn main() {
    initialize(Language::Ithkuil).unwrap();
    let output = PathBuf::from(std::env::args().nth(1).expect("Provide a PNG capture path"));
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    interface_app()
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Demo {
            output,
            started: Instant::now(),
            stage: 0,
            ready_frame: 0,
        })
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                let mut window = windows.single_mut().unwrap();
                window.title = "Lince Laboratory · New Ithkuil".into();
                window.resolution.set(1440.0, 1100.0);
                commands.spawn((BoxRoot, Workspaces::default()));
            },
        )
        .add_systems(Update, update)
        .run();
}
