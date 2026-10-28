use super::*;
use super::super::super::ime;
use lince_interface::host::magnification::Caret;
use lince_interface::host::Rectangle;

fn rectangle(pixels: &[u8], size: [u32; 2], low: [f64; 2], high: [f64; 2], color: [u8; 3], bgra: bool) {
    let mut first = [f64::INFINITY; 2];
    let mut last = [f64::NEG_INFINITY; 2];
    let start = low.map(|value| (value - 3.0).max(0.0).floor() as usize);
    let end: [usize; 2] = std::array::from_fn(|axis| (high[axis] + 3.0).min(f64::from(size[axis])).ceil() as usize);
    for y in start[1]..end[1] {
        for x in start[0]..end[0] {
            let offset = (y * size[0] as usize + x) * 4;
            let rgb = if bgra { [pixels[offset + 2], pixels[offset + 1], pixels[offset]] }
                else { [pixels[offset], pixels[offset + 1], pixels[offset + 2]] };
            if (0..3).all(|axis| rgb[axis].abs_diff(color[axis]) <= 20) {
                first[0] = first[0].min(x as f64); first[1] = first[1].min(y as f64);
                last[0] = last[0].max(x as f64 + 1.0); last[1] = last[1].max(y as f64 + 1.0);
            }
        }
    }
    assert!(first.iter().all(|value| value.is_finite()), "Rotated consumer pixels are absent");
    clients::compare(first, low, "rotated consumer leading edges");
    clients::compare(last, high, "rotated consumer trailing edges");
}

fn turned(rotation: QuarterTurn, low: [f64; 2], high: [f64; 2]) -> ([f64; 2], [f64; 2]) {
    let points = [[low[0], low[1]], [low[0], high[1]], [high[0], low[1]], [high[0], high[1]]].map(|point| physical(rotation, point));
    (std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min)),
        std::array::from_fn(|axis| points.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max)))
}

fn captured_rectangle(fixture: &mut Fixture, rotation: QuarterTurn, low: [f64; 2], high: [f64; 2], color: [u8; 3], cursors: bool) {
    let output = fixture.client.output.as_ref().unwrap().clone();
    let (source, session) = capture_lifetime::output_session_options(fixture, &output, if cursors { Options::PaintCursors } else { Options::empty() });
    let pending = capture_lifetime::Pending::new(fixture, &session, B_SIZE);
    let pixels = capture_lifetime::completed(fixture, &pending);
    let (low, high) = turned(rotation, low, high);
    rectangle(&pixels, B_SIZE, low, high, color, true);
    pending.destroy(fixture); session.destroy(); source.destroy(); fixture.exchange();
}

fn exclude(fixture: &mut Fixture, id: SurfaceId, excluded: bool) {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.resource::<lince_interface::host::capture::PolicyGate>().invalidate();
        let entity = world.query::<(Entity, &native::layout::desktop::ApplicationWindow)>().iter(world)
            .find_map(|(entity, window)| (window.surface == id).then_some(entity)).unwrap();
        world.get_mut::<native::layout::LayoutBox>(entity).unwrap().capture_excluded = excluded;
    });
    fixture.until(|fixture| fixture.desktop.server.capture_policy_gate().read().is_some_and(|read|
        read.policy().is_some_and(|policy| policy.windows.get(&id) == Some(&excluded))));
}

