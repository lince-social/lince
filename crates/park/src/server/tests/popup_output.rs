use super::*;
use lince_interface::host::{
    ContentMapping, GeometrySnapshot, ItemGeometry, ItemId, Rectangle as CanvasRectangle,
};

fn mapped_popup(
    harness: &mut Harness,
    window: &Window,
    serial: Option<u32>,
) -> (
    wl_surface::WlSurface,
    xdg_surface::XdgSurface,
    xdg_popup::XdgPopup,
) {
    let qh = harness.queue.handle();
    let shell = harness.client.shell.as_ref().unwrap();
    let positioner = shell.create_positioner(&qh, ());
    positioner.set_size(128, 64);
    positioner.set_anchor_rect(16, 16, 1, 1);
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = shell.get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    if let Some(serial) = serial {
        popup.grab(harness.client.seat.as_ref().unwrap(), serial);
    }
    surface.commit();
    harness.sync();
    (surface, xdg, popup)
}

#[test]
fn destroying_only_the_popup_role_invalidates_window_snapshots_and_preserves_parent_content() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _parent_buffer = harness.attach(&window, [320, 240]);
    let (surface, _xdg, popup) = mapped_popup(&mut harness, &window, None);
    let buffer = harness.buffer([128, 64]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    let before = harness.server.windows()[0].clone();
    assert_eq!(before.popups.len(), 1);
    let revision = harness.server.window_revision();
    popup.destroy();
    harness.sync();
    assert!(surface.is_alive());
    let after = harness.server.windows()[0].clone();
    assert!(after.mapped);
    assert!(after.popups.is_empty());
    assert!(Arc::ptr_eq(
        &before.nodes[0].content,
        &after.nodes[0].content
    ));
    assert_eq!(before.popups.len(), 1);
    assert!(harness.server.window_revision() > revision);
}

#[test]
fn the_first_parent_motion_after_a_popup_grab_ends_reaches_the_client_at_its_new_position() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _parent_buffer = harness.attach(&window, [320, 240]);
    let parent = harness.server.windows()[0].clone();
    harness.server.focus(
        Some(parent.id),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    harness
        .server
        .pointer_motion(Some((parent.surface.clone(), [45.0, 55.0])), 1)
        .unwrap();
    harness.server.keyboard(30, true, 2, |_| false).unwrap();
    harness.sync();
    let serial = harness.client.key_serial;
    harness.server.keyboard(30, false, 3, |_| false).unwrap();
    let (_surface, _xdg, popup) = mapped_popup(&mut harness, &window, Some(serial));
    assert!(harness.server.popup_grab_active());
    popup.destroy();
    harness.sync();
    assert!(!harness.server.popup_grab_active());
    harness.client.pointer_positions.clear();
    harness
        .server
        .pointer_motion(Some((parent.surface.clone(), [151.0, 173.0])), 4)
        .unwrap();
    harness.sync();
    assert_eq!(
        harness
            .server
            .state
            .seat
            .get_pointer()
            .unwrap()
            .current_focus(),
        Some(parent.surface)
    );
    assert_eq!(
        harness.client.pointer_positions.last(),
        Some(&[151.0, 173.0])
    );
}

#[cfg(feature = "nested")]
#[test]
fn a_replacement_management_grab_survives_pointer_motion_after_the_popup_grab_ended() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _parent_buffer = harness.attach(&window, [320, 240]);
    let parent = harness.server.windows()[0].clone();
    harness.server.focus(
        Some(parent.id),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.sync();
    let serial = harness.client.key_serial;
    harness.server.keyboard(30, false, 2, |_| false).unwrap();
    let (_surface, _xdg, popup) = mapped_popup(&mut harness, &window, Some(serial));
    popup.destroy();
    harness.sync();
    assert!(!harness.server.popup_grab_active());
    harness
        .server
        .pointer_motion(Some((parent.surface.clone(), [45.0, 55.0])), 3)
        .unwrap();
    assert!(harness.server.begin_manipulation(272));
    harness.server.manipulation_button(272, true, 4);
    harness
        .server
        .pointer_motion(Some((parent.surface, [151.0, 173.0])), 5)
        .unwrap();
    assert!(harness.server.manipulation_grabbed());
    harness.server.manipulation_button(272, false, 6);
    assert!(!harness.server.manipulation_grabbed());
}

