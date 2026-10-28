use super::*;
use crate::presentation::{Frame, Presentation};
use lince_interface::host::WakeSignal;

fn button(canvas: &mut Presentation, text: &str) -> Entity {
    let world = canvas.world_mut();
    world
        .query::<(Entity, &Children, &native::actions::ActionButton)>()
        .iter(world)
        .find(|(entity, children, _)| {
            world
                .get::<ComputedNode>(*entity)
                .is_some_and(|node| node.size().min_element() > 0.0)
                && world
                    .get::<InheritedVisibility>(*entity)
                    .is_some_and(|visible| visible.get())
                && children.iter().any(|child| {
                    world
                        .get::<Text>(child)
                        .is_some_and(|text_node| text_node.0 == text)
                })
        })
        .map(|(entity, _, _)| entity)
        .unwrap()
}

pub(super) fn center(canvas: &mut Presentation, entity: Entity) -> Vec2 {
    let world = canvas.world_mut();
    world
        .get::<UiGlobalTransform>(entity)
        .unwrap()
        .transform_point2(Vec2::ZERO)
        * world
            .get::<ComputedNode>(entity)
            .unwrap()
            .inverse_scale_factor()
}

fn visible_bounds(canvas: &mut Presentation, entity: Entity) -> Option<Rect> {
    let center = center(canvas, entity);
    let world = canvas.world_mut();
    let node = world.get::<ComputedNode>(entity).unwrap();
    let mut bounds = Rect::from_center_size(center, node.size() * node.inverse_scale_factor());
    if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(entity) {
        bounds = bounds.intersect(Rect::from_corners(
            clip.clip.min * node.inverse_scale_factor(),
            clip.clip.max * node.inverse_scale_factor(),
        ));
    }
    (bounds.size().min_element() > 0.0).then_some(bounds)
}

pub(super) fn capture(name: &str, size: [u32; 2], scale: f64, frame: &Frame) {
    let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") else {
        return;
    };
    let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
    let mut bytes = format!("P6\n{} {}\n255\n", size[0], size[1]).into_bytes();
    for pixel in pixels.chunks_exact(4) {
        bytes.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(
        std::path::PathBuf::from(path).join(format!(
            "overview-wayland-{name}-{}-{}.ppm",
            size[0],
            (scale * 100.0).round() as u32
        )),
        bytes,
    )
    .unwrap();
}

fn assert_workspace_names_are_rendered(
    canvas: &mut Presentation,
    size: [u32; 2],
    scale: f64,
    frame: &Frame,
) {
    let world = canvas.world_mut();
    let captions: Vec<_> = world
        .query::<(Entity, &Text, &ChildOf)>()
        .iter(world)
        .filter(|(_, text, parent)| {
            (text.0.starts_with("1 · ") || text.0.starts_with("2 · "))
                && world
                    .get::<native::actions::ActionButton>(parent.parent())
                    .is_some()
        })
        .map(|(entity, text, _)| (entity, text.0.clone()))
        .collect();
    assert_eq!(captions.len(), 2);
    assert_rendered_text(canvas, size, scale, frame, captions);
}

fn assert_rendered_text(
    canvas: &mut Presentation,
    size: [u32; 2],
    scale: f64,
    frame: &Frame,
    captions: Vec<(Entity, String)>,
) {
    let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
    for (entity, text) in captions {
        let bounds = visible_bounds(canvas, entity)
            .unwrap_or_else(|| panic!("The overview text has no visible bounds: {text}"));
        let min = (bounds.min.as_dvec2() * scale)
            .floor()
            .max(bevy::math::DVec2::ZERO)
            .as_uvec2();
        let max = (bounds.max.as_dvec2() * scale)
            .ceil()
            .as_uvec2()
            .min(UVec2::from_array(size));
        let mut ink = 0;
        for y in min.y..max.y {
            for x in min.x..max.x {
                let offset = ((y * size[0] + x) * 4) as usize;
                if pixels[offset..offset + 3]
                    .iter()
                    .all(|channel| *channel > 170)
                {
                    ink += 1;
                }
            }
        }
        assert!(ink > 10, "The overview text has no rendered pixels: {text}");
    }
}

pub(super) fn current(canvas: &mut Presentation, harness: &mut Harness) -> Frame {
    canvas.sync_windows(&harness.server.windows()).unwrap();
    let frame = settle(canvas);
    for request in &frame.windows {
        let visible = frame.geometry.as_ref().is_some_and(|geometry| {
            geometry
                .items
                .iter()
                .any(|item| item.id == ItemId::Application(request.surface))
        });
        harness
            .server
            .configure_presented_state(
                request.surface,
                request.size.map(|size| size as i32),
                request.state,
                visible,
            )
            .unwrap();
    }
    harness.sync();
    frame
}

