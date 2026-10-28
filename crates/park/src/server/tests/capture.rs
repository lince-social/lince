use super::*;
use wayland_protocols::ext::{
    image_capture_source::v1::client::{ext_image_capture_source_v1::ExtImageCaptureSourceV1 as Source,
        ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1 as OutputFactory,
        ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1 as ToplevelFactory},
    image_copy_capture::v1::client::{ext_image_copy_capture_manager_v1::{self as manager, ExtImageCopyCaptureManagerV1 as Manager},
        ext_image_copy_capture_session_v1::{self as session, ExtImageCopyCaptureSessionV1 as Session},
        ext_image_copy_capture_cursor_session_v1::{self as cursor, ExtImageCopyCaptureCursorSessionV1 as Cursor},
        ext_image_copy_capture_frame_v1::{self as frame, ExtImageCopyCaptureFrameV1 as Frame}},
};

#[derive(Default)]
pub(super) struct Capture {
    pub(in crate::server::tests) outputs: Option<OutputFactory>,
    pub(in crate::server::tests) toplevels: Option<ToplevelFactory>,
    pub(in crate::server::tests) manager: Option<Manager>,
    pub(in crate::server::tests) sizes: Vec<[u32; 2]>,
    pub(in crate::server::tests) stopped: usize,
    pub(in crate::server::tests) failures: Vec<frame::FailureReason>,
    pub(in crate::server::tests) ready: usize,
    pub(in crate::server::tests) damage: Vec<[i32; 4]>,
    pub(in crate::server::tests) cursor_events: Vec<String>,
}

pub(super) fn bind(state: &mut Client, registry: &wl_registry::WlRegistry, name: u32, interface: &str, qh: &QueueHandle<Client>) {
    match interface {
        "ext_output_image_capture_source_manager_v1" => state.capture.outputs = Some(registry.bind(name, 1, qh, ())),
        "ext_foreign_toplevel_image_capture_source_manager_v1" => state.capture.toplevels = Some(registry.bind(name, 1, qh, ())),
        "ext_image_copy_capture_manager_v1" => state.capture.manager = Some(registry.bind(name, 1, qh, ())),
        _ => {}
    }
}

delegate_noop!(Client: ignore Source);
delegate_noop!(Client: ignore OutputFactory);
delegate_noop!(Client: ignore ToplevelFactory);
delegate_noop!(Client: ignore Manager);

impl Dispatch<Cursor, ()> for Client {
    fn event(state: &mut Self, _: &Cursor, event: cursor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let event = match event {
            cursor::Event::Enter => "enter".into(),
            cursor::Event::Leave => "leave".into(),
            cursor::Event::Position { x, y } => format!("position:{x},{y}"),
            cursor::Event::Hotspot { x, y } => format!("hotspot:{x},{y}"),
            _ => return,
        };
        state.capture.cursor_events.push(event);
    }
}

impl Dispatch<Session, ()> for Client {
    fn event(state: &mut Self, _: &Session, event: session::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            session::Event::BufferSize { width, height } => state.capture.sizes.push([width, height]),
            session::Event::Stopped => state.capture.stopped += 1,
            _ => {}
        }
    }
}

impl Dispatch<Frame, ()> for Client {
    fn event(state: &mut Self, _: &Frame, event: frame::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            frame::Event::Failed { reason } => state.capture.failures.push(reason.into_result().unwrap()),
            frame::Event::Ready => state.capture.ready += 1,
            frame::Event::Damage { x, y, width, height } => state.capture.damage.push([x, y, width, height]),
            _ => {}
        }
    }
}

fn harness() -> Harness {
    Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::Capture, crate::launch::Capability::WindowList]))
}

fn output_session(harness: &mut Harness) -> (Source, Session) {
    let qh = harness.queue.handle();
    let source = harness.client.capture.outputs.as_ref().unwrap().create_source(harness.client.output.as_ref().unwrap(), &qh, ());
    let session = harness.client.capture.manager.as_ref().unwrap().create_session(&source, manager::Options::empty(), &qh, ());
    harness.sync();
    (source, session)
}

