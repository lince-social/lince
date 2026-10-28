use super::*;
use crate::{
    desktop::{Desktop, renderer},
    presentation::Presentation,
    runtime::{CanvasWorker, Configuration, Input},
};
use lince_interface::host::WakeSignal;
use smithay::{
    backend::{
        allocator::Fourcc,
        egl::{EGLContext, EGLDevice, EGLDisplay},
        renderer::{
            Bind, Offscreen,
            gles::{GlesRenderer, GlesTexture},
        },
    },
    utils::Transform,
};

mod ready;
mod control;
mod privacy;
mod hover_lifetime;
mod ime;
mod workspace_history;
mod affinity;
mod focus_policy;
mod opening;
mod configuration;
mod magnification;
mod native_pointer;
mod outputs;
mod appearance;
mod capture_lifetime;
mod windowed_fullscreen;
mod compact;
mod scratch;
#[cfg(feature = "session")]
mod power;

struct Fixture {
    size: [u32; 2],
    desktop: Desktop,
    connection: Connection,
    queue: EventQueue<Client>,
    client: Client,
    renderer: GlesRenderer,
    composition: renderer::Composition,
    texture: GlesTexture,
}

impl Fixture {
    fn new(
        harness: Harness,
        runtime: &tokio::runtime::Runtime,
        directory: &std::path::Path,
    ) -> Self {
        Self::with_size(harness, runtime, directory, [1000, 720])
    }