#[test]
fn starting_a_repaint_advances_callbacks_without_releasing_client_buffers() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let done = Arc::new(AtomicBool::new(false));
    window.surface.frame(&harness.queue.handle(), done.clone());
    window.surface.commit();
    harness.sync();
    let ready = harness.server.windows()[0].clone();
    let released = harness.client.releases;
    harness.server.frame_started_nodes(&ready.nodes);
    harness.sync();
    assert!(done.load(Ordering::Acquire));
    assert_eq!(harness.client.releases, released);
    assert!(
        ready
            .nodes
            .iter()
            .all(|node| !node.content.released.load(Ordering::Acquire))
    );
}

#[test]
fn desynchronized_children_advance_independently_and_synchronized_commits_wait() {
    for desynchronized in [false, true] {
        let mut harness = Harness::new();
        let window = harness.window();
        let _buffer = harness.attach(&window, [320, 240]);
        let qh = harness.queue.handle();
        let child = harness
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&qh, ());
        let role = harness
            .client
            .subcompositor
            .as_ref()
            .unwrap()
            .get_subsurface(&child, &window.surface, &qh, ());
        if desynchronized {
            role.set_desync();
        }
        let buffer = harness.buffer([64, 64]);
        child.attach(Some(&buffer), 0, 0);
        child.commit();
        window.surface.commit();
        harness.sync();
        let drawn = harness.server.windows()[0].clone();
        let parent_done = Arc::new(AtomicBool::new(false));
        window.surface.frame(&qh, parent_done.clone());
        let child_done = Arc::new(AtomicBool::new(false));
        let replacement = harness.buffer([64, 64]);
        child.frame(&qh, child_done.clone());
        child.attach(Some(&replacement), 0, 0);
        child.commit();
        window.surface.commit();
        harness.sync();
        let represented = harness.server.callback_nodes(&drawn);
        harness.server.presented_nodes(&represented);
        harness.sync();
        assert_eq!(parent_done.load(Ordering::Acquire), desynchronized);
        assert!(!child_done.load(Ordering::Acquire));
        let next = harness.server.windows()[0].clone();
        harness.server.presented_nodes(&next.nodes);
        harness.sync();
        assert!(parent_done.load(Ordering::Acquire));
        assert!(child_done.load(Ordering::Acquire));
    }
}

#[test]
fn frame_only_commits_reuse_visible_pixels_and_visual_changes_wait_for_their_frame() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let painted = harness.server.windows()[0].clone();
    let revision = harness.server.window_revision();
    let qh = harness.queue.handle();
    let done = Arc::new(AtomicBool::new(false));
    window.surface.frame(&qh, done.clone());
    window.surface.commit();
    harness.sync();
    assert_eq!(harness.server.window_revision(), revision);
    assert!(!done.load(Ordering::Acquire));
    harness.server.presented_nodes(&painted.nodes);
    harness.sync();
    assert!(!done.load(Ordering::Acquire));
    let represented = harness.server.callback_nodes(&painted);
    harness.server.presented_nodes(&represented);
    harness.sync();
    assert!(done.load(Ordering::Acquire));
    let resized = Arc::new(AtomicBool::new(false));
    window.surface.set_buffer_scale(2);
    window.surface.frame(&qh, resized.clone());
    window.surface.commit();
    harness.sync();
    assert!(harness.server.window_revision() > revision);
    harness.server.presented_nodes(&painted.nodes);
    harness.sync();
    assert!(!resized.load(Ordering::Acquire));
    let current = harness.server.windows()[0].clone();
    harness.server.presented_nodes(&current.nodes);
    harness.sync();
    assert!(resized.load(Ordering::Acquire));
}