fn request(harness: &mut Harness, session: &Session, size: [i32; 2]) -> Frame {
    let frame = session.create_frame(&harness.queue.handle(), ());
    let buffer = harness.buffer(size);
    frame.attach_buffer(&buffer);
    frame.damage_buffer(0, 0, size[0], size[1]);
    frame.capture();
    harness.sync();
    frame
}

#[test]
fn privacy_publication_retires_an_actual_queued_frame_without_stopping_the_output_session() {
    let mut harness = harness();
    let gate = harness.server.capture_policy_gate();
    gate.publish(lince_interface::host::capture::Policy::default());
    let (_source, session) = output_session(&mut harness);
    let frame = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    let active = harness.server.capture_status();
    assert_eq!(active.sessions.len(), 1);
    assert!(harness.server.capture_job_valid(&job));
    gate.invalidate();
    assert!(!harness.server.capture_job_valid(&job));
    let reason = harness.server.capture_job_failure(&job).unwrap();
    job.frame.fail(reason);
    harness.sync();
    assert_eq!(harness.client.capture.stopped, 0);
    assert!(harness.server.capture_status().sessions[0].paused);
    assert_eq!(harness.client.capture.failures.last(), Some(&frame::FailureReason::Stopped));
    assert_eq!(harness.client.capture.ready, 0);
    frame.destroy();
    session.destroy();
    harness.sync();
    assert!(harness.server.capture_status().sessions.is_empty());
}

#[test]
fn actual_capture_status_uses_connection_lifetimes_and_stop_retires_one_session() {
    let mut harness = harness();
    let (_a, first) = output_session(&mut harness);
    let (_b, second) = output_session(&mut harness);
    let status = harness.server.capture_status();
    assert_eq!(status.sessions.len(), 2);
    assert_eq!(status.sessions[0].connection, status.sessions[1].connection);
    assert_ne!(status.sessions[0].id, status.sessions[1].id);
    let survivor = status.sessions[1].id;
    harness.server.stop_capture_session(status.sessions[0].id);
    harness.sync();
    assert_eq!(harness.client.capture.stopped, 1);
    let status = harness.server.capture_status();
    assert_eq!(status.sessions.len(), 1);
    assert_eq!(status.sessions[0].id, survivor);
    first.destroy();
    second.destroy();
    harness.sync();
    assert!(harness.server.capture_status().sessions.is_empty());
    assert!(harness.server.capture_jobs().is_empty());
}

#[test]
fn a_private_native_window_source_stops_without_redirecting_to_a_public_window() {
    let mut harness = harness();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    harness.sync();
    let surface = harness.server.windows()[0].id;
    let gate = harness.server.capture_policy_gate();
    gate.publish(lince_interface::host::capture::Policy { windows: BTreeMap::from([(surface, false)]), excluded: BTreeSet::new(), ..Default::default() });
    let qh = harness.queue.handle();
    let source = harness.client.capture.toplevels.as_ref().unwrap().create_source(&harness.client.foreign_handles[0], &qh, ());
    let session = harness.client.capture.manager.as_ref().unwrap().create_session(&source, manager::Options::empty(), &qh, ());
    harness.sync();
    let frame = request(&mut harness, &session, [320, 240]);
    let job = harness.server.capture_jobs().pop().unwrap();
    gate.publish(lince_interface::host::capture::Policy { windows: BTreeMap::from([(surface, true)]), excluded: BTreeSet::new(), ..Default::default() });
    harness.sync();
    assert!(job.frame.has_failed());
    assert!(!harness.server.capture_job_valid(&job));
    assert_eq!(harness.client.capture.stopped, 1);
    assert_eq!(harness.client.capture.ready, 0);
    assert!(harness.server.capture_status().sessions.is_empty());
    frame.destroy();
    session.destroy();
    source.destroy();
    harness.sync();
}

