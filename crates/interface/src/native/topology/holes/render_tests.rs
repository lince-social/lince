use super::*;
use bevy::{
    camera::{ImageRenderTarget, NormalizedRenderTarget, RenderTarget},
    input::{ButtonState, mouse::MouseButtonInput},
    picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    window::{PrimaryWindow, WindowEvent},
};

#[derive(Resource, Default)]
struct Clicks(usize);

#[derive(Resource, Default)]
struct Captured(Option<Image>);

fn update(app: &mut App, count: usize) {
    for _ in 0..count {
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn click(app: &mut App, image: &Handle<Image>, screen: Vec2) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .unwrap();
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(screen));
    let location = Location {
        target: NormalizedRenderTarget::Image(ImageRenderTarget {
            handle: image.clone(),
            scale_factor: 1.0,
        }),
        position: screen,
    };
    for action in [
        PointerAction::Move { delta: Vec2::ZERO },
        PointerAction::Press(PointerButton::Primary),
        PointerAction::Release(PointerButton::Primary),
    ] {
        if matches!(action, PointerAction::Press(_) | PointerAction::Release(_)) {
            app.world_mut()
                .write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
                    button: MouseButton::Left,
                    state: if matches!(action, PointerAction::Press(_)) {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                    window,
                }));
        }
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            location.clone(),
            action,
        ));
        update(app, 3);
    }
}

fn capture(app: &mut App, image: &Handle<Image>) -> Image {
    app.world_mut().resource_mut::<Captured>().0 = None;
    app.world_mut()
        .spawn(Screenshot::image(image.clone()))
        .observe(
            |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                captured.0 = Some(event.image.clone());
            },
        );
    for _ in 0..200 {
        update(app, 1);
        if let Some(image) = app.world_mut().resource_mut::<Captured>().0.take() {
            return image;
        }
    }
    panic!("Workspace hole capture timed out")
}

fn offscreen_presentation_app() -> App {
    let plugins = DefaultPlugins
        .build()
        .disable::<bevy::log::LogPlugin>()
        .disable::<bevy::winit::WinitPlugin>()
        .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
        .set(bevy::render::RenderPlugin {
            synchronous_pipeline_compilation: true,
            ..default()
        })
        .set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (800, 640).into(),
                ..default()
            }),
            close_when_requested: false,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        });
    let mut app = crate::native::app::presentation_app(
        std::env::temp_dir().join(format!("lince-hole-test-assets-{}", std::process::id())),
        plugins,
    );
    app.insert_resource(crate::host::WakeSignal::new(|| {}));
    app
}

#[test]
#[ignore = "requires a native render adapter"]
fn workspace_hole_pixels_and_clicks_reach_only_the_exposed_sand() {
    use crate::native::actions::Action;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut app = offscreen_presentation_app();
    app.insert_resource(crate::native::workspace::WorkspaceFile::new(
        directory.path().join("interface.json"),
    ))
    .init_resource::<Clicks>()
    .init_resource::<Captured>();
    app.world_mut()
        .query::<&mut Window>()
        .single_mut(app.world_mut())
        .unwrap()
        .resolution
        .set(800.0, 600.0);
    let image = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            800,
            600,
            bevy::render::render_resource::TextureFormat::Bgra8UnormSrgb,
            None,
        ));
    let target = RenderTarget::Image(ImageRenderTarget {
        handle: image.clone(),
        scale_factor: 1.0,
    });
    app.add_systems(
        PostStartup,
        move |mut commands: Commands, cameras: Query<Entity, With<Camera>>| {
            for camera in &cameras {
                commands.entity(camera).insert(target.clone());
            }
        },
    );
    let root = app
        .world_mut()
        .spawn(crate::native::container::BoxRoot)
        .id();
    app.finish();
    app.cleanup();
    update(&mut app, 5);
    crate::native::edit_mode::EditAction::Close.apply(app.world_mut(), root);
    let lower = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(300.0),
            },
            WorkspaceMember(1),
            ChildOf(root),
            Button,
            BackgroundColor(Color::srgb(0.05, 0.85, 0.15)),
        ))
        .observe(|_: On<Pointer<Click>>, mut clicks: ResMut<Clicks>| {
            clicks.0 += 1;
        })
        .id();
    crate::native::workspace::create(app.world_mut(), root);
    app.world_mut().spawn((
        CanvasItem {
            position: DVec2::ZERO,
            size: Vec2::new(700.0, 500.0),
        },
        WorkspaceMember(2),
        ChildOf(root),
        BackgroundColor(Color::srgb(0.1, 0.15, 0.85)),
    ));
    let mut hole = InfluenceArea::new(
        crate::native::area::AreaShape::Circle,
        DVec2::ZERO,
        DVec2::splat(200.0),
    );
    hole.hole = true;
    let opening = crate::native::area::spawn_area(app.world_mut(), root, 2, hole).unwrap();
    update(&mut app, 80);
    let open = capture(&mut app, &image);
    let center = open.get_color_at(400, 300).unwrap().to_srgba();
    let corner = open.get_color_at(480, 380).unwrap().to_srgba();
    assert!(
        center.green > center.blue * 2.0,
        "Lower Sand must show through the hole: {center:?}"
    );
    assert!(
        corner.blue > corner.green * 2.0,
        "Round-hole corners must retain the upper workspace: {corner:?}"
    );
    click(&mut app, &image, Vec2::new(400.0, 300.0));
    assert_eq!(app.world().resource::<Clicks>().0, 1);
    click(&mut app, &image, Vec2::new(480.0, 380.0));
    assert_eq!(app.world().resource::<Clicks>().0, 1);
    let surface = app
        .world()
        .get::<super::super::presentation::Surface>(lower)
        .unwrap();
    assert!(surface.visible);
    app.world_mut()
        .get_mut::<InfluenceArea>(opening)
        .unwrap()
        .enabled = false;
    update(&mut app, 10);
    let closed = capture(&mut app, &image);
    let center = closed.get_color_at(400, 300).unwrap().to_srgba();
    assert!(
        center.blue > center.green * 2.0,
        "Closing the hole must restore the upper workspace: {center:?}"
    );
    click(&mut app, &image, Vec2::new(400.0, 300.0));
    assert_eq!(app.world().resource::<Clicks>().0, 1);
    assert!(
        !app.world()
            .get::<super::super::presentation::Surface>(lower)
            .unwrap()
            .visible
    );
}
