use super::*;
use super::{capture_lifetime, configuration, privacy};
use crate::{launch::Capability, session::{PowerFrame, PowerState, power_request_current}};
use wayland_protocols_wlr::output_power_management::v1::client::zwlr_output_power_v1::{self as protocol, ZwlrOutputPowerV1 as Power};

struct Peer {
    connection: Connection,
    queue: EventQueue<Client>,
    client: Client,
}

impl Peer {
    fn connect(fixture: &mut Fixture) -> Self {
        let (server, socket) = std::os::unix::net::UnixStream::pair().unwrap();
        fixture.desktop.server.insert_authorized_client(server, vec![Capability::OutputControl], false).unwrap();
        let connection = Connection::from_socket(socket).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut peer = Self { connection, queue, client: Client::default() };
        peer.exchange(fixture);
        assert!(peer.client.power.manager.is_some());
        assert!(peer.client.capture.manager.is_none());
        peer
    }

    fn exchange(&mut self, fixture: &mut Fixture) {
        synchronize(&mut fixture.desktop.server, &self.connection, &mut self.queue, &mut self.client).unwrap();
    }

    fn control(&mut self, fixture: &mut Fixture) -> Power {
        let control = self.client.power.manager.as_ref().unwrap().get_output_power(self.client.output.as_ref().unwrap(), &self.queue.handle(), ());
        self.exchange(fixture);
        control
    }

    fn request(&mut self, fixture: &mut Fixture, control: &Power, on: bool) -> crate::server::PowerRequest {
        control.set_mode(if on { protocol::Mode::On } else { protocol::Mode::Off });
        self.exchange(fixture);
        let mut requests = fixture.desktop.server.output_power_requests();
        assert_eq!(requests.len(), 1);
        requests.pop().unwrap()
    }
}

