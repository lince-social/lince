use super::*;

fn center(canvas: &Presentation, entity: Entity) -> Vec2 {
    let world = canvas.app.world();
    let computed = world.get::<ComputedNode>(entity).unwrap();
    world
        .get::<UiGlobalTransform>(entity)
        .unwrap()
        .transform_point2(Vec2::ZERO)
        * computed.inverse_scale_factor()
}

fn drag(canvas: &mut Presentation, source: Entity, target: Vec2, cancel: bool) {
    let name = canvas
        .app
        .world()
        .get::<Children>(source)
        .unwrap()
        .iter()
        .find_map(|child| {
            canvas
                .app
                .world()
                .get::<Text>(child)
                .map(|text| text.0.clone())
        })
        .unwrap();
    let origin = center(canvas, source);
    canvas.pointer(origin.to_array()).unwrap();
    settle(canvas);
    canvas.button(MouseButton::Left, true).unwrap();
    settle(canvas);
    canvas
        .pointer((origin + Vec2::new(8.0, 0.0)).to_array())
        .unwrap();
    settle(canvas);
    canvas.pointer(target.to_array()).unwrap();
    let frame = settle(canvas);
    assert!(frame.input.keyboard_modal);
    assert!(
        canvas
            .world_mut()
            .query::<&Name>()
            .iter(canvas.app.world())
            .any(|name| name.as_str() == "overview.drag")
    );
    capture("overview-window-drag-preview", canvas.render_size, &frame);
    if cancel {
        key(canvas, KeyCode::Escape, true);
        key(canvas, KeyCode::Escape, false);
    }
    canvas.button(MouseButton::Left, false).unwrap();
    settle(canvas);
    if !cancel {
        let world = canvas.app.world();
        let focused = world
            .resource::<bevy::input_focus::InputFocus>()
            .get()
            .unwrap();
        assert!(
            world
                .get::<Children>(focused)
                .unwrap()
                .iter()
                .any(|child| world.get::<Text>(child).is_some_and(|text| text.0 == name)),
            "Overview did not focus the moved item's destination thumbnail"
        );
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn actual_overview_pointer_drag_reorders_rows_transfers_once_and_cancels_without_losing_native_content()
 {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    let mut canvas = Presentation::new(
        size,
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    canvas
        .world_mut()
        .remove_resource::<native::layout::desktop::DefaultLayout>();
    native::workspace::create(canvas.world_mut(), root);
    let second = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    for workspace in [1, second] {
        canvas
            .world_mut()
            .get_mut::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter_mut()
            .find(|entry| entry.id == workspace)
            .unwrap()
            .desktop
            .set_mode(WorkspaceLayout::Tiled);
        native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
    }
    let mut items = Vec::new();
    for (workspace, name, color) in [
        (1, "Drag A", Color::srgb(0.9, 0.1, 0.1)),
        (1, "Drag B", Color::srgb(0.1, 0.9, 0.1)),
        (second, "Drag C", Color::srgb(0.1, 0.1, 0.9)),
    ] {
        let layout = native::layout::LayoutBox::new(Vec2::new(300.0, 200.0));
        let entity = canvas
            .world_mut()
            .spawn((
                native::canvas::CanvasItem {
                    position: bevy::math::DVec2::new(600.0, -300.0),
                    size: Vec2::new(300.0, 200.0),
                },
                layout,
                native::workspace::WorkspaceMember(workspace),
                BackgroundColor(color),
                Name::new(name),
                ChildOf(root),
            ))
            .id();
        items.push((entity, layout.id));
    }
    let editor = canvas
        .world_mut()
        .spawn((
            native::sand::editable("Keep this draft during overview drag"),
            ChildOf(items[0].0),
        ))
        .id();
    native::workspace::switch_output(canvas.world_mut(), root, second);
    settle(&mut canvas);
    native::workspace::switch_output(canvas.world_mut(), root, 1);
    settle(&mut canvas);
    chord(&mut canvas, KeyCode::Space);
    let before = settle(&mut canvas);
    let original_pixels = probe_pixels(before.dmabuf().unwrap()).unwrap();
    capture("overview-before-window-drag", size, &before);
    let source = named_button(&mut canvas, "Drag A");
    let anchor = named_button(&mut canvas, "Drag B");
    let node = canvas.app.world().get::<ComputedNode>(anchor).unwrap();
    let bottom = center(&canvas, anchor)
        + Vec2::new(0.0, node.size().y * node.inverse_scale_factor() * 0.45);
    drag(&mut canvas, source, bottom, false);
    let model = &canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries
        .iter()
        .find(|entry| entry.id == 1)
        .unwrap()
        .desktop;
    assert_eq!(
        model.tiled_rows(),
        vec![
            vec![lince_interface::layout::desktop::PlacementId(items[1].1)],
            vec![lince_interface::layout::desktop::PlacementId(items[0].1)]
        ]
    );
    assert!(native::layout::desktop::overview::active(
        canvas.app.world()
    ));
    let source = named_button(&mut canvas, "Drag A");
    let target = named_button(&mut canvas, "Drag C");
    let node = canvas.app.world().get::<ComputedNode>(target).unwrap();
    let target =
        center(&canvas, target) + Vec2::new(node.size().x * node.inverse_scale_factor() * 0.3, 0.0);
    drag(&mut canvas, source, target, false);
    let frame = settle(&mut canvas);
    assert!(frame.input.keyboard_modal);
    assert_ne!(
        probe_pixels(frame.dmabuf().unwrap()).unwrap(),
        original_pixels
    );
    capture("overview-after-window-drag", size, &frame);
    let moved = named_button(&mut canvas, "Drag A");
    let neighbour = named_button(&mut canvas, "Drag C");
    assert!(
        center(&canvas, moved).x > center(&canvas, neighbour).x,
        "The inactive tiled preview did not show the accepted horizontal row position"
    );
    let spaces = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap();
    assert_eq!(spaces.active, 1);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::WorkspaceMember>(items[0].0)
            .unwrap()
            .0,
        second
    );
    assert_eq!(
        spaces
            .entries
            .iter()
            .find(|entry| entry.id == second)
            .unwrap()
            .desktop
            .items()
            .len(),
        2
    );
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(items[0].0)
            .unwrap()
            .id,
        items[0].1
    );
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "Keep this draft during overview drag"
    );
    let source = named_button(&mut canvas, "Drag A");
    let target = named_button(&mut canvas, "Drag B");
    let target = center(&canvas, target);
    drag(&mut canvas, source, target, true);
    assert!(!native::layout::desktop::overview::active(
        canvas.app.world()
    ));
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::WorkspaceMember>(items[0].0)
            .unwrap()
            .0,
        second
    );
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(items[0].0)
            .unwrap()
            .id,
        items[0].1
    );
    assert!(
        !canvas
            .world_mut()
            .query::<&Name>()
            .iter(canvas.app.world())
            .any(|name| name.as_str() == "overview.drag")
    );
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn actual_first_free_canvas_drag_preserves_unmanaged_sands_in_active_inactive_and_spatial_views() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    for spatial in [false, true] {
        for active_source in [false, true] {
            fresh_free_drag(spatial, active_source);
        }
    }
}

