use super::*;
use crate::launch::Capability;
use std::os::unix::fs::FileExt;
use wayland_protocols::ext::{
    image_capture_source::v1::client::ext_image_capture_source_v1::ExtImageCaptureSourceV1 as Source,
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1 as CaptureFrame,
        ext_image_copy_capture_manager_v1::Options,
        ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1 as Session,
    },
};

const SENTINEL: [u8; 4] = [37, 83, 129, 177];

mod peers;
mod retained;

pub(super) struct Pending {
    file: std::fs::File,
    buffer: wl_buffer::WlBuffer,
    frame: CaptureFrame,
    size: [u32; 2],
    ready: usize,
    failures: usize,
}

impl Pending {
    pub(super) fn new(fixture: &mut Fixture, session: &Session, size: [u32; 2]) -> Self {
        let pending = Self::from_client(&fixture.client, &fixture.queue, session, size);
        fixture.exchange();
        pending
    }

    fn from_client(client: &Client, queue: &EventQueue<Client>, session: &Session, size: [u32; 2]) -> Self {
        let mut file = tempfile::tempfile().unwrap();
        let bytes = size[0] as usize * size[1] as usize * 4;
        file.write_all(&SENTINEL.repeat(bytes / 4)).unwrap();
        let qh = queue.handle();
        let pool = client.shm.as_ref().unwrap().create_pool(file.as_fd(), bytes as i32, &qh, ());
        let buffer = pool.create_buffer(0, size[0] as i32, size[1] as i32, (size[0] * 4) as i32, wl_shm::Format::Argb8888, &qh, ());
        pool.destroy();
        let frame = session.create_frame(&qh, ());
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, size[0] as i32, size[1] as i32);
        frame.capture();
        Self { file, buffer, frame, size, ready: client.capture.ready, failures: client.capture.failures.len() }
    }

    pub(super) fn pixels(&self) -> Vec<u8> {
        let mut bytes = vec![0; self.size[0] as usize * self.size[1] as usize * 4];
        self.file.read_exact_at(&mut bytes, 0).unwrap();
        bytes
    }

    pub(super) fn destroy(self, fixture: &mut Fixture) {
        self.frame.destroy();
        self.buffer.destroy();
        fixture.exchange();
    }
}

pub(super) fn output_session(fixture: &mut Fixture) -> (Source, Session) {
    let output = fixture.client.output.as_ref().unwrap().clone();
    output_session_for(fixture, &output)
}

pub(super) fn output_session_for(fixture: &mut Fixture, output: &wl_output::WlOutput) -> (Source, Session) {
    output_session_options(fixture, output, Options::empty())
}

pub(super) fn output_session_options(fixture: &mut Fixture, output: &wl_output::WlOutput, options: Options) -> (Source, Session) {
    let qh = fixture.queue.handle();
    let source = fixture.client.capture.outputs.as_ref().unwrap().create_source(output, &qh, ());
    let session = fixture.client.capture.manager.as_ref().unwrap().create_session(&source, options, &qh, ());
    fixture.exchange();
    (source, session)
}

pub(super) fn submitted(fixture: &mut Fixture, pending: &Pending) -> [usize; 4] {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        fixture.tick();
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
        fixture.exchange();
        assert_eq!(fixture.client.capture.ready, pending.ready, "The readback was written before its queued lifetime could be observed");
        assert_eq!(fixture.client.capture.failures.len(), pending.failures, "Preparing the real GLES readback failed: {:?}", fixture.client.capture.failures);
        let state = fixture.composition.capture_test_state();
        if state[1] > 0 { return state; }
        assert!(Instant::now() < deadline, "No real GLES capture readback was submitted");
    }
}