#[test]
fn output_capture_waits_for_authoritative_policy_for_a_newly_mapped_window() {
    let mut harness = harness();
    let gate = harness.server.capture_policy_gate();
    gate.publish(lince_interface::host::capture::Policy::default());
    let (_source, session) = output_session(&mut harness);
    let session_id = harness.server.capture_status().sessions[0].id;
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    harness.sync();
    assert_eq!(harness.client.capture.stopped, 0);
    assert!(harness.server.capture_status().sessions[0].paused);
    assert_eq!(harness.server.capture_status().sessions[0].id, session_id);
    let surface = harness.server.windows()[0].id;
    gate.publish(lince_interface::host::capture::Policy { windows: BTreeMap::from([(surface, false)]), excluded: BTreeSet::new(), ..Default::default() });
    let frame = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert!(harness.server.capture_job_valid(&job));
    assert!(!harness.server.capture_status().sessions[0].paused);
    job.frame.fail(lince_smithay_capture::wayland::image_copy_capture::CaptureFailureReason::Unknown);
    frame.destroy();
    session.destroy();
    harness.sync();
}

#[test]
fn output_capture_demand_tracks_retained_jobs_and_private_output_uses_a_filtered_frame() {
    let mut harness = harness();
    let gate = harness.server.capture_policy_gate();
    gate.publish(lince_interface::host::capture::Policy { windows: BTreeMap::new(), excluded: BTreeSet::from([[11; 16]]), ..Default::default() });
    let (_source, session) = output_session(&mut harness);
    assert!(harness.server.capture_demand().is_empty());
    let frame = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert_eq!(harness.server.capture_demand().outputs, BTreeSet::from([OutputId(1)]));
    assert!(harness.server.capture_job_valid(&job));
    assert!(gate.read().unwrap().requires_filter());
    assert!(!job.mappings.is_empty());
    drop(job);
    harness.sync();
    assert!(harness.server.capture_demand().is_empty());
    frame.destroy();
    session.destroy();
    harness.sync();
}

#[test]
fn changing_output_mapping_retires_a_queued_frame_without_changing_its_buffer_size() {
    let mut harness = harness();
    let (_source, session) = output_session(&mut harness);
    let frame = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    harness.server.configure_outputs(&[super::super::Monitor { id: OutputId(1), name: "Capture display".into(), origin: [200, 30], size: [640, 480] }]).unwrap();
    assert!(!harness.server.capture_job_valid(&job));
    assert_eq!(job.size, [640, 480]);
    drop(job);
    frame.destroy();
    session.destroy();
    harness.sync();
}

#[test]
fn changing_only_color_generation_retires_queued_capture_without_stopping_the_session_or_changing_mapping() {
    use lince_interface::host::{color::{ColorMode, OutputColor}, display::ColorStatus};
    let mut harness = harness();
    harness.server.registered_output_catalog().unwrap();
    let before = harness.server.display_snapshot();
    let (_source, session) = output_session(&mut harness);
    let frame = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert!(harness.server.capture_job_valid(&job));
    let pipeline = OutputColor { mode: ColorMode::Hdr10, peak_nits: 1000 };
    let status = ColorStatus { hdr10: true, prepared: Some(pipeline), effective: Some(Default::default()),
        pending: true, revision: 2, ..Default::default() };
    harness.server.output_color(OutputId(1), status.clone());
    assert_eq!(harness.server.display_snapshot().revision, before.revision);
    assert_eq!(harness.server.display_snapshot().outputs, before.outputs);
    assert!(!harness.server.capture_job_valid(&job));
    let failure = harness.server.capture_job_failure(&job).unwrap();
    job.frame.fail(failure);
    harness.sync();
    frame.destroy();
    let next = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    harness.server.output_color(OutputId(1), ColorStatus { pending: false, effective: Some(pipeline), ..status });
    assert!(harness.server.capture_job_valid(&job));
    assert_eq!(harness.client.capture.stopped, 0);
    assert_eq!(harness.server.capture_status().sessions.len(), 1);
    job.frame.fail(lince_smithay_capture::wayland::image_copy_capture::CaptureFailureReason::Unknown);
    next.destroy(); session.destroy(); harness.sync();
}

