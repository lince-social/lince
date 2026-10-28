use super::*;
use super::super::{capture_lifetime, ime, opening};
use crate::launch::Capability;
use lince_interface::host::magnification::{Adjustment, Caret};
use wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_manager_v1::Options;

const OUTPUT_B: &str = "Caret-B";
const B_SIZE: [u32; 2] = [800, 600];

fn factor(fixture: &mut Fixture, adjustment: Adjustment, expected: f64) {
    let previous = fixture.desktop.magnification_snapshot().last_request;
    fixture.desktop.send(Input::Action(DesktopAction::Magnify(adjustment))).unwrap();
    fixture.until(|fixture| fixture.desktop.magnification_snapshot().last_request > previous
        && fixture.desktop.magnification_snapshot().head(OutputId(2)).is_some_and(|head| head.requested == expected && head.effective == Some(expected)));
}

fn pixel_from_client(points: [[f64; 2]; 3], relative: [f64; 2]) -> [f64; 2] {
    std::array::from_fn(|axis| {
        let center = (154.0 * points[0][axis] + 156.0 * points[1][axis] + 123.0 * points[2][axis]) / 433.0;
        let first = points[1][axis] - points[0][axis]; let second = points[2][axis] - points[0][axis];
        center + relative[0] * (8.0 * first - 27.0 * second) / -866.0 + relative[1] * (20.0 * second - 38.0 * first) / -866.0
    })
}

fn color_bounds(pixels: &[u8], size: [u32; 2], bgra: bool, near: [f64; 2], extent: [f64; 2], cursor: bool) -> Option<([f64; 2], [f64; 2])> {
    let low: [usize; 2] = std::array::from_fn(|axis| (near[axis] - 4.0).max(0.0).floor() as usize);
    let high: [usize; 2] = std::array::from_fn(|axis| (near[axis] + extent[axis] + 4.0).min(f64::from(size[axis])).max(0.0).ceil() as usize);
    let mut first = [f64::INFINITY; 2]; let mut last = [f64::NEG_INFINITY; 2];
    for y in low[1]..high[1] {
        for x in low[0]..high[0] {
            let offset = (y * size[0] as usize + x) * 4;
            let rgb = if bgra { [pixels[offset + 2], pixels[offset + 1], pixels[offset]] } else { [pixels[offset], pixels[offset + 1], pixels[offset + 2]] };
            let matches = if cursor { rgb[0] > 225 && (60..=120).contains(&rgb[1]) && rgb[2] < 20 }
                else { rgb[0] > 225 && rgb[1] < 20 && rgb[2] > 225 };
            if matches {
                first[0] = first[0].min(x as f64); first[1] = first[1].min(y as f64);
                last[0] = last[0].max(x as f64 + 1.0); last[1] = last[1].max(y as f64 + 1.0);
            }
        }
    }
    first.iter().all(|value| value.is_finite()).then_some((first, last))
}

