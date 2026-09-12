use bevy::{
    asset::RenderAssetUsages,
    diagnostic::FrameCount,
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    winit::WinitSettings,
};
use lince_interface::{
    app::interface_app,
    castle::{Castle, PresentCastles},
    container::BoxRoot,
    icons::{Icon, IconButton},
    sand::{ImageSand, Square},
};

#[derive(Resource)]
struct Fixture {
    root: Entity,
    group: Entity,
    image: Handle<Image>,
    button: Option<Entity>,
    screenshots: usize,
    path: String,
}

fn setup(world: &mut World) {
    let root = world.spawn(BoxRoot).id();
    let group = world
        .spawn((
            Castle,
            ChildOf(root),
            Node {
                position_type: PositionType::Absolute,
                left: px(80),
                top: px(80),
                width: px(240),
                height: px(160),
                ..default()
            },
        ))
        .id();
    world.spawn((
        Square,
        ChildOf(group),
        BackgroundColor(Color::srgb(1.0, 0.0, 0.0)),
        Node {
            position_type: PositionType::Absolute,
            width: px(80),
            height: px(80),
            ..default()
        },
    ));
    let nested = world
        .spawn((
            Castle,
            ChildOf(group),
            Node {
                position_type: PositionType::Absolute,
                left: px(120),
                width: px(80),
                height: px(80),
                ..default()
            },
        ))
        .id();
    let image = world.resource::<Assets<Image>>().reserve_handle();
    world.spawn((
        ImageSand,
        ImageNode::new(image.clone()),
        ChildOf(nested),
        Node {
            width: px(80),
            height: px(80),
            ..default()
        },
    ));
    world.spawn((
        Square,
        ChildOf(root),
        BackgroundColor(Color::srgb(0.0, 0.0, 1.0)),
        Node {
            position_type: PositionType::Absolute,
            left: px(400),
            top: px(80),
            width: px(80),
            height: px(80),
            ..default()
        },
    ));
    world.insert_resource(Fixture {
        root,
        group,
        image,
        button: None,
        screenshots: 0,
        path: std::env::args().nth(1).expect("provide output path"),
    });
}

fn capture(world: &mut World, ready: bool) {
    let path = format!(
        "{}.{}.png",
        world.resource::<Fixture>().path,
        if ready { "ready" } else { "waiting" }
    );
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            move |event: On<ScreenshotCaptured>, mut fixture: ResMut<Fixture>| {
                let red = event.image.get_color_at(100, 100).unwrap().to_srgba();
                let green = event.image.get_color_at(220, 100).unwrap().to_srgba();
                let blue = event.image.get_color_at(420, 100).unwrap().to_srgba();
                assert_eq!(red.red > 0.9 && red.green < 0.1, ready);
                assert_eq!(green.green > 0.9 && green.red < 0.1, ready);
                assert!(blue.blue > 0.9 && blue.red < 0.1);
                fixture.screenshots += 1;
            },
        );
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 8 {
        capture(world, false);
    }
    if frame == 16 {
        let image = world.resource::<Fixture>().image.id();
        world
            .resource_mut::<Assets<Image>>()
            .insert(
                image,
                Image::new_fill(
                    Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    &[0, 255, 0, 255],
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                ),
            )
            .unwrap();
    }
    if frame == 32 {
        let root = world.resource::<Fixture>().root;
        let button = world
            .spawn((
                IconButton::new(Icon::Plus, "A complete button"),
                ChildOf(root),
            ))
            .id();
        world.resource_mut::<Fixture>().button = Some(button);
    }
    if frame == 40 {
        capture(world, true);
    }
    if frame == 50 {
        assert_eq!(world.resource::<Fixture>().screenshots, 2);
        println!(
            "Castle smoke passed: delayed GPU image, nested groups, unrelated Sand, late button creation and complete rendered frames."
        );
        world.write_message(AppExit::Success);
    }
}

fn check_button(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 8 || frame == 40 {
        let group = world.resource::<Fixture>().group;
        assert_eq!(
            world.get::<InheritedVisibility>(group).unwrap().get(),
            frame == 40
        );
    }
    let Some(button) = world.resource::<Fixture>().button else {
        return;
    };
    let shown = world.get::<InheritedVisibility>(button).unwrap().get();
    if frame == 32 {
        assert!(!shown);
    }
    if shown {
        let children = world.get::<Children>(button).unwrap();
        assert_eq!(children.len(), 2);
        assert!(
            children
                .iter()
                .any(|child| world.get::<Square>(child).is_some())
        );
        assert!(
            children
                .iter()
                .any(|child| world.get::<ImageSand>(child).is_some())
        );
        for child in children.iter() {
            assert!(world.get::<InheritedVisibility>(child).unwrap().get());
            assert!(
                world
                    .get::<ComputedNode>(child)
                    .unwrap()
                    .size()
                    .min_element()
                    > 0.0
            );
        }
    }
    if frame >= 38 {
        assert!(shown);
    }
}

fn main() {
    interface_app()
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, setup)
        .add_systems(
            PostUpdate,
            exercise.after(lince_interface::actions::ApplyActions),
        )
        .add_systems(Last, check_button.after(PresentCastles))
        .run();
}