#[test]
fn backend_power_off_retires_output_capture_and_wake_reuses_the_same_session_and_geometry() {
    let mut harness = harness();
    harness.server.capture_policy_gate().publish(lince_interface::host::capture::Policy::default());
    let before = harness.server.display_snapshot();
    let (_source, session) = output_session(&mut harness);
    let frame = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert!(harness.server.capture_job_valid(&job));
    harness.server.output_power_state(OutputId(1), Some(false));
    assert!(!harness.server.capture_job_valid(&job));
    assert!(harness.server.capture_status().sessions[0].paused);
    let failure = harness.server.capture_job_failure(&job).unwrap();
    job.frame.fail(failure); harness.sync();
    frame.destroy();
    let sleeping = request(&mut harness, &session, [640, 480]);
    assert!(harness.server.capture_jobs().is_empty());
    assert_eq!(harness.client.capture.failures.last(), Some(&frame::FailureReason::Unknown));
    assert_eq!(harness.client.capture.stopped, 0);
    sleeping.destroy();
    harness.server.output_power_state(OutputId(1), Some(true));
    let wake = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert!(harness.server.capture_job_valid(&job));
    assert!(!harness.server.capture_status().sessions[0].paused);
    assert_eq!(harness.server.display_snapshot().outputs, before.outputs);
    assert_eq!(harness.server.display_snapshot().revision, before.revision);
    job.frame.fail(lince_smithay_capture::wayland::image_copy_capture::CaptureFailureReason::Unknown);
    wake.destroy(); session.destroy(); harness.sync();
}

#[test]
fn capture_artwork_owner_follows_the_actual_focused_surface_of_a_shared_client() {
    let mut harness = harness();
    let first = harness.window();
    let _first = harness.attach(&first, [320, 240]);
    harness.sync();
    let first = harness.server.windows()[0].clone();
    let second = harness.window();
    let _second = harness.attach(&second, [320, 240]);
    harness.sync();
    let second = harness.server.windows().into_iter().find(|window| window.id != first.id).unwrap();
    assert_eq!(first.surface.client().unwrap().id(), second.surface.client().unwrap().id());
    for window in [first, second] {
        harness.server.pointer_motion(Some((window.surface, [10.0, 10.0])), 1).unwrap();
        harness.sync();
        assert_ne!(harness.client.pointer_enter_serial, 0);
        assert_eq!(harness.server.capture_pointer_owner(), CaptureContentOwner::Window(window.id));
    }
}

#[test]
fn capture_globals_require_the_capture_capability() {
    for capability in [None, Some(crate::launch::Capability::WindowList), Some(crate::launch::Capability::WindowControl), Some(crate::launch::Capability::Capture)] {
        let harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(capability.into_iter().collect()));
        let allowed = capability == Some(crate::launch::Capability::Capture);
        assert_eq!(harness.client.capture.manager.is_some(), allowed);
        assert_eq!(harness.client.capture.outputs.is_some(), allowed);
        assert_eq!(harness.client.capture.toplevels.is_some(), allowed);
    }
}

#[test]
fn capture_ready_waits_for_backend_completion_and_emits_full_initial_damage() {
    let mut harness = harness();
    let (source, session) = output_session(&mut harness);
    assert_eq!(harness.client.capture.sizes, vec![[640, 480]]);
    source.destroy();
    let frame = request(&mut harness, &session, [640, 480]);
    assert_eq!(harness.client.capture.ready, 0);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert!(harness.server.capture_job_valid(&job));
    assert_eq!(job.target, CaptureTarget::Output(OutputId(1)));
    job.frame.success(smithay::utils::Transform::Normal, None, Duration::from_secs(1));
    harness.sync();
    assert_eq!(harness.client.capture.ready, 1);
    assert_eq!(harness.client.capture.damage, vec![[0, 0, 640, 480]]);
    assert!(harness.client.capture.failures.is_empty());
    frame.destroy();
    session.destroy();
    harness.sync();
}

