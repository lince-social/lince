use super::shortcuts::{capture, click, key, named_button, settle};
use super::*;
use crate::server::{Monitor, Server};
use lince_interface::{
    host::{
        HostRequest, OutputId,
        display::{DisplayAction, DisplayOutcome, DisplayRequest, OutputMode},
    },
    native::actions::Action,
};
use smithay::backend::allocator::Buffer;
use std::sync::mpsc::Receiver;

pub(super) fn labelled(canvas: &mut Presentation, caption: &str) -> Entity {
    canvas
        .world_mut()
        .query::<(Entity, &bevy::a11y::AccessibilityNode)>()
        .iter(canvas.app.world())
        .find(|(_, node)| node.label() == Some(caption))
        .map(|(entity, _)| entity)
        .unwrap()
}

pub(super) fn reveal(canvas: &mut Presentation, entity: Entity) {
    let mut steps = Vec::new();
    for _ in 0..16 {
        let world = canvas.app.world();
        let point = world
            .get::<UiGlobalTransform>(entity)
            .unwrap()
            .transform_point2(Vec2::ZERO);
        if world
            .get::<bevy::ui::CalculatedClip>(entity)
            .is_none_or(|clip| clip.clip.contains(point))
        {
            return;
        }
        let mut ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        let mut scroll = None;
        for _ in 0..32 {
            let Some(current) = ancestor else {
                break;
            };
            if world.get::<Node>(current).is_some_and(|node| node.overflow.x == OverflowAxis::Scroll || node.overflow.y == OverflowAxis::Scroll)
                && let (Some(node), Some(transform)) = (world.get::<ComputedNode>(current), world.get::<UiGlobalTransform>(current))
            {
                let mut visible = Rect::from_center_size(transform.transform_point2(Vec2::ZERO), node.size());
                if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(current) { visible = visible.intersect(clip.clip); }
                visible = visible.intersect(Rect::from_corners(Vec2::ZERO, Vec2::from_array(canvas.render_size.map(|value| value as f32))));
                let overflow = world.get::<Node>(current).unwrap().overflow;
                let axes = [overflow.x == OverflowAxis::Scroll && (point.x < visible.min.x || point.x > visible.max.x),
                    overflow.y == OverflowAxis::Scroll && (point.y < visible.min.y || point.y > visible.max.y)];
                if visible.width() > 0.0 && visible.height() > 0.0 && axes.into_iter().any(|axis| axis) {
                    scroll = Some((current, visible, axes));
                    break;
                }
            }
            ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
        }
        let (scroll, visible, axes) = scroll.expect("An offscreen control needs a visible scrollable ancestor");
        let center = world
            .get::<UiGlobalTransform>(scroll)
            .unwrap()
            .transform_point2(Vec2::ZERO);
        let extent = world.get::<ComputedNode>(scroll).unwrap().size();
        let before = world.get::<ScrollPosition>(scroll).unwrap().0;
        let content = world.get::<ComputedNode>(scroll).unwrap().content_size;
        steps.push((point, scroll, before, center, extent, content));
        let delta = [if axes[0] { (center.x - point.x).clamp(-512.0, 512.0) } else { 0.0 },
            if axes[1] { (center.y - point.y).clamp(-512.0, 512.0) } else { 0.0 }];
        canvas
            .pointer([visible.max.x - 3.0, visible.center().y])
            .unwrap();
        settle(canvas);
        canvas
            .scroll(
                delta,
                bevy::input::mouse::MouseScrollUnit::Pixel,
                bevy::input::touch::TouchPhase::Moved,
            )
            .unwrap();
        settle(canvas);
    }
    let frame = settle(canvas);
    capture("control-unreachable", canvas.render_size, &frame);
    let caption = canvas.app.world().get::<bevy::a11y::AccessibilityNode>(entity).and_then(|node| node.label()).unwrap_or("unnamed control");
    panic!("The control {caption} was not reachable through wheel scrolling: {steps:?}");
}

