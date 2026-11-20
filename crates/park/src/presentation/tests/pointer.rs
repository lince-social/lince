use super::*;
use bevy::{
    ecs::message::MessageUpdateSystems,
    picking::pointer::{PointerId, PointerInput},
};

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn ordinary_click_opens_edit_mode_from_the_workspace_controls() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    let mut canvas = Presentation::new(size, directory.path().into(), WakeSignal::new(|| {}))
        .unwrap();
    super::shortcuts::settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    let toggle = canvas.world_mut().get::<native::edit_mode::EditMode>(root).unwrap().toggle;
    super::shortcuts::pixel_pointer(&mut canvas, [8.0, 712.0]);
    super::shortcuts::settle(&mut canvas);
    super::shortcuts::click(&mut canvas, toggle, size);
    assert!(canvas.world_mut().get::<native::edit_mode::EditMode>(root).unwrap().enabled);
}

#[test]
fn host_pointer_targets_follow_outputs_without_retargeting_content_surfaces() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Image>>()
        .add_message::<PointerInput>()
        .add_systems(First, route_pointer_target.after(MessageUpdateSystems));
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(output_image([32, 32]));
    let content = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(output_image([16, 16]));
    let window_target = RenderTarget::Window(bevy::window::WindowRef::Entity(window))
        .normalize(Some(window))
        .unwrap();
    let image_target = RenderTarget::Image(target.clone().into())
        .normalize(Some(window))
        .unwrap();
    let content_target = RenderTarget::Image(content.into())
        .normalize(Some(window))
        .unwrap();
    let location = Location {
        target: window_target,
        position: Vec2::new(12.0, 14.0),
    };
    let mouse = app
        .world_mut()
        .spawn((PointerId::Mouse, PointerLocation::new(location.clone())))
        .id();
    let inner = app
        .world_mut()
        .spawn((
            native::topology::input::CONTENT_POINTER,
            PointerLocation::new(Location {
                target: content_target.clone(),
                position: Vec2::new(3.0, 4.0),
            }),
        ))
        .id();
    app.insert_resource(OutputTarget(RenderTarget::Image(target.into())));
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        location,
        PointerAction::Press(bevy::picking::pointer::PointerButton::Primary),
    ));
    app.update();
    let messages = app.world().resource::<Messages<PointerInput>>();
    let event = MessageCursor::<PointerInput>::default()
        .read(messages)
        .next()
        .unwrap()
        .clone();
    assert_eq!(event.location.target, image_target);
    assert_eq!(event.location.position, Vec2::new(12.0, 14.0));
    assert_eq!(
        app.world()
            .get::<PointerLocation>(mouse)
            .unwrap()
            .location()
            .unwrap()
            .target,
        image_target
    );
    assert_eq!(
        app.world()
            .get::<PointerLocation>(inner)
            .unwrap()
            .location()
            .unwrap()
            .target,
        content_target
    );
    let replacement = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(output_image([64, 64]));
    let replacement_target = RenderTarget::Image(replacement.clone().into())
        .normalize(Some(window))
        .unwrap();
    app.insert_resource(OutputTarget(RenderTarget::Image(replacement.into())));
    app.update();
    assert_eq!(
        app.world()
            .get::<PointerLocation>(mouse)
            .unwrap()
            .location()
            .unwrap()
            .target,
        replacement_target
    );
    assert_eq!(
        app.world()
            .get::<PointerLocation>(inner)
            .unwrap()
            .location()
            .unwrap()
            .target,
        content_target
    );
}
