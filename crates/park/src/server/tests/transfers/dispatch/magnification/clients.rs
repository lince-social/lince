use super::*;
use crate::launch::Capability;

const SIZE: [u32; 2] = [1000, 720];
const INCREASE: u32 = 50;
const RESET: u32 = 49;
pub(in super::super) const MARKERS: [([i32; 2], [u8; 3]); 3] = [
    ([-18, -12], [255, 0, 0]),
    ([2, 15], [0, 255, 255]),
    ([20, -4], [255, 255, 0]),
];

pub(in super::super) fn pattern(fixture: &Fixture, size: [u32; 2]) -> wl_buffer::WlBuffer {
    let mut pixels = [8, 8, 8, 255].repeat(size[0] as usize * size[1] as usize);
    for (offset, color) in MARKERS {
        let center: [i32; 2] = std::array::from_fn(|axis| size[axis] as i32 / 2 + offset[axis]);
        for y in center[1] - 4..center[1] + 4 {
            for x in center[0] - 3..center[0] + 3 {
                assert!(x >= 0 && y >= 0 && x < size[0] as i32 && y < size[1] as i32);
                let pixel = ((y as u32 * size[0] + x as u32) * 4) as usize;
                pixels[pixel..pixel + 4].copy_from_slice(&[color[2], color[1], color[0], 255]);
            }
        }
    }
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&pixels).unwrap();
    let qh = fixture.queue.handle();
    let pool = fixture.client.shm.as_ref().unwrap().create_pool(file.as_fd(), pixels.len() as i32, &qh, ());
    let buffer = pool.create_buffer(0, size[0] as i32, size[1] as i32, (size[0] * 4) as i32, wl_shm::Format::Argb8888, &qh, ());
    pool.destroy();
    buffer
}

pub(in super::super) fn geometry(fixture: &Fixture, surface: SurfaceId) -> lince_interface::host::ItemGeometry {
    fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter()
        .find(|item| item.id == ItemId::Application(surface)).unwrap_or_else(|| panic!("Missing geometry for {surface:?}: output={:?}, items={:?}, windows={:?}",
            fixture.desktop.current.as_ref().unwrap().frame.output,
            fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().map(|item| (item.id, item.output)).collect::<Vec<_>>(),
            fixture.desktop.current.as_ref().unwrap().windows.iter().map(|window| (window.id, window.mapped, window.title.clone())).collect::<Vec<_>>())).clone()
}

pub(in super::super) fn repaint(fixture: &mut Fixture, window: &ClientWindow, surface: SurfaceId, buffers: &mut Vec<wl_buffer::WlBuffer>) -> [u32; 2] {
    for _ in 0..16 {
        settle(fixture);
        let size = fixture.client.window_configurations[&window.role.id()].0;
        assert!(size.into_iter().all(|value| value > 0));
        let size = size.map(|value| value as u32);
        let previous = fixture.desktop.server.windows().into_iter().find(|window| window.id == surface).unwrap();
        let buffer = pattern(fixture, size);
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32);
        window.surface.commit();
        buffers.push(buffer);
        fixture.until(|fixture| fixture.desktop.server.windows().iter().find(|window| window.id == surface)
            .is_some_and(|window| window.nodes[0].content.version != previous.nodes[0].content.version
                && window.nodes[0].layout.size == size.map(f64::from)));
        let version = fixture.desktop.server.windows().into_iter().find(|window| window.id == surface).unwrap().nodes[0].content.version;
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().find(|window| window.id == surface)
            .is_some_and(|window| window.nodes[0].content.version == version)));
        if fixture.client.window_configurations[&window.role.id()].0 == size.map(|value| value as i32) { return size; }
    }
    panic!("The magnification client did not settle its actual configure/ack/buffer dimensions")
}