fn fresh_free_drag(spatial: bool, active_source: bool) {
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    let mut canvas = Presentation::new(
        size,
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    canvas
        .world_mut()
        .remove_resource::<native::layout::desktop::DefaultLayout>();
    canvas.create_workspace(root.to_bits()).unwrap();
    let destination = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    for workspace in [1, destination] {
        canvas
            .world_mut()
            .get_mut::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter_mut()
            .find(|entry| entry.id == workspace)
            .unwrap()
            .desktop = lince_interface::layout::desktop::Workspace::new(WorkspaceLayout::Free);
        native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
    }
    if active_source {
        canvas.activate_workspace(root.to_bits(), 1).unwrap();
    }
    if spatial {
        native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
    }
    let active = if active_source { 1 } else { destination };
    let home = native::canvas::CanvasItem {
        position: bevy::math::DVec2::new(617.0, -223.0),
        size: Vec2::new(300.0, 200.0),
    };
    let layout = native::layout::LayoutBox::new(home.size);
    let item = canvas
        .world_mut()
        .spawn((
            home,
            layout,
            native::workspace::WorkspaceMember(1),
            BackgroundColor(Color::srgb(0.9, 0.1, 0.1)),
            Name::new("Fresh unmanaged Sand"),
            ChildOf(root),
        ))
        .id();
    let neighbour = canvas
        .world_mut()
        .spawn((
            native::canvas::CanvasItem {
                position: bevy::math::DVec2::new(200.0, -400.0),
                size: Vec2::new(300.0, 200.0),
            },
            native::layout::LayoutBox::new(Vec2::new(300.0, 200.0)),
            native::workspace::WorkspaceMember(destination),
            BackgroundColor(Color::srgb(0.1, 0.1, 0.9)),
            Name::new("Free destination Sand"),
            ChildOf(root),
        ))
        .id();
    let editor = canvas
        .world_mut()
        .spawn((
            native::sand::editable("Never focused or saved"),
            ChildOf(item),
        ))
        .id();
    settle(&mut canvas);
    assert!(
        canvas
            .app
            .world()
            .get::<native::layout::desktop::Managed>(item)
            .is_none()
    );
    assert!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .find(|entry| entry.id == 1)
            .unwrap()
            .desktop
            .items()
            .is_empty()
    );
    let view = *canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    chord(&mut canvas, KeyCode::Space);
    let before = settle(&mut canvas);
    capture(
        &format!("overview-fresh-free-before-{spatial}-{active_source}"),
        size,
        &before,
    );
    let source = named_button(&mut canvas, "Fresh unmanaged Sand");
    let target = named_button(&mut canvas, "Free destination Sand");
    let end = center(&canvas, target);
    drag(&mut canvas, source, end, false);
    let frame = settle(&mut canvas);
    capture(
        &format!("overview-fresh-free-after-{spatial}-{active_source}"),
        size,
        &frame,
    );
    assert!(frame.input.keyboard_modal);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active,
        active
    );
    for entity in [item, neighbour] {
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::WorkspaceMember>(entity)
                .unwrap()
                .0,
            destination
        );
    }
    assert_eq!(
        *canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(item)
            .unwrap(),
        layout
    );
    let item_state = canvas
        .app
        .world()
        .get::<native::canvas::CanvasItem>(item)
        .unwrap();
    assert_eq!(item_state.position, home.position);
    assert_eq!(item_state.size, home.size);
    assert!(
        *canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap()
            == view
    );
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "Never focused or saved"
    );
    let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
    assert!(
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] > 180 && pixel[1] < 80 && pixel[2] < 80)
            .count()
            > 100
    );
    chord(&mut canvas, KeyCode::Space);
    assert!(!settle(&mut canvas).input.keyboard_modal);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active,
        active
    );
}