pub(super) fn write(canvas: &mut Presentation, caption: &str, value: &str, size: [u32; 2]) {
    let entity = labelled(canvas, caption);
    reveal(canvas, entity);
    click(canvas, entity, size);
    assert_eq!(canvas.app.world().resource::<bevy::input_focus::InputFocus>().get(), Some(entity), "Clicking {caption} must focus that field");
    key(canvas, KeyCode::ControlLeft, true);
    canvas
        .key(KeyCode::KeyA, Key::Character("a".into()), None, true, false)
        .unwrap();
    settle(canvas);
    canvas
        .key(
            KeyCode::KeyA,
            Key::Character("a".into()),
            None,
            false,
            false,
        )
        .unwrap();
    settle(canvas);
    key(canvas, KeyCode::ControlLeft, false);
    let input = canvas.app.world().get::<bevy::text::EditableText>(entity).unwrap();
    assert_eq!(input.editor.raw_selection().text_range(), 0..input.editor.raw_text().len(), "Select all must cover {caption}");
    canvas.text(value.into()).unwrap();
    settle(canvas);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(entity)
            .unwrap()
            .value()
            .to_string(),
        value
    );
}

fn publish(canvas: &mut Presentation, server: &Server) {
    canvas
        .outputs(
            server
                .output_viewports()
                .into_iter()
                .map(|(name, viewport)| crate::presentation::Output { name, viewport })
                .collect(),
        )
        .unwrap();
    canvas
        .world_mut()
        .insert_resource(server.display_snapshot());
    settle(canvas);
}

fn bridge(
    canvas: &mut Presentation,
    server: &mut Server,
    receiver: &Receiver<HostRequest>,
    path: &std::path::Path,
) -> Vec<bool> {
    while let Ok(request) = receiver.try_recv() {
        let HostRequest::Display(request) = request else {
            panic!("Monitor settings sent an unrelated host action");
        };
        server.display_request(request, |heads| {
            crate::commands::Commands::save_outputs(path, heads)
                .map(|warning| assert!(warning.is_none()))
        });
    }
    let mut phases = Vec::new();
    for _ in 0..8 {
        let requests = server.output_requests();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            phases.push(request.test);
            let accepted = server
                .apply_virtual_output_configuration(&request, [2560, 1440])
                .unwrap();
            assert!(
                accepted,
                "The rendered monitor draft must pass the real virtual-output backend"
            );
            server.output_configuration_finished(request.token, accepted);
        }
    }
    assert!(server.output_requests().is_empty());
    publish(canvas, server);
    phases
}

fn output_pixels(frame: &Frame, id: OutputId) -> Vec<u8> {
    if frame.output == Some(id) {
        return super::probe_pixels(frame.dmabuf().unwrap()).unwrap();
    }
    let output = frame.outputs.iter().find(|output| output.id == id).unwrap();
    super::probe_pixels(output.dmabuf()).unwrap()
}