fn admission_only(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> (Fixture, ClientWindow, wl_buffer::WlBuffer, SurfaceId) {
    let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::Capture, Capability::WindowList]));
    harness.server.registered_output_catalog().unwrap();
    harness.server.output_power_backend(true);
    harness.server.output_power_state(OutputId(1), Some(true));
    let window = harness.window();
    let buffer = paint(&mut harness, &window, [320, 240], [255, 0, 255, 255]);
    let id = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, runtime, directory);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().any(|window| window.id == id)));
    fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
        let workspaces = world.get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|workspace| workspace.id).collect::<Vec<_>>();
        for workspace in workspaces { native::workspace_config::set_physics(world, root, workspace, false); }
    });
    (fixture, window, buffer, id)
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; admission-only backend state does not prove physical DPMS"]
fn a_real_power_peer_keeps_the_original_deadline_while_gpu_readback_and_ordinary_input_progress_then_a_fresh_authorized_peer_recovers() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, window, _buffer, id) = admission_only(&runtime, directory.path());
    let before = fixture.desktop.server.display_snapshot().configuration();
    let (source, session) = capture_lifetime::output_session(&mut fixture);
    let pending = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
    capture_lifetime::submitted(&mut fixture, &pending);
    let mut peer = Peer::connect(&mut fixture);
    let control = peer.control(&mut fixture);
    let modes = peer.client.power.modes.clone();
    let admitted = Instant::now();
    let request = peer.request(&mut fixture, &control, false);
    let deadline = request.deadline;
    assert!(deadline > admitted && deadline <= Instant::now() + Duration::from_secs(2));
    assert!(power_request_current(&fixture.desktop.server, &request, Instant::now()));
    configuration::application_input(&mut fixture, id, &window);
    while Instant::now() < deadline {
        fixture.tick(); peer.exchange(&mut fixture);
        assert_eq!(request.deadline, deadline);
        assert_eq!(peer.client.power.modes, modes, "An admitted request was reported as an observed physical mode");
        assert!(peer.client.power.failed.is_empty());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!power_request_current(&fixture.desktop.server, &request, Instant::now()));
    fixture.desktop.server.output_power_failed(request); peer.exchange(&mut fixture);
    assert_eq!(peer.client.power.failed, [control.id().protocol_id()]);
    capture_lifetime::rejected(&mut fixture, &pending); pending.destroy(&mut fixture);
    assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before);
    assert!(fixture.desktop.server.windows().iter().any(|window| window.id == id && window.mapped));
    let mut fresh = Peer::connect(&mut fixture);
    let current = fresh.control(&mut fixture);
    let request = fresh.request(&mut fixture, &current, true);
    assert!(power_request_current(&fixture.desktop.server, &request, Instant::now()));
    control.destroy(); peer.exchange(&mut fixture);
    assert!(power_request_current(&fixture.desktop.server, &request, Instant::now()));
    current.destroy(); fresh.exchange(&mut fixture);
    assert!(!power_request_current(&fixture.desktop.server, &request, Instant::now()));
    fixture.desktop.server.output_power_failed(request); fresh.exchange(&mut fixture);
    assert!(fresh.client.power.failed.is_empty());
    let captured = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
    let pixels = capture_lifetime::completed(&mut fixture, &captured);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[0] > 235 && pixel[1] < 20 && pixel[2] > 235));
    captured.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
    configuration::application_input(&mut fixture, id, &window);
    assert!(fixture.desktop.running);
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; rendered protection and frame admission do not prove physical DPMS"]
fn delayed_gpu_frame_owners_retire_across_lock_unlock_output_and_power_epochs_and_current_protected_pixels_and_input_recover() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, window, _buffer, id) = admission_only(&runtime, directory.path());
    let before = fixture.desktop.server.display_snapshot().configuration();
    let unlocked = PowerFrame::capture(&fixture.desktop.server, OutputId(1), 11).unwrap();
    assert!(unlocked.current(&fixture.desktop.server, OutputId(1), 11));
    let (source, session) = capture_lifetime::output_session(&mut fixture);
    let pending = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
    capture_lifetime::submitted(&mut fixture, &pending);
    let qh = fixture.queue.handle();
    let lock = fixture.client.lock_manager.as_ref().unwrap().lock(&qh, ()); fixture.exchange();
    assert!(fixture.desktop.server.locked());
    assert!(!unlocked.current(&fixture.desktop.server, OutputId(1), 11));
    let protected = PowerFrame::capture(&fixture.desktop.server, OutputId(1), 11).unwrap();
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let role = lock.get_lock_surface(&surface, fixture.client.output.as_ref().unwrap(), &qh, ()); fixture.exchange();
    let size = *fixture.client.lock_sizes.last().unwrap();
    let buffer = privacy::buffer(&fixture, size, [40, 80, 20, 255]);
    surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32); surface.commit();
    fixture.until(|fixture| fixture.client.lock_replies.last() == Some(&true));
    capture_lifetime::rejected(&mut fixture, &pending); pending.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
    let (_, pixels) = fixture.output_pixels(OutputId(1));
    assert!(pixels.chunks_exact(4).all(|pixel| pixel[0].abs_diff(20) <= 1 && pixel[1].abs_diff(80) <= 1 && pixel[2].abs_diff(40) <= 1 && pixel[3] == 255),
        "The first rendered protected output retained unlocked application pixels");
    assert!(protected.current(&fixture.desktop.server, OutputId(1), 11));
    assert!(!protected.current(&fixture.desktop.server, OutputId(1), 12));
    assert!(!protected.current(&fixture.desktop.server, OutputId(2), 11));
    let mut peer = Peer::connect(&mut fixture);
    let control = peer.control(&mut fixture);
    let request = peer.request(&mut fixture, &control, true);
    assert!(power_request_current(&fixture.desktop.server, &request, Instant::now()));
    fixture.client.key_recipients.clear();
    fixture.desktop.key(30, true, 70).unwrap(); fixture.desktop.key(30, false, 71).unwrap(); fixture.tick();
    assert_eq!(fixture.client.key_recipients, [(Some(surface.id()), 30, true), (Some(surface.id()), 30, false)]);
    fixture.desktop.server.output_power_failed(request); peer.exchange(&mut fixture);
    assert!(fixture.desktop.server.locked());
    assert_eq!(peer.client.power.failed, [control.id().protocol_id()]);
    lock.unlock_and_destroy(); fixture.until(|fixture| !fixture.desktop.server.locked());
    assert!(!protected.current(&fixture.desktop.server, OutputId(1), 11));
    assert!(!unlocked.current(&fixture.desktop.server, OutputId(1), 11), "A complete lock/unlock transition revived a pre-lock GPU frame");
    let current = PowerFrame::capture(&fixture.desktop.server, OutputId(1), 11).unwrap();
    assert!(current.current(&fixture.desktop.server, OutputId(1), 11));
    assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before);
    role.destroy(); surface.destroy(); buffer.destroy(); control.destroy(); peer.exchange(&mut fixture); fixture.exchange();
    configuration::application_input(&mut fixture, id, &window);
    let mut old_peer = Peer::connect(&mut fixture);
    let old_control = old_peer.control(&mut fixture);
    let obsolete = old_peer.request(&mut fixture, &old_control, false);
    fixture.desktop.server.disconnect_outputs(); fixture.tick();
    assert!(!current.current(&fixture.desktop.server, OutputId(1), 11));
    old_peer.exchange(&mut fixture);
    assert!(!power_request_current(&fixture.desktop.server, &obsolete, Instant::now()));
    assert_eq!(old_peer.client.power.failed, [old_control.id().protocol_id()]);
    fixture.desktop.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Replacement".into(), origin: [0, 0], size: [1000, 720] }]).unwrap();
    fixture.desktop.server.registered_output_catalog().unwrap(); fixture.desktop.invalidate_outputs().unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry| geometry.outputs.len() == 1)));
    assert!(!current.current(&fixture.desktop.server, OutputId(1), 11), "A same-ID replacement inherited an earlier output's prepared frame");
    assert!(PowerFrame::capture(&fixture.desktop.server, OutputId(1), 12).unwrap().current(&fixture.desktop.server, OutputId(1), 12));
    fixture.desktop.server.output_power_state(OutputId(1), Some(true));
    let mut replacement = Peer::connect(&mut fixture);
    let control = replacement.control(&mut fixture);
    let fresh = replacement.request(&mut fixture, &control, true);
    fixture.desktop.server.output_power_failed(obsolete); replacement.exchange(&mut fixture);
    assert!(replacement.client.power.failed.is_empty());
    assert!(power_request_current(&fixture.desktop.server, &fresh, Instant::now()));
    control.destroy(); replacement.exchange(&mut fixture);
    assert!(!power_request_current(&fixture.desktop.server, &fresh, Instant::now()));
    fixture.desktop.server.output_power_failed(fresh);
    old_control.destroy(); old_peer.exchange(&mut fixture);
    assert_eq!(old_peer.client.power.failed.len(), 1);
    assert!(fixture.desktop.server.windows().iter().any(|window| window.id == id && window.mapped));
    configuration::application_input(&mut fixture, id, &window);
    assert!(fixture.desktop.running);
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; request cancellation cycles do not prove physical Off/On observations"]
fn twenty_separately_authorized_power_peer_replacements_retire_requests_preserve_windows_and_bar_reservations_and_recover_capture_and_input() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let (mut fixture, window, _buffer, id) = admission_only(&runtime, directory.path());
    let qh = fixture.queue.handle();
    let bar = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let role = fixture.client.layer_shell.as_ref().unwrap().get_layer_surface(&bar, fixture.client.output.as_ref(),
        zwlr_layer_shell_v1::Layer::Top, "power-reservation-fixture".into(), &qh, ());
    role.set_size(120, 24); role.set_anchor(zwlr_layer_surface_v1::Anchor::Top); role.set_exclusive_zone(24);
    bar.commit(); fixture.exchange();
    let buffer = privacy::buffer(&fixture, [120, 24], [30, 30, 30, 255]);
    bar.attach(Some(&buffer), 0, 0); bar.damage_buffer(0, 0, 120, 24); bar.commit();
    fixture.until(|fixture| fixture.desktop.layers.iter().any(|layer| layer.namespace == "power-reservation-fixture"));
    let before = fixture.desktop.server.display_snapshot();
    assert_ne!(before.outputs[0].usable, Some(before.outputs[0].bounds));
    let mut baseline = None;
    let mut samples = Vec::new();
    for cycle in 0..20 {
        let mut peer = Peer::connect(&mut fixture);
        let control = peer.control(&mut fixture);
        let off = peer.request(&mut fixture, &control, false);
        let on = peer.request(&mut fixture, &control, true);
        assert!(!power_request_current(&fixture.desktop.server, &off, Instant::now()));
        assert!(power_request_current(&fixture.desktop.server, &on, Instant::now()));
        fixture.desktop.server.output_power_failed(off); peer.exchange(&mut fixture);
        assert!(peer.client.power.failed.is_empty());
        let owner = PowerFrame::capture(&fixture.desktop.server, OutputId(1), 1).unwrap();
        let (source, session) = capture_lifetime::output_session(&mut fixture);
        let pending = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
        capture_lifetime::submitted(&mut fixture, &pending);
        rustix::net::shutdown(&peer.connection, rustix::net::Shutdown::Both).unwrap();
        fixture.until(|fixture| !power_request_current(&fixture.desktop.server, &on, Instant::now()));
        fixture.desktop.server.output_power_failed(on);
        assert!(owner.current(&fixture.desktop.server, OutputId(1), 1), "A disconnected helper retired another owner's ordinary frame");
        let pixels = capture_lifetime::completed(&mut fixture, &pending);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[0] > 235 && pixel[1] < 20 && pixel[2] > 235));
        pending.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
        let mut restarted = Peer::connect(&mut fixture);
        let fresh = restarted.control(&mut fixture);
        let request = restarted.request(&mut fixture, &fresh, true);
        assert!(power_request_current(&fixture.desktop.server, &request, Instant::now()));
        let mut power = PowerState::with_waiting(request);
        let prepared = PowerFrame::capture(&fixture.desktop.server, OutputId(1), power.epoch).unwrap();
        let queued = cycle % 2 == 0;
        power.pause(&mut fixture.desktop.server, queued);
        assert_eq!(power.epoch, 1);
        assert_eq!(power.on, queued);
        assert!(!power.confirmed_on(), "Intent or a paused pending wake cannot complete a fresh On request without a matching observation");
        assert!(!prepared.current(&fixture.desktop.server, OutputId(1), power.epoch));
        restarted.exchange(&mut fixture);
        assert_eq!(restarted.client.power.failed, [fresh.id().protocol_id()]);
        fresh.destroy(); restarted.exchange(&mut fixture);
        assert_eq!(restarted.client.power.failed.len(), 1);
        rustix::net::shutdown(&restarted.connection, rustix::net::Shutdown::Both).unwrap();
        fixture.until(|fixture| fixture.desktop.server.power_test_resources() == [0; 3]);
        assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before.configuration());
        assert_eq!(fixture.desktop.server.display_snapshot().outputs, before.outputs);
        assert!(fixture.desktop.server.windows().iter().any(|window| window.id == id && window.mapped));
        assert!(fixture.desktop.layers.iter().any(|layer| layer.namespace == "power-reservation-fixture"));
        configuration::application_input(&mut fixture, id, &window);
        drop(control); drop(fresh); drop(peer); drop(restarted);
        let descriptors = fixture.inspect(|canvas| canvas.capture_test_resources().descriptors);
        let actual = (fixture.composition.color_test_resources(), descriptors);
        samples.push(serde_json::json!({ "cycle": cycle, "queued": queued, "epoch_after_pause": power.epoch,
            "on_intent_after_pause": power.on, "power_resources": fixture.desktop.server.power_test_resources(),
            "gles_resources": actual.0, "descriptors": descriptors, "window": id.0,
            "bar_retained": true, "ordinary_input_recovered": true }));
        if let Some(expected) = baseline { assert_eq!(actual, expected, "Settled helper lifetimes retained compositor GPU resources"); }
        else { baseline = Some(actual); }
        assert!(fixture.desktop.server.output_power_requests().is_empty());
        assert!(fixture.desktop.running);
    }
    role.destroy(); bar.destroy(); buffer.destroy(); fixture.exchange();
    assert!(!fixture.desktop.server.locked());
    configuration::application_input(&mut fixture, id, &window);
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("power-helper-lifetimes.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "scope": "software admission, pause and GPU/connection ownership; not physical DPMS", "cycles": samples,
        })).unwrap()).unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; preview and protection admission do not prove physical DPMS"]