pub(super) fn rejected(fixture: &mut Fixture, pending: &Pending) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
        fixture.exchange();
        assert_eq!(fixture.client.capture.ready, pending.ready, "A retired readback emitted Ready");
        assert!(pending.pixels().chunks_exact(4).all(|pixel| pixel == SENTINEL), "A retired readback wrote client SHM before its lifetime was revalidated");
        let state = fixture.composition.capture_test_state();
        if fixture.client.capture.failures.len() > pending.failures && state[0] == 0 && state[1] == 0 { break; }
        assert!(Instant::now() < deadline, "The failed job or its real GPU resources were not retired: {state:?}");
        fixture.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fixture.client.capture.failures.len(), pending.failures + 1);
    assert!(fixture.desktop.running);
}

pub(super) fn completed(fixture: &mut Fixture, pending: &Pending) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        fixture.tick();
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, true).unwrap();
        fixture.exchange();
        assert_eq!(fixture.client.capture.failures.len(), pending.failures, "A fresh valid capture failed: {:?}", fixture.client.capture.failures);
        if fixture.client.capture.ready > pending.ready { break; }
        assert!(Instant::now() < deadline, "The fresh real capture never completed");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(fixture.client.capture.ready, pending.ready + 1);
    pending.pixels()
}

fn action(fixture: &mut Fixture, action: DesktopAction) {
    fixture.desktop.send(Input::Action(action)).unwrap();
    fixture.inspect(|_| ());
}

fn enter(fixture: &mut Fixture) {
    fixture.desktop.key(28, true, 30).unwrap();
    fixture.desktop.key(28, false, 31).unwrap();
    fixture.inspect(|_| ());
}

fn window_item(fixture: &mut Fixture, surface: SurfaceId) -> (Entity, [u8; 16]) {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &native::layout::desktop::ApplicationWindow, &native::layout::LayoutBox)>().iter(world)
            .find_map(|(entity, window, layout)| (window.surface == surface).then_some((entity, layout.id))).unwrap()
    })
}

fn exclude(fixture: &mut Fixture, surface: SurfaceId) {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.resource::<lince_interface::host::capture::PolicyGate>().invalidate();
        let target = world.query::<(Entity, &native::layout::desktop::ApplicationWindow)>().iter(world)
            .find(|(_, window)| window.surface == surface).unwrap().0;
        world.get_mut::<native::layout::LayoutBox>(target).unwrap().capture_excluded = true;
    });
}

fn decorations(fixture: &mut Fixture, owner: Entity) -> usize {
    fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let surface = world.get::<native::topology::presentation::Surface>(owner).unwrap();
        let face = surface.face;
        world.get::<Children>(face).into_iter().flat_map(|children| children.into_iter()).filter(|child| world.get::<Mesh3d>(**child).is_some()).count()
    })
}