#[test]
fn destroying_session_does_not_cancel_a_frame_already_created_from_it() {
    let mut harness = harness();
    let (_source, session) = output_session(&mut harness);
    let frame = session.create_frame(&harness.queue.handle(), ());
    session.destroy();
    harness.sync();
    frame.attach_buffer(&harness.buffer([640, 480]));
    frame.capture();
    harness.sync();
    assert!(harness.client.capture.failures.is_empty(), "capture failures: {:?}", harness.client.capture.failures);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert!(harness.server.capture_job_valid(&job));
    job.frame.success(smithay::utils::Transform::Normal, None, Duration::from_secs(1));
    harness.sync();
    assert_eq!(harness.client.capture.ready, 1);
    frame.destroy();
    harness.sync();
}

#[test]
fn destroying_cursor_session_does_not_cancel_a_frame_already_created_from_it() {
    for destroy_cursor in [false, true] {
        let mut harness = harness();
        let qh = harness.queue.handle();
        let pointer = harness.client.seat.as_ref().unwrap().get_pointer(&qh, ());
        let source = harness.client.capture.outputs.as_ref().unwrap().create_source(harness.client.output.as_ref().unwrap(), &qh, ());
        let cursor = harness.client.capture.manager.as_ref().unwrap().create_pointer_cursor_session(&source, &pointer, &qh, ());
        let session = cursor.get_capture_session(&qh, ());
        harness.sync();
        let frame = session.create_frame(&qh, ());
        if destroy_cursor { cursor.destroy(); } else { session.destroy(); }
        harness.sync();
        frame.attach_buffer(&harness.buffer([24, 24]));
        frame.capture();
        harness.sync();
        assert!(harness.client.capture.failures.is_empty(), "capture failures: {:?}", harness.client.capture.failures);
        let job = harness.server.capture_jobs().pop().unwrap();
        assert!(job.cursor);
        assert!(harness.server.capture_job_valid(&job));
        job.frame.success(smithay::utils::Transform::Normal, None, Duration::from_secs(1));
        harness.sync();
        assert_eq!(harness.client.capture.ready, 1);
        frame.destroy();
        harness.sync();
    }
}

#[test]
fn lock_invalidates_frames_already_handed_to_the_backend() {
    let mut harness = harness();
    let (_source, session) = output_session(&mut harness);
    let _frame = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    let _lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(!harness.server.capture_job_valid(&job));
    assert_eq!(harness.client.capture.failures, vec![frame::FailureReason::Stopped]);
    assert_eq!(harness.client.capture.ready, 0);
}

#[test]
fn buffer_constraints_reject_mismatched_sizes_and_destroyed_frames_cancel_work() {
    let mut harness = harness();
    let (_source, session) = output_session(&mut harness);
    let frame = request(&mut harness, &session, [320, 240]);
    assert_eq!(harness.client.capture.failures, vec![frame::FailureReason::BufferConstraints]);
    assert!(harness.server.capture_jobs().is_empty());
    frame.destroy();
    harness.sync();
    let frame = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    frame.destroy();
    harness.sync();
    assert!(!harness.server.capture_job_valid(&job));
}