    fn with_size(harness: Harness, runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2]) -> Self {
        Self::with_connection(harness, runtime, directory, size, None)
    }

    fn with_cell(harness: Harness, runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], cell: lince_interface::native::app::CellHandle) -> Self {
        Self::with_connection(harness, runtime, directory, size, Some(cell))
    }

    fn with_linear_output(harness: Harness, runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2]) -> Self {
        let worker = CanvasWorker::spawn_for_compositor(Configuration { showoff: false, size, directory: directory.into(), runtime: runtime.handle().clone(), cell: None },
            WakeSignal::new(|| {}), true).unwrap();
        Self::with_worker(harness, size, worker)
    }

    fn with_connection(harness: Harness, runtime: &tokio::runtime::Runtime, directory: &std::path::Path, size: [u32; 2], cell: Option<lince_interface::native::app::CellHandle>) -> Self {
        let worker = CanvasWorker::spawn(Configuration {
            showoff: false,
            size,
            directory: directory.into(),
            runtime: runtime.handle().clone(),
            cell,
        })
        .unwrap();
        Self::with_worker(harness, size, worker)
    }

    fn with_worker(harness: Harness, size: [u32; 2], worker: CanvasWorker) -> Self {
        let Harness {
            server,
            connection,
            queue,
            client,
        } = harness;
        let device = EGLDevice::enumerate()
            .unwrap()
            .find(|device| !device.is_software())
            .unwrap();
        let display = unsafe { EGLDisplay::new(device) }.unwrap();
        let context = EGLContext::new(&display).unwrap();
        let mut renderer = unsafe { GlesRenderer::new(context) }.unwrap();
        let texture = renderer
            .create_buffer(Fourcc::Argb8888, (size[0] as i32, size[1] as i32).into())
            .unwrap();
        let mut composition = renderer::Composition::new(WakeSignal::new(|| {})).unwrap();
        composition.prepare_color(&mut renderer).unwrap();
        Self {
            size,
            desktop: Desktop::new(server, worker, size, 1, true),
            connection,
            queue,
            client,
            renderer,
            texture,
            composition,
        }
    }

    fn exchange(&mut self) {
        synchronize(
            &mut self.desktop.server,
            &self.connection,
            &mut self.queue,
            &mut self.client,
        )
        .unwrap();
    }

    fn tick(&mut self) {
        self.exchange();
        self.desktop.dispatch().unwrap();
        for request in self.desktop.server.output_requests() {
            let accepted = self.desktop.server.apply_virtual_output_configuration(&request, self.desktop.size).unwrap();
            self.desktop.server.output_configuration_finished(request.token, accepted);
            if accepted && !request.test { self.desktop.invalidate_outputs().unwrap(); }
        }
        assert!(self.desktop.worker.failure().is_none());
        self.desktop
            .accept(&mut self.composition, &mut self.renderer)
            .unwrap();
        if let Some(frame) = self.desktop.pending.as_ref().or(self.desktop.current.as_ref()) {
            self.composition.enable_client_formats(&self.renderer, &mut self.desktop.server, &frame.frame).unwrap();
        }
        if self
            .desktop
            .pending
            .as_ref()
            .is_some_and(|frame| frame.frame.ready().unwrap())
            || self.desktop.pending.is_none() && self.desktop.current.is_some() && self.desktop.redraw
        {
            let independent = self.desktop.pending.as_ref().or(self.desktop.current.as_ref()).and_then(|frame| {
                let outputs = &frame.frame.geometry.as_ref()?.outputs;
                (outputs.iter().any(|output| output.rotation != Default::default())
                    || outputs.len() == 1 && outputs[0].source_size() != self.size
                    || outputs.len() > 1 && outputs.iter().any(|output| output.raster.scale != [1.0; 2]))
                    .then(|| (frame.clone(), outputs.clone()))
            });
            if let Some((frame, outputs)) = independent {
                let mut protected = true;
                for output in outputs { protected &= self.draw_output(&frame, &output, false).1; }
                if protected { self.desktop.server.lock_presented(); }
                self.desktop.redraw = false;
                self.exchange();
                return;
            }
            let presented = {
                let mut target = self.renderer.bind(&mut self.texture).unwrap();
                let frame = self.desktop.pending.as_deref().or(self.desktop.current.as_deref());
                let output = frame.and_then(|frame| frame.frame.geometry.as_ref()).filter(|geometry| geometry.outputs.len() == 1)
                    .and_then(|geometry| geometry.outputs.first()).filter(|output| output.raster.scale != [1.0; 2]);
                if let Some(output) = output {
                    self.composition.render_output(&mut self.renderer, &mut target, output, &mut self.desktop.server, frame, self.desktop.pointer, Default::default()).unwrap()
                } else { self.composition.render(
                        &mut self.renderer,
                        &mut target,
                        (self.size[0] as i32, self.size[1] as i32).into(),
                        Transform::Normal,
                        &mut self.desktop.server,
                        frame,
                        self.desktop.pointer,
                    )
                    .unwrap() }
            };
            presented.sync.wait().unwrap();
            if presented.locked { self.desktop.server.lock_presented(); }
            self.desktop.presented(presented, 0).unwrap();
            self.desktop.redraw = false;
        }
        self.exchange();
    }

    fn draw_output(&mut self, frame: &Arc<crate::runtime::Completed>, output: &lince_interface::host::OutputViewport, read: bool) -> (Vec<u8>, bool) {
        use smithay::backend::renderer::{ExportMem, TextureMapping};
        let size = output.source_size().map(|value| i32::try_from(value).unwrap());
        let mut texture: GlesTexture = self.renderer.create_buffer(Fourcc::Argb8888, (size[0], size[1]).into()).unwrap();
        let (presented, pixels) = {
            let mut target = self.renderer.bind(&mut texture).unwrap();
            let presented = self.composition.render_output(&mut self.renderer, &mut target, output, &mut self.desktop.server,
                Some(frame.as_ref()), self.desktop.pointer, Default::default()).unwrap();
            presented.sync.wait().unwrap();
            let mut pixels = Vec::new();
            if read {
                let mapping = self.renderer.copy_framebuffer(&target, smithay::utils::Rectangle::from_size((size[0], size[1]).into()), Fourcc::Abgr8888).unwrap();
                let source = self.renderer.map_texture(&mapping).unwrap();
                let stride = size[0] as usize * 4;
                pixels.reserve(source.len());
                for row in 0..size[1] as usize {
                    let row = if mapping.flipped() { size[1] as usize - row - 1 } else { row };
                    pixels.extend_from_slice(&source[row * stride..(row + 1) * stride]);
                }
            }
            (Arc::new(presented), pixels)
        };
        let protected = presented.locked;
        self.desktop.output_presented(output.id, frame.clone(), presented, renderer::capture::monotonic(), 0).unwrap();
        (pixels, protected)
    }

    fn output_pixels(&mut self, id: OutputId) -> (lince_interface::host::OutputViewport, Vec<u8>) {
        let frame = self.desktop.current.as_ref().unwrap().clone();
        let output = frame.frame.geometry.as_ref().unwrap().outputs.iter().find(|output| output.id == id).unwrap().clone();
        let pixels = self.draw_output(&frame, &output, true).0;
        (output, pixels)
    }

    #[track_caller]
    fn until(&mut self, mut ready: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.tick();
            if ready(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "The real dispatcher did not reach the expected state: current={:?}, pending={:?}, hover={:?}, dragging={}, pointer={:?}, sent={:?}, native_keyboard={}",
                self.desktop.current.as_ref().map(|current| (
                    current.input_serial,
                    current.frame.input.keyboard_modal,
                    &current.frame.input.drop_target,
                    current.frame.output,
                    &current.frame.input.workspaces
                )),
                self.desktop.pending_output,
                self.desktop.server.native_file_drag(),
                self.desktop.server.dragging(),
                self.desktop.pointer,
                self.desktop.pointer_sent,
                self.desktop.native_keyboard,
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn inspect<T: Send + 'static>(
        &mut self,
        read: impl FnOnce(&mut Presentation) -> T + Send + 'static,
    ) -> T {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let serial = self
            .desktop
            .worker
            .input(Input::Test(Box::new(move |canvas| {
                sender.send(read(canvas)).ok().unwrap()
            })))
            .unwrap();
        self.until(|fixture| {
            fixture
                .desktop
                .current
                .as_ref()
                .is_some_and(|current| current.input_serial >= serial)
        });
        receiver.recv_timeout(Duration::from_secs(1)).unwrap()
    }

    fn pause(&self) -> (u64, std::sync::mpsc::SyncSender<()>) {
        let (blocked, paused) = std::sync::mpsc::sync_channel(1);
        let (release, resume) = std::sync::mpsc::sync_channel(1);
        let serial = self
            .desktop
            .worker
            .input(Input::Test(Box::new(move |_| {
                blocked.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(10)).unwrap();
            })))
            .unwrap();
        paused.recv_timeout(Duration::from_secs(5)).unwrap();
        (serial, release)
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_file_hover_survives_pending_presentation_and_uses_the_dispatcher_for_activation_and_drop() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    const BODY: &str = "Imported through the compositor dispatcher.";
    let path = directory.path().join("dispatcher.txt");
    std::fs::write(&path, BODY).unwrap();
    let mut harness = Harness::new();
    harness
        .server
        .configure_outputs(&[Monitor {
            id: OutputId(1),
            name: "Dispatcher output".into(),
            origin: [0, 0],
            size: [1000, 720],
        }])
        .unwrap();
    harness.sync();
    let window = harness.window();
    let _buffer = paint(&mut harness, &window, [320, 240], [0, 255, 0, 255]);
    let id = harness.server.windows()[0].id;
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    fixture.until(|fixture| {
        fixture.desktop.current.as_ref().is_some_and(|current| {
            current.frame.geometry.as_ref().is_some_and(|geometry| {
                geometry
                    .items
                    .iter()
                    .any(|item| item.id == ItemId::Application(id))
            })
        })
    });
    let (owner, destination, original) = fixture.inspect(move |canvas| {
        let original = member(canvas, id);
        let world = canvas.world_mut();
        let root = world
            .query_filtered::<Entity, With<native::workspace::Workspaces>>()
            .single(world)
            .unwrap();
        canvas.create_workspace(root.to_bits()).unwrap();
        let destination = canvas
            .world_mut()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        for workspace in [original.1, destination] {
            native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
        }
        canvas
            .activate_workspace(root.to_bits(), original.1)
            .unwrap();
        (root.to_bits(), destination, original)
    });
    fixture
        .desktop
        .send(Input::Action(DesktopAction::ToggleOverview))
        .unwrap();
    fixture.until(|fixture| {
        fixture
            .desktop
            .current
            .as_ref()
            .is_some_and(|current| current.frame.input.keyboard_modal)
    });
    let point = fixture.inspect(move |canvas| {
        let world = canvas.world_mut();
        let entity = world
            .query::<(Entity, &Name)>()
            .iter(world)
            .find(|(_, name)| name.as_str() == format!("overview.workspace.{destination}"))
            .unwrap()
            .0;
        super::overview::center(canvas, entity)
            .as_dvec2()
            .to_array()
    });
    let surface = fixture.desktop.server.windows()[0].surface.clone();
    fixture.desktop.server.focus_surface(
        Some(surface.clone()),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    fixture
        .desktop
        .server
        .pointer_motion(Some((surface, [20.0, 20.0])), 1)
        .unwrap();
    fixture.desktop.server.pointer_button(272, true, 2);
    fixture.exchange();
    let qh = fixture.queue.handle();
    let manager = fixture.client.data_manager.as_ref().unwrap();
    let device = manager.get_data_device(fixture.client.seat.as_ref().unwrap(), &qh, ());
    let source = manager.create_data_source(&qh, ());
    source.offer("text/uri-list".into());
    source.set_actions(wl_data_device_manager::DndAction::Copy);
    fixture.client.drop_payload = format!("file://{}\r\n", path.display()).into_bytes();
    device.start_drag(
        Some(&source),
        &window.surface,
        None,
        fixture.client.pointer_serial,
    );
    for _ in 0..4 {
        fixture.exchange();
    }
    let (serial, release) = fixture.pause();
    fixture.desktop.pending_output = Some((serial, None));
    fixture.desktop.pointer = point;
    fixture.desktop.motion(3).unwrap();
    for _ in 0..8 {
        fixture.exchange();
    }
    assert!(fixture.desktop.server.native_file_drag().is_some());
    fixture.desktop.dispatch().unwrap();
    assert!(fixture.desktop.pending_output.is_some());
    release.send(()).unwrap();
    fixture.until(|fixture| fixture.desktop.pending_output.is_none());
    let previous = fixture.desktop.server.native_file_drag().unwrap();
    let (_, release) = fixture.pause();
    let mut queued = 0;
    for _ in 0..=128 {
        match fixture
            .desktop
            .worker
            .input(Input::Pointer(point.map(|value| value as f32)))
        {
            Ok(_) => queued += 1,
            Err(std::sync::mpsc::TrySendError::Full(_)) => break,
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                panic!("The worker disconnected during the queue-pressure fixture")
            }
        }
    }
    assert_eq!(queued, 128);
    let surface = fixture.desktop.server.windows()[0].surface.clone();
    fixture
        .desktop
        .server
        .pointer_motion(Some((surface, [20.0, 20.0])), 4)
        .unwrap();
    for _ in 0..4 {
        fixture.exchange();
    }
    let native = fixture.desktop.server.native_surface().unwrap();
    fixture
        .desktop
        .server
        .pointer_motion(Some((native, point)), 5)
        .unwrap();
    for _ in 0..4 {
        fixture.exchange();
    }
    let replacement = fixture.desktop.server.native_file_drag().unwrap();
    assert_ne!(previous, replacement);
    fixture.desktop.dispatch().unwrap();
    assert!(matches!(
        fixture
            .desktop
            .worker
            .input(Input::Pointer(point.map(|value| value as f32))),
        Err(std::sync::mpsc::TrySendError::Full(_))
    ));
    release.send(()).unwrap();
    fixture.until(|fixture| {
        fixture.desktop.current.as_ref().is_some_and(|current| {
            !current.frame.input.keyboard_modal && current.frame.input.drop_target.is_some()
        })
    });
    assert_eq!(
        fixture.inspect(move |canvas| canvas
            .world_mut()
            .get::<native::workspace::Workspaces>(Entity::from_bits(owner))
            .unwrap()
            .active),
        destination
    );
    assert_eq!(fixture.inspect(move |canvas| member(canvas, id)), original);
    fixture.desktop.button(272, false, 6).unwrap();
    fixture.until(|fixture| fixture.client.drop_finished == 1);
    let deadline = Instant::now() + Duration::from_secs(10);
    let choice = loop {
        let choice = fixture.inspect(|canvas| {
            super::hover::control(canvas, "Display as Plain text Sand")?;
            super::hover::control(canvas, "Use editable text Sand")
        });
        if let Some(choice) = choice {
            break choice;
        }
        assert!(
            Instant::now() < deadline,
            "The dispatcher drop never reached the normal file choice"
        );
    };
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let trace = observed.clone();
    let point = fixture.inspect(move |canvas| {
        let press = trace.clone();
        let release = trace.clone();
        let click = trace.clone();
        canvas
            .world_mut()
            .entity_mut(choice)
            .observe(
                move |_: On<bevy::picking::events::Pointer<bevy::picking::events::Press>>| {
                    press.lock().unwrap().push("press");
                },
            )
            .observe(
                move |_: On<bevy::picking::events::Pointer<bevy::picking::events::Release>>| {
                    release.lock().unwrap().push("release");
                },
            )
            .observe(
                move |_: On<bevy::picking::events::Pointer<bevy::picking::events::Click>>| {
                    click.lock().unwrap().push("click");
                },
            )
            .observe(move |_: On<bevy::ui_widgets::Activate>| {
                trace.lock().unwrap().push("activate");
            });
        super::overview::center(canvas, choice)
            .as_dvec2()
            .to_array()
    });
    fixture.desktop.pointer = point;
    fixture.desktop.motion(7).unwrap();
    fixture.tick();
    assert!(
        matches!(
            fixture.desktop.target(),
            crate::desktop::input::Target::Native
        ),
        "The file choice must route to the native canvas"
    );
    let (_, release) = fixture.pause();
    for pressed in [true, false] {
        fixture.desktop.button(272, pressed, 8).unwrap();
    }
    release.send(()).unwrap();
    let serial = fixture
        .desktop
        .worker
        .input(Input::Test(Box::new(|_| {})))
        .unwrap();
    fixture.until(|fixture| {
        fixture
            .desktop
            .current
            .as_ref()
            .is_some_and(|current| current.input_serial >= serial)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let imported = loop {
        let imported = fixture.inspect(|canvas| {
            let world = canvas.world_mut();
            let fields: Vec<_> = world
                .query::<(Entity, &bevy::text::EditableText)>()
                .iter(world)
                .filter(|(_, text)| text.value().to_string() == BODY)
                .map(|(entity, _)| entity)
                .collect();
            fields
                .into_iter()
                .map(|mut owner| {
                    while world
                        .get::<native::workspace::WorkspaceMember>(owner)
                        .is_none()
                    {
                        owner = world.get::<ChildOf>(owner).unwrap().parent();
                    }
                    world
                        .get::<native::workspace::WorkspaceMember>(owner)
                        .unwrap()
                        .0
                })
                .collect::<Vec<_>>()
        });
        assert!(
            imported.len() <= 1,
            "The same file was imported more than once"
        );
        if let Some(workspace) = imported.first() {
            break *workspace;
        }
        if Instant::now() >= deadline {
            super::overview::capture(
                "dispatcher-import-missing",
                [1000, 720],
                1.0,
                &fixture.desktop.current.as_ref().unwrap().frame,
            );
            let windows = fixture.inspect(|canvas| {
                let world = canvas.world_mut();
                world
                    .query::<&bevy::window::Window>()
                    .iter(world)
                    .map(|window| (window.focused, window.cursor_position()))
                    .collect::<Vec<_>>()
            });
            panic!(
                "The acknowledged click never produced the requested editable Sand: observed={:?}, windows={windows:?}, pointer={point:?}, pending={:?}, dragging={}",
                observed.lock().unwrap(),
                fixture.desktop.pending_output,
                fixture.desktop.server.dragging()
            );
        }
    };
    assert_eq!(imported, destination);
    assert_eq!(fixture.inspect(move |canvas| member(canvas, id)), original);
    assert_eq!(fixture.client.drop_performed, 1);
    assert_eq!(fixture.client.drop_finished, 1);
    assert!(fixture.desktop.server.native_file_drag().is_none());
    let observed = observed.lock().unwrap();
    for expected in ["press", "click", "release", "activate"] {
        assert_eq!(
            observed.iter().filter(|event| **event == expected).count(),
            1,
            "Observed pointer sequence: {observed:?}"
        );
    }
    super::overview::capture(
        "dispatcher-imported",
        [1000, 720],
        1.0,
        &fixture.desktop.current.as_ref().unwrap().frame,
    );
    source.destroy();
    device.release();
    fixture.exchange();
}