#[test]
fn desynchronized_children_can_update_while_the_toplevel_waits_to_be_remapped() {
    let mut harness = Harness::new();
    let window = harness.window();
    let parent_buffer = harness.attach(&window, [320, 240]);
    let qh = harness.queue.handle();
    let child = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let role = harness
        .client
        .subcompositor
        .as_ref()
        .unwrap()
        .get_subsurface(&child, &window.surface, &qh, ());
    role.set_desync();
    let child_buffer = harness.buffer([64, 64]);
    child.attach(Some(&child_buffer), 0, 0);
    child.commit();
    harness.sync();
    assert_eq!(harness.server.windows()[0].nodes.len(), 2);
    for _ in 0..3 {
        window.surface.attach(None, 0, 0);
        window.surface.commit();
        harness.sync();
        let serials = harness.client.serials;
        child.attach(Some(&child_buffer), 0, 0);
        child.damage_buffer(0, 0, 64, 64);
        child.commit();
        harness.sync();
        assert!(!harness.server.windows()[0].mapped);
        assert_eq!(harness.client.serials, serials);
        window.surface.commit();
        harness.sync();
        assert!(harness.client.serials > serials);
        window.surface.attach(Some(&parent_buffer), 0, 0);
        window.surface.commit();
        harness.sync();
        assert!(harness.server.windows()[0].mapped);
        assert_eq!(harness.server.windows()[0].nodes.len(), 2);
    }
}

#[cfg(feature = "nested")]
#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn changing_client_buffers_rebinds_materials_without_changing_window_geometry() {
    use std::io::Write;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = crate::presentation::Presentation::new(
        [640, 480],
        directory.path().into(),
        lince_interface::host::WakeSignal::new(|| {}),
    )
    .unwrap();
    for _ in 0..8 {
        canvas.probe_frame().unwrap();
    }
    let mut harness = Harness::new();
    let window = harness.window();
    let qh = harness.queue.handle();
    let mut retained = Vec::new();
    for (size, color) in [
        ([320, 240], [255, 0, 0]),
        ([320, 240], [0, 255, 0]),
        ([320, 240], [0, 0, 255]),
        ([400, 280], [255, 0, 0]),
    ] {
        let mut file = tempfile::tempfile().unwrap();
        let pixels = [color[2], color[1], color[0], 255].repeat((size[0] * size[1]) as usize);
        file.write_all(&pixels).unwrap();
        let pool = harness.client.shm.as_ref().unwrap().create_pool(
            file.as_fd(),
            pixels.len() as i32,
            &qh,
            (),
        );
        let buffer = pool.create_buffer(
            0,
            size[0],
            size[1],
            size[0] * 4,
            wl_shm::Format::Argb8888,
            &qh,
            (),
        );
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, size[0], size[1]);
        window.surface.commit();
        harness.sync();
        assert!(!canvas.sync_windows(&harness.server.windows()).unwrap());
        for _ in 0..8 {
            canvas.probe_frame().unwrap();
        }
        for _ in 0..6 {
            let frame = canvas.probe_frame().unwrap();
            let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
            let colored = pixels
                .chunks_exact(4)
                .filter(|pixel| {
                    pixel[..3]
                        .iter()
                        .zip(color)
                        .all(|(actual, expected)| actual.abs_diff(expected) < 30)
                })
                .count();
            if colored <= 20_000 {
                let mut colors = HashMap::<[u8; 3], usize>::new();
                for pixel in pixels.chunks_exact(4) {
                    *colors.entry(pixel[..3].try_into().unwrap()).or_default() += 1;
                }
                let mut colors: Vec<_> = colors.into_iter().collect();
                colors.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
                eprintln!(
                    "Geometry: {:?}; colors: {:?}",
                    frame.geometry,
                    &colors[..colors.len().min(12)]
                );
            }
            assert!(
                colored > 20_000,
                "Client pixels disappeared after a buffer update: {size:?}, {color:?}, {colored}"
            );
        }
        retained.push((file, pool, buffer));
    }
}

