use bevy::{
    a11y::AccessibilityNode,
    diagnostic::FrameCount,
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    app::interface_app,
    container::BoxRoot,
    customization::{CustomizationAction, GlobalCustomizationPanelToggle},
    sand_store::{SandKind, spawn_sand},
    tokens::{ColorScheme, ThemeSettings, Token, document::ThemeDocument},
    workspace::WorkspaceFile,
};

#[derive(Resource)]
struct Capture {
    path: String,
    saved: u8,
    sand: Option<Entity>,
}

fn capture(world: &mut World, scheme: ColorScheme, suffix: &str) {
    let sand = world.resource::<Capture>().sand.unwrap();
    assert_eq!(world.resource::<ThemeSettings>().scheme, scheme);
    let node = world.get::<Node>(sand).unwrap();
    assert_eq!(
        node.border_radius,
        BorderRadius::all(px(Token::Roundness.default_value(scheme).number()))
    );
    assert_eq!(
        node.border,
        UiRect::all(px(Token::BorderWidth.default_value(scheme).number()))
    );
    assert_eq!(
        world.get::<BackgroundColor>(sand).unwrap().0,
        Token::SandBackground.default_value(scheme).color()
    );
    for label in ["Export theme…", "Copy theme"] {
        assert!(
            world
                .query::<&AccessibilityNode>()
                .iter(world)
                .any(|node| node.label() == Some(label))
        );
    }
    let bounds = lince_interface::topology::presentation::bounds(world, sand).unwrap();
    let scale = world
        .query::<&Window>()
        .single(world)
        .unwrap()
        .resolution
        .scale_factor();
    let corner = ((bounds.min + Vec2::ONE) * scale).as_uvec2();
    let colors = [Token::CanvasBackground, Token::CanvasGrid].map(|token| {
        let color = token.default_value(scheme).color().to_srgba();
        [color.red, color.green, color.blue]
    });
    let path = format!("{}.{suffix}", world.resource::<Capture>().path);
    std::fs::write(
        format!("{path}.json"),
        ThemeDocument::export(world.resource::<ThemeSettings>()).unwrap(),
    )
    .unwrap();
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("{path}.png")))
        .observe(
            move |event: On<ScreenshotCaptured>, mut capture: ResMut<Capture>| {
                let pixel = event
                    .image
                    .get_color_at(corner.x, corner.y)
                    .unwrap()
                    .to_srgba();
                for (index, value) in [pixel.red, pixel.green, pixel.blue].into_iter().enumerate() {
                    let low = colors[0][index].min(colors[1][index]) - 0.02;
                    let high = colors[0][index].max(colors[1][index]) + 0.02;
                    assert!(
                        (low..=high).contains(&value),
                        "{scheme:?} rounded corners must show the canvas"
                    );
                }
                capture.saved += 1;
            },
        );
}

fn exercise(world: &mut World) {
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    match world.resource::<FrameCount>().0 {
        8 => {
            let sand = spawn_sand(
                world,
                root,
                1,
                SandKind::Square,
                "",
                DVec2::new(-320.0, 0.0),
            );
            world.resource_mut::<Capture>().sand = Some(sand);
            let font = world
                .resource::<lince_interface::theme::Typography>()
                .text(22.0);
            world.spawn((
                Text::new("Make yourself at home"),
                font,
                lince_interface::token_style::text(Token::SandInk),
                Node {
                    margin: UiRect::all(px(16)),
                    ..default()
                },
                ChildOf(sand),
            ));
            world.trigger(GlobalCustomizationPanelToggle { entity: root });
        }
        12 => CustomizationAction::Scheme(ColorScheme::ComfyPink).apply(world, root),
        20 => capture(world, ColorScheme::ComfyPink, "comfy-pink"),
        26 => CustomizationAction::Scheme(ColorScheme::Moss).apply(world, root),
        34 => capture(world, ColorScheme::Moss, "moss"),
        46 => {
            assert_eq!(world.resource::<Capture>().saved, 2);
            println!(
                "Theme smoke passed: Comfy Pink and Moss render their colors, corners and borders with export controls; both JSON files saved."
            );
            world.write_message(AppExit::Success);
        }
        1800 => panic!("Theme smoke timed out"),
        _ => {}
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    interface_app()
        .insert_resource(WorkspaceFile::new(directory.path().join("interface.json")))
        .insert_resource(Capture {
            path: std::env::args()
                .nth(1)
                .expect("provide screenshot path prefix"),
            saved: 0,
            sand: None,
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(
            Startup,
            |mut commands: Commands, mut windows: Query<&mut Window>| {
                windows.single_mut().unwrap().resolution.set(1440.0, 960.0);
                commands.spawn(BoxRoot);
            },
        )
        .add_systems(Update, exercise)
        .run();
}