fn motion(canvas: &mut Presentation, harness: &mut Harness, point: Vec2) {
    let frame = current(canvas, harness);
    assert!(frame.owns_modal_pointer(point.as_dvec2().to_array()));
    let native = harness.server.native_surface().unwrap();
    harness
        .server
        .pointer_motion(Some((native, point.as_dvec2().to_array())), 10)
        .unwrap();
    canvas.pointer(point.to_array()).unwrap();
    current(canvas, harness);
}

fn pressed(canvas: &mut Presentation, harness: &mut Harness, value: bool) {
    harness.server.pointer_button(272, value, 11);
    canvas
        .button(bevy::input::mouse::MouseButton::Left, value)
        .unwrap();
    current(canvas, harness);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn overview_drag_keeps_real_wayland_content_and_hidden_protocol_sizes_at_fractional_scales() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    for (size, scale) in [([1000, 720], 1.0), ([1000, 720], 1.25), ([360, 720], 1.5)] {
        workflow(size, scale);
    }
}

fn workflow(size: [u32; 2], scale: f64) {
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new(
        size,
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = {
        let world = canvas.world_mut();
        world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap()
    };
    let mut harness = Harness::new();
    harness
        .server
        .configure_scaled_outputs(
            &[Monitor {
                id: OutputId(1),
                name: "Overview test output".into(),
                origin: [0, 0],
                size: size.map(|value| value as i32),
            }],
            &[(OutputId(1), scale)].into_iter().collect(),
        )
        .unwrap();
    harness.sync();
    canvas
        .outputs(
            harness
                .server
                .output_viewports()
                .into_iter()
                .map(|(name, viewport)| crate::presentation::Output { name, viewport })
                .collect(),
        )
        .unwrap();
    let window = harness.window();
    window.role.set_app_id("lince.validation.overview".into());
    window.role.set_title("Live overview window".into());
    let mut buffers = vec![paint(&mut harness, &window, [320, 240], [0, 255, 0, 255])];
    let snapshot = harness
        .server
        .windows()
        .into_iter()
        .find(|window| window.app_id == "lince.validation.overview")
        .unwrap();
    let id = snapshot.id;
    let surface = snapshot.surface;
    current(&mut canvas, &mut harness);
    let configured = harness.client.window_configurations[&window.role.id()]
        .0
        .map(|value| value as u32);
    buffers.push(paint(&mut harness, &window, configured, [0, 255, 0, 255]));
    current(&mut canvas, &mut harness);
    let (entity, source, identity) = member(&mut canvas, id);
    canvas.create_workspace(root.to_bits()).unwrap();
    let destination = canvas
        .world_mut()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    native::workspace_config::set_physics(canvas.world_mut(), root, source, false);
    native::workspace_config::set_physics(canvas.world_mut(), root, destination, false);
    let layout = native::layout::LayoutBox::new(Vec2::new(280.0, 180.0));
    let note = canvas
        .world_mut()
        .spawn((
            native::canvas::CanvasItem {
                position: bevy::math::DVec2::new(400.0, -300.0),
                size: Vec2::new(280.0, 180.0),
            },
            layout,
            native::workspace::WorkspaceMember(destination),
            Name::new("Destination note"),
            BackgroundColor(Color::srgb(0.1, 0.1, 0.9)),
            ChildOf(root),
        ))
        .id();
    current(&mut canvas, &mut harness);
    canvas
        .world_mut()
        .get_mut::<native::workspace::Workspaces>(root)
        .unwrap()
        .entries
        .iter_mut()
        .find(|entry| entry.id == destination)
        .unwrap()
        .desktop
        .set_row_height(PlacementId(layout.id), Width::Pixels(320.0))
        .unwrap();
    current(&mut canvas, &mut harness);
    canvas.activate_workspace(root.to_bits(), source).unwrap();
    current(&mut canvas, &mut harness);
    let configured = harness.client.window_configurations[&window.role.id()]
        .0
        .map(|value| value as u32);
    buffers.push(paint(&mut harness, &window, configured, [0, 255, 0, 255]));
    current(&mut canvas, &mut harness);
    harness
        .server
        .pointer_motion(Some((surface.clone(), [20.0, 20.0])), 1)
        .unwrap();
    harness.server.pointer_button(272, true, 2);
    harness.server.pointer_button(272, false, 3);
    harness.sync();
    assert!(
        harness
            .client
            .pointer_buttons
            .ends_with(&[(272, true), (272, false)])
    );
    canvas
        .desktop_action(DesktopAction::ToggleOverview)
        .unwrap();
    let before = current(&mut canvas, &mut harness);
    assert!(before.input.keyboard_modal);
    capture("before", size, scale, &before);
    assert_workspace_names_are_rendered(&mut canvas, size, scale, &before);
    let configure_count = harness.client.configures.len();
    let client_size = harness.client.window_configurations[&window.role.id()].0;
    let buttons = harness.client.pointer_buttons.len();
    let from = button(&mut canvas, "Live overview window");
    let to = button(&mut canvas, "Destination note");
    let origin = visible_bounds(&mut canvas, from)
        .expect("The source thumbnail is not visible")
        .center();
    let positions = [center(&mut canvas, from), center(&mut canvas, to)];
    motion(&mut canvas, &mut harness, origin);
    pressed(&mut canvas, &mut harness, true);
    motion(&mut canvas, &mut harness, origin + Vec2::new(8.0, 0.0));
    let dragging = current(&mut canvas, &mut harness);
    capture("dragging", size, scale, &dragging);
    let (ghost, captions) = {
        let world = canvas.world_mut();
        let (ghost, children) = world
            .query::<(Entity, &Name, &Children)>()
            .iter(world)
            .find(|(_, name, _)| name.as_str() == "overview.drag")
            .map(|(entity, _, children)| (entity, children))
            .unwrap();
        let captions: Vec<_> = children
            .iter()
            .filter_map(|child| world.get::<Text>(child).map(|text| (child, text.0.clone())))
            .collect();
        (ghost, captions)
    };
    assert_eq!(captions.len(), 1);
    let ghost_bounds = visible_bounds(&mut canvas, ghost).unwrap();
    assert!(
        f64::from(ghost_bounds.height()) * scale < f64::from(size[1]) * 0.5,
        "The short drag feedback fills the overview instead of fitting its text"
    );
    assert_rendered_text(&mut canvas, size, scale, &dragging, captions);
    for (entity, original) in [from, to].into_iter().zip(positions) {
        assert!(
            f64::from(center(&mut canvas, entity).distance(original)) * scale <= 1.0,
            "Starting a drag shifted its overview targets"
        );
    }
    let target = visible_bounds(&mut canvas, to)
        .expect("The destination thumbnail is not visible after the drag begins");
    let end = target.center() + Vec2::new(target.width() * 0.3, 0.0);
    motion(&mut canvas, &mut harness, end);
    capture(
        "destination",
        size,
        scale,
        &current(&mut canvas, &mut harness),
    );
    pressed(&mut canvas, &mut harness, false);
    let frame = current(&mut canvas, &mut harness);
    assert!(frame.input.keyboard_modal);
    let back = button(&mut canvas, lince_interface::i18n::Token::Back.text());
    let back_center = center(&mut canvas, back);
    let half = {
        let world = canvas.world_mut();
        let node = world.get::<ComputedNode>(back).unwrap();
        node.size() * node.inverse_scale_factor() * 0.5
    };
    let bounds = frame
        .geometry
        .as_ref()
        .unwrap()
        .outputs
        .iter()
        .find(|output| output.id == OutputId(1))
        .unwrap()
        .bounds;
    for point in [back_center - half, back_center + half] {
        assert!(
            bounds.contains(point.as_dvec2().to_array()),
            "The overview Back control is outside the scaled output: {point:?}"
        );
    }
    assert_eq!(member(&mut canvas, id), (entity, destination, identity));
    assert_eq!(
        canvas
            .world_mut()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active,
        source
    );
    assert_eq!(
        model(&mut canvas, root, destination).tiled_rows(),
        vec![vec![PlacementId(layout.id), identity]]
    );
    let client = button(&mut canvas, "Live overview window");
    let neighbour = button(&mut canvas, "Destination note");
    assert!(center(&mut canvas, client).x > center(&mut canvas, neighbour).x);
    let world = canvas.world_mut();
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        Some(client)
    );
    assert_eq!(
        world
            .get::<native::layout::desktop::ApplicationWindow>(entity)
            .unwrap()
            .content_size,
        configured
    );
    assert_eq!(
        world
            .get::<native::workspace::WorkspaceMember>(note)
            .unwrap()
            .0,
        destination
    );
    assert_eq!(harness.client.pointer_buttons.len(), buttons);
    assert_eq!(harness.client.configures.len(), configure_count);
    assert_ne!(
        frame
            .windows
            .iter()
            .find(|window| window.surface == id)
            .unwrap()
            .size,
        client_size.map(|value| value as u32),
        "The fixture did not exercise a changed hidden rectangle"
    );
    assert_eq!(
        harness.client.window_configurations[&window.role.id()].0,
        client_size
    );
    assert_eq!(
        harness
            .server
            .windows()
            .iter()
            .find(|window| window.id == id)
            .unwrap()
            .surface,
        surface
    );
    let pixels = crate::presentation::probe_pixels(frame.dmabuf().unwrap()).unwrap();
    assert!(
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[1] > 180 && pixel[0] < 50 && pixel[2] < 50)
            .count()
            > 100
    );
    capture("after", size, scale, &frame);
    motion(&mut canvas, &mut harness, back_center);
    pressed(&mut canvas, &mut harness, true);
    pressed(&mut canvas, &mut harness, false);
    assert!(!current(&mut canvas, &mut harness).input.keyboard_modal);
    assert_eq!(harness.client.pointer_buttons.len(), buttons);
}
