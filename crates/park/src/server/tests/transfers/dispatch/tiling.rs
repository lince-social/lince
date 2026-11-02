use super::*;

fn rectangle(fixture: &Fixture, id: SurfaceId) -> lince_interface::host::Rectangle {
    fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter()
        .find(|item| item.id == ItemId::Application(id)).unwrap().mapping.canvas
}

fn open(fixture: &mut Fixture, name: &str) -> (ClientWindow, wl_buffer::WlBuffer, SurfaceId) {
    let qh = fixture.queue.handle();
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
    let role = xdg.get_toplevel(&qh, ());
    role.set_app_id(name.into());
    surface.commit();
    fixture.exchange();
    fixture.exchange();
    let buffer = privacy::buffer(fixture, [1000, 700], [0, 0, 255, 255]);
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, 1000, 700);
    surface.commit();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().any(|window| window.app_id == name && window.mapped));
    let id = fixture.desktop.server.windows().iter().find(|window| window.app_id == name).unwrap().id;
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Application(id)))));
    fixture.desktop.send(Input::Activate(id)).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&surface));
    (ClientWindow { surface, xdg, role }, buffer, id)
}

fn chord(fixture: &mut Fixture, key: u32) {
    fixture.desktop.key(125, true, 10).unwrap();
    fixture.desktop.key(key, true, 11).unwrap();
    fixture.desktop.key(key, false, 12).unwrap();
    fixture.desktop.key(125, false, 13).unwrap();
    fixture.inspect(|_| ());
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn full_width_wayland_window_and_later_windows_form_a_nonoverlapping_row_with_real_shortcuts_and_zoom() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::new();
    harness.server.configure_outputs(&[Monitor { id: lince_interface::host::OutputId(1), name: "Tiling-fixture".into(), origin: [0, 0], size: [1000, 720] }]).unwrap();
    harness.sync();
    let first = harness.window();
    let _first_buffer = paint(&mut harness, &first, [1000, 700], [0, 255, 0, 255]);
    let first_id = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.items.iter().any(|item| item.id == ItemId::Application(first_id)))));
    fixture.desktop.send(Input::Activate(first_id)).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&first.surface));
    fixture.desktop.send(Input::Action(DesktopAction::SetLayout(WorkspaceLayout::Tiled))).unwrap();
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        world.get_mut::<native::canvas::CanvasView>(root).unwrap().set_zoom(1.0);
    });
    first.role.set_maximized();
    fixture.exchange();
    fixture.desktop.dispatch().unwrap();
    let state = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let entity = world.query::<(Entity, &native::layout::desktop::ApplicationWindow)>().iter(world)
            .find(|(_, window)| window.surface == first_id).map(|(entity, _)| entity).unwrap();
        let root = world.get::<ChildOf>(entity).unwrap().parent();
        let id = PlacementId(world.get::<native::layout::LayoutBox>(entity).unwrap().id);
        let model = &world.get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop;
        (model.mode(), model.maximized(id), model.item_width(id), world.get::<native::canvas::CanvasItem>(entity).unwrap().size)
    });
    assert_eq!(state.0, WorkspaceLayout::Tiled);
    assert!(state.1, "The mapped client maximize request was not applied: {state:?}");
    fixture.until(|fixture| rectangle(fixture, first_id).size()[0] > 900.0);
    let before = rectangle(&fixture, first_id).size();
    let (second, _second_buffer, second_id) = open(&mut fixture, "tiling.second");
    let (third, _third_buffer, third_id) = open(&mut fixture, "tiling.third");
    for (left, right) in [(first_id, second_id), (second_id, third_id)] {
        let a = rectangle(&fixture, left);
        let b = rectangle(&fixture, right);
        assert!(a.origin()[0] + a.size()[0] <= b.origin()[0], "Adjacent tiled clients overlap: {a:?} {b:?}");
    }
    assert_eq!(rectangle(&fixture, first_id).size(), before);
    fixture.desktop.send(Input::Activate(first_id)).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&first.surface));
    for window in [&second, &third] {
        chord(&mut fixture, 38);
        fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&window.surface));
    }
    chord(&mut fixture, 35);
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&second.surface));
    fixture.desktop.send(Input::Activate(first_id)).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&first.surface));
    chord(&mut fixture, 57);
    let (zoom, modal) = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        (world.get::<native::canvas::CanvasView>(root).unwrap().zoom, native::shortcuts::keyboard_owned(world))
    });
    assert_eq!(zoom, 0.5);
    assert!(!modal);
    chord(&mut fixture, 38);
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&second.surface));
    chord(&mut fixture, 38);
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&third.surface));
    assert_eq!(fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        world.get::<native::canvas::CanvasView>(root).unwrap().zoom
    }), 0.5);
    assert_eq!(rectangle(&fixture, first_id).size()[0], before[0] * 0.5);
    let (output, pixels) = fixture.output_pixels(lince_interface::host::OutputId(1));
    for (id, green) in [(first_id, true), (second_id, false)] {
        let rectangle = rectangle(&fixture, id);
        let point: [usize; 2] = std::array::from_fn(|axis| {
            let low = rectangle.origin()[axis].max(0.0);
            let high = (rectangle.origin()[axis] + rectangle.size()[axis]).min(output.bounds.size()[axis]);
            assert!(high > low);
            ((low + high) * 0.5) as usize
        });
        let offset = (point[1] * output.source_size()[0] as usize + point[0]) * 4;
        let sample = &pixels[offset..offset + 4];
        if green { assert!(sample[1] > 200 && sample[0] < 20 && sample[2] < 20, "The original tile lost its own visible pixels: {sample:?}"); }
        else { assert!(sample[1] < 20 && (sample[0] > 200 || sample[2] > 200), "The new tile was covered by its neighbor: {sample:?}"); }
    }
    chord(&mut fixture, 57);
    let selected = rectangle(&fixture, third_id);
    assert!(selected.origin()[0] >= -0.01 && selected.origin()[0] + selected.size()[0] <= output.bounds.size()[0] + 0.01,
        "Returning to normal zoom hid the item selected in the tiled view: {selected:?}");
    assert_eq!(fixture.client.keyboard_focus.as_ref(), Some(&third.surface));
    fixture.desktop.key(42, true, 20).unwrap();
    chord(&mut fixture, 36);
    fixture.desktop.key(42, false, 21).unwrap();
    fixture.inspect(|_| ());
    let upper = rectangle(&fixture, first_id);
    let lower = rectangle(&fixture, third_id);
    assert!(upper.origin()[1] + upper.size()[1] <= lower.origin()[1]);
    chord(&mut fixture, 37);
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&first.surface));
    chord(&mut fixture, 38);
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&second.surface));
    chord(&mut fixture, 36);
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&third.surface));
    fixture.desktop.worker.stop().unwrap();
}