fn rectangle(pixels: &[u8], size: [u32; 2], bgra: bool, origin: [f64; 2], extent: [f64; 2], cursor: bool) {
    let (first, last) = color_bounds(pixels, size, bgra, origin, extent, cursor).expect("The expected actual popup/cursor pixels are absent");
    clients::compare(first, origin, "independent popup/cursor leading edges");
    clients::compare(last, std::array::from_fn(|axis| origin[axis] + extent[axis]), "independent popup/cursor trailing edges");
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_application_and_layer_carets_anchor_their_mixed_scale_head_without_a_popup_and_cursor_pixels_capture_and_owner_loss_follow_the_same_mapping() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("park.json"), serde_json::to_vec(&serde_json::json!({"outputs":{(OUTPUT):{"scale":1.25},(OUTPUT_B):{"scale":2.0,"magnification":1.0}}})).unwrap()).unwrap();
    let mut fixture = create_authorized(&runtime, directory.path(), [1000, 720], 1.25, vec![Capability::Capture, Capability::WindowList]);
    fixture.desktop.server.configure_scaled_outputs(&[
        Monitor { id: OutputId(1), name: OUTPUT.into(), origin: [120, 40], size: [1000, 720] },
        Monitor { id: OutputId(2), name: OUTPUT_B.into(), origin: [920, 80], size: B_SIZE.map(|value| value as i32) },
    ], &std::collections::BTreeMap::from([(OutputId(1), 1.25), (OutputId(2), 2.0)])).unwrap();
    fixture.desktop.server.registered_output_catalog().unwrap(); fixture.desktop.invalidate_outputs().unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2)));
    let serial = fixture.desktop.worker.input(Input::Output(OutputId(2))).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial && frame.frame.output == Some(OutputId(2))));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let ids = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
        for id in ids { native::workspace_config::set_physics(world, root, id, false); }
    });
    let (window, initial, surface) = opening::map(&mut fixture);
    window.role.set_max_size(240, 180); window.surface.commit(); fixture.exchange();
    fixture.desktop.send(Input::Application(surface)).unwrap(); settle(&mut fixture);
    let mut buffers = vec![initial];
    let size = clients::repaint(&mut fixture, &window, surface, &mut buffers);
    let item = clients::geometry(&fixture, surface); assert_eq!(item.output, OutputId(2));
    let (viewport, baseline) = fixture.output_pixels(OutputId(2));
    let points: [[f64; 2]; 3] = std::array::from_fn(|index| {
        let local = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(clients::MARKERS[index].0[axis]));
        clients::marker(&baseline, B_SIZE, viewport.raster.point(item.mapping.canvas_point(local).unwrap()), 1.0, clients::MARKERS[index].1, false)
    });
    let first_mapping = fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(OutputId(1)).unwrap().clone();
    let qh = fixture.queue.handle();
    let input = text_input_tests::input(&fixture.client, &qh); fixture.exchange();
    assert_eq!(fixture.client.text_input.entered.get(&input.id()), Some(&window.surface));
    let caret = [size[0] as i32 / 2, size[1] as i32 / 2, 2, 18];
    let source_anchor = [f64::from(caret[0]) + 1.0, f64::from(caret[1]) + 9.0];
    let nominal_anchor = pixel_from_client(points, std::array::from_fn(|axis| source_anchor[axis] - f64::from(size[axis] / 2)));
    let expected_caret = Caret::Application { surface, rectangle: Rectangle::new([f64::from(caret[0]), f64::from(caret[1])], [2.0, 18.0]).unwrap() };
    text_input_tests::enable(&input, caret); fixture.exchange();
    fixture.until(|fixture| fixture.desktop.server.magnification_caret() == Some(expected_caret));
    assert!(fixture.desktop.server.windows().iter().find(|window| window.id == surface).unwrap().popups.is_empty());
    let configured = fixture.client.configures.len();
    for step in 1..=4 { factor(&mut fixture, Adjustment::Increase, 1.0 + f64::from(step) * 0.25); }
    let (_, pixels) = fixture.output_pixels(OutputId(2));
    for index in 0..clients::MARKERS.len() {
        let expected = std::array::from_fn(|axis| f64::from(B_SIZE[axis]) * 0.5 + 2.0 * (points[index][axis] - nominal_anchor[axis]));
        clients::compare(clients::marker(&pixels, B_SIZE, expected, 2.0, clients::MARKERS[index].1, false), expected, "no-provider application caret lens");
    }
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(OutputId(1)), Some(&first_mapping));
    assert_eq!(clients::geometry(&fixture, surface), item);
    assert!(fixture.client.configures[configured..].iter().all(|configured| *configured == size.map(|value| value as i32)));
    let mut method = ime::Ime::connect(&mut fixture);
    let popup = method.popup(&mut fixture, [72, 32], [255, 0, 255, 255]);
    settle(&mut fixture);
    assert!(fixture.desktop.server.magnification_caret().is_none());
    assert!(fixture.desktop.server.windows().iter().find(|window| window.id == surface).unwrap().popups.is_empty(), "An inactive text input exposed a provider popup");
    text_input_tests::enable(&input, caret); fixture.exchange();
    method.until(&mut fixture, |fixture, method| method.active() && fixture.desktop.current.as_ref().is_some_and(|frame|
        frame.windows.iter().find(|window| window.id == surface).is_some_and(|window| window.popups.len() == 1)));
    let origin = fixture.desktop.server.windows().iter().find(|window| window.id == surface).unwrap().popups[0].origin;
    let nominal = pixel_from_client(points, [f64::from(origin[0]) - f64::from(size[0] / 2), f64::from(origin[1]) - f64::from(size[1] / 2)]);
    let popup_origin = std::array::from_fn(|axis| f64::from(B_SIZE[axis]) * 0.5 + 2.0 * (nominal[axis] - nominal_anchor[axis]));
    let (_, pixels) = fixture.output_pixels(OutputId(2));
    let right = pixel_from_client(points, [f64::from(origin[0] + 72) - f64::from(size[0] / 2), f64::from(origin[1] + 32) - f64::from(size[1] / 2)]);
    let popup_size = std::array::from_fn(|axis| 2.0 * (right[axis] - nominal[axis]));
    rectangle(&pixels, B_SIZE, false, popup_origin, popup_size, false);
    popup.destroy(&mut fixture, &mut method);
    method.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame|
        frame.windows.iter().find(|window| window.id == surface).is_some_and(|window| window.popups.is_empty())));
    let position = [300.0, 200.0];
    fixture.desktop.move_pointer(viewport.raster.logical_point(position), 50).unwrap(); fixture.tick();
    fixture.until(|fixture| fixture.client.pointer_focus.as_ref() == Some(&window.surface));
    let pointer = fixture.client.seat.as_ref().unwrap().get_pointer(&qh, ()); fixture.exchange();
    let cursor = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let cursor_buffer = super::super::privacy::buffer(&fixture, [20, 12], [0, 96, 255, 255]);
    pointer.set_cursor(fixture.client.pointer_enter_serial, Some(&cursor), 5, 3);
    cursor.attach(Some(&cursor_buffer), 0, 0); cursor.damage_buffer(0, 0, 20, 12); cursor.commit();
    fixture.exchange(); settle(&mut fixture);
    let cursor_origin = [position[0] - 20.0, position[1] - 12.0];
    let cursor_size = [80.0, 48.0];
    let (_, pixels) = fixture.output_pixels(OutputId(2)); rectangle(&pixels, B_SIZE, false, cursor_origin, cursor_size, true);
    let output = fixture.client.output.as_ref().unwrap().clone();
    let (paint_source, paint_session) = capture_lifetime::output_session_options(&mut fixture, &output, Options::PaintCursors);
    let painted = capture_lifetime::Pending::new(&mut fixture, &paint_session, B_SIZE);
    let captured = capture_lifetime::completed(&mut fixture, &painted);
    rectangle(&captured, B_SIZE, true, cursor_origin, cursor_size, true); painted.destroy(&mut fixture);
    let (plain_source, plain_session) = capture_lifetime::output_session_for(&mut fixture, &output);
    let plain = capture_lifetime::Pending::new(&mut fixture, &plain_session, B_SIZE);
    let captured = capture_lifetime::completed(&mut fixture, &plain);
    assert!(color_bounds(&captured, B_SIZE, true, cursor_origin, cursor_size, true).is_none());
    for index in 0..clients::MARKERS.len() {
        let expected = std::array::from_fn(|axis| f64::from(B_SIZE[axis]) * 0.5 + 2.0 * (points[index][axis] - nominal_anchor[axis]));
        clients::compare(clients::marker(&captured, B_SIZE, expected, 2.0, clients::MARKERS[index].1, true), expected, "cursor-free output retains actual public content");
    }
    plain.destroy(&mut fixture);
    let window_source = fixture.client.capture.toplevels.as_ref().unwrap().create_source(&fixture.client.foreign_handles[0], &qh, ());
    let window_session = fixture.client.capture.manager.as_ref().unwrap().create_session(&window_source, Options::empty(), &qh, ()); fixture.exchange();
    assert_eq!(fixture.client.capture.sizes.last(), Some(&size));
    let direct = capture_lifetime::Pending::new(&mut fixture, &window_session, size);
    let captured = capture_lifetime::completed(&mut fixture, &direct);
    for (offset, color) in clients::MARKERS {
        let expected = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(offset[axis]));
        clients::compare(clients::marker(&captured, size, expected, 1.0, color, true), expected, "unmagnified direct-window capture");
    }
    direct.destroy(&mut fixture); window_session.destroy(); window_source.destroy();
    paint_session.destroy(); paint_source.destroy(); plain_session.destroy(); plain_source.destroy();
    pointer.set_cursor(fixture.client.pointer_enter_serial, None, 0, 0); cursor.destroy(); cursor_buffer.destroy(); pointer.release(); fixture.exchange();
    factor(&mut fixture, Adjustment::Reset, 1.0);
    let layer_surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let layer = fixture.client.layer_shell.as_ref().unwrap().get_layer_surface(&layer_surface, Some(&output), zwlr_layer_shell_v1::Layer::Overlay, "park-caret-fixture".into(), &qh, ());
    layer.set_size(180, 100); layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
    layer.set_margin(60, 0, 0, 100); layer.set_exclusive_zone(0); layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive);
    layer_surface.commit(); fixture.exchange();
    let layer_buffer = clients::pattern(&fixture, [180, 100]); layer_surface.attach(Some(&layer_buffer), 0, 0); layer_surface.damage_buffer(0, 0, 180, 100); layer_surface.commit();
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&layer_surface));
    assert_eq!(fixture.client.text_input.entered.get(&input.id()), Some(&layer_surface));
    text_input_tests::enable(&input, [80, 30, 2, 18]); fixture.exchange();
    let expected = Caret::Layer { output: OutputId(2), rectangle: Rectangle::new([1100.0, 170.0], [2.0, 18.0]).unwrap() };
    fixture.until(|fixture| fixture.desktop.server.magnification_caret() == Some(expected));
    for step in 1..=4 { factor(&mut fixture, Adjustment::Increase, 1.0 + f64::from(step) * 0.25); }
    let (_, pixels) = fixture.output_pixels(OutputId(2));
    for (offset, color) in clients::MARKERS {
        let expected = [400.0 + 4.0 * (90.0 + f64::from(offset[0]) - 81.0), 300.0 + 4.0 * (50.0 + f64::from(offset[1]) - 39.0)];
        clients::compare(clients::marker(&pixels, B_SIZE, expected, 2.0, color, false), expected, "layer caret lens from declared margins and source markers");
    }
    let popup = method.popup(&mut fixture, [72, 32], [255, 0, 255, 255]);
    method.until(&mut fixture, |fixture, method| method.active() && fixture.desktop.layers.iter().any(|layer| layer.output == OutputId(2) && layer.popups.len() == 1));
    let overlay = fixture.desktop.layers.iter().find(|layer| layer.output == OutputId(2) && !layer.popups.is_empty()).unwrap();
    assert_eq!(overlay.origin, [1020, 140]); assert_eq!(overlay.popups[0].origin, [80, 48]);
    let (_, pixels) = fixture.output_pixels(OutputId(2)); rectangle(&pixels, B_SIZE, false, [396.0, 336.0], [288.0, 128.0], false);
    fixture.client.key_recipients.clear(); press(&mut fixture, 30);
    fixture.until(|fixture| fixture.client.key_recipients.len() == 2);
    assert_eq!(fixture.client.key_recipients, [(Some(layer_surface.id()), 30, true), (Some(layer_surface.id()), 30, false)]);
    method.method.commit_string("layer".into()); method.method.commit(method.serial());
    method.until(&mut fixture, |fixture, _| fixture.client.text_input.commits.last() == Some(&(input.id(), Some("layer".into()))));
    assert_eq!(fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(OutputId(1)), Some(&first_mapping));
    layer.destroy(); layer_surface.destroy(); layer_buffer.destroy(); fixture.exchange();
    fixture.until(|fixture| fixture.desktop.layers.is_empty() && fixture.desktop.server.magnification_caret().is_none());
    popup.destroy(&mut fixture, &mut method);
    window.surface.attach(None, 0, 0); window.surface.commit(); fixture.exchange();
    let (fresh, _fresh_buffer, fresh_id) = opening::map(&mut fixture);
    fixture.desktop.send(Input::Application(fresh_id)).unwrap(); settle(&mut fixture);
    fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&fresh.surface));
    text_input_tests::enable(&input, [30, 40, 2, 18]); fixture.exchange();
    fixture.until(|fixture| matches!(fixture.desktop.server.magnification_caret(), Some(Caret::Application { surface, rectangle })
        if surface == fresh_id && rectangle.origin() == [30.0, 40.0]));
    fixture.client.key_recipients.clear(); press(&mut fixture, 30);
    fixture.until(|fixture| fixture.client.key_recipients.len() == 2);
    assert_eq!(fixture.client.key_recipients, [(Some(fresh.surface.id()), 30, true), (Some(fresh.surface.id()), 30, false)]);
    assert!(fixture.desktop.running && fixture.desktop.keys.is_empty() && fixture.desktop.buttons.is_empty());
    input.destroy(); method.destroy(&mut fixture); fixture.desktop.worker.stop().unwrap(); drop(buffers);
}
