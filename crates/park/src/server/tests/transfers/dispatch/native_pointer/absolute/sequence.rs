use super::*;
use super::super::super::{configuration, opening};
use lince_interface::host::{OutputId, WorkspaceLayout};

const A: OutputId = OutputId(1);
const B: OutputId = OutputId(2);
const A_NAME: &str = "Absolute-A";
const B_NAME: &str = "Absolute-B";

mod retirement;
mod lifetimes;

struct Scene {
    fixture: Fixture,
    field: Entity,
    window: ClientWindow,
    surface: SurfaceId,
    devices: [AbsoluteDevice; 2],
    output_a: wl_output::WlOutput,
    _buffer: wl_buffer::WlBuffer,
}

fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> Scene {
    let a = Monitor { id: A, name: A_NAME.into(), origin: [40, 40], size: [640, 480] };
    let b = Monitor { id: B, name: B_NAME.into(), origin: [700, 80], size: [800, 600] };
    let mut server = Server::new([1800, 1000]).unwrap();
    server.configure_scaled_outputs(&[a.clone()], &std::collections::BTreeMap::from([(A, 1.25)])).unwrap();
    let mut harness = Harness::connect(server);
    let output_a = harness.client.output.as_ref().unwrap().clone();
    harness.server.configure_scaled_outputs(&[a, b], &std::collections::BTreeMap::from([(A, 1.25), (B, 2.0)])).unwrap();
    harness.server.registered_output_catalog().unwrap(); harness.sync();
    let mut fixture = Fixture::with_size(harness, runtime, directory, [1800, 1000]);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 2)));
    fixture.desktop.send(Input::Output(A)).unwrap(); fixture.inspect(|_| ());
    let (window, buffer, surface) = opening::map(&mut fixture);
    window.role.set_max_size(240, 180); window.surface.commit(); fixture.exchange();
    fixture.desktop.send(Input::Output(B)).unwrap(); fixture.inspect(|_| ());
    let workspace = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().outputs.iter().find(|output| output.id == B).unwrap().workspace;
    let field = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspaces = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
        for workspace in workspaces { native::workspace_config::set_physics(world, root, workspace, false); }
        world.get_mut::<native::topology::view::View>(root).unwrap().spatial = false;
        let owner = native::sand_store::spawn_sand(world, root, workspace, native::sand_store::SandKind::EditableText,
            "Absolute sequence editor", bevy::math::DVec2::ZERO);
        let field = world.get::<native::sand_store::StoredSand>(owner).unwrap().content.unwrap();
        world.get_mut::<bevy::text::EditableText>(field).unwrap().editor.set_text("");
        field
    });
    fixture.desktop.send(Input::Action(DesktopAction::SetLayout(WorkspaceLayout::Tiled))).unwrap(); fixture.inspect(|_| ());
    let mut touch = device();
    touch.id = "sequence-touch".into(); touch.key = Some(touch.id.clone()); touch.generation = 1001;
    touch.name = "Sequence touch A".into(); touch.associated_output = Some(B_NAME.into()); touch.tablet = false;
    let mut pen = device();
    pen.id = "sequence-pen".into(); pen.key = Some(pen.id.clone()); pen.generation = 2002;
    pen.name = "Sequence pen B".into(); pen.associated_output = Some(A_NAME.into()); pen.touch = false;
    let devices = [touch, pen];
    install_devices(&mut fixture, &devices, 1);
    assert!(fixture.desktop.server.tablet_device(devices[1].generation, TabletDescriptor { name: devices[1].name.clone(), usb_id: None, syspath: None }));
    let qh = fixture.queue.handle();
    let _seat = fixture.client.tablet.manager.as_ref().unwrap().get_tablet_seat(fixture.client.seat.as_ref().unwrap(), &qh, ());
    let _touch = fixture.client.seat.as_ref().unwrap().get_touch(&qh, ()); fixture.exchange();
    fixture.inspect(|_| ());
    Scene { fixture, field, window, surface, devices, output_a, _buffer: buffer }
}

fn install_devices(fixture: &mut Fixture, devices: &[AbsoluteDevice], revision: u64) {
    fixture.desktop.absolute_settings(AbsoluteSnapshot { available: true, revision,
        settings: devices.iter().map(|device| (device.key.clone().unwrap(), AbsolutePreference { output: device.associated_output.clone(),
            buttons: std::collections::BTreeMap::from([(331, 272)]), ..Default::default() })).collect(), devices: devices.to_vec(), ..Default::default() }).unwrap();
    fixture.inspect(|_| ());
}

fn native_point(scene: &mut Scene) -> [f64; 2] {
    let field = scene.field;
    let (output, points) = scene.fixture.inspect(move |canvas| canvas.native_test_points(field)).expect("The native editor must have a rendered input projection");
    assert_eq!(output, B);
    let logical = std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() / 4.0);
    let frame = &scene.fixture.desktop.current.as_ref().unwrap().frame;
    let mapping = frame.input.mappings.get(B).unwrap();
    assert!(frame.input.native_pointer.target(logical).is_some());
    let physical = mapping.logical_to_physical(logical).unwrap();
    std::array::from_fn(|axis| physical[axis] / f64::from(mapping.physical_size()[axis]))
}

fn application_point(scene: &Scene) -> [f64; 2] {
    let frame = &scene.fixture.desktop.current.as_ref().unwrap().frame;
    let item = frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Application(scene.surface)).unwrap();
    assert_eq!(item.output, A);
    let logical = item.mapping.canvas_point(std::array::from_fn(|axis| item.mapping.surface.origin()[axis] + item.mapping.surface.size()[axis] * 0.5)).unwrap();
    let mapping = frame.input.mappings.get(A).unwrap();
    let physical = mapping.logical_to_physical(logical).unwrap();
    std::array::from_fn(|axis| physical[axis] / f64::from(mapping.physical_size()[axis]))
}

