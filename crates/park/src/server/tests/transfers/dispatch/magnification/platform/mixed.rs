use super::*;
use super::super::clients;
use super::super::super::{ime, opening};
use lince_interface::host::magnification::Adjustment;

const TEST: &str = "server::tests::transfers_tests::dispatch::magnification::platform::mixed::mixed_scale_nonzero_origin_heads_keep_independent_text_caret_ime_and_real_client_lens_input";
const FIRST: &str = "First mixed-output Unicode field";
const SECOND: &str = "Second mixed-output Unicode field";

fn mapping(fixture: &Fixture, output: OutputId) -> lince_interface::host::mapping::Mapping {
    fixture.desktop.current.as_ref().unwrap().frame.input.mappings.get(output).unwrap().clone()
}

fn magnify(fixture: &mut Fixture, output: OutputId, change: Adjustment, expected: f64) {
    let previous = fixture.desktop.magnification_snapshot().last_request;
    fixture.desktop.send(Input::Action(DesktopAction::Magnify(change))).unwrap();
    fixture.until(|fixture| {
        let snapshot = fixture.desktop.magnification_snapshot();
        snapshot.last_request > previous && snapshot.head(output).is_some_and(|head| head.requested == expected && head.effective == Some(expected))
    });
    assert!(matches!(fixture.desktop.magnification_snapshot().response.as_ref().unwrap().outcome, Outcome::Saved { .. }));
}

fn extents(provider: &Provider, object: &bus::Object) -> bus::Result<(i32, i32, i32, i32)> {
    Ok(bus::proxy(&provider.bus, object, "org.a11y.atspi.Text")?.call("GetCharacterExtents", &(0_i32, atspi_common::CoordType::Screen))?)
}

fn child(root: std::path::PathBuf) -> bus::Result<()> {
    exercise(root, lince_interface::host::mapping::QuarterTurn::Normal)
}

