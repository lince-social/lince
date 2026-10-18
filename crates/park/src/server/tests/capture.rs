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
    outputs: Option<OutputFactory>, toplevels: Option<ToplevelFactory>, manager: Option<Manager>,
    sizes: Vec<[u32; 2]>, stopped: usize, failures: Vec<frame::FailureReason>, ready: usize, damage: Vec<[i32; 4]>,
    cursor_events: Vec<String>,
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
    let job = harness.server.capture_jobs().pop().unwrap();
    assert!(harness.server.capture_job_valid(&job));
    job.frame.success(smithay::utils::Transform::Normal, None, Duration::from_secs(1));
    harness.sync();
    assert_eq!(harness.client.capture.ready, 1);
    frame.destroy();
    harness.sync();
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
    assert!(job.paint_cursor);
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    assert!(!harness.server.capture_job_valid(&job));
    assert_eq!(harness.client.capture.stopped, 1);
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