fn pen(fixture: &mut Fixture, device: &AbsoluteDevice, point: [f64; 2], phase: TabletPhase) {
    assert_eq!(fixture.desktop.tablet_event(crate::session::TabletReport { id: 92, device: device.clone(),
        descriptor: TabletToolDescriptor { tool_type: TabletToolType::Pen, hardware_serial: 92, hardware_id_wacom: 0,
            capabilities: TabletToolCapabilities::PRESSURE | TabletToolCapabilities::TILT },
        axes: TabletAxes { pressure: Some(0.5), tilt: Some([-12.0, 17.0]), ..Default::default() }, kind: TabletKind::Pen,
        phase, normalized: point, time: 200, proximity: phase != TabletPhase::Leave,
        held: matches!(phase, TabletPhase::Tip(true) | TabletPhase::Button { pressed: true, .. }) }), None);
}

fn key(fixture: &mut Fixture, code: u32) {
    fixture.desktop.key(code, true, 201).unwrap(); fixture.desktop.key(code, false, 202).unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; supplied device reports do not prove physical libinput behavior"]
fn successive_devices_type_into_their_own_native_client_and_layer_targets_across_outputs_despite_an_earlier_output_receipt_and_full_worker_queue() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut scene = create(&runtime, directory.path());
    for layer in [false, true] {
        let overlay = if layer {
            let qh = scene.fixture.queue.handle();
            let surface = scene.fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
            let role = scene.fixture.client.layer_shell.as_ref().unwrap().get_layer_surface(&surface, Some(&scene.output_a),
                zwlr_layer_shell_v1::Layer::Overlay, "absolute-sequence-layer".into(), &qh, ());
            role.set_size(180, 100); role.set_exclusive_zone(-1);
            role.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left); role.set_margin(30, 0, 0, 50);
            role.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::OnDemand);
            surface.commit(); scene.fixture.exchange();
            let buffer = super::super::super::privacy::buffer(&scene.fixture, [180, 100], [30, 90, 150, 255]);
            surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, 180, 100); surface.commit();
            scene.fixture.until(|fixture| fixture.desktop.layers.iter().any(|layer| layer.namespace == "absolute-sequence-layer"));
            Some((surface, role, buffer))
        } else { None };
        for pressure in [false, true] {
            let field = scene.field;
            scene.fixture.inspect(move |canvas| canvas.world_mut().get_mut::<bevy::text::EditableText>(field).unwrap().editor.set_text(""));
            configuration::application_input(&mut scene.fixture, scene.surface, &scene.window);
            let native = native_point(&mut scene);
            let other = if overlay.is_some() { [(50.0 + 90.0) * 1.25 / 640.0, (30.0 + 50.0) * 1.25 / 480.0] } else { application_point(&scene) };
            let pen_device = scene.devices[1].clone();
            pen(&mut scene.fixture, &pen_device, other, TabletPhase::Enter);
            scene.fixture.desktop.flush_touch().unwrap();
            scene.fixture.exchange();
            assert!(!scene.fixture.desktop.absolute_waiting(), "Hover alone acquired a focus consumption barrier");
            assert_eq!(scene.fixture.client.keyboard_focus.as_ref(), Some(&scene.window.surface), "Hover alone changed the keyboard recipient");
            pen(&mut scene.fixture, &pen_device, other, TabletPhase::Leave);
            let earlier = scene.fixture.desktop.worker.input(Input::Output(A)).unwrap();
            scene.fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.input_serial >= earlier));
            scene.fixture.client.key_recipients.clear();
            let mut resume = if pressure { let (_, release) = scene.fixture.pause(); fill(&scene.fixture, 0, 0); Some(Resume(Some(release))) } else { None };
            let touch = scene.devices[0].clone();
            scene.fixture.desktop.touch_down(&touch, 0, native, 210); scene.fixture.desktop.flush_touch().unwrap();
            key(&mut scene.fixture, 30);
            scene.fixture.desktop.touch_up(&touch, 0, 211);
            pen(&mut scene.fixture, &pen_device, other, TabletPhase::Enter);
            pen(&mut scene.fixture, &pen_device, other, TabletPhase::Tip(true)); scene.fixture.desktop.flush_touch().unwrap();
            key(&mut scene.fixture, 48);
            pen(&mut scene.fixture, &pen_device, other, TabletPhase::Tip(false)); pen(&mut scene.fixture, &pen_device, other, TabletPhase::Leave);
            assert!(scene.fixture.desktop.absolute_waiting());
            assert!(scene.fixture.client.key_recipients.is_empty());
            assert!(scene.fixture.desktop.keys.is_empty());
            if let Some(release) = resume.as_mut().and_then(|resume| resume.0.take()) { release.send(()).unwrap(); }
            scene.fixture.until(|fixture| !fixture.desktop.input_pending());
            scene.fixture.exchange();
            let text = scene.fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(field).unwrap().value().to_string());
            assert_eq!(text, "a");
            let recipient = overlay.as_ref().map_or_else(|| scene.window.surface.id(), |(surface, _, _)| surface.id());
            assert_eq!(scene.fixture.client.key_recipients, [(Some(recipient.clone()), 48, true), (Some(recipient), 48, false)]);
            assert!(scene.fixture.desktop.keys.is_empty());
            assert!(!scene.fixture.desktop.absolute_waiting());
            assert!(scene.fixture.desktop.running);
        }
        if let Some((surface, role, buffer)) = overlay { role.destroy(); surface.destroy(); buffer.destroy(); scene.fixture.exchange(); }
    }
    scene.fixture.desktop.worker.stop().unwrap();
}