fn exercise(root: std::path::PathBuf, rotation: lince_interface::host::mapping::QuarterTurn) -> bus::Result<()> {
    let provider = Provider::new(&root)?;
    provider.status.set_property("ScreenReaderEnabled", true)?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build()?;
    let _entered = runtime.enter();
    let path = root.join("mixed-profile"); std::fs::create_dir(&path)?;
    let mut fixture = create_authorized(&runtime, &path, [1000, 720], 1.25,
        vec![crate::launch::Capability::Capture, crate::launch::Capability::WindowList]);
    let monitors = [Monitor { id: OutputId(1), name: OUTPUT.into(), origin: [120, 40], size: [1000, 720] },
        Monitor { id: OutputId(2), name: "Mixed-companion".into(), origin: [920, 80], size: [800, 600] }];
    fixture.desktop.server.configure_scaled_outputs(&monitors, &std::collections::BTreeMap::from([(OutputId(1), 1.25), (OutputId(2), 2.0)])).unwrap();
    fixture.desktop.server.registered_output_catalog().unwrap();
    if rotation != lince_interface::host::mapping::QuarterTurn::Normal {
        let snapshot = fixture.desktop.server.display_snapshot();
        let mut heads = snapshot.configuration();
        heads.get_mut(&OutputId(2)).unwrap().as_mut().unwrap().rotation = rotation;
        assert!(fixture.desktop.server.install_virtual_outputs(snapshot.heads, &heads, [1800, 1000], false).unwrap());
    }
    fixture.desktop.invalidate_outputs().unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.outputs.len() == 2 && geometry.outputs.iter().any(|output| output.id == OutputId(2) && output.rotation == rotation))));
    let workspaces: Vec<_> = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs.iter().map(|output| (output.id, output.workspace)).collect();
    let ordinary = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let mut fields = Vec::new();
        for (output, workspace) in workspaces {
            native::workspace_config::set_physics(world, root, workspace, false);
            let field = field(world, root, workspace, if output == OutputId(1) { FIRST } else { SECOND }, BODY, false);
            fields.push((output, field));
        }
        fields
    });
    fixture.desktop.send(Input::Action(DesktopAction::SetLayout(WorkspaceLayout::Tiled))).unwrap();
    settle(&mut fixture);
    let first = observe(&mut fixture, |_| Ok(bus::named_role(&provider.bus, FIRST, Some(atspi_common::Role::Entry))?))?;
    let reader_a = bus::proxy(&provider.bus, &first, "org.a11y.atspi.Text")?;
    assert_eq!(reader_a.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, BODY);
    assert!(reader_a.call::<_, _, bool>("SetCaretOffset", &(2_i32,))?);
    observe(&mut fixture, |_| Ok((reader_a.get_property::<i32>("CaretOffset")? == 2).then_some(())))?;
    text_pixels(&mut fixture, &provider, &first, OutputId(1))?;
    let first_bounds = extents(&provider, &first)?;
    let first_mapping = mapping(&fixture, OutputId(1));
    let serial = fixture.desktop.worker.input(Input::Output(OutputId(2))).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= serial && frame.frame.output == Some(OutputId(2))));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        world.get_mut::<native::topology::view::View>(root).unwrap().spatial = true;
    });
    let second = observe(&mut fixture, |_| Ok(bus::named_role(&provider.bus, SECOND, Some(atspi_common::Role::Entry))?))?;
    let reader_b = bus::proxy(&provider.bus, &second, "org.a11y.atspi.Text")?;
    assert_eq!(reader_b.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, BODY);
    assert!(reader_b.call::<_, _, bool>("SetCaretOffset", &(2_i32,))?);
    observe(&mut fixture, |_| Ok((reader_b.get_property::<i32>("CaretOffset")? == 2).then_some(())))?;
    text_pixels(&mut fixture, &provider, &second, OutputId(2))?;
    rotated_pixels(&mut fixture, rotation)?;
    let second_field = ordinary.iter().find(|(output, _)| *output == OutputId(2)).unwrap().1;
    let mut factor = 1.0;
    for target in [2.0, 4.0] {
        while factor < target { factor += 0.25; magnify(&mut fixture, OutputId(2), Adjustment::Increase, factor); }
        text_pixels(&mut fixture, &provider, &second, OutputId(2))?;
        rotated_pixels(&mut fixture, rotation)?;
        assert_eq!(extents(&provider, &first)?, first_bounds, "Changing the companion lens moved the first output's text bounds");
        assert_eq!(mapping(&fixture, OutputId(1)), first_mapping, "A companion mapping replaced the first output's valid revision");
        assert_eq!(reader_a.call::<_, _, String>("GetText", &(0_i32, -1_i32))?, BODY);
    }
    assert!(reader_b.call::<_, _, bool>("AddSelection", &(0_i32, 3_i32))?);
    observe(&mut fixture, |_| Ok((reader_b.call::<_, _, (i32, i32)>("GetSelection", &(0_i32,))? == (0, 3)).then_some(())))?;
    assert!(reader_b.call::<_, _, bool>("SetCaretOffset", &(2_i32,))?);
    observe(&mut fixture, |_| Ok((reader_b.get_property::<i32>("CaretOffset")? == 2).then_some(())))?;
    let mut method = ime::Ime::connect(&mut fixture);
    method.until(&mut fixture, |fixture, method| method.active() && fixture.desktop.current.as_ref().is_some_and(|frame|
        frame.frame.input.editor.as_ref().is_some_and(|editor| editor.owner == second_field.to_bits())));
    let popup = method.popup(&mut fixture, [32, 16], [255, 0, 255, 255]);
    method.until(&mut fixture, |fixture, _| fixture.desktop.popups.iter().any(|popup|
        popup.output == OutputId(2) && popup.owner == crate::desktop::renderer::PopupOwner::Native));
    rotated_pixels(&mut fixture, rotation)?;
    popup.destroy(&mut fixture, &mut method);
    method.method.set_preedit_string("界".into(), 0, 3); method.method.commit(method.serial());
    method.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.text_composing));
    while factor > 2.0 { factor -= 0.25; magnify(&mut fixture, OutputId(2), Adjustment::Decrease, factor); }
    method.exchange(&mut fixture);
    assert!(method.active());
    assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<EditableText>(second_field).unwrap().is_composing()),
        "A geometry-only lens change discarded valid native IME composition");
    method.method.commit_string("界".into()); method.method.commit(method.serial());
    let committed = "aé界界 e\u{301} z";
    method.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame|
        frame.frame.input.editor.as_ref().is_some_and(|editor| editor.surrounding == committed)));
    method.method.set_preedit_string("pending".into(), 0, 7); method.method.commit(method.serial());
    method.until(&mut fixture, |fixture, _| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.input.text_composing));
    let old_serial = method.serial();
    magnify(&mut fixture, OutputId(2), Adjustment::Reset, 1.0);
    let (window, initial, surface) = opening::map(&mut fixture);
    window.role.set_max_size(240, 180); window.surface.commit(); fixture.exchange();
    fixture.desktop.send(Input::Application(surface)).unwrap();
    method.until(&mut fixture, |_, method| !method.active());
    method.method.commit_string("late".into()); method.method.commit(old_serial); method.exchange(&mut fixture);
    assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<EditableText>(second_field).unwrap().value().to_string()), committed);
    method.destroy(&mut fixture);
    let mut buffers = vec![initial];
    let size = clients::repaint(&mut fixture, &window, surface, &mut buffers);
    let original = clients::geometry(&fixture, surface);
    assert_eq!(original.output, OutputId(2));
    let (viewport, pixels) = fixture.output_pixels(OutputId(2));
    let physical_size = viewport.source_size();
    assert_eq!(viewport.raster.scale, [2.0; 2]);
    assert_eq!(viewport.bounds.origin(), [920.0, 80.0]);
    let anchor_source = original.mapping.canvas_point(size.map(|value| f64::from(value / 2))).unwrap();
    let anchor = viewport.raster.point(anchor_source);
    assert!((0..2).all(|axis| anchor[axis] > f64::from(physical_size[axis]) * 0.25 && anchor[axis] < f64::from(physical_size[axis]) * 0.75));
    let baseline: [[f64; 2]; 3] = std::array::from_fn(|index| {
        let local = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(clients::MARKERS[index].0[axis]));
        clients::marker(&pixels, physical_size, viewport.raster.point(original.mapping.canvas_point(local).unwrap()), 1.0, clients::MARKERS[index].1, false)
    });
    fixture.desktop.move_pointer(anchor_source, 70).unwrap(); fixture.tick(); settle(&mut fixture);
    let configures = fixture.client.configures.len();
    factor = 1.0;
    while factor < 2.0 { factor += 0.25; magnify(&mut fixture, OutputId(2), Adjustment::Increase, factor); }
    let (_, pixels) = fixture.output_pixels(OutputId(2));
    for index in 0..clients::MARKERS.len() {
        let expected = std::array::from_fn(|axis| f64::from(physical_size[axis]) * 0.5 + 2.0 * (baseline[index][axis] - anchor[axis]));
        let actual = clients::marker(&pixels, physical_size, expected, 2.0, clients::MARKERS[index].1, false);
        clients::compare(actual, expected, "mixed-head client lens");
        let local = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(clients::MARKERS[index].0[axis]));
        clients::pointer_click(&mut fixture, OutputId(2), actual, &window.surface, local);
    }
    fixture.client.key_recipients.clear(); press(&mut fixture, 30);
    fixture.until(|fixture| fixture.client.key_recipients.len() == 2);
    assert_eq!(fixture.client.key_recipients, [(Some(window.surface.id()), 30, true), (Some(window.surface.id()), 30, false)]);
    assert_eq!(clients::geometry(&fixture, surface), original);
    assert!(fixture.client.configures[configures..].iter().all(|configured| *configured == size.map(|value| value as i32)));
    assert_eq!(mapping(&fixture, OutputId(1)), first_mapping);
    assert_eq!(extents(&provider, &first)?, first_bounds);
    assert!(fixture.desktop.running && fixture.desktop.keys.is_empty() && fixture.desktop.buttons.is_empty());
    fixture.desktop.worker.stop()?;
    drop(buffers);
    Ok(())
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and the private D-Bus and AT-SPI providers"]
fn mixed_scale_nonzero_origin_heads_keep_independent_text_caret_ime_and_real_client_lens_input() -> bus::Result<()> {
    bus::run(TEST, child)
}