#[derive(Clone, Copy, Debug)]
enum Change { Privacy, Source, Revoke, Lock, Deadline, ColorThenLock, ColorThenVt, ColorThenOutput }

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_effect_capture_revalidates_privacy_source_revocation_lock_and_deadline_before_writing_shm() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut samples = Vec::new();
    let changes = [Change::Privacy, Change::Source, Change::Revoke, Change::Lock, Change::Deadline,
        Change::ColorThenLock, Change::ColorThenVt, Change::ColorThenOutput];
    let selected = std::env::var("LINCE_PARK_TEST_CAPTURE_CHANGE").ok();
    assert!(selected.as_ref().is_none_or(|selected| changes.iter().any(|change| format!("{change:?}") == *selected)), "Unknown capture retirement case: {selected:?}");
    for change in changes.into_iter().filter(|change| selected.as_ref().is_none_or(|selected| format!("{change:?}") == *selected)) {
        eprintln!("Queued capture retirement: {change:?}");
        let profile = directory.path().join(format!("{change:?}"));
        std::fs::create_dir_all(&profile).unwrap();
        let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::Capture, Capability::WindowList]));
        harness.server.registered_output_catalog().unwrap();
        let first = harness.window();
        let _first_buffer = paint(&mut harness, &first, [320, 240], [255, 0, 255, 255]);
        let first_id = harness.server.windows()[0].id;
        let second = harness.window();
        let _second_buffer = paint(&mut harness, &second, [320, 240], [0, 255, 0, 255]);
        let second_id = harness.server.windows().into_iter().find(|window| window.id != first_id).unwrap().id;
        let mut fixture = Fixture::new(harness, &runtime, &profile);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|current| current.frame.geometry.as_ref().is_some_and(|geometry|
            [first_id, second_id].iter().all(|id| geometry.items.iter().any(|item| item.id == ItemId::Application(*id))))));
        appearance::effects(&mut fixture, true);
        fixture.until(|fixture| fixture.desktop.server.capture_policy_gate().read().is_some_and(|read| read.policy().is_some_and(|policy|
            [first_id, second_id].iter().all(|id| policy.windows.get(id) == Some(&false)))));
        let (source, session) = output_session(&mut fixture);
        let warmup = Pending::new(&mut fixture, &session, [1000, 720]);
        drop(completed(&mut fixture, &warmup));
        warmup.destroy(&mut fixture);
        let pending = Pending::new(&mut fixture, &session, [1000, 720]);
        let state = submitted(&mut fixture, &pending);
        let mut lock = None;
        if matches!(change, Change::ColorThenLock | Change::ColorThenVt | Change::ColorThenOutput) {
            let revision = fixture.desktop.server.display_snapshot().color[&OutputId(1)].revision + 1;
            fixture.desktop.server.output_color(OutputId(1), lince_interface::host::display::ColorStatus {
                prepared: Some(Default::default()), effective: Some(Default::default()), revision, ..Default::default() });
        }
        match change {
            Change::Privacy => {
                exclude(&mut fixture, first_id);
            }
            Change::Source | Change::ColorThenOutput => fixture.desktop.server.configure_outputs(&[crate::server::Monitor { id: lince_interface::host::OutputId(1), name: "Capture-Replacement".into(), origin: [0, 0], size: [1000, 720] }]).unwrap(),
            Change::Revoke => {
                let id = fixture.desktop.server.capture_status().sessions[0].id;
                fixture.desktop.server.stop_capture_session(id);
            }
            Change::Lock | Change::ColorThenLock => {
                lock = Some(fixture.client.lock_manager.as_ref().unwrap().lock(&fixture.queue.handle(), ()));
                fixture.exchange();
                assert!(fixture.desktop.server.locked());
            }
            Change::Deadline => std::thread::sleep(Duration::from_millis(2100)),
            Change::ColorThenVt => fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, false).unwrap(),
        }
        rejected(&mut fixture, &pending);
        samples.push(serde_json::json!({ "change": format!("{change:?}"), "waiting_at_submission": state[0], "readbacks_at_submission": state[1],
            "unfinished_gpu_at_observation": state[2], "retired_readbacks_at_observation": state[3], "ready_delta": fixture.client.capture.ready - pending.ready,
            "failure": format!("{:?}", fixture.client.capture.failures.last().unwrap()), "shm_untouched": true, "settled_state": fixture.composition.capture_test_state() }));
        pending.destroy(&mut fixture);
        if matches!(change, Change::Lock | Change::ColorThenLock) {
            assert!(fixture.desktop.server.capture_status().sessions.is_empty());
            assert!(lock.is_some());
        } else {
            let cyan = privacy::buffer(&fixture, [320, 240], [255, 255, 0, 255]);
            second.surface.attach(Some(&cyan), 0, 0);
            second.surface.damage_buffer(0, 0, 320, 240);
            second.surface.commit();
            fixture.tick();
            let version = fixture.desktop.server.windows().into_iter().find(|window| window.id == second_id).unwrap().nodes[0].content.version;
            fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().any(|window| window.id == second_id
                && window.nodes.iter().any(|node| node.content.version == version))));
            let replacement = if matches!(change, Change::Source | Change::Revoke | Change::ColorThenOutput) { Some(output_session(&mut fixture)) } else { None };
            let active = replacement.as_ref().map_or(&session, |(_, session)| session);
            let fresh = Pending::new(&mut fixture, active, [1000, 720]);
            let pixels = completed(&mut fixture, &fresh);
            assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] > 220 && pixel[2] < 40).count() > 200, "Fresh public content did not recover after {change:?}");
            if matches!(change, Change::Privacy) { assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0); }
            fresh.destroy(&mut fixture);
            if let Some((source, session)) = replacement { session.destroy(); source.destroy(); fixture.exchange(); }
        }
        session.destroy();
        source.destroy();
        fixture.exchange();
        assert_eq!(fixture.composition.capture_test_state(), [0; 4]);
    }
    if let Some(path) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("appearance-capture-lifetimes.json"), serde_json::to_vec_pretty(&samples).unwrap()).unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn direct_window_capture_keeps_its_raw_alpha_and_retired_source_cannot_retarget_another_client() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::Capture, Capability::WindowList]));
    let first = harness.window();
    let _first_buffer = paint(&mut harness, &first, [320, 240], [0, 64, 0, 64]);
    let first_id = harness.server.windows()[0].id;
    let second = harness.window();
    let _second_buffer = paint(&mut harness, &second, [320, 240], [255, 0, 255, 255]);
    let second_id = harness.server.windows().into_iter().find(|window| window.id != first_id).unwrap().id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|current| current.frame.geometry.as_ref().is_some_and(|geometry|
        [first_id, second_id].iter().all(|id| geometry.items.iter().any(|item| item.id == ItemId::Application(*id))))));
    appearance::effects(&mut fixture, true);
    fixture.until(|fixture| fixture.desktop.server.capture_policy_gate().read().is_some_and(|read| read.policy().is_some_and(|policy|
        [first_id, second_id].iter().all(|id| policy.windows.get(id) == Some(&false)))));
    let qh = fixture.queue.handle();
    let source = fixture.client.capture.toplevels.as_ref().unwrap().create_source(&fixture.client.foreign_handles[0], &qh, ());
    let session = fixture.client.capture.manager.as_ref().unwrap().create_session(&source, Options::empty(), &qh, ());
    fixture.exchange();
    assert_eq!(fixture.client.capture.sizes.last(), Some(&[320, 240]));
    let direct = Pending::new(&mut fixture, &session, [320, 240]);
    let pixels = completed(&mut fixture, &direct);
    let actual = pixels.chunks_exact(4).filter(|pixel| [0_u8, 64, 0, 64].iter().zip(*pixel).all(|(expected, actual)| expected.abs_diff(*actual) <= 3)).count();
    assert!(actual > 320 * 240 * 9 / 10, "Direct-window capture acquired canvas alpha, backdrop or decoration pixels: only {actual} raw-reference pixels matched");
    assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0);
    direct.destroy(&mut fixture);
    let pending = Pending::new(&mut fixture, &session, [320, 240]);
    let state = submitted(&mut fixture, &pending);
    assert!(state[1] > 0);
    first.surface.attach(None, 0, 0);
    first.surface.commit();
    fixture.exchange();
    fixture.until(|fixture| fixture.desktop.server.windows().iter().all(|window| window.id != first_id || !window.mapped));
    rejected(&mut fixture, &pending);
    pending.destroy(&mut fixture);
    assert!(fixture.desktop.server.windows().iter().any(|window| window.id == second_id && window.mapped));
    let (output_source, output_session) = output_session(&mut fixture);
    let output = Pending::new(&mut fixture, &output_session, [1000, 720]);
    let pixels = completed(&mut fixture, &output);
    assert!(pixels.chunks_exact(4).all(|pixel| pixel[3] == 255), "An output source inherited the retired raw window's alpha role");
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count() > 200);
    output.destroy(&mut fixture);
    output_session.destroy();
    output_source.destroy();
    session.destroy();
    source.destroy();
    fixture.exchange();
    assert_eq!(fixture.composition.capture_test_state(), [0; 4]);
    assert!(fixture.desktop.running);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_group_and_scratch_privacy_changes_retire_old_pixels_and_keep_real_client_focus_and_public_updates() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for grouped in [true, false] {
        let profile = directory.path().join(if grouped { "group" } else { "scratch" });
        std::fs::create_dir_all(&profile).unwrap();
        let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::Capture, Capability::WindowList]));
        let first = harness.window();
        let _first_buffer = paint(&mut harness, &first, [320, 240], [255, 0, 255, 255]);
        let first_id = harness.server.windows()[0].id;
        let second = harness.window();
        let _second_buffer = paint(&mut harness, &second, [320, 240], [0, 255, 0, 255]);
        let second_id = harness.server.windows().into_iter().find(|window| window.id != first_id).unwrap().id;
        let mut fixture = Fixture::new(harness, &runtime, &profile);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
            [first_id, second_id].iter().all(|id| geometry.items.iter().any(|item| item.id == ItemId::Application(*id))))));
        appearance::effects(&mut fixture, true);
        let (first_entity, first_placement) = window_item(&mut fixture, first_id);
        let (second_entity, second_placement) = window_item(&mut fixture, second_id);
        action(&mut fixture, DesktopAction::SelectPlacement { workspace: 1, item: Some(first_placement) });
        if grouped {
            action(&mut fixture, DesktopAction::CreateCompactGroup);
            let choice = fixture.inspect(move |canvas| {
                let name = format!("compact.choice.{}", second_entity.to_bits());
                let world = canvas.world_mut();
                world.query::<(Entity, &Name)>().iter(world).find_map(|(entity, current)| (current.as_str() == name).then_some(entity)).unwrap()
            });
            magnification::focus_entity(&mut fixture, choice, "the second compact member");
            enter(&mut fixture);
            magnification::focus(&mut fixture, "Create selected group");
            enter(&mut fixture);
        } else {
            action(&mut fixture, DesktopAction::MarkScratch);
        }
        let (source, session) = output_session(&mut fixture);
        let warmup = Pending::new(&mut fixture, &session, [1000, 720]);
        let pixels = completed(&mut fixture, &warmup);
        assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count() > 200, "The initial selected client was not actually visible");
        warmup.destroy(&mut fixture);
        let pending = Pending::new(&mut fixture, &session, [1000, 720]);
        submitted(&mut fixture, &pending);
        exclude(&mut fixture, first_id);
        action(&mut fixture, if grouped { DesktopAction::NextCompactMember } else { DesktopAction::HideScratch });
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
            geometry.items.iter().any(|item| item.id == ItemId::Application(second_id)) && !geometry.items.iter().any(|item| item.id == ItemId::Application(first_id)))));
        rejected(&mut fixture, &pending);
        pending.destroy(&mut fixture);
        assert_eq!(decorations(&mut fixture, first_entity), 0);
        assert_eq!(decorations(&mut fixture, second_entity), 2);
        let cyan = privacy::buffer(&fixture, [320, 240], [255, 255, 0, 255]);
        second.surface.attach(Some(&cyan), 0, 0);
        second.surface.damage_buffer(0, 0, 320, 240);
        second.surface.commit();
        fixture.tick();
        let version = fixture.desktop.server.windows().into_iter().find(|window| window.id == second_id).unwrap().nodes[0].content.version;
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.windows.iter().any(|window| window.id == second_id
            && window.nodes.iter().any(|node| node.content.version == version))));
        let fresh = Pending::new(&mut fixture, &session, [1000, 720]);
        let pixels = completed(&mut fixture, &fresh);
        assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0);
        assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] > 220 && pixel[2] < 40).count() > 200, "The selected public client stopped updating after a hidden private source retired");
        fresh.destroy(&mut fixture);
        let point = {
            let geometry = fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == ItemId::Application(second_id)).unwrap();
            let origin = geometry.mapping.surface.origin();
            let size = geometry.mapping.surface.size();
            geometry.mapping.canvas_point([origin[0] + size[0] * 0.5, origin[1] + size[1] * 0.5]).unwrap()
        };
        fixture.desktop.move_pointer(point, 40).unwrap();
        fixture.tick();
        assert_eq!(fixture.desktop.server.capture_pointer_owner(), CaptureContentOwner::Window(second_id));
        let keys = fixture.client.keys.len();
        let buttons = fixture.client.pointer_buttons.len();
        fixture.desktop.button(272, true, 41).unwrap();
        fixture.desktop.button(272, false, 42).unwrap();
        fixture.desktop.key(45, true, 43).unwrap();
        fixture.desktop.key(45, false, 44).unwrap();
        fixture.until(|fixture| fixture.client.keys.len() >= keys + 2 && fixture.client.pointer_buttons.len() >= buttons + 2);
        assert_eq!(&fixture.client.keys[keys..], &[(45, true), (45, false)]);
        assert_eq!(&fixture.client.pointer_buttons[buttons..], &[(272, true), (272, false)]);
        if grouped {
            action(&mut fixture, DesktopAction::PreviousCompactMember);
        } else {
            action(&mut fixture, DesktopAction::ShowScratch);
        }
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
            geometry.items.iter().any(|item| item.id == ItemId::Application(first_id)))));
        let owner = appearance::pixels(&mut fixture, if grouped { "queued-group-private-owner" } else { "queued-scratch-private-owner" });
        assert!(owner.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count() > 200);
        let filtered = Pending::new(&mut fixture, &session, [1000, 720]);
        let pixels = completed(&mut fixture, &filtered);
        assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0, "A revealed private utility item leaked through its own image or an effect");
        filtered.destroy(&mut fixture);
        assert_eq!(window_item(&mut fixture, first_id), (first_entity, first_placement));
        assert_eq!(window_item(&mut fixture, second_id), (second_entity, second_placement));
        if !grouped {
            action(&mut fixture, DesktopAction::HideScratch);
            fixture.inspect(move |canvas| {
                let world = canvas.world_mut();
                let root = world.query_filtered::<Entity, With<native::workspace::Workspaces>>().single(world).unwrap();
                assert_eq!(world.get::<native::workspace::Workspaces>(root).unwrap().entries[0].desktop.focused(), Some(lince_interface::layout::desktop::PlacementId(second_placement)));
            });
            assert_eq!(decorations(&mut fixture, first_entity), 0);
        }
        session.destroy();
        source.destroy();
        fixture.exchange();
        assert_eq!(fixture.composition.capture_test_state(), [0; 4]);
        assert!(fixture.desktop.running);
    }
}

