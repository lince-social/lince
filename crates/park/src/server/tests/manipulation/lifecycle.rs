use super::*;
use crate::{
    desktop::Desktop,
    runtime::{CanvasWorker, Configuration, Input},
};
use lince_interface::host::{ContentMapping, DesktopAction, WorkspaceLayout};
use smithay::backend::winit::WinitEvent;

#[path = "lifecycle/changes.rs"]
mod changes;
#[path = "lifecycle/queued.rs"]
mod queued;
#[cfg(feature = "session")]
#[path = "lifecycle/suspension.rs"]
mod suspension;

struct Fixture {
    desktop: Desktop,
    harness: Harness,
    window: Window,
    id: SurfaceId,
}

impl Fixture {
    fn new(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> Self {
        Self::with_cell(runtime, directory, None)
    }

    fn connected(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> Self {
        let engine = Arc::new(runtime.block_on(engine::Engine::open_memory()).unwrap());
        let cell = lince_interface::native::app::CellHandle(cell::CellRuntime {
            engine: engine.clone(),
            store: engine.store.clone(),
            commands: Default::default(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            information: None,
            speech: None,
            fiote: None,
        });
        Self::with_cell(runtime, directory, Some(cell))
    }

    fn with_cell(
        runtime: &tokio::runtime::Runtime,
        directory: &std::path::Path,
        cell: Option<lince_interface::native::app::CellHandle>,
    ) -> Self {
        let mut harness = Harness::new();
        let window = harness.window();
        let buffer = opaque_buffer(&harness, [320, 240]);
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.commit();
        harness.sync();
        let id = harness.server.windows()[0].id;
        let server = std::mem::replace(&mut harness.server, Server::new([640, 480]).unwrap());
        let worker = CanvasWorker::spawn(Configuration {
            showoff: false,
            size: [640, 480],
            directory: directory.into(),
            runtime: runtime.handle().clone(),
            cell,
        })
        .unwrap();
        let mut desktop = Desktop::new(server, worker, [640, 480], 1, true);
        desktop.dispatch().unwrap();
        advance(&mut desktop, 0);
        desktop
            .worker
            .input(Input::Action(DesktopAction::SetLayout(
                WorkspaceLayout::Free,
            )))
            .unwrap();
        let serial = desktop
            .worker
            .input(Input::Window(
                id,
                WindowRequest {
                    maximized: Some(false),
                    ..Default::default()
                },
            ))
            .unwrap();
        advance(&mut desktop, serial);
        requests::configure(&mut desktop, &mut harness, &window, id);
        Self {
            desktop,
            harness,
            window,
            id,
        }
    }

    fn sync(&mut self) {
        client_sync(&mut self.desktop, &mut self.harness);
    }

    fn flush(&mut self) {
        let serial = barrier(&mut self.desktop);
        advance(&mut self.desktop, serial);
    }

    fn configure(&mut self) {
        requests::configure_client(&mut self.desktop, &mut self.harness, &self.window, self.id);
        let covered = self
            .desktop
            .current
            .as_ref()
            .is_some_and(|frame| frame.frame.input.modal || frame.frame.input.keyboard_modal);
        if !covered && !self.desktop.server.locked() {
            visible_client_pixels(&self.desktop, self.id);
        }
    }

    fn begin(&mut self, client: bool, queued: bool) -> u32 {
        let start = center(mapping(&self.desktop, self.id));
        self.desktop.move_pointer(start, 1).unwrap();
        let button = if client {
            self.desktop.button(272, true, 2).unwrap();
            self.sync();
            self.window.role.resize(
                self.harness.client.seat.as_ref().unwrap(),
                self.harness.client.pointer_serial,
                xdg_toplevel::ResizeEdge::Right,
            );
            self.sync();
            if !queued {
                self.desktop.dispatch().unwrap();
            }
            272
        } else {
            self.desktop.key(125, true, 2).unwrap();
            self.desktop.button(273, true, 3).unwrap();
            self.desktop.key(125, false, 4).unwrap();
            273
        };
        assert!(self.desktop.server.manipulation_grabbed());
        if !queued {
            self.desktop
                .move_pointer([start[0] + 24.0, start[1] + 12.0], 5)
                .unwrap();
            self.flush();
            assert!(
                self.desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .input
                    .manipulation
                    .is_some()
            );
            assert!(
                self.desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .windows
                    .iter()
                    .find(|window| window.surface == self.id)
                    .unwrap()
                    .state
                    .resizing
            );
            self.configure();
        }
        self.desktop.button(274, true, 6).unwrap();
        self.sync();
        assert_eq!(
            self.harness.client.pointer_buttons,
            if client {
                vec![(272, true)]
            } else {
                Vec::new()
            }
        );
        button
    }

    fn cancelled(&mut self, initial: ContentMapping) {
        self.flush();
        assert!(
            self.desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        assert!(
            !self
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .windows
                .iter()
                .find(|window| window.surface == self.id)
                .unwrap()
                .state
                .resizing
        );
        self.configure();
        assert_eq!(mapping(&self.desktop, self.id), initial);
    }

    fn release_and_check_normal_input(&mut self, button: u32, client: bool) {
        self.desktop.dispatch().unwrap();
        self.flush();
        assert!(!self.desktop.current.as_ref().unwrap().frame.input.modal);
        assert!(
            !self
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .keyboard_modal
        );
        visible_client_pixels(&self.desktop, self.id);
        let expected = if client {
            vec![(272, true)]
        } else {
            Vec::new()
        };
        self.desktop.button(button, false, 20).unwrap();
        self.desktop.button(275, true, 21).unwrap();
        self.desktop.button(275, false, 22).unwrap();
        self.sync();
        assert_eq!(self.harness.client.pointer_buttons, expected);
        self.desktop.button(274, false, 23).unwrap();
        self.sync();
        assert_eq!(self.harness.client.pointer_buttons, expected);
        assert!(!self.desktop.server.manipulation_grabbed());
        assert!(self.desktop.server.state.buttons.is_empty());
        self.flush();
        self.desktop
            .move_pointer(center(mapping(&self.desktop, self.id)), 24)
            .unwrap();
        for step in 0..3 {
            self.desktop.button(273, true, 25 + step * 2).unwrap();
            self.sync();
            let surface = self
                .desktop
                .server
                .windows()
                .into_iter()
                .find(|window| window.id == self.id)
                .unwrap()
                .surface;
            assert!(
                self.desktop
                    .server
                    .state
                    .authorized_input(self.harness.client.pointer_serial.into(), &surface)
            );
            self.desktop.button(273, false, 26 + step * 2).unwrap();
        }
        self.sync();
        let mut expected = expected;
        expected.extend([(273, true), (273, false)].repeat(3));
        assert_eq!(self.harness.client.pointer_buttons, expected);
        self.harness.client.pointer_buttons.clear();
    }
}

#[derive(Clone, Copy, Debug)]
enum Takeover {
    Focus,
    Lock,
    Layer,
    Modal,
    KeyboardModal,
}

enum Recovery {
    Focus,
    Lock(ext_session_lock_v1::ExtSessionLockV1, wl_surface::WlSurface),
    Layer(
        zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        wl_surface::WlSurface,
    ),
    Modal,
}

impl Takeover {
    fn apply(self, fixture: &mut Fixture) -> Recovery {
        match self {
            Self::Focus => {
                fixture.desktop.event(WinitEvent::Focus(false)).unwrap();
                Recovery::Focus
            }
            Self::Lock => {
                let qh = fixture.harness.queue.handle();
                let owner = fixture
                    .harness
                    .client
                    .lock_manager
                    .as_ref()
                    .unwrap()
                    .lock(&qh, ());
                fixture.sync();
                let surface = fixture
                    .harness
                    .client
                    .compositor
                    .as_ref()
                    .unwrap()
                    .create_surface(&qh, ());
                let _role = owner.get_lock_surface(
                    &surface,
                    fixture.harness.client.output.as_ref().unwrap(),
                    &qh,
                    (),
                );
                fixture.sync();
                let buffer = opaque_buffer(&fixture.harness, [640, 480]);
                surface.attach(Some(&buffer), 0, 0);
                surface.commit();
                fixture.sync();
                fixture.desktop.server.lock_presented();
                fixture.desktop.focus().unwrap();
                assert!(fixture.desktop.server.locked());
                let target = fixture.desktop.server.lock_surfaces()[0].2[0]
                    .surface
                    .clone();
                assert_eq!(
                    fixture
                        .desktop
                        .server
                        .state
                        .seat
                        .get_keyboard()
                        .unwrap()
                        .current_focus(),
                    Some(target)
                );
                Recovery::Lock(owner, surface)
            }
            Self::Layer => {
                let qh = fixture.harness.queue.handle();
                let surface = fixture
                    .harness
                    .client
                    .compositor
                    .as_ref()
                    .unwrap()
                    .create_surface(&qh, ());
                let role = fixture
                    .harness
                    .client
                    .layer_shell
                    .as_ref()
                    .unwrap()
                    .get_layer_surface(
                        &surface,
                        None,
                        zwlr_layer_shell_v1::Layer::Overlay,
                        "park-lifecycle-test".into(),
                        &qh,
                        (),
                    );
                role.set_size(640, 30);
                role.set_anchor(
                    zwlr_layer_surface_v1::Anchor::Top
                        | zwlr_layer_surface_v1::Anchor::Left
                        | zwlr_layer_surface_v1::Anchor::Right,
                );
                role.set_keyboard_interactivity(
                    zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive,
                );
                surface.commit();
                fixture.sync();
                let buffer = opaque_buffer(&fixture.harness, [640, 30]);
                surface.attach(Some(&buffer), 0, 0);
                surface.commit();
                fixture.sync();
                fixture.desktop.layers = fixture.desktop.server.layers();
                fixture.desktop.focus().unwrap();
                let target = fixture.desktop.layers[0].surface.clone();
                assert_eq!(
                    fixture
                        .desktop
                        .server
                        .state
                        .seat
                        .get_keyboard()
                        .unwrap()
                        .current_focus(),
                    Some(target)
                );
                Recovery::Layer(role, surface)
            }
            Self::Modal | Self::KeyboardModal => {
                let action = if matches!(self, Self::KeyboardModal) {
                    DesktopAction::Logout
                } else {
                    DesktopAction::OpenOperation
                };
                let serial = fixture.desktop.worker.input(Input::Action(action)).unwrap();
                advance(&mut fixture.desktop, serial);
                assert!(fixture.desktop.current.as_ref().unwrap().frame.input.modal);
                assert_eq!(
                    fixture
                        .desktop
                        .current
                        .as_ref()
                        .unwrap()
                        .frame
                        .input
                        .keyboard_modal,
                    matches!(self, Self::KeyboardModal)
                );
                assert!(fixture.desktop.worker.request().is_none());
                Recovery::Modal
            }
        }
    }
}

impl Recovery {
    fn finish(self, fixture: &mut Fixture) {
        match self {
            Self::Focus => fixture.desktop.event(WinitEvent::Focus(true)).unwrap(),
            Self::Lock(owner, surface) => {
                owner.unlock_and_destroy();
                surface.destroy();
                fixture.sync();
                assert!(!fixture.desktop.server.locked());
                fixture.desktop.focus().unwrap();
            }
            Self::Layer(role, surface) => {
                role.destroy();
                surface.destroy();
                fixture.sync();
                fixture.desktop.layers = fixture.desktop.server.layers();
                fixture.desktop.focus().unwrap();
            }
            Self::Modal => {
                fixture.desktop.key(1, true, 15).unwrap();
                fixture.desktop.key(1, false, 16).unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    fixture.desktop.dispatch().unwrap();
                    fixture.flush();
                    if !fixture.desktop.current.as_ref().unwrap().frame.input.modal {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "Operation did not close through native Wayland keyboard input"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
        fixture.flush();
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn active_modifier_and_client_resizes_cancel_on_takeover_and_quarantine_every_held_button() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    for client in [false, true] {
        let mut fixture =
            Fixture::new(&runtime, &directory.path().join(format!("active-{client}")));
        for takeover in [
            Takeover::Focus,
            Takeover::Lock,
            Takeover::Layer,
            Takeover::Modal,
            Takeover::KeyboardModal,
        ] {
            eprintln!("active client={client}, takeover={takeover:?}");
            let initial = mapping(&fixture.desktop, fixture.id);
            let button = fixture.begin(client, false);
            let recovery = takeover.apply(&mut fixture);
            fixture.cancelled(initial);
            recovery.finish(&mut fixture);
            fixture.release_and_check_normal_input(button, client);
        }
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn queued_client_resize_claims_are_cancelled_by_takeover_without_leaking_releases() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Fixture::new(&runtime, directory.path());
    for takeover in [
        Takeover::Focus,
        Takeover::Lock,
        Takeover::Layer,
        Takeover::Modal,
        Takeover::KeyboardModal,
    ] {
        eprintln!("queued takeover={takeover:?}");
        let initial = mapping(&fixture.desktop, fixture.id);
        let button = fixture.begin(true, true);
        let recovery = takeover.apply(&mut fixture);
        fixture.cancelled(initial);
        recovery.finish(&mut fixture);
        fixture.release_and_check_normal_input(button, true);
    }
    fixture.desktop.worker.stop().unwrap();
}