fn rotated_pixels(fixture: &mut Fixture, rotation: lince_interface::host::mapping::QuarterTurn) -> bus::Result<()> {
    use lince_interface::host::mapping::QuarterTurn;
    use super::super::super::capture_lifetime;
    let (viewport, pixels) = fixture.output_pixels(OutputId(2));
    assert_eq!(viewport.rotation, rotation);
    let size = viewport.source_size();
    assert_eq!(size, match rotation { QuarterTurn::Normal | QuarterTurn::UpsideDown => [800, 600], _ => [600, 800] });
    let (source, session) = capture_lifetime::output_session(fixture);
    let pending = capture_lifetime::Pending::new(fixture, &session, [800, 600]);
    let captured = capture_lifetime::completed(fixture, &pending);
    let mut ink = 0;
    for y in 0..size[1] {
        for x in 0..size[0] {
            let index = ((y * size[0] + x) * 4) as usize;
            let rgb = &pixels[index..index + 3];
            if rgb[0] >= 80 || rgb[1] <= 180 || rgb[2] >= 80 { continue; }
            let [px, py] = match rotation {
                QuarterTurn::Normal => [x, y], QuarterTurn::Clockwise90 => [799 - y, x],
                QuarterTurn::UpsideDown => [799 - x, 599 - y], QuarterTurn::Clockwise270 => [y, 599 - x],
            };
            let index = ((py * 800 + px) * 4) as usize;
            let actual = [captured[index + 2], captured[index + 1], captured[index]];
            assert!((0..3).all(|axis| actual[axis].abs_diff(rgb[axis]) <= 1), "Physical capture disagrees with native Unicode ink under {rotation:?}");
            ink += 1;
        }
    }
    assert!(ink > 2, "No independently rendered native Unicode ink was compared");
    if let Some(popup) = fixture.desktop.popups.iter().find(|popup| popup.output == OutputId(2)
        && popup.owner == crate::desktop::renderer::PopupOwner::Native)
    {
        let bounds = popup.mapping.canvas;
        let point = viewport.raster.point(std::array::from_fn(|axis| bounds.origin()[axis] + bounds.size()[axis] * 0.5));
        let [x, y] = point.map(|value| value.floor() as u32);
        assert!(x < size[0] && y < size[1]);
        let [x, y] = match rotation {
            QuarterTurn::Normal => [x, y], QuarterTurn::Clockwise90 => [799 - y, x],
            QuarterTurn::UpsideDown => [799 - x, 599 - y], QuarterTurn::Clockwise270 => [y, 599 - x],
        };
        let index = ((y * 800 + x) * 4) as usize;
        assert!(captured[index] > 235 && captured[index + 1] < 20 && captured[index + 2] > 235,
            "The native IME popup was not captured at its rotated caret anchor");
    }
    pending.destroy(fixture); session.destroy(); source.destroy(); fixture.exchange();
    Ok(())
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and the private D-Bus and AT-SPI providers"]
fn clockwise_native_unicode_carets_ime_and_external_accessibility_bounds_match_rotated_capture_and_companion_input() -> bus::Result<()> {
    bus::run("server::tests::transfers_tests::dispatch::magnification::platform::mixed::clockwise_native_unicode_carets_ime_and_external_accessibility_bounds_match_rotated_capture_and_companion_input",
        |root| exercise(root, lince_interface::host::mapping::QuarterTurn::Clockwise90))
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and the private D-Bus and AT-SPI providers"]
fn inverted_native_unicode_carets_ime_and_external_accessibility_bounds_match_rotated_capture_and_companion_input() -> bus::Result<()> {
    bus::run("server::tests::transfers_tests::dispatch::magnification::platform::mixed::inverted_native_unicode_carets_ime_and_external_accessibility_bounds_match_rotated_capture_and_companion_input",
        |root| exercise(root, lince_interface::host::mapping::QuarterTurn::UpsideDown))
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and the private D-Bus and AT-SPI providers"]
fn counterclockwise_native_unicode_carets_ime_and_external_accessibility_bounds_match_rotated_capture_and_companion_input() -> bus::Result<()> {
    bus::run("server::tests::transfers_tests::dispatch::magnification::platform::mixed::counterclockwise_native_unicode_carets_ime_and_external_accessibility_bounds_match_rotated_capture_and_companion_input",
        |root| exercise(root, lince_interface::host::mapping::QuarterTurn::Clockwise270))
}