pub(in super::super) fn marker(pixels: &[u8], size: [u32; 2], near: [f64; 2], factor: f64, color: [u8; 3], bgra: bool) -> [f64; 2] {
    let radius = 12.0 * factor;
    let low: [usize; 2] = std::array::from_fn(|axis| (near[axis] - radius).max(0.0).floor() as usize);
    let high: [usize; 2] = std::array::from_fn(|axis| (near[axis] + radius).min(f64::from(size[axis])).ceil() as usize);
    let mut weight = 0.0;
    let mut center = [0.0; 2];
    let mut count = 0;
    for y in low[1]..high[1] {
        for x in low[0]..high[0] {
            let offset = (y * size[0] as usize + x) * 4;
            let rgb = if bgra { [pixels[offset + 2], pixels[offset + 1], pixels[offset]] } else { [pixels[offset], pixels[offset + 1], pixels[offset + 2]] };
            let wanted = (0..3).filter(|channel| color[*channel] == 255).map(|channel| rgb[channel]).min().unwrap();
            let other = (0..3).filter(|channel| color[*channel] == 0).map(|channel| rgb[channel]).max().unwrap();
            let mass = f64::from(wanted.saturating_sub(other).saturating_sub(32));
            if mass == 0.0 { continue; }
            center[0] += (x as f64 + 0.5) * mass;
            center[1] += (y as f64 + 0.5) * mass;
            weight += mass;
            count += 1;
        }
    }
    if (count < 4 || weight == 0.0) && let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let name = format!("missing-marker-{}-{}-{}", std::process::id(), near[0].round(), near[1].round());
        let path = directory.join(&name).with_extension("rgba");
        std::fs::write(&path, pixels).unwrap();
        std::fs::write(directory.join(&name).with_extension("json"), serde_json::to_vec(&serde_json::json!({ "size": size, "bgra": bgra, "near": near, "color": color })).unwrap()).unwrap();
        eprintln!("Missing-marker pixel evidence: {}", path.display());
    }
    assert!(count >= 4 && weight > 0.0, "The independently expected marker {color:?} is absent near {near:?}");
    center.map(|sum| sum / weight)
}

pub(in super::super) fn compare(observed: [f64; 2], expected: [f64; 2], label: &str) {
    for axis in 0..2 {
        assert!((observed[axis] - expected[axis]).abs() <= 1.0,
            "{label}: actual marker {observed:?} differs from independent pixel expectation {expected:?}");
    }
}

pub(in super::super) fn pointer_click(fixture: &mut Fixture, output: OutputId, physical: [f64; 2], surface: &wl_surface::WlSurface, local: [f64; 2]) {
    let raster = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs.iter().find(|candidate| candidate.id == output).unwrap().raster;
    fixture.client.pointer_positions.clear();
    fixture.desktop.move_pointer(raster.logical_point(physical), 40).unwrap();
    fixture.tick();
    let buttons = fixture.client.button_recipients.len();
    fixture.desktop.button(272, true, 41).unwrap();
    fixture.desktop.button(272, false, 42).unwrap();
    fixture.until(|fixture| fixture.client.button_recipients.len() >= buttons + 2);
    assert_eq!(&fixture.client.button_recipients[buttons..], &[(Some(surface.id()), 272, true), (Some(surface.id()), 272, false)]);
    let actual = *fixture.client.pointer_positions.last().expect("A real client click needs an actual pointer coordinate");
    for axis in 0..2 { assert!((actual[axis] - local[axis]).abs() <= 1.0, "Wrong client coordinate: {actual:?} != {local:?}"); }
}

fn click(fixture: &mut Fixture, physical: [f64; 2], surface: &wl_surface::WlSurface, local: [f64; 2]) {
    pointer_click(fixture, OutputId(1), physical, surface, local);
    fixture.client.keys.clear();
    press(fixture, 30);
    fixture.until(|fixture| fixture.client.keys.len() >= 2);
    assert_eq!(fixture.client.keyboard_focus, Some(surface.clone()));
    assert_eq!(fixture.client.keys, [(30, true), (30, false)]);
}

fn shortcut(fixture: &mut Fixture, code: u32, expected: f64) {
    let previous = fixture.desktop.magnification_snapshot().last_request;
    super::super::windowed_fullscreen::chord_key(fixture, code);
    fixture.until(|fixture| fixture.desktop.magnification_snapshot().last_request > previous && saved(fixture, expected));
    assert!(matches!(fixture.desktop.magnification_snapshot().response.as_ref().unwrap().outcome, Outcome::Saved { .. }));
}