fn power_admission_during_scale_preview_and_color_change_keeps_lock_rollback_protected_pixels_original_deadlines_and_fresh_capture() {
    use lince_interface::host::display::{DisplayAction, DisplayRequest, ColorStatus};
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park.json");
    let original = br#"{"keyboard":{"layout":"br"}}"#;
    std::fs::write(&path, original).unwrap();
    let (mut fixture, window, _buffer, id) = admission_only(&runtime, directory.path());
    fixture.desktop.commands(path.clone(), directory.path().join("wayland-power-fixture"));
    let before = fixture.desktop.server.display_snapshot();
    let owner = PowerFrame::capture(&fixture.desktop.server, OutputId(1), 0).unwrap();
    let (source, session) = capture_lifetime::output_session(&mut fixture);
    let captured = Instant::now();
    let pending = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
    capture_lifetime::submitted(&mut fixture, &pending);
    let mut heads = before.configuration();
    heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap().scale = 1.25;
    fixture.desktop.server.display_request(DisplayRequest { id: before.last_request + 1,
        action: DisplayAction::Preview { revision: before.revision, heads } }, |_| panic!("A preview cannot persist output settings"));
    fixture.until(|fixture| fixture.desktop.server.display_snapshot().preview.is_some()
        && fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
            geometry.outputs.iter().any(|output| output.id == OutputId(1) && output.raster.scale == [1.25; 2]))));
    assert!(!owner.current(&fixture.desktop.server, OutputId(1), 0));
    capture_lifetime::rejected(&mut fixture, &pending); pending.destroy(&mut fixture);
    assert!(captured.elapsed() < Duration::from_secs(2), "Preview retirement reached capture expiry instead of proving mapping invalidation");
    let captured = Instant::now();
    let pending = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
    capture_lifetime::submitted(&mut fixture, &pending);
    let revision = fixture.desktop.server.display_snapshot().color[&OutputId(1)].revision + 1;
    fixture.desktop.server.output_color(OutputId(1), ColorStatus { revision, prepared: Some(Default::default()),
        effective: Some(Default::default()), ..Default::default() });
    capture_lifetime::rejected(&mut fixture, &pending); pending.destroy(&mut fixture);
    assert!(captured.elapsed() < Duration::from_secs(2), "Color retirement reached capture expiry instead of proving generation invalidation");
    let mut peer = Peer::connect(&mut fixture);
    let control = peer.control(&mut fixture);
    let request = peer.request(&mut fixture, &control, false);
    let deadline = request.deadline;
    let preview_owner = PowerFrame::capture(&fixture.desktop.server, OutputId(1), 0).unwrap();
    let pending = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
    capture_lifetime::submitted(&mut fixture, &pending);
    let qh = fixture.queue.handle();
    let lock = fixture.client.lock_manager.as_ref().unwrap().lock(&qh, ()); fixture.exchange();
    fixture.until(|fixture| fixture.desktop.server.display_snapshot().preview.is_none()
        && fixture.desktop.server.display_snapshot().configuration() == before.configuration());
    assert!(fixture.desktop.server.locked());
    assert!(!preview_owner.current(&fixture.desktop.server, OutputId(1), 0));
    assert_eq!(request.deadline, deadline);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    capture_lifetime::rejected(&mut fixture, &pending); pending.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
    assert!(matches!(fixture.client.capture.failures.last(), Some(wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_frame_v1::FailureReason::Stopped)),
        "The lock case must prove protection retirement rather than pass on capture expiry");
    let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let role = lock.get_lock_surface(&surface, fixture.client.output.as_ref().unwrap(), &qh, ()); fixture.exchange();
    let size = *fixture.client.lock_sizes.last().unwrap();
    let buffer = privacy::buffer(&fixture, size, [90, 30, 10, 255]);
    surface.attach(Some(&buffer), 0, 0); surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32); surface.commit(); fixture.exchange();
    let server_surface = fixture.desktop.server.lock_surfaces()[0].2[0].surface.clone();
    let version = fixture.desktop.server.lock_surfaces()[0].2[0].content.version;
    fixture.until(|fixture| fixture.composition.color_test_surface_ready(&server_surface, version));
    let (_, pixels) = fixture.output_pixels(OutputId(1));
    assert!(pixels.chunks_exact(4).all(|pixel| pixel[0].abs_diff(10) <= 1 && pixel[1].abs_diff(30) <= 1 && pixel[2].abs_diff(90) <= 1 && pixel[3] == 255),
        "Rollback/first protected rendering reused pre-lock or preview pixels");
    fixture.desktop.server.output_power_failed(request); peer.exchange(&mut fixture);
    assert_eq!(peer.client.power.failed, [control.id().protocol_id()]);
    assert!(fixture.desktop.server.locked());
    fixture.until(|fixture| fixture.client.lock_replies.last() == Some(&true));
    lock.unlock_and_destroy(); fixture.until(|fixture| !fixture.desktop.server.locked());
    role.destroy(); surface.destroy(); buffer.destroy(); control.destroy(); peer.exchange(&mut fixture); fixture.exchange();
    let (source, session) = capture_lifetime::output_session(&mut fixture);
    let fresh = capture_lifetime::Pending::new(&mut fixture, &session, [1000, 720]);
    let pixels = capture_lifetime::completed(&mut fixture, &fresh);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[0] > 235 && pixel[1] < 20 && pixel[2] > 235));
    fresh.destroy(&mut fixture); session.destroy(); source.destroy(); fixture.exchange();
    configuration::application_input(&mut fixture, id, &window);
    assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before.configuration());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(fixture.desktop.running);
}