fn output_capture(frame: &Frame, id: OutputId, name: &str) {
    let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") else {
        return;
    };
    let buffer = if frame.output == Some(id) {
        frame.dmabuf().unwrap()
    } else {
        frame
            .outputs
            .iter()
            .find(|output| output.id == id)
            .unwrap()
            .dmabuf()
    };
    let size = buffer.size();
    let mut bytes = format!("P6\n{} {}\n255\n", size.w, size.h).into_bytes();
    for pixel in output_pixels(frame, id).chunks_exact(4) {
        bytes.extend_from_slice(&pixel[..3]);
    }
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        std::path::PathBuf::from(directory).join(format!("{name}.ppm")),
        bytes,
    )
    .unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn monitor_panel_arranges_tests_confirms_reverts_and_identifies_the_named_virtual_output() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1280, 720];
    let mut server = Server::new([2560, 1440]).unwrap();
    server.display_bounds(Some([2560, 1440]));
    server
        .configure_outputs(&[
            Monitor {
                id: OutputId(1),
                name: "Park-1".into(),
                origin: [0, 0],
                size: [1280, 720],
            },
            Monitor {
                id: OutputId(2),
                name: "Park-2".into(),
                origin: [1280, 0],
                size: [1280, 720],
            },
        ])
        .unwrap();
    server.registered_output_catalog().unwrap();
    let mut catalog = server.display_snapshot().heads;
    for head in &mut catalog {
        head.modes.push(OutputMode {
            size: [960, 540],
            refresh_mhz: 75000,
            preferred: false,
        });
    }
    server.output_catalog(catalog).unwrap();
    let before = server.display_snapshot();
    let mut canvas = Presentation::new(
        size,
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let (sender, receiver) = std::sync::mpsc::sync_channel(32);
    canvas.host_requests(sender.clone());
    publish(&mut canvas, &server);
    let root = canvas.canvas_root().unwrap();
    let camera = *canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
    settle(&mut canvas);
    let open = named_button(&mut canvas, "Arrange monitors");
    click(&mut canvas, open, size);
    let frame = settle(&mut canvas);
    assert!(frame.input.keyboard_modal);
    assert!(frame.input.bindings.is_empty());
    capture("monitors-wide", size, &frame);
    let second = canvas
        .world_mut()
        .query::<(Entity, &Name)>()
        .iter(canvas.app.world())
        .find(|(_, name)| name.as_str() == "monitor.diagram.Park-2")
        .map(|(entity, _)| entity)
        .unwrap();
    key(&mut canvas, KeyCode::Tab, true);
    key(&mut canvas, KeyCode::Tab, false);
    assert_eq!(
        canvas
            .app
            .world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(second),
        "Tab must reach the next monitor diagram item, independently of duplicate selector captions"
    );
    reveal(&mut canvas, second);
    click(&mut canvas, second, size);
    let rectangle = second;
    reveal(&mut canvas, rectangle);
    let point = canvas
        .app
        .world()
        .get::<UiGlobalTransform>(rectangle)
        .unwrap()
        .transform_point2(Vec2::ZERO);
    canvas.pointer(point.to_array()).unwrap();
    settle(&mut canvas);
    canvas.button(MouseButton::Left, true).unwrap();
    settle(&mut canvas);
    for offset in [Vec2::new(-15.0, -10.0), Vec2::new(-420.0, -30.0)] {
        canvas.pointer((point + offset).to_array()).unwrap();
        settle(&mut canvas);
    }
    canvas.button(MouseButton::Left, false).unwrap();
    settle(&mut canvas);
    let x = labelled(&mut canvas, "X");
    assert_ne!(
        canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(x)
            .unwrap()
            .value()
            .to_string(),
        "1280"
    );
    assert_eq!(server.display_snapshot().heads, before.heads);
    assert!(
        canvas.app.world().get::<native::canvas::CanvasView>(root) == Some(&camera),
        "Dragging a monitor draft must not move the underlying canvas camera"
    );
    capture("monitors-drag-draft", size, &settle(&mut canvas));
    key(&mut canvas, KeyCode::Escape, true);
    key(&mut canvas, KeyCode::Escape, false);
    let open = named_button(&mut canvas, "Arrange monitors");
    click(&mut canvas, open, size);
    let second = named_button(&mut canvas, "Park-2");
    reveal(&mut canvas, second);
    click(&mut canvas, second, size);
    write(&mut canvas, "X", "-960", size);
    write(&mut canvas, "Y", "0", size);
    write(&mut canvas, "Scale", "1.25", size);
    let mode = named_button(&mut canvas, "960 × 540 · 75.000 Hz");
    reveal(&mut canvas, mode);
    click(&mut canvas, mode, size);
    let apply = named_button(&mut canvas, "Apply");
    eprintln!("Monitor fixture: applying selected resolution/refresh");
    click(&mut canvas, apply, size);
    assert_eq!(
        bridge(
            &mut canvas,
            &mut server,
            &receiver,
            &directory.path().join("park.json")
        ),
        vec![true, false]
    );
    let preview = server.display_snapshot();
    assert!(preview.preview.is_some());
    assert_eq!(preview.heads[0].position, [960, 0]);
    assert_eq!(preview.heads[1].position, [0, 0]);
    assert_eq!(preview.heads[1].scale, 1.25);
    assert_eq!(
        preview.heads[1].modes[preview.heads[1].current.unwrap()].refresh_mhz,
        75000
    );
    assert_eq!(
        server
            .output(OutputId(2))
            .unwrap()
            .current_mode()
            .unwrap()
            .refresh,
        75000
    );
    assert!(!directory.path().join("park.json").exists());
    capture("monitors-confirmation", size, &settle(&mut canvas));
    let revert = named_button(&mut canvas, "Revert");
    eprintln!("Monitor fixture: reverting preview");
    click(&mut canvas, revert, size);
    assert_eq!(
        bridge(
            &mut canvas,
            &mut server,
            &receiver,
            &directory.path().join("park.json")
        ),
        vec![false]
    );
    assert_eq!(server.display_snapshot().heads, before.heads);
    assert!(!directory.path().join("park.json").exists());
    eprintln!("Monitor fixture: configuring confirmed scale");
    write(&mut canvas, "Scale", "1.25", size);
    let apply = named_button(&mut canvas, "Apply");
    click(&mut canvas, apply, size);
    bridge(
        &mut canvas,
        &mut server,
        &receiver,
        &directory.path().join("park.json"),
    );
    let keep = named_button(&mut canvas, "Keep changes");
    eprintln!("Monitor fixture: saving confirmation");
    click(&mut canvas, keep, size);
    assert!(
        bridge(
            &mut canvas,
            &mut server,
            &receiver,
            &directory.path().join("park.json")
        )
        .is_empty()
    );
    assert_eq!(
        server.display_snapshot().response.unwrap().outcome,
        DisplayOutcome::Kept
    );
    assert!(directory.path().join("park.json").is_file());
    let baseline = settle(&mut canvas);
    let pixels = output_pixels(&baseline, OutputId(2));
    let identify = named_button(&mut canvas, "Identify");
    eprintln!("Monitor fixture: identifying second output");
    reveal(&mut canvas, identify);
    click(&mut canvas, identify, size);
    bridge(
        &mut canvas,
        &mut server,
        &receiver,
        &directory.path().join("park.json"),
    );
    let identified = settle(&mut canvas);
    let after = output_pixels(&identified, OutputId(2));
    let white = pixels
        .chunks_exact(4)
        .zip(after.chunks_exact(4))
        .filter(|(before, after)| {
            before[..3].iter().all(|channel| *channel < 100)
                && after[..3].iter().all(|channel| *channel > 180)
        })
        .count();
    assert!(
        white > 100,
        "The identification label did not reach the named nonfocused output: {white}"
    );
    output_capture(&identified, OutputId(2), "monitors-identify-second");
    let snapshot = server.display_snapshot();
    let mut heads = snapshot.configuration();
    heads.get_mut(&OutputId(2)).unwrap().as_mut().unwrap().scale = 1.5;
    server.display_request(
        DisplayRequest {
            id: snapshot.last_request + 1,
            action: DisplayAction::Preview {
                revision: snapshot.revision,
                heads,
            },
        },
        |_| unreachable!(),
    );
    for expected_test in [true, false] {
        let request = server.output_requests().pop().unwrap();
        assert_eq!(request.test, expected_test);
        assert!(
            server
                .apply_virtual_output_configuration(&request, [2560, 1440])
                .unwrap()
        );
        server.output_configuration_finished(request.token, true);
    }
    server.display_request(
        DisplayRequest {
            id: server.display_snapshot().last_request + 1,
            action: DisplayAction::Identify {
                output: OutputId(2),
            },
        },
        |_| unreachable!(),
    );
    publish(&mut canvas, &server);
    let resized = settle(&mut canvas);
    assert!(
        output_pixels(&resized, OutputId(2))
            .chunks_exact(4)
            .filter(|pixel| pixel[..3].iter().all(|channel| *channel > 180))
            .count()
            > 100
    );
    output_capture(&resized, OutputId(2), "monitors-identify-resized");
    let revert = named_button(&mut canvas, "Revert");
    click(&mut canvas, revert, size);
    bridge(
        &mut canvas,
        &mut server,
        &receiver,
        &directory.path().join("park.json"),
    );
    assert_eq!(server.display_snapshot().heads[1].scale, 1.25);
    let deadline = server.display_snapshot().identification.unwrap().deadline;
    server.advance_display(deadline);
    publish(&mut canvas, &server);
    assert!(
        canvas
            .world_mut()
            .query::<&Name>()
            .iter(canvas.app.world())
            .all(|name| !name.as_str().starts_with("monitor.identify."))
    );
    let first = named_button(&mut canvas, "Park-1");
    reveal(&mut canvas, first);
    click(&mut canvas, first, size);
    let baseline = settle(&mut canvas);
    let pixels = output_pixels(&baseline, OutputId(1));
    let identify = named_button(&mut canvas, "Identify");
    reveal(&mut canvas, identify);
    click(&mut canvas, identify, size);
    bridge(
        &mut canvas,
        &mut server,
        &receiver,
        &directory.path().join("park.json"),
    );
    let identified = settle(&mut canvas);
    let after = output_pixels(&identified, OutputId(1));
    let white = pixels
        .chunks_exact(4)
        .zip(after.chunks_exact(4))
        .filter(|(before, after)| {
            before[..3].iter().all(|channel| *channel < 100)
                && after[..3].iter().all(|channel| *channel > 180)
        })
        .count();
    assert!(
        white > 100,
        "The identification label did not reach the focused output: {white}"
    );
    output_capture(&identified, OutputId(1), "monitors-identify-first");
    server.advance_display(server.display_snapshot().identification.unwrap().deadline);
    publish(&mut canvas, &server);
    let second = named_button(&mut canvas, "Park-2");
    reveal(&mut canvas, second);
    click(&mut canvas, second, size);
    for narrow in [[640, 640], [360, 720]] {
        eprintln!("Monitor fixture: narrow layout {}", narrow[0]);
        assert!(canvas.resize(narrow).is_err());
        let mut catalogue = server.display_snapshot().heads;
        let mode = OutputMode {
            size: narrow.map(|size| size as i32),
            refresh_mhz: 60000,
            preferred: false,
        };
        catalogue
            .iter_mut()
            .find(|head| head.id == OutputId(1))
            .unwrap()
            .modes
            .push(mode.clone());
        server.output_catalog(catalogue).unwrap();
        let snapshot = server.display_snapshot();
        let mut heads = snapshot.configuration();
        heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap().mode = mode;
        sender
            .try_send(HostRequest::Display(DisplayRequest {
                id: snapshot.last_request + 1,
                action: DisplayAction::Preview {
                    revision: snapshot.revision,
                    heads,
                },
            }))
            .unwrap();
        bridge(
            &mut canvas,
            &mut server,
            &receiver,
            &directory.path().join("park.json"),
        );
        let snapshot = server.display_snapshot();
        sender
            .try_send(HostRequest::Display(DisplayRequest {
                id: snapshot.last_request + 1,
                action: DisplayAction::Keep {
                    token: snapshot.preview.unwrap().token,
                },
            }))
            .unwrap();
        bridge(
            &mut canvas,
            &mut server,
            &receiver,
            &directory.path().join("park.json"),
        );
        let frame = settle(&mut canvas);
        capture(&format!("monitors-narrow-{}", narrow[0]), narrow, &frame);
        for caption in ["Apply", "Keep changes", "Revert", "Close"] {
            let entity = named_button(&mut canvas, caption);
            let point = canvas
                .app
                .world()
                .get::<UiGlobalTransform>(entity)
                .unwrap()
                .transform_point2(Vec2::ZERO);
            assert!(
                point.x >= 0.0
                    && point.x < narrow[0] as f32
                    && point.y >= 0.0
                    && point.y < narrow[1] as f32,
                "Monitor footer control is outside the viewport: {caption} {point:?}"
            );
        }
    }
    key(&mut canvas, KeyCode::Escape, true);
    key(&mut canvas, KeyCode::Escape, false);
    assert!(!settle(&mut canvas).input.keyboard_modal);
    assert!(receiver.try_recv().is_err());
    assert_eq!(server.display_snapshot().heads[0].scale, 1.0);
    assert_eq!(server.display_snapshot().heads[1].scale, 1.25);
}