pub(super) fn application_and_layer(fixture: &mut Fixture, window: &ClientWindow, id: SurfaceId, size: [u32; 2], markers: &Layer, rotation: QuarterTurn) {
    fixture.desktop.send(Input::Activate(id)).unwrap();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&window.surface));
    let qh = fixture.queue.handle();
    let input = text_input_tests::input(&fixture.client, &qh);
    fixture.exchange();
    let caret = [size[0] as i32 / 2, size[1] as i32 / 2, 2, 18];
    text_input_tests::enable(&input, caret); fixture.exchange();
    let expected = Caret::Application { surface: id, rectangle: Rectangle::new([f64::from(caret[0]), f64::from(caret[1])], [2.0, 18.0]).unwrap() };
    fixture.until(|fixture| fixture.desktop.server.magnification_caret() == Some(expected));
    let mut method = ime::Ime::connect(fixture);
    text_input_tests::enable(&input, caret); fixture.exchange();
    let popup = method.popup(fixture, [32, 16], [255, 0, 255, 255]);
    method.until(fixture, |fixture, method| method.active() && fixture.desktop.current.as_ref().is_some_and(|frame|
        frame.windows.iter().find(|window| window.id == id).is_some_and(|window| window.popups.len() == 1)));
    let origin = fixture.desktop.server.windows().into_iter().find(|window| window.id == id).unwrap().popups[0].origin;
    assert_eq!(origin, [caret[0], caret[1] + caret[3]], "Rotation changed the client-local IME anchor");
    let item = clients::geometry(fixture, id);
    let (viewport, pixels) = fixture.output_pixels(B);
    let low = viewport.raster.point(item.mapping.canvas_point(origin.map(f64::from)).unwrap());
    let high = viewport.raster.point(item.mapping.canvas_point([f64::from(origin[0] + 32), f64::from(origin[1] + 16)]).unwrap());
    rectangle(&pixels, viewport.source_size(), low, high, [255, 0, 255], false);
    captured_rectangle(fixture, rotation, low, high, [255, 0, 255], false);
    method.method.commit_string("rotated application".into()); method.method.commit(method.serial());
    method.until(fixture, |fixture, _| fixture.client.text_input.commits.last() == Some(&(input.id(), Some("rotated application".into()))));
    let handle = fixture.client.foreign_handles.iter().find(|handle| fixture.client.foreign_names.get(&handle.id()).is_some_and(|title| title == B_NAME)).unwrap();
    let direct_source = fixture.client.capture.toplevels.as_ref().unwrap().create_source(handle, &qh, ());
    let direct_session = fixture.client.capture.manager.as_ref().unwrap().create_session(&direct_source, Options::empty(), &qh, ());
    fixture.exchange();
    let direct = capture_lifetime::Pending::new(fixture, &direct_session, size);
    capture_lifetime::submitted(fixture, &direct);
    exclude(fixture, id, true);
    capture_lifetime::rejected(fixture, &direct);
    direct.destroy(fixture); direct_session.destroy(); direct_source.destroy(); fixture.exchange();
    let (source, session) = capture_lifetime::output_session(fixture);
    let filtered = capture_lifetime::Pending::new(fixture, &session, B_SIZE);
    let pixels = capture_lifetime::completed(fixture, &filtered);
    assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 235 && pixel[1] < 20 && pixel[2] > 235).count(), 0,
        "A rotated private application's IME popup contributed capture pixels");
    let (offset, color) = clients::MARKERS[0];
    let point = [(60.0 + 90.0 + f64::from(offset[0])) * SCALE, (40.0 + 50.0 + f64::from(offset[1])) * SCALE];
    pixel(&pixels, B_SIZE, physical(rotation, point), true, color);
    filtered.destroy(fixture); session.destroy(); source.destroy(); fixture.exchange();
    exclude(fixture, id, false);
    popup.destroy(fixture, &mut method);
    markers.role.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive); markers.surface.commit();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&markers.surface));
    text_input_tests::enable(&input, [80, 30, 2, 18]); fixture.exchange();
    let expected = Caret::Layer { output: B, rectangle: Rectangle::new([840.0, 150.0], [2.0, 18.0]).unwrap() };
    fixture.until(|fixture| fixture.desktop.server.magnification_caret() == Some(expected));
    let popup = method.popup(fixture, [32, 16], [255, 0, 255, 255]);
    method.until(fixture, |fixture, method| method.active() && fixture.desktop.layers.iter().any(|layer|
        layer.output == B && layer.namespace == "rotation-markers" && layer.popups.len() == 1));
    let overlay = fixture.desktop.layers.iter().find(|layer| layer.namespace == "rotation-markers").unwrap();
    assert_eq!(overlay.origin, [760, 120]); assert_eq!(overlay.popups[0].origin, [80, 48]);
    let low = [(60.0 + 80.0) * SCALE, (40.0 + 48.0) * SCALE];
    let high = [(60.0 + 112.0) * SCALE, (40.0 + 64.0) * SCALE];
    let (viewport, pixels) = fixture.output_pixels(B);
    rectangle(&pixels, viewport.source_size(), low, high, [255, 0, 255], false);
    captured_rectangle(fixture, rotation, low, high, [255, 0, 255], false);
    popup.destroy(fixture, &mut method);
    let local = [90.0, 50.0];
    let position = [(60.0 + local[0]) * SCALE, (40.0 + local[1]) * SCALE];
    clients::pointer_click(fixture, B, position, &markers.surface, local);
    let pointer = fixture.client.seat.as_ref().unwrap().get_pointer(&qh, ()); fixture.exchange();
    let cursor = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let buffer = privacy::buffer(fixture, [20, 12], [0, 96, 255, 255]);
    pointer.set_cursor(fixture.client.pointer_enter_serial, Some(&cursor), 5, 3);
    cursor.attach(Some(&buffer), 0, 0); cursor.damage_buffer(0, 0, 20, 12); cursor.commit(); fixture.exchange();
    fixture.inspect(|_| ());
    let low = [position[0] - 5.0 * SCALE, position[1] - 3.0 * SCALE];
    let high = [low[0] + 20.0 * SCALE, low[1] + 12.0 * SCALE];
    let (viewport, pixels) = fixture.output_pixels(B);
    rectangle(&pixels, viewport.source_size(), low, high, [255, 96, 0], false);
    captured_rectangle(fixture, rotation, low, high, [255, 96, 0], true);
    let (plain_source, plain_session) = capture_lifetime::output_session(fixture);
    let plain = capture_lifetime::Pending::new(fixture, &plain_session, B_SIZE);
    let pixels = capture_lifetime::completed(fixture, &plain);
    assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] < 20 && (76..=116).contains(&pixel[1]) && pixel[2] > 235).count(), 0,
        "A cursor-free rotated output capture painted software cursor pixels");
    plain.destroy(fixture); plain_session.destroy(); plain_source.destroy(); fixture.exchange();
    let output = fixture.client.output.as_ref().unwrap().clone();
    let source = fixture.client.capture.outputs.as_ref().unwrap().create_source(&output, &qh, ());
    let standalone = fixture.client.capture.manager.as_ref().unwrap().create_pointer_cursor_session(&source, &pointer, &qh, ());
    let session = standalone.get_capture_session(&qh, ()); fixture.exchange();
    fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap(); fixture.exchange();
    let size = *fixture.client.capture.sizes.last().unwrap();
    assert_eq!(size, [25, 15]);
    let pending = capture_lifetime::Pending::new(fixture, &session, size);
    let pixels = capture_lifetime::completed(fixture, &pending);
    assert!(pixels.chunks_exact(4).all(|pixel| pixel[0] < 20 && (76..=116).contains(&pixel[1]) && pixel[2] > 235 && pixel[3] == 255));
    assert!(fixture.client.capture.cursor_events.iter().any(|event| event == "hotspot:6,4"));
    pending.destroy(fixture); session.destroy(); standalone.destroy(); source.destroy();
    pointer.set_cursor(fixture.client.pointer_enter_serial, None, 0, 0); cursor.destroy(); buffer.destroy(); pointer.release();
    input.disable(); input.commit(); input.destroy();
    method.destroy(fixture);
    markers.role.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None); markers.surface.commit();
    fixture.exchange();
    assert!(fixture.desktop.buttons.is_empty());
}
