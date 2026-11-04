use super::*;

fn delivered(canvas: &mut Presentation, window: Entity, position: [f32; 2]) {
    canvas
        .world_mut()
        .resource_mut::<Messages<KeyboardInput>>()
        .clear();
    canvas
        .world_mut()
        .resource_mut::<Messages<bevy::window::CursorMoved>>()
        .clear();
    canvas.pointer(position).unwrap();
    canvas
        .key(
            KeyCode::KeyA,
            Key::Character("a".into()),
            Some("a".into()),
            true,
            false,
        )
        .unwrap();
    let world = canvas.app.world();
    assert_eq!(
        MessageCursor::<KeyboardInput>::default()
            .read(world.resource::<Messages<KeyboardInput>>())
            .last()
            .unwrap()
            .window,
        window
    );
    assert_eq!(
        MessageCursor::<bevy::window::CursorMoved>::default()
            .read(world.resource::<Messages<bevy::window::CursorMoved>>())
            .last()
            .unwrap()
            .window,
        window
    );
    assert_eq!(
        world.get::<Window>(window).unwrap().cursor_position(),
        Some(Vec2::from_array(position))
    );
    let target = canvas
        .input_snapshot(None)
        .drop_target
        .expect("The real canvas must remain a current transfer recipient");
    assert_eq!(target.window, window.to_bits());
    assert_eq!(target.output, lince_interface::host::OutputId(1));
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn live_primary_window_selection_keeps_input_and_transfer_targets_current_through_replacement() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas =
        super::appearance::launch(&directory.path().join("workspace.json"), [640, 480]);
    canvas
        .outputs(vec![super::transfers::output(1, 0.0, [640, 480])])
        .unwrap();
    super::shortcuts::settle(&mut canvas);
    let original = canvas.input_window().unwrap();
    let template = canvas.app.world().get::<Window>(original).unwrap().clone();
    let root = canvas.canvas_root().unwrap();
    delivered(&mut canvas, original, [120.0, 90.0]);
    canvas
        .world_mut()
        .entity_mut(original)
        .remove::<PrimaryWindow>();
    assert!(canvas.input_window().is_err());
    assert!(canvas.pointer([140.0, 100.0]).is_err());
    assert!(
        canvas
            .key(
                KeyCode::KeyA,
                Key::Character("a".into()),
                None,
                false,
                false
            )
            .is_err()
    );
    assert!(canvas.input_snapshot(None).drop_target.is_none());
    let replacement = canvas
        .world_mut()
        .spawn((
            template.clone(),
            PrimaryWindow,
            Name::new("replacement"),
            native::actions::WindowActionTarget(root),
        ))
        .id();
    assert_eq!(canvas.input_window().unwrap(), replacement);
    delivered(&mut canvas, replacement, [160.0, 110.0]);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<Window>(original)
            .unwrap()
            .cursor_position(),
        Some(Vec2::new(120.0, 90.0))
    );
    canvas
        .world_mut()
        .entity_mut(original)
        .insert(PrimaryWindow);
    let expected = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .iter(world)
            .next()
            .unwrap()
    };
    assert_eq!(canvas.input_window().unwrap(), expected);
    delivered(&mut canvas, expected, [190.0, 130.0]);
    canvas
        .world_mut()
        .entity_mut(original)
        .remove::<PrimaryWindow>();
    canvas.world_mut().despawn(replacement);
    assert!(canvas.input_window().is_err());
    let replacement = canvas
        .world_mut()
        .spawn((
            template,
            PrimaryWindow,
            Transform::default(),
            Name::new("new-archetype"),
            native::actions::WindowActionTarget(root),
        ))
        .id();
    assert_eq!(canvas.input_window().unwrap(), replacement);
    delivered(&mut canvas, replacement, [210.0, 150.0]);
    canvas.world_mut().despawn(replacement);
    assert!(canvas.input_snapshot(None).drop_target.is_none());
    canvas
        .world_mut()
        .entity_mut(original)
        .insert(PrimaryWindow);
    assert_eq!(canvas.input_window().unwrap(), original);
    delivered(&mut canvas, original, [230.0, 170.0]);
    canvas
        .world_mut()
        .resource_mut::<Messages<KeyboardInput>>()
        .clear();
    canvas
        .world_mut()
        .resource_mut::<Messages<bevy::window::CursorMoved>>()
        .clear();
    canvas
        .world_mut()
        .resource_mut::<Messages<WindowEvent>>()
        .clear();
    canvas.finish().unwrap();
}