#[test]
fn old_output_sources_cannot_capture_a_replacement_output_with_the_same_runtime_id() {
    let mut harness = harness();
    let (source, _first_session) = output_session(&mut harness);
    harness.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Replacement".into(), origin: [0, 0], size: [640, 480] }]).unwrap();
    harness.sync();
    assert_eq!(harness.client.capture.stopped, 1);
    let session = harness.client.capture.manager.as_ref().unwrap().create_session(&source, manager::Options::empty(), &harness.queue.handle(), ());
    harness.sync();
    assert_eq!(harness.client.capture.stopped, 2);
    let frame = request(&mut harness, &session, [640, 480]);
    assert!(harness.server.capture_jobs().is_empty());
    assert_eq!(harness.client.capture.failures.last(), Some(&frame::FailureReason::Stopped));
    frame.destroy();
    session.destroy();
    harness.sync();
}

#[test]
fn window_sources_capture_only_the_selected_toplevel_and_stop_on_unmap() {
    let mut harness = harness();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    harness.sync();
    let qh = harness.queue.handle();
    let source = harness.client.capture.toplevels.as_ref().unwrap().create_source(&harness.client.foreign_handles[0], &qh, ());
    let session = harness.client.capture.manager.as_ref().unwrap().create_session(&source, manager::Options::PaintCursors, &qh, ());
    harness.sync();
    assert_eq!(harness.client.capture.sizes.last(), Some(&[320, 240]));
    let frame = request(&mut harness, &session, [320, 240]);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert_eq!(job.target, CaptureTarget::Window(harness.server.windows()[0].id));
    assert!(harness.server.capture_demand().outputs.is_empty());
    assert_eq!(harness.server.capture_demand().windows, BTreeSet::from([harness.server.windows()[0].id]));
    assert!(job.paint_cursor);
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    assert!(!harness.server.capture_job_valid(&job));
    assert_eq!(harness.client.capture.stopped, 1);
    drop(job);
    assert!(harness.server.capture_demand().is_empty());
    frame.destroy();
    session.destroy();
    source.destroy();
    harness.sync();
}

#[test]
fn cursor_metadata_tracks_intersection_and_hotspot_at_frame_completion() {
    let mut harness = harness();
    let qh = harness.queue.handle();
    let pointer = harness.client.seat.as_ref().unwrap().get_pointer(&qh, ());
    let source = harness.client.capture.outputs.as_ref().unwrap().create_source(harness.client.output.as_ref().unwrap(), &qh, ());
    let cursor = harness.client.capture.manager.as_ref().unwrap().create_pointer_cursor_session(&source, &pointer, &qh, ());
    let session = cursor.get_capture_session(&qh, ());
    harness.sync();
    harness.server.capture_cursor(CaptureTarget::Output(OutputId(1)), [32, 24], Some([-4, 50]));
    harness.sync();
    assert_eq!(harness.client.capture.sizes.last(), Some(&[32, 24]));
    assert_eq!(harness.client.capture.cursor_events, ["enter", "hotspot:0,0", "position:-4,50"]);
    let frame = request(&mut harness, &session, [32, 24]);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert!(job.cursor);
    assert!(!job.paint_cursor);
    harness.server.capture_cursor_ready(job.target, [8, 12]);
    job.frame.success(smithay::utils::Transform::Normal, None, Duration::from_secs(1));
    harness.sync();
    assert_eq!(harness.client.capture.cursor_events.last().map(String::as_str), Some("hotspot:8,12"));
    assert_eq!(harness.client.capture.ready, 1);
    harness.server.capture_cursor(CaptureTarget::Output(OutputId(1)), [1, 1], None);
    harness.sync();
    assert_eq!(harness.client.capture.cursor_events.last().map(String::as_str), Some("leave"));
    frame.destroy();
    cursor.destroy();
    session.destroy();
    source.destroy();
    harness.sync();
}