#[test]
fn reactive_popup_follows_parent_output_and_fractional_usable_region() {
    let mut harness = Harness::new();
    harness
        .server
        .configure_scaled_outputs(
            &[
                Monitor {
                    id: OutputId(1),
                    name: "First".into(),
                    origin: [0, 0],
                    size: [640, 480],
                },
                Monitor {
                    id: OutputId(2),
                    name: "Second".into(),
                    origin: [640, 0],
                    size: [640, 480],
                },
            ],
            &BTreeMap::from([(OutputId(2), 1.25)]),
        )
        .unwrap();
    harness.sync();
    let window = harness.window();
    let _parent_buffer = harness.attach(&window, [320, 240]);
    let id = harness.server.windows()[0].id;
    let rectangle = CanvasRectangle::new([0.0; 2], [320.0, 240.0]).unwrap();
    let mut geometry = GeometrySnapshot {
        revision: 1,
        outputs: harness
            .server
            .output_viewports()
            .into_iter()
            .map(|(_, viewport)| viewport)
            .collect(),
        focused: None,
        items: vec![ItemGeometry {
            id: ItemId::Application(id),
            output: OutputId(1),
            mapping: ContentMapping {
                corners: None,
                canvas: rectangle,
                surface: rectangle,
            },
            floating: false,
            clip: None,
        }],
    };
    harness
        .server
        .presented_outputs(&geometry, Some(OutputId(1)));
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(160, 80);
    positioner.set_anchor_rect(300, 200, 1, 1);
    positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
    positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
    positioner.set_constraint_adjustment(
        xdg_positioner::ConstraintAdjustment::SlideX | xdg_positioner::ConstraintAdjustment::SlideY,
    );
    positioner.set_reactive();
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    surface.commit();
    harness.sync();
    let buffer = harness.buffer([160, 80]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    harness.sync();
    let before = *harness.client.popup_configures.last().unwrap();
    let count = harness.client.popup_configures.len();
    geometry.revision += 1;
    geometry.items[0].output = OutputId(2);
    geometry.items[0].mapping.canvas =
        CanvasRectangle::new([1040.0, 200.0], [320.0, 240.0]).unwrap();
    let usable = CanvasRectangle::new([640.0, 40.0], [512.0, 200.0]).unwrap();
    geometry
        .outputs
        .iter_mut()
        .find(|output| output.id == OutputId(2))
        .unwrap()
        .usable = Some(usable);
    harness
        .server
        .presented_outputs(&geometry, Some(OutputId(2)));
    harness.sync();
    assert!(harness.client.popup_configures.len() > count);
    let after = *harness.client.popup_configures.last().unwrap();
    assert_ne!(after, before);
    assert_eq!([after[2], after[3]], [160, 80]);
    assert!(1040 + after[0] >= 640);
    assert!(1040 + after[0] + after[2] <= 1152);
    assert!(200 + after[1] >= 40);
    assert!(200 + after[1] + after[3] <= 240);
    assert_eq!(harness.client.popup_done, 0);
    popup.destroy();
    xdg.destroy();
    surface.destroy();
    positioner.destroy();
    harness.sync();
}

#[cfg(feature = "nested")]
#[test]
#[ignore = "requires hardware EGL"]
fn popup_pixels_survive_offset_outputs_controls_scaling_and_subsurfaces() {
    use std::io::Write;
    let mut harness = Harness::new();
    let window = harness.window();
    let _parent = harness.attach(&window, [320, 240]);
    let qh = harness.queue.handle();
    let positioner = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .create_positioner(&qh, ());
    positioner.set_size(64, 48);
    positioner.set_anchor_rect(20, 16, 1, 1);
    positioner.set_anchor(xdg_positioner::Anchor::TopLeft);
    positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
    let surface = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let xdg = harness
        .client
        .shell
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &qh, ());
    let popup = xdg.get_popup(Some(&window.xdg), &positioner, &qh, ());
    surface.commit();
    harness.sync();
    let mut file = tempfile::tempfile().unwrap();
    let mut content = Vec::new();
    for y in 0..96 {
        for _ in 0..128 {
            content.extend_from_slice(if y < 48 {
                &[0, 0, 255, 255]
            } else {
                &[0, 255, 0, 255]
            });
        }
    }
    file.write_all(&content).unwrap();
    let pool = harness.client.shm.as_ref().unwrap().create_pool(
        file.as_fd(),
        content.len() as i32,
        &qh,
        (),
    );
    let buffer = pool.create_buffer(0, 128, 96, 512, wl_shm::Format::Argb8888, &qh, ());
    surface.set_buffer_scale(2);
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, 128, 96);
    surface.commit();
    harness.sync();
    let child = harness
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let child_role = harness
        .client
        .subcompositor
        .as_ref()
        .unwrap()
        .get_subsurface(&child, &surface, &qh, ());
    child_role.set_position(32, 12);
    let mut child_file = tempfile::tempfile().unwrap();
    child_file
        .write_all(&[255, 0, 0, 255].repeat(8 * 8))
        .unwrap();
    let child_pool =
        harness
            .client
            .shm
            .as_ref()
            .unwrap()
            .create_pool(child_file.as_fd(), 8 * 8 * 4, &qh, ());
    let child_buffer = child_pool.create_buffer(0, 8, 8, 32, wl_shm::Format::Argb8888, &qh, ());
    child.attach(Some(&child_buffer), 0, 0);
    child.damage_buffer(0, 0, 8, 8);
    child.commit();
    surface.commit();
    harness.sync();
    let snapshot = harness.server.windows()[0].clone();
    assert_eq!(snapshot.popups.len(), 1);
    assert_eq!(snapshot.popups[0].nodes.len(), 2);
    let clips = vec![
        CanvasRectangle::new([320.0, 36.0], [320.0, 204.0]).unwrap(),
        CanvasRectangle::new([460.0, 0.0], [180.0, 36.0]).unwrap(),
    ];
    for (origin, scale) in [
        ([360.0, 8.0], 1.0),
        ([360.0, 8.0], 1.5),
        ([290.0, -20.0], 1.0),
    ] {
        let item = ItemGeometry {
            id: ItemId::Application(snapshot.id),
            output: OutputId(2),
            mapping: ContentMapping {
                corners: None,
                canvas: CanvasRectangle::new(origin, [320.0 * scale, 240.0 * scale]).unwrap(),
                surface: CanvasRectangle::new([0.0; 2], [320.0, 240.0]).unwrap(),
            },
            floating: true,
            clip: None,
        };
        let mapping = snapshot.popups[0].mapping(&snapshot, &item).unwrap();
        let pixels = crate::desktop::renderer::tests::popup_pixels(
            snapshot.clone(),
            item,
            clips.clone(),
            [640, 240],
        )
        .unwrap();
        let mut visible = 0;
        for y in 0..240 {
            for x in 0..640 {
                let point = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                let local = mapping.surface_point(point);
                let expected = if clips.iter().any(|clip| clip.contains(point)) {
                    local
                        .map(|point| {
                            if (32.0..40.0).contains(&point[0]) && (12.0..20.0).contains(&point[1])
                            {
                                [255, 0, 0, 255]
                            } else if point[1] < 24.0 {
                                [0, 0, 255, 255]
                            } else {
                                [0, 255, 0, 255]
                            }
                        })
                        .unwrap_or([0; 4])
                } else {
                    [0; 4]
                };
                if expected[3] != 0 {
                    visible += 1;
                }
                let at = (y * 640 + x) as usize * 4;
                assert_eq!(
                    &pixels[at..at + 4],
                    &expected,
                    "popup at {origin:?}, scale {scale}, pixel {x},{y}"
                );
            }
        }
        assert!(visible > 100);
    }
    child_role.destroy();
    child.destroy();
    popup.destroy();
    xdg.destroy();
    surface.destroy();
    positioner.destroy();
    harness.sync();
}