fn popup(fixture: &mut Fixture, window: &ClientWindow, surface: SurfaceId, size: [u32; 2], anchor: [f64; 2]) {
    use wayland_protocols::xdg::shell::client::xdg_positioner::{Anchor, Gravity};
    let qh = fixture.queue.handle();
    let positioner = fixture.client.shell.as_ref().unwrap().create_positioner(&qh, ());
    positioner.set_size(80, 60);
    positioner.set_anchor_rect(size[0] as i32 - 1, size[1] as i32 / 2, 1, 1);
    positioner.set_anchor(Anchor::Right); positioner.set_gravity(Gravity::Right); positioner.set_offset(8, 0);
    let popup_surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let popup_xdg = fixture.client.shell.as_ref().unwrap().get_xdg_surface(&popup_surface, &qh, ());
    let popup = popup_xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    popup_surface.commit(); fixture.tick();
    let buffer = super::super::privacy::buffer(fixture, [80, 60], [255, 0, 255, 255]);
    popup_surface.attach(Some(&buffer), 0, 0); popup_surface.damage_buffer(0, 0, 80, 60); popup_surface.commit();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter()
        .find(|window| window.id == surface).is_some_and(|window| window.popups.len() == 1)));
    let origin = fixture.desktop.current.as_ref().unwrap().windows.iter().find(|window| window.id == surface).unwrap().popups[0].origin;
    let local = [5.0, 20.0];
    let parent = geometry(fixture, surface);
    let source = parent.mapping.canvas_point(std::array::from_fn(|axis| f64::from(origin[axis]) + local[axis])).unwrap();
    assert!(parent.surface_point(source).is_none(), "The magnified popup must exercise pixels outside its parent");
    let raster = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs[0].raster;
    let baseline = raster.point(source);
    let physical: [f64; 2] = std::array::from_fn(|axis| f64::from(SIZE[axis]) * 0.5 + 2.0 * (baseline[axis] - anchor[axis]));
    assert!((0..2).all(|axis| physical[axis] >= 1.0 && physical[axis] < f64::from(SIZE[axis] - 1)));
    let image = super::super::appearance::pixels(fixture, "magnification-popup-outside-parent");
    let offset = (physical[1] as usize * SIZE[0] as usize + physical[0] as usize) * 4;
    assert!(image[offset] > 230 && image[offset + 1] < 20 && image[offset + 2] > 230,
        "The native popup outside its parent is missing at the independently expected magnified pixel");
    pointer_click(fixture, OutputId(1), physical, &popup_surface, local);
    popup.destroy(); popup_xdg.destroy(); popup_surface.destroy(); buffer.destroy(); positioner.destroy();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter()
        .find(|window| window.id == surface).is_some_and(|window| window.popups.is_empty())));
    assert!(fixture.desktop.buttons.is_empty());
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn actual_wayland_client_magnification_uses_independent_pixels_and_input_without_resizing_in_flat_spatial_free_and_tiled_views() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (spatial, layout, scale) in [(false, WorkspaceLayout::Free, 1.25), (true, WorkspaceLayout::Tiled, 2.0),
        (true, WorkspaceLayout::Free, 1.25), (false, WorkspaceLayout::Tiled, 2.0)]
    {
        let name = format!("{layout:?}-{spatial}-{scale}");
        let path = directory.path().join(&name); std::fs::create_dir(&path).unwrap();
        let mut fixture = create_authorized(&runtime, &path, SIZE, scale, vec![Capability::Capture]);
        super::super::windowed_fullscreen::capture_shortcut(&mut fixture, "accessibility.magnification-increase", "Increase display magnification", INCREASE, true);
        super::super::windowed_fullscreen::capture_shortcut(&mut fixture, "accessibility.magnification-reset", "Reset display magnification", RESET, true);
        let (window, initial, surface) = super::super::opening::map(&mut fixture);
        window.role.set_max_size(240, 180); window.surface.commit(); fixture.exchange();
        fixture.inspect(move |canvas| {
            let world = canvas.world_mut();
            let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
            let ids = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
            for id in ids { native::workspace_config::set_physics(world, root, id, false); }
            world.get_mut::<native::topology::view::View>(root).unwrap().spatial = spatial;
        });
        fixture.desktop.send(Input::Action(DesktopAction::SetLayout(layout))).unwrap();
        fixture.desktop.send(Input::Application(surface)).unwrap();
        settle(&mut fixture);
        let mut buffers = vec![initial];
        let size = repaint(&mut fixture, &window, surface, &mut buffers);
        let original = geometry(&fixture, surface);
        let viewport = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs[0].clone();
        let source_anchor = original.mapping.canvas_point(size.map(|value| f64::from(value / 2))).unwrap();
        let anchor = viewport.raster.point(source_anchor);
        assert!((0..2).all(|axis| anchor[axis] > f64::from(SIZE[axis]) * 0.25 && anchor[axis] < f64::from(SIZE[axis]) * 0.75),
            "The oracle needs an unclamped client anchor inside the central viewport: {anchor:?}");
        fixture.desktop.move_pointer(source_anchor, 30).unwrap(); fixture.tick(); settle(&mut fixture);
        let image = super::super::appearance::pixels(&mut fixture, &format!("lens-baseline-{name}"));
        let baseline: [[f64; 2]; 3] = std::array::from_fn(|index| {
            let local = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(MARKERS[index].0[axis]));
            let locator = viewport.raster.point(original.mapping.canvas_point(local).unwrap());
            marker(&image, SIZE, locator, 1.0, MARKERS[index].1, false)
        });
        let original_view = view(&mut fixture);
        let configures = fixture.client.configures.len();
        let (capture_source, session) = super::super::capture_lifetime::output_session(&mut fixture);
        let pending = super::super::capture_lifetime::Pending::new(&mut fixture, &session, SIZE);
        super::super::capture_lifetime::submitted(&mut fixture, &pending);
        let mut current = 1.0;
        for factor in [2.0, 4.0, 1.0] {
            fixture.desktop.move_pointer(viewport.raster.logical_point(if current == 1.0 { anchor } else { SIZE.map(|value| f64::from(value) * 0.5) }), 50).unwrap();
            fixture.tick(); settle(&mut fixture);
            if factor == 1.0 { shortcut(&mut fixture, RESET, 1.0); }
            else {
                while current < factor { current += 0.25; shortcut(&mut fixture, INCREASE, current); }
            }
            let image = super::super::appearance::pixels(&mut fixture, &format!("lens-{factor}-{name}"));
            let expected: [[f64; 2]; 3] = std::array::from_fn(|index| std::array::from_fn(|axis| {
                if factor == 1.0 { baseline[index][axis] } else { f64::from(SIZE[axis]) * 0.5 + factor * (baseline[index][axis] - anchor[axis]) }
            }));
            for index in 0..MARKERS.len() {
                let observed = marker(&image, SIZE, expected[index], factor, MARKERS[index].1, false);
                compare(observed, expected[index], &format!("{name}/{factor}"));
                let local = std::array::from_fn(|axis| f64::from(size[axis] / 2) + f64::from(MARKERS[index].0[axis]));
                click(&mut fixture, observed, &window.surface, local);
            }
            assert_eq!(view(&mut fixture), original_view, "Display magnification moved the world camera");
            assert_eq!(geometry(&fixture, surface), original, "Display magnification changed client or nominal shader geometry");
            assert!(fixture.client.configures[configures..].iter().all(|configured| *configured == size.map(|value| value as i32)),
                "Display magnification emitted a client-size configure");
            assert_eq!(fixture.client.window_configurations[&window.role.id()].0, size.map(|value| value as i32));
            if factor == 2.0 {
                super::super::capture_lifetime::rejected(&mut fixture, &pending);
                let fresh = super::super::capture_lifetime::Pending::new(&mut fixture, &session, SIZE);
                let captured = super::super::capture_lifetime::completed(&mut fixture, &fresh);
                for index in 0..MARKERS.len() { compare(marker(&captured, SIZE, expected[index], factor, MARKERS[index].1, true), expected[index], "fresh magnified capture"); }
                fresh.destroy(&mut fixture);
                if !spatial && layout == WorkspaceLayout::Free { popup(&mut fixture, &window, surface, size, anchor); }
            }
            current = factor;
        }
        pending.destroy(&mut fixture);
        session.destroy(); capture_source.destroy(); fixture.exchange();
        fixture.desktop.worker.stop().unwrap();
        drop(buffers);
    }
}