#[derive(Clone, Copy, Debug)]
enum OutputChange { Scale, Mode, Reservation, LastOutput, LockedHotplug }

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_capture_survives_only_matching_output_mappings_and_last_output_recovery_keeps_protection_and_clients() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for change in [OutputChange::Scale, OutputChange::Mode, OutputChange::Reservation, OutputChange::LastOutput, OutputChange::LockedHotplug] {
        let profile = directory.path().join(format!("output-{change:?}"));
        std::fs::create_dir_all(&profile).unwrap();
        let mut server = Server::new([1000, 720]).unwrap();
        server.configure_outputs(&[Monitor { id: OutputId(1), name: "Capture-Output".into(), origin: [0, 0], size: [1000, 720] }]).unwrap();
        server.registered_output_catalog().unwrap();
        let mut harness = Harness::connect_authorized(server, Some(vec![Capability::Capture, Capability::WindowList]));
        let first = harness.window();
        let _first_buffer = paint(&mut harness, &first, [320, 240], [255, 0, 255, 255]);
        let first_id = harness.server.windows()[0].id;
        let second = harness.window();
        let _second_buffer = paint(&mut harness, &second, [320, 240], [0, 255, 0, 255]);
        let second_id = harness.server.windows().into_iter().find(|window| window.id != first_id).unwrap().id;
        let mut fixture = Fixture::new(harness, &runtime, &profile);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
            [first_id, second_id].iter().all(|id| geometry.items.iter().any(|item| item.id == ItemId::Application(*id))))));
        appearance::effects(&mut fixture, true);
        let original = [window_item(&mut fixture, first_id), window_item(&mut fixture, second_id)];
        exclude(&mut fixture, first_id);
        action(&mut fixture, DesktopAction::SelectPlacement { workspace: 1, item: Some(original[1].1) });
        let (source, session) = output_session(&mut fixture);
        let warmup = Pending::new(&mut fixture, &session, [1000, 720]);
        let pixels = completed(&mut fixture, &warmup);
        assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0);
        assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40).count() > 200);
        warmup.destroy(&mut fixture);
        let pending = Pending::new(&mut fixture, &session, [1000, 720]);
        submitted(&mut fixture, &pending);
        let mut bar = None;
        let mut lock = None;
        match change {
            OutputChange::Mode => {
                fixture.desktop.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Capture-Output".into(), origin: [0, 0], size: [960, 600] }]).unwrap();
                fixture.desktop.server.registered_output_catalog().unwrap();
                fixture.desktop.invalidate_outputs().unwrap();
            }
            OutputChange::Scale => {
                fixture.desktop.server.configure_scaled_outputs(&[Monitor { id: OutputId(1), name: "Capture-Output".into(), origin: [60, 40], size: [1000, 720] }],
                    &std::collections::BTreeMap::from([(OutputId(1), 1.25)])).unwrap();
                fixture.desktop.server.registered_output_catalog().unwrap();
                fixture.desktop.invalidate_outputs().unwrap();
            }
            OutputChange::Reservation => {
                let qh = fixture.queue.handle();
                let surface = fixture.client.compositor.as_ref().unwrap().create_surface(&qh, ());
                let role = fixture.client.layer_shell.as_ref().unwrap().get_layer_surface(&surface, Some(fixture.client.output.as_ref().unwrap()),
                    zwlr_layer_shell_v1::Layer::Top, "capture-reservation".into(), &qh, ());
                role.set_size(0, 28);
                role.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left | zwlr_layer_surface_v1::Anchor::Right);
                role.set_exclusive_zone(28);
                surface.commit();
                fixture.exchange();
                let buffer = privacy::buffer(&fixture, [1000, 28], [32, 32, 32, 255]);
                surface.attach(Some(&buffer), 0, 0);
                surface.damage_buffer(0, 0, 1000, 28);
                surface.commit();
                fixture.exchange();
                bar = Some((surface, role, buffer));
            }
            OutputChange::LastOutput | OutputChange::LockedHotplug => {
                if matches!(change, OutputChange::LockedHotplug) {
                    lock = Some(fixture.client.lock_manager.as_ref().unwrap().lock(&fixture.queue.handle(), ()));
                    fixture.exchange();
                    assert!(fixture.desktop.server.locked());
                }
                fixture.desktop.server.disconnect_outputs();
                fixture.desktop.server.output_catalog(Vec::new()).unwrap();
                fixture.desktop.invalidate_outputs().unwrap();
                fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.dmabuf().is_none()));
                for (owner, _) in original { assert_eq!(decorations(&mut fixture, owner), 0); }
            }
        }
        rejected(&mut fixture, &pending);
        pending.destroy(&mut fixture);
        if matches!(change, OutputChange::LastOutput | OutputChange::LockedHotplug) {
            fixture.desktop.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Capture-Output".into(), origin: [0, 0], size: [1000, 720] }]).unwrap();
            fixture.desktop.server.registered_output_catalog().unwrap();
            fixture.desktop.invalidate_outputs().unwrap();
        }
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.dmabuf().is_some()));
        assert_eq!(window_item(&mut fixture, first_id), original[0]);
        assert_eq!(window_item(&mut fixture, second_id), original[1]);
        assert!(fixture.desktop.server.windows().iter().filter(|window| window.mapped).count() == 2);
        if matches!(change, OutputChange::LockedHotplug) {
            assert!(fixture.desktop.server.locked());
            assert!(lock.is_some());
            assert!(fixture.desktop.server.capture_status().sessions.is_empty());
            let pixels = appearance::pixels(&mut fixture, "locked-hotplug-protected");
            assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0);
            assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40).count(), 0);
            for (owner, _) in original { assert_eq!(decorations(&mut fixture, owner), 0); }
        } else {
            let (fresh_source, fresh_session) = output_session(&mut fixture);
            let size = if matches!(change, OutputChange::Mode) { [960, 600] } else { [1000, 720] };
            let fresh = Pending::new(&mut fixture, &fresh_session, size);
            let pixels = completed(&mut fixture, &fresh);
            assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0);
            assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40).count() > 200);
            assert_eq!(decorations(&mut fixture, original[1].0), 2);
            fresh.destroy(&mut fixture);
            fresh_session.destroy();
            fresh_source.destroy();
        }
        if let Some((surface, role, buffer)) = bar { role.destroy(); surface.destroy(); buffer.destroy(); }
        session.destroy();
        source.destroy();
        fixture.exchange();
        assert_eq!(fixture.composition.capture_test_state(), [0; 4]);
        assert!(fixture.desktop.running);
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_readback_pause_keeps_shm_and_timers_quiet_then_resumes_or_rejects_changed_privacy() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for change_privacy in [false, true] {
        let profile = directory.path().join(format!("pause-{change_privacy}"));
        std::fs::create_dir_all(&profile).unwrap();
        let mut harness = Harness::connect_authorized(Server::new([1000, 720]).unwrap(), Some(vec![Capability::Capture, Capability::WindowList]));
        let window = harness.window();
        let _buffer = paint(&mut harness, &window, [320, 240], [255, 0, 255, 255]);
        let surface = harness.server.windows()[0].id;
        let mut fixture = Fixture::new(harness, &runtime, &profile);
        fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref().is_some_and(|geometry|
            geometry.items.iter().any(|item| item.id == ItemId::Application(surface)))));
        appearance::effects(&mut fixture, true);
        let (source, session) = output_session(&mut fixture);
        let warmup = Pending::new(&mut fixture, &session, [1000, 720]);
        drop(completed(&mut fixture, &warmup));
        warmup.destroy(&mut fixture);
        let pending = Pending::new(&mut fixture, &session, [1000, 720]);
        submitted(&mut fixture, &pending);
        fixture.desktop.event(smithay::backend::winit::WinitEvent::Focus(false)).unwrap();
        assert!(!fixture.desktop.focused);
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, false).unwrap();
        let paused = fixture.composition.capture_test_state();
        assert!(paused[1] > 0);
        assert!(fixture.composition.capture_timeout().is_none());
        assert_eq!(fixture.desktop.server.capture_status().sessions.len(), 1);
        assert!(fixture.desktop.server.capture_status().sessions.iter().all(|session| session.paused));
        std::thread::sleep(Duration::from_millis(2100));
        if change_privacy { exclude(&mut fixture, surface); }
        fixture.composition.capture(&mut fixture.renderer, &mut fixture.desktop, false).unwrap();
        fixture.exchange();
        assert!(fixture.composition.capture_timeout().is_none());
        assert_eq!(fixture.client.capture.ready, pending.ready);
        assert_eq!(fixture.client.capture.failures.len(), pending.failures);
        assert!(pending.pixels().chunks_exact(4).all(|pixel| pixel == SENTINEL));
        assert_eq!(fixture.composition.capture_test_state()[1], paused[1]);
        fixture.desktop.event(smithay::backend::winit::WinitEvent::Focus(true)).unwrap();
        assert!(fixture.desktop.focused);
        if change_privacy {
            rejected(&mut fixture, &pending);
            let fresh = Pending::new(&mut fixture, &session, [1000, 720]);
            let pixels = completed(&mut fixture, &fresh);
            assert_eq!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count(), 0);
            fresh.destroy(&mut fixture);
        } else {
            let pixels = completed(&mut fixture, &pending);
            assert!(pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 220).count() > 200);
        }
        pending.destroy(&mut fixture);
        session.destroy();
        source.destroy();
        fixture.exchange();
        assert_eq!(fixture.composition.capture_test_state(), [0; 4]);
    }
}