#[test]
fn inactive_seat_holds_existing_capture_and_refuses_new_sessions() {
    let mut harness = harness();
    let (source, session) = output_session(&mut harness);
    harness.server.capture_suspend(true);
    let frame = request(&mut harness, &session, [640, 480]);
    let mut job = harness.server.capture_jobs().pop().unwrap();
    job.deadline = Instant::now() - Duration::from_secs(1);
    assert!(harness.server.capture_job_valid(&job));
    assert!(harness.server.capture_timeout().is_none());
    let rejected = harness.client.capture.manager.as_ref().unwrap().create_session(&source, manager::Options::empty(), &harness.queue.handle(), ());
    harness.sync();
    assert_eq!(harness.client.capture.stopped, 1);
    harness.server.capture_suspend(false);
    assert!(!harness.server.capture_job_valid(&job));
    job.frame.fail(lince_smithay_capture::wayland::image_copy_capture::CaptureFailureReason::Unknown);
    frame.destroy();
    rejected.destroy();
    session.destroy();
    source.destroy();
    harness.sync();
}

#[test]
fn resuming_capture_preserves_each_pending_requests_original_deadline_and_fresh_work_recovers() {
    let mut harness = harness();
    let (_source, session) = output_session(&mut harness);
    let expired = request(&mut harness, &session, [640, 480]);
    let deadline = Instant::now() - Duration::from_secs(1);
    harness.server.capture_test_deadline(deadline);
    harness.server.capture_suspend(true);
    harness.server.capture_suspend(false);
    {
        let job = harness.server.capture_jobs().pop().unwrap();
        assert_eq!(job.deadline, deadline);
        assert_eq!(harness.server.capture_job_failure(&job), Some(lince_smithay_capture::wayland::image_copy_capture::CaptureFailureReason::Unknown));
        job.frame.fail(lince_smithay_capture::wayland::image_copy_capture::CaptureFailureReason::Unknown);
    }
    harness.sync();
    assert!(harness.server.capture_demand().is_empty());
    assert_eq!(harness.client.capture.failures.len(), 1);
    expired.destroy(); harness.sync();
    let fresh = request(&mut harness, &session, [640, 480]);
    let job = harness.server.capture_jobs().pop().unwrap();
    assert!(harness.server.capture_job_valid(&job));
    assert!(job.deadline > Instant::now());
    drop(job);
    fresh.destroy(); session.destroy(); harness.sync();
}

#[cfg(feature = "nested")]
#[test]
#[ignore = "requires hardware EGL; included explicitly in Park validation preparation"]
fn capture_gpu_window_export_preserves_pixels_and_row_orientation() {
    use std::io::Write;
    let mut harness = harness();
    let window = harness.window();
    let mut file = tempfile::tempfile().unwrap();
    let mut expected = Vec::new();
    for y in 0..48 { for _ in 0..64 { expected.extend_from_slice(if y < 24 { &[0, 0, 255, 255] } else { &[0, 255, 0, 255] }); } }
    file.write_all(&expected).unwrap();
    let qh = harness.queue.handle();
    let pool = harness.client.shm.as_ref().unwrap().create_pool(file.as_fd(), expected.len() as i32, &qh, ());
    let buffer = pool.create_buffer(0, 64, 48, 64 * 4, wl_shm::Format::Argb8888, &qh, ());
    pool.destroy();
    window.surface.attach(Some(&buffer), 0, 0);
    window.surface.damage_buffer(0, 0, 64, 48);
    window.surface.commit();
    harness.sync();
    let source = harness.client.capture.toplevels.as_ref().unwrap().create_source(&harness.client.foreign_handles[0], &qh, ());
    let session = harness.client.capture.manager.as_ref().unwrap().create_session(&source, manager::Options::empty(), &qh, ());
    harness.sync();
    let frame = request(&mut harness, &session, [64, 48]);
    let job = harness.server.capture_jobs().pop().unwrap();
    let pixels = crate::desktop::renderer::capture::probe_window(job, harness.server.windows()[0].clone()).unwrap();
    assert_eq!(pixels, expected);
    harness.sync();
    assert_eq!(harness.client.capture.ready, 1);
    assert!(harness.client.capture.failures.is_empty());
    frame.destroy();
    session.destroy();
    source.destroy();
    harness.sync();
}
