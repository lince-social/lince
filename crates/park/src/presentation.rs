use crate::graphics::{ExportedImage, ProbeFailure, VulkanRenderer};
use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::{
        RenderApp, RenderPlugin,
        render_asset::RenderAssets,
        renderer::{
            RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue,
            WgpuWrapper,
        },
        settings::RenderCreation,
        texture::GpuImage,
    },
    window::PrimaryWindow,
};
use lince_interface::{
    host::WakeSignal,
    native::{
        self,
        app::{CellHandle, OutputTarget},
    },
};
use std::{path::PathBuf, rc::Rc, sync::Arc};

#[cfg(feature = "native-client-gate")]
mod client_probe;
mod clients;
mod outputs;
mod records;
#[cfg(feature = "native-client-gate")]
pub use client_probe::client_probe;
pub use outputs::Output;

pub struct Presentation {
    app: App,
    renderer: Rc<VulkanRenderer>,
    target: Handle<Image>,
    windows: std::collections::HashMap<lince_interface::host::SurfaceId, clients::Window>,
    painted_windows:
        std::collections::BTreeMap<lince_interface::host::SurfaceId, crate::server::WindowSnapshot>,
    identities: std::collections::HashMap<
        lince_interface::host::SurfaceId,
        lince_interface::layout::desktop::PlacementId,
    >,
    uploads: Vec<Arc<crate::server::ClientBuffer>>,
    rejected: std::collections::HashSet<lince_interface::host::SurfaceId>,
    output_view: Option<lince_interface::host::OutputViewport>,
    render_size: [u32; 2],
    render_origin: [f64; 2],
    render_fullscreen: bool,
    last_geometry: Option<lince_interface::host::GeometrySnapshot>,
    window_requests: std::collections::HashMap<
        lince_interface::host::SurfaceId,
        lince_interface::host::WindowRequest,
    >,
    output_state: outputs::State,
    application_focus: Option<lince_interface::host::SurfaceId>,
    canonical_root: Option<Entity>,
    records: Option<records::Records>,
}

pub struct Frame {
    pub output: Option<lince_interface::host::OutputId>,
    image: ExportedImage,
    pub geometry: Option<Arc<lince_interface::host::GeometrySnapshot>>,
    pub input: InputSnapshot,
    pub windows: Vec<WindowConfiguration>,
    pub fullscreen: bool,
    pub origin: [f64; 2],
    pub outputs: Vec<OutputFrame>,
}

pub struct OutputFrame {
    pub id: lince_interface::host::OutputId,
    image: ExportedImage,
    pub origin: [f64; 2],
    pub fullscreen: bool,
}

impl OutputFrame {
    pub fn dmabuf(&self) -> &smithay::backend::allocator::dmabuf::Dmabuf {
        &self.image.dmabuf
    }
}

pub struct WindowConfiguration {
    pub surface: lince_interface::host::SurfaceId,
    pub size: [u32; 2],
    pub state: lince_interface::host::WindowState,
}

pub struct InputSnapshot {
    pub workspaces: crate::server::WorkspaceSnapshot,
    pub editor: Option<crate::native_input::Editor>,
    pub drop_target: Option<crate::native_input::transfer::Target>,
    pub controls: Vec<lince_interface::host::Rectangle>,
    pub modal: bool,
    pub keyboard: bool,
    pub bindings: Vec<native::shortcuts::DesktopBinding>,
}

#[derive(Resource)]
struct FrameFailure(String);

fn connect_cell(app: &mut App, runtime: CellHandle, data: PathBuf) -> std::io::Result<records::Records> {
    let records = records::Records::load(data.join("park-windows.json"))?;
    let storage = tokio::runtime::Handle::current().block_on(runtime.0.interface_storage())?;
    app.insert_resource(native::sound::Audio::open(
        data.clone(), app.world().get_resource::<WakeSignal>().cloned(),
    ));
    app.insert_resource(native::workspace::WorkspaceFile::with_settings(
        data.join("interface.json"),
        storage,
    )?)
    .insert_resource(runtime)
    .add_plugins(native::cell_bridge::CellBridgePlugin);
    Ok(records)
}

impl Frame {
    pub fn ready(&self) -> Result<bool, ProbeFailure> {
        if !self.image.ready()? { return Ok(false); }
        for output in &self.outputs {
            if !output.image.ready()? { return Ok(false); }
        }
        Ok(true)
    }

    pub fn wait_for_probe(&self) -> Result<(), ProbeFailure> {
        self.image.wait_for_probe()?;
        for output in &self.outputs { output.image.wait_for_probe()?; }
        Ok(())
    }

    pub fn dmabuf(&self) -> &smithay::backend::allocator::dmabuf::Dmabuf {
        &self.image.dmabuf
    }
}

pub fn probe() -> Result<(), ProbeFailure> {
    use native::actions::Action;
    let runtime = tokio::runtime::Runtime::new().map_err(|error| ProbeFailure {
        stage: "native_runtime".into(),
        detail: error.to_string(),
    })?;
    let _entered = runtime.enter();
    let directory = tempfile_directory()?;
    let mut canvas = Presentation::new(
        [640, 360],
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )?;
    let mut frame = None;
    for _ in 0..10 {
        frame = Some(canvas.probe_frame()?);
    }
    let before = probe_pixels(frame.as_ref().unwrap().dmabuf())?;
    let world = canvas.world_mut();
    let root = world
        .query_filtered::<Entity, With<native::container::BoxRoot>>()
        .iter(world)
        .next()
        .ok_or_else(|| input_failure("the native canvas root was not created"))?;
    native::edit_mode::EditAction::Open.apply(world, root);
    native::edit_mode::EditAction::AddSand(native::sand_store::SandKind::EditableText)
        .apply(world, root);
    for _ in 0..10 {
        frame = Some(canvas.probe_frame()?);
    }
    let after = probe_pixels(frame.as_ref().unwrap().dmabuf())?;
    let world = canvas.world_mut();
    if !world
        .query::<&native::sand_store::StoredSand>()
        .iter(world)
        .any(|sand| sand.kind == native::sand_store::SandKind::EditableText)
    {
        return Err(input_failure(
            "the native Editable Text Sand was not created",
        ));
    }
    let changed = before
        .chunks_exact(4)
        .zip(after.chunks_exact(4))
        .filter(|(before, after)| {
            before
                .iter()
                .zip(*after)
                .any(|(before, after)| before.abs_diff(*after) > 8)
        })
        .count();
    if changed < 1000 {
        return Err(input_failure(
            "the native Sand did not change the rendered canvas",
        ));
    }
    canvas.resize([800, 480])?;
    for _ in 0..3 {
        frame = Some(canvas.probe_frame()?);
    }
    use smithay::backend::allocator::Buffer;
    if frame.as_ref().unwrap().dmabuf().size() != (800, 480).into() {
        return Err(input_failure(
            "native output resize did not reach the exported frame",
        ));
    }
    native::edit_mode::EditAction::Close.apply(canvas.app.world_mut(), root);
    outputs::probe(&mut canvas)?;
    Ok(())
}

fn tempfile_directory() -> Result<tempfile::TempDir, ProbeFailure> {
    tempfile::tempdir().map_err(|error| ProbeFailure {
        stage: "native_probe_directory".into(),
        detail: error.to_string(),
    })
}

pub(crate) fn probe_pixels(
    buffer: &smithay::backend::allocator::dmabuf::Dmabuf,
) -> Result<Vec<u8>, ProbeFailure> {
    use smithay::backend::{
        allocator::{Buffer, Fourcc},
        egl::{EGLContext, EGLDevice, EGLDisplay},
        renderer::{ExportMem, ImportDma, gles::GlesRenderer},
    };
    let stage = |error: &dyn std::fmt::Display| ProbeFailure {
        stage: "native_probe_pixels".into(),
        detail: error.to_string(),
    };
    let device = EGLDevice::enumerate()
        .map_err(|error| stage(&error))?
        .find(|device| !device.is_software())
        .ok_or_else(|| input_failure("no hardware EGL device for the native canvas proof"))?;
    let display = unsafe { EGLDisplay::new(device) }.map_err(|error| stage(&error))?;
    let context = EGLContext::new(&display).map_err(|error| stage(&error))?;
    let mut renderer = unsafe { GlesRenderer::new(context) }.map_err(|error| stage(&error))?;
    let texture = renderer
        .import_dmabuf(buffer, None)
        .map_err(|error| stage(&error))?;
    let mapping = renderer
        .copy_texture(
            &texture,
            smithay::utils::Rectangle::from_size(buffer.size()),
            Fourcc::Abgr8888,
        )
        .map_err(|error| stage(&error))?;
    Ok(renderer
        .map_texture(&mapping)
        .map_err(|error| stage(&error))?
        .to_vec())
}

impl Presentation {
    pub fn window_request(
        &mut self,
        surface: lince_interface::host::SurfaceId,
        request: lince_interface::host::WindowRequest,
    ) {
        let pending = self.window_requests.entry(surface).or_default();
        if request.minimized.is_some() { pending.minimized = request.minimized; }
        if request.fullscreen.is_some() {
            pending.fullscreen = request.fullscreen;
        }
        if request.maximized.is_some() {
            pending.maximized = request.maximized;
        }
    }

    fn apply_window_requests(&mut self) {
        let requests: Vec<_> = self
            .window_requests
            .iter()
            .map(|(id, request)| (*id, *request))
            .collect();
        for (id, request) in requests {
            let Some(window) = self.windows.get(&id) else {
                continue;
            };
            let world = self.app.world_mut();
            let Some((root, workspace, placement)) =
                world.get::<ChildOf>(window.entity).and_then(|parent| {
                    Some((
                        parent.parent(),
                        world
                            .get::<native::workspace::WorkspaceMember>(window.entity)?
                            .0,
                        lince_interface::layout::desktop::PlacementId(
                            world.get::<native::layout::LayoutBox>(window.entity)?.id,
                        ),
                    ))
                })
            else {
                continue;
            };
            if let Some(mut spaces) = world.get_mut::<native::workspace::Workspaces>(root)
                && let Some(space) = spaces
                    .entries
                    .iter_mut()
                    .find(|space| space.id == workspace)
                && space.desktop.request_state(placement, request)
            {
                self.window_requests.remove(&id);
            }
        }
    }

    pub fn desktop_action(
        &mut self,
        action: lince_interface::host::DesktopAction,
    ) -> Result<(), ProbeFailure> {
        use native::actions::Action;
        if action == lince_interface::host::DesktopAction::InspectWindowRecord {
            self.inspect_window_record();
            return Ok(());
        }
        self.application_focus = None;
        if self.output_action(action)? {
            return Ok(());
        }
        let root = self.canvas_root()?;
        action.apply(self.app.world_mut(), root);
        Ok(())
    }

    pub fn activate_workspace(&mut self, owner: u64, workspace: u64) -> Result<(), ProbeFailure> {
        let root = self.canvas_root()?;
        if root.to_bits() != owner || native::laboratory::temporary(self.app.world(), root, workspace) { return Ok(()); }
        let index = self.app.world().get::<native::workspace::Workspaces>(root)
            .and_then(|spaces| spaces.entries.iter().position(|space| space.id == workspace))
            .and_then(|index| u32::try_from(index).ok()).and_then(|index| index.checked_add(1));
        if let Some(index) = index { self.desktop_action(lince_interface::host::DesktopAction::Workspace(index))?; }
        Ok(())
    }

    fn workspace_snapshot(&mut self) -> crate::server::WorkspaceSnapshot {
        let Ok(root) = self.canvas_root() else { return Default::default() };
        let world = self.app.world();
        let Some(spaces) = world.get::<native::workspace::Workspaces>(root) else { return Default::default() };
        let views = world.get_resource::<native::output_views::OutputViews>();
        crate::server::WorkspaceSnapshot {
            owner: root.to_bits(),
            entries: spaces.entries.iter().filter(|space| !native::laboratory::temporary(world, root, space.id))
                .map(|space| crate::server::WorkspaceEntry {
                    id: space.id, name: space.name.chars().take(1024).collect(),
                    active: views.map_or(spaces.active == space.id, |views| views.outputs.iter().any(|output| output.workspace == space.id)),
                }).take(4096).collect(),
        }
    }

    pub fn focus_application(&mut self, surface: lince_interface::host::SurfaceId) {
        if let Some(window) = self.windows.get(&surface) {
            self.application_focus = Some(surface);
            native::layout::desktop::focus_item(self.app.world_mut(), window.entity);
            if let Some(mut focus) = self
                .app
                .world_mut()
                .get_resource_mut::<bevy::input_focus::InputFocus>()
            {
                focus.clear();
            }
        }
    }

    pub fn viewport(
        &mut self,
        viewport: lince_interface::host::OutputViewport,
    ) -> Result<(), ProbeFailure> {
        self.outputs(vec![Output {
            name: format!("Park-{}", viewport.id.0),
            viewport,
        }])
    }

    fn refresh_viewport(&mut self) -> Result<(), ProbeFailure> {
        if !self.output_state.outputs.is_empty() {
            return self.refresh_outputs();
        }
        let Some(viewport) = self.output_view.clone() else {
            return Ok(());
        };
        let fullscreen = self
            .app
            .world_mut()
            .query::<&native::workspace::Workspaces>()
            .iter(self.app.world())
            .any(|spaces| {
                spaces
                    .entries
                    .iter()
                    .find(|entry| entry.id == spaces.active)
                    .is_some_and(|workspace| workspace.desktop.fullscreen().is_some())
            });
        let region = if fullscreen {
            Some(viewport.bounds)
        } else {
            viewport.usable
        };
        let size = region.map_or([1; 2], |region| {
            region.size().map(|value| value.round().max(1.0) as u32)
        });
        let origin = region.map_or([0.0; 2], |region| {
            std::array::from_fn(|axis| region.origin()[axis] - viewport.bounds.origin()[axis])
        });
        if size != self.render_size {
            self.resize(size)?;
        }
        let bounds = lince_interface::host::Rectangle::new([0.0; 2], size.map(f64::from))
            .ok_or_else(|| input_failure("invalid native view bounds"))?;
        let local = lince_interface::host::OutputViewport {
            bounds,
            usable: region.map(|_| bounds),
            physical_size: size,
            ..viewport
        };
        let root = self.canvas_root()?;
        if self
            .app
            .world()
            .get::<native::layout::desktop::HostViewport>(root)
            .is_none_or(|current| current.0 != local)
        {
            self.app.world_mut().entity_mut(root).insert((
                native::layout::desktop::Output(local.id),
                native::layout::desktop::HostViewport(local),
            ));
        }
        self.render_origin = origin;
        self.render_fullscreen = fullscreen;
        Ok(())
    }

    fn canvas_root(&mut self) -> Result<Entity, ProbeFailure> {
        if let Some(root) = self.canonical_root {
            return Ok(root);
        }
        let world = self.app.world_mut();
        let root = world
            .query_filtered::<Entity, (
                With<native::container::BoxRoot>,
                Without<native::laboratory::LaboratoryRoot>,
            )>()
            .iter(world)
            .next()
            .ok_or_else(|| input_failure("the canvas has not started"))?;
        self.canonical_root = Some(root);
        Ok(root)
    }
    pub fn host_requests(
        &mut self,
        sender: std::sync::mpsc::SyncSender<lince_interface::host::HostRequest>,
    ) {
        self.app
            .insert_resource(native::layout::desktop::HostRequests(sender));
    }
    pub fn new(size: [u32; 2], assets: PathBuf, wake: WakeSignal) -> Result<Self, ProbeFailure> {
        Self::new_with_cell(size, assets, wake, None)
    }

    pub(crate) fn new_with_cell(
        size: [u32; 2],
        assets: PathBuf,
        wake: WakeSignal,
        connection: Option<(CellHandle, PathBuf)>,
    ) -> Result<Self, ProbeFailure> {
        validate_size(size)?;
        let renderer = Rc::new(VulkanRenderer::new()?);
        let render_creation = RenderCreation::manual(
            RenderDevice::from(renderer.device.clone()),
            RenderQueue(Arc::new(WgpuWrapper::new(renderer.queue.clone()))),
            RenderAdapterInfo(WgpuWrapper::new(renderer.adapter.get_info())),
            RenderAdapter(Arc::new(WgpuWrapper::new(renderer.adapter.clone()))),
            RenderInstance(Arc::new(WgpuWrapper::new(renderer._instance.clone()))),
        );
        let plugins = DefaultPlugins
            .build()
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
            .set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: (size[0], size[1]).into(),
                    title: "Lince Park canvas".into(),
                    ..default()
                }),
                exit_condition: bevy::window::ExitCondition::DontExit,
                close_when_requested: false,
                ..default()
            })
            .set(RenderPlugin {
                render_creation,
                synchronous_pipeline_compilation: true,
                ..default()
            });
        let mut app = native::app::presentation_app(assets, plugins);
        app.insert_resource(bevy::render::error_handler::RenderErrorHandler(
            |error, world, _| {
                world.insert_resource(FrameFailure(format!("{error:?}")));
                bevy::render::error_handler::RenderErrorPolicy::StopRendering
            },
        ));
        app.insert_resource(native::layout::desktop::DefaultLayout(
            lince_interface::host::WorkspaceLayout::ScrollingColumns,
        ));
        app.sub_app_mut(RenderApp).insert_resource(wake.clone());
        app.insert_resource(wake)
            .add_systems(Startup, native::app::canvas)
            .add_systems(
                PostUpdate,
                refresh_camera_targets.before(bevy::camera::CameraUpdateSystems),
            );
        clients::install(&mut app)?;
        crate::native_input::install(&mut app);
        let records = connection
            .map(|(runtime, data)| connect_cell(&mut app, runtime, data))
            .transpose()
            .map_err(|error| input_failure(&error.to_string()))?;
        app.finish();
        app.cleanup();
        let target = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(output_image(size));
        app.insert_resource(OutputTarget(RenderTarget::Image(target.clone().into())));
        Ok(Self {
            app,
            renderer,
            target,
            windows: Default::default(),
            painted_windows: Default::default(),
            identities: Default::default(),
            uploads: Vec::new(),
            rejected: Default::default(),
            output_view: None,
            render_size: size,
            render_origin: [0.0; 2],
            render_fullscreen: false,
            last_geometry: None,
            window_requests: Default::default(),
            output_state: Default::default(),
            application_focus: None,
            canonical_root: None,
            records,
        })
    }

    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    pub fn pointer(&mut self, position: [f32; 2]) -> Result<(), ProbeFailure> {
        if position.iter().any(|value| !value.is_finite()) {
            return Err(input_failure("pointer coordinates must be finite"));
        }
        let window = self.input_window()?;
        let world = self.app.world_mut();
        let position = Vec2::from_array(position);
        let mut state = world.get_mut::<Window>(window).unwrap();
        let delta = state.cursor_position().map(|last| position - last);
        state.set_cursor_position(Some(position));
        let event = bevy::window::CursorMoved {
            window,
            position,
            delta,
        };
        world.write_message(bevy::window::WindowEvent::CursorMoved(event.clone()));
        world.write_message(event);
        Ok(())
    }

    pub fn button(
        &mut self,
        button: bevy::input::mouse::MouseButton,
        pressed: bool,
    ) -> Result<(), ProbeFailure> {
        if pressed {
            self.application_focus = None;
        }
        let event = bevy::input::mouse::MouseButtonInput {
            window: self.input_window()?,
            button,
            state: if pressed {
                bevy::input::ButtonState::Pressed
            } else {
                bevy::input::ButtonState::Released
            },
        };
        self.app
            .world_mut()
            .write_message(bevy::window::WindowEvent::MouseButtonInput(event.clone()));
        self.app.world_mut().write_message(event);
        Ok(())
    }

    pub fn key(
        &mut self,
        physical: KeyCode,
        logical: bevy::input::keyboard::Key,
        text: Option<String>,
        pressed: bool,
        repeat: bool,
    ) -> Result<(), ProbeFailure> {
        let event = bevy::input::keyboard::KeyboardInput {
            window: self.input_window()?,
            key_code: physical,
            logical_key: logical,
            text: text.map(Into::into),
            repeat,
            state: if pressed {
                bevy::input::ButtonState::Pressed
            } else {
                bevy::input::ButtonState::Released
            },
        };
        self.app
            .world_mut()
            .write_message(bevy::window::WindowEvent::KeyboardInput(event.clone()));
        self.app.world_mut().write_message(event);
        Ok(())
    }

    pub fn scroll(
        &mut self,
        delta: [f32; 2],
        unit: bevy::input::mouse::MouseScrollUnit,
        phase: bevy::input::touch::TouchPhase,
    ) -> Result<(), ProbeFailure> {
        if delta.iter().any(|value| !value.is_finite()) {
            return Err(input_failure("scroll values must be finite"));
        }
        let event = bevy::input::mouse::MouseWheel {
            window: self.input_window()?,
            x: delta[0],
            y: delta[1],
            unit,
            phase,
        };
        self.app
            .world_mut()
            .write_message(bevy::window::WindowEvent::MouseWheel(event.clone()));
        self.app.world_mut().write_message(event);
        Ok(())
    }

    pub fn focus(&mut self, focused: bool) -> Result<(), ProbeFailure> {
        let window = self.input_window()?;
        self.app
            .world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .focused = focused;
        let event = bevy::window::WindowFocused { window, focused };
        self.app
            .world_mut()
            .write_message(bevy::window::WindowEvent::WindowFocused(event.clone()));
        self.app.world_mut().write_message(event);
        if !focused {
            crate::native_input::cancel(self.app.world_mut());
            cancel_input(self.app.world_mut(), window);
        }
        Ok(())
    }

    pub fn text(&mut self, value: String) -> Result<(), ProbeFailure> {
        let event = bevy::window::Ime::Commit {
            window: self.input_window()?,
            value,
        };
        self.app
            .world_mut()
            .write_message(bevy::window::WindowEvent::Ime(event.clone()));
        self.app.world_mut().write_message(event);
        Ok(())
    }

    pub(crate) fn finish(&mut self) -> Result<(), ProbeFailure> {
        native::laboratory::close(self.app.world_mut());
        self.frame()?;
        let window = self.input_window()?;
        let focused = self
            .app
            .world()
            .get::<Window>(window)
            .is_some_and(|window| window.focused);
        self.focus(false)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut requested = false;
        let result = (|| {
            loop {
                self.frame()?;
                if requested && let Some(exit) = self.app.should_exit() {
                    return if exit == AppExit::Success {
                        Ok(())
                    } else {
                        Err(input_failure("Native workspace shutdown failed"))
                    };
                }
                if !requested && native::app::text_ready_to_close(self.app.world_mut()) {
                    self.app.world_mut().write_message(AppExit::Success);
                    requested = true;
                }
                if std::time::Instant::now() >= deadline {
                    return Err(input_failure(
                        "Logout cancelled because native changes could not be saved. Resolve pending edits or storage errors and try again.",
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        })();
        if result.is_err() {
            native::app::cancel_close(self.app.world_mut());
            self.focus(focused)?;
        }
        result
    }

    fn input_window(&mut self) -> Result<Entity, ProbeFailure> {
        let world = self.app.world_mut();
        world
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .iter(world)
            .next()
            .ok_or_else(|| input_failure("the canvas input viewport is unavailable"))
    }

    pub fn resize(&mut self, size: [u32; 2]) -> Result<(), ProbeFailure> {
        validate_size(size)?;
        let world = self.app.world_mut();
        let target = world
            .resource_mut::<Assets<Image>>()
            .add(output_image(size));
        let destination = RenderTarget::Image(target.clone().into());
        if let Some(camera) = world
            .get_resource::<native::topology::presentation::SceneCamera>()
            .map(|camera| camera.0)
        {
            world.entity_mut(camera).insert(destination.clone());
        }
        if let Some(camera) = world
            .get_resource::<native::topology::presentation::BackgroundCamera>()
            .map(|camera| camera.0)
        {
            world.entity_mut(camera).insert(destination.clone());
        }
        for mut window in world
            .query_filtered::<&mut Window, With<PrimaryWindow>>()
            .iter_mut(world)
        {
            window.resolution.set_physical_resolution(size[0], size[1]);
        }
        world.insert_resource(OutputTarget(destination));
        world
            .resource_mut::<Assets<Image>>()
            .remove(self.target.id());
        self.target = target;
        self.render_size = size;
        Ok(())
    }

    pub fn probe_frame(&mut self) -> Result<Frame, ProbeFailure> {
        let frame = self.frame()?;
        frame.wait_for_probe()?;
        Ok(frame)
    }

    pub(crate) fn imports_ready(&self) -> bool { self.renderer.imports_ready() }

    pub fn frame(&mut self) -> Result<Frame, ProbeFailure> {
        if let Some(records) = &mut self.records {
            records.tick(self.app.world_mut(), &self.windows, &self.identities);
        }
        if let Some(uid) = self
            .records
            .as_mut()
            .and_then(records::Records::take_inspection)
        {
            let root = self.canvas_root()?;
            native::full_record::open(
                self.app.world_mut(),
                root,
                &uid,
                native::protein_area::Source::Local,
            );
        }
        self.apply_window_requests();
        self.refresh_viewport()?;
        self.app.update();
        if let Some(error) = self.app.world().get_resource::<FrameFailure>() {
            return Err(ProbeFailure {
                stage: "native_frame".into(),
                detail: error.0.clone(),
            });
        }
        self.apply_window_requests();
        let texture = self
            .app
            .sub_app(RenderApp)
            .world()
            .resource::<RenderAssets<GpuImage>>()
            .get(self.target.id())
            .ok_or_else(|| ProbeFailure {
                stage: "native_frame".into(),
                detail: "the canvas output is not prepared".into(),
            })?
            .texture
            .clone();
        let uploads = std::mem::take(&mut self.uploads);
        self.renderer.queue.on_submitted_work_done(move || {
            for buffer in uploads { buffer.release(); }
        });
        let input = self.input_snapshot();
        let windows = self.window_configurations();
        let mut outputs = Vec::new();
        for (id, target) in &self.output_state.targets {
            if Some(*id) == self.output_state.assignments.focused() {
                continue;
            }
            let Some(texture) = self
                .app
                .sub_app(RenderApp)
                .world()
                .resource::<RenderAssets<GpuImage>>()
                .get(target.image.id())
            else {
                continue;
            };
            outputs.push(OutputFrame {
                id: *id,
                image: self.renderer.export_copy(&texture.texture)?,
                origin: target.origin,
                fullscreen: target.fullscreen,
            });
        }
        Ok(Frame {
            output: self.output_state.assignments.focused(),
            image: self.renderer.export_copy(&texture)?,
            geometry: self.frame_geometry().map(Arc::new),
            input,
            windows,
            fullscreen: self.render_fullscreen,
            origin: self.render_origin,
            outputs,
        })
    }

    fn window_configurations(&self) -> Vec<WindowConfiguration> {
        if self.output_state.outputs.is_empty()
            && !self.render_fullscreen
            && self
                .output_view
                .as_ref()
                .is_some_and(|view| view.usable.is_none())
        {
            return Vec::new();
        }
        let world = self.app.world();
        self.windows
            .iter()
            .filter_map(|(id, window)| {
                let parent = world.get::<ChildOf>(window.entity)?.parent();
                let spaces = world.get::<native::workspace::Workspaces>(parent)?;
                let workspace_id = world
                    .get::<native::workspace::WorkspaceMember>(window.entity)?
                    .0;
                let item = world.get::<native::canvas::CanvasItem>(window.entity)?;
                let workspace = spaces
                    .entries
                    .iter()
                    .find(|space| space.id == workspace_id)?;
                let placement = lince_interface::layout::desktop::PlacementId(
                    world.get::<native::layout::LayoutBox>(window.entity)?.id,
                );
                Some(WindowConfiguration {
                    surface: *id,
                    size: [
                        item.size.x.round().max(1.0) as u32,
                        item.size.y.round().max(1.0) as u32,
                    ],
                    state: lince_interface::host::WindowState {
                        minimized: workspace.desktop.minimized(placement),
                        fullscreen: workspace.desktop.fullscreen() == Some(placement),
                        maximized: workspace.desktop.maximized(placement),
                    },
                })
            })
            .collect()
    }

    fn frame_geometry(&mut self) -> Option<lince_interface::host::GeometrySnapshot> {
        let mut geometry = self.geometry()?.clone();
        if let Some(views) = self
            .app
            .world()
            .get_resource::<native::output_views::OutputViews>()
            .cloned()
            && let Some(root) = views.canvas
        {
            geometry.items = views
                .outputs
                .iter()
                .filter(|output| output.usable.is_some())
                .flat_map(|output| {
                    native::topology::output_views::geometry(self.app.world_mut(), root, output)
                })
                .collect();
        }
        geometry.items.retain(|item| {
            let lince_interface::host::ItemId::Application(surface) = item.id else {
                return true;
            };
            self.windows.get(&surface).is_some_and(|window| {
                let world = self.app.world();
                world
                    .get::<native::castle::Pending>(window.entity)
                    .is_none()
                    && native::topology::presentation::displayed(world, window.entity)
                    && world
                        .get::<native::topology::presentation::Surface>(window.entity)
                        .is_some_and(|surface| surface.visible)
            })
        });
        if geometry
            .focused
            .is_some_and(|focused| !geometry.items.iter().any(|item| item.id == focused))
        {
            geometry.focused = None;
        }
        if let Some(surface) = self.application_focus {
            let focused = lince_interface::host::ItemId::Application(surface);
            if geometry.items.iter().any(|item| {
                item.id == focused
                    && self
                        .output_state
                        .assignments
                        .focused()
                        .is_none_or(|output| output == item.output)
            }) {
                geometry.focused = Some(focused);
            } else if self.windows.get(&surface).is_none_or(|window| {
                self.app
                    .world()
                    .get::<native::castle::Pending>(window.entity)
                    .is_none()
                    && native::topology::presentation::displayed(self.app.world(), window.entity)
            }) {
                self.application_focus = None;
            }
        }
        if !self.output_state.outputs.is_empty() {
            for item in &mut geometry.items {
                let target = self.output_state.targets.get(&item.output)?;
                item.mapping = item.mapping.translated(target.origin)?;
                if let Some(polygons) = &mut item.clip {
                    for point in polygons.iter_mut().flatten() {
                        for axis in 0..2 {
                            point[axis] += target.origin[axis];
                        }
                    }
                }
            }
            geometry.outputs = self
                .output_state
                .outputs
                .iter()
                .map(|output| {
                    let mut viewport = output.viewport.clone();
                    viewport.workspace = self
                        .output_state
                        .assignments
                        .outputs()
                        .iter()
                        .find(|candidate| candidate.id == viewport.id)
                        .unwrap()
                        .active;
                    if let Some((_, output_id, workspace, _)) =
                        native::laboratory::scope(self.app.world())
                        && viewport.id == output_id
                    {
                        viewport.workspace = workspace;
                    }
                    viewport
                })
                .collect();
        } else if let Some(viewport) = &self.output_view {
            let offset: [f64; 2] = std::array::from_fn(|axis| {
                viewport.bounds.origin()[axis] + self.render_origin[axis]
            });
            for item in &mut geometry.items {
                item.mapping = item.mapping.translated(offset)?;
            }
            let mut output = viewport.clone();
            if let Some(local) = geometry.outputs.first() {
                output.workspace = local.workspace;
            }
            geometry.outputs = vec![output];
            if !self.render_fullscreen && viewport.usable.is_none() {
                geometry.items.clear();
                geometry.focused = None;
            }
        }
        geometry.revision = self.last_geometry.as_ref().map_or(1, |previous| {
            previous.revision
                + u64::from(
                    previous.outputs != geometry.outputs
                        || previous.items != geometry.items
                        || previous.focused != geometry.focused,
                )
        });
        self.last_geometry = Some(geometry.clone());
        Some(geometry)
    }

    fn input_snapshot(&mut self) -> InputSnapshot {
        use bevy::input_focus::tab_navigation::TabGroup;
        let workspaces = self.workspace_snapshot();
        let world = self.app.world_mut();
        let output = self.target.id();
        let offset: [f64; 2] = std::array::from_fn(|axis| {
            self.output_view
                .as_ref()
                .map_or(0.0, |viewport| viewport.bounds.origin()[axis])
                + self.render_origin[axis]
        });
        let window = world.query_filtered::<Entity, With<PrimaryWindow>>().iter(world).next();
        let output_id = self.output_view.as_ref().map_or(lince_interface::host::OutputId(1), |output| output.id);
        let drop_target = window.and_then(|window| crate::native_input::transfer::capture(world, window, output, output_id, offset));
        let nodes: Vec<_> = world
            .query::<(
                Entity,
                &ComputedNode,
                &UiGlobalTransform,
                &bevy::ui::ComputedUiTargetCamera,
                Option<&InheritedVisibility>,
                Option<&TabGroup>,
                Option<&GlobalZIndex>,
            )>()
            .iter(world)
            .filter_map(|(entity, node, transform, camera, visible, modal, z)| {
                let camera = camera.get()?;
                let RenderTarget::Image(target) = world.get::<RenderTarget>(camera)? else {
                    return None;
                };
                if target.handle.id() != output
                    || visible.is_some_and(|visible| !visible.get())
                    || node.size().min_element() <= 0.0
                {
                    return None;
                }
                let control = world.get::<native::actions::ActionButton>(entity).is_some()
                    || world.get::<bevy::text::EditableText>(entity).is_some()
                    || z.is_some_and(|z| z.0 > 0);
                let rectangle = if control {
                    let half = node.size() * 0.5;
                    let corners = [
                        -half,
                        Vec2::new(half.x, -half.y),
                        half,
                        Vec2::new(-half.x, half.y),
                    ]
                    .map(|point| transform.transform_point2(point));
                    let mut low = corners
                        .into_iter()
                        .fold(Vec2::splat(f32::INFINITY), Vec2::min);
                    let mut high = corners
                        .into_iter()
                        .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
                    if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(entity) {
                        low = low.max(clip.clip.min);
                        high = high.min(clip.clip.max);
                    }
                    if (high - low).min_element() <= 0.0 {
                        return None;
                    }
                    lince_interface::host::Rectangle::new(
                        std::array::from_fn(|axis| f64::from(low[axis]) + offset[axis]),
                        (high - low).as_dvec2().to_array(),
                    )
                } else {
                    None
                };
                Some((rectangle, modal.is_some_and(|group| group.modal)))
            })
            .collect();
        let modal = nodes.iter().any(|(_, modal)| *modal)
            || world
                .query::<&native::edit_mode::EditMode>()
                .iter(world)
                .any(|mode| mode.enabled);
        let mut focused = world
            .get_resource::<bevy::input_focus::InputFocus>()
            .and_then(bevy::input_focus::InputFocus::get);
        let mut keyboard = modal;
        for _ in 0..native::layout::MAX_DEPTH {
            let Some(entity) = focused else {
                break;
            };
            keyboard |= world.get::<bevy::text::EditableText>(entity).is_some();
            focused = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        InputSnapshot {
            workspaces,
            drop_target,
            editor: crate::native_input::capture(world, output, offset),
            controls: nodes
                .into_iter()
                .filter_map(|(rectangle, _)| rectangle)
                .collect(),
            modal,
            keyboard,
            bindings: native::shortcuts::desktop_bindings(world),
        }
    }
}

fn refresh_camera_targets(
    mut cameras: Query<&mut Projection, (With<Camera>, Changed<RenderTarget>)>,
) {
    for mut projection in &mut cameras {
        projection.set_changed();
    }
}

impl Drop for Presentation {
    fn drop(&mut self) {
        let _ = self.renderer.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(5)),
        });
        for buffer in self.uploads.drain(..) {
            buffer.release();
        }
    }
}

fn output_image(size: [u32; 2]) -> Image {
    let mut image =
        Image::new_target_texture(size[0], size[1], wgpu::TextureFormat::Bgra8UnormSrgb, None);
    image.texture_descriptor.usage |= wgpu::TextureUsages::COPY_SRC;
    image
}

fn validate_size(size: [u32; 2]) -> Result<(), ProbeFailure> {
    if size.iter().any(|size| *size == 0 || *size > 16384) {
        return Err(ProbeFailure {
            stage: "native_output_size".into(),
            detail: "invalid output dimensions".into(),
        });
    }
    Ok(())
}

fn input_failure(detail: &str) -> ProbeFailure {
    ProbeFailure {
        stage: "native_input".into(),
        detail: detail.into(),
    }
}

fn cancel_input(world: &mut World, window: Entity) {
    use bevy::{
        ecs::message::Messages,
        input::{
            keyboard::{Key, KeyboardFocusLost, KeyboardInput},
            mouse::{MouseButton, MouseButtonInput},
        },
        picking::pointer::{PointerAction, PointerId, PointerInput, PointerLocation},
        window::{CursorLeft, WindowEvent},
    };
    world.resource_mut::<Messages<KeyboardInput>>().clear();
    world.resource_mut::<Messages<MouseButtonInput>>().clear();
    let retained = world
        .resource_mut::<Messages<WindowEvent>>()
        .drain()
        .filter(|event| {
            !matches!(
                event,
                WindowEvent::KeyboardInput(_)
                    | WindowEvent::MouseButtonInput(_)
                    | WindowEvent::MouseWheel(_)
            )
        })
        .collect::<Vec<_>>();
    world.write_message_batch(retained);
    world.resource_mut::<ButtonInput<KeyCode>>().release_all();
    world.resource_mut::<ButtonInput<Key>>().release_all();
    world
        .resource_mut::<ButtonInput<MouseButton>>()
        .release_all();
    world.write_message(KeyboardFocusLost);
    world
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(None);
    let left = CursorLeft { window };
    world.write_message(WindowEvent::CursorLeft(left.clone()));
    world.write_message(left);
    let location = world
        .query::<(&PointerId, &PointerLocation)>()
        .iter(world)
        .find(|(id, _)| **id == native::topology::input::CONTENT_POINTER)
        .and_then(|(_, location)| location.location.clone());
    if let Some(location) = location {
        world.write_message(PointerInput {
            pointer_id: native::topology::input::CONTENT_POINTER,
            location,
            action: PointerAction::Cancel,
        });
    }
    if let Some(mut state) = world.get_resource_mut::<native::topology::input::PointerState>() {
        *state = Default::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod showoff;
    use bevy::{
        ecs::message::{MessageCursor, Messages},
        input::{
            ButtonState, InputPlugin,
            keyboard::{Key, KeyboardInput},
            mouse::{MouseButton, MouseButtonInput},
        },
        picking::pointer::{Location, PointerAction, PointerInput, PointerLocation},
        window::WindowEvent,
    };

    #[test]
    #[ignore = "requires hardware Vulkan and EGL"]
    fn connected_cell_is_attached_before_render_plugins_finish() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(32 * 1024 * 1024)
            .build()
            .unwrap();
        let _entered = runtime.enter();
        let directory = tempfile::tempdir().unwrap();
        let engine = Arc::new(runtime.block_on(engine::Engine::open_memory()).unwrap());
        let cell = CellHandle(cell::CellRuntime {
            commands: cell::terminal::commands::CommandHost::new(directory.path().into()),
            store: engine.store.clone(),
            engine,
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            information: None,
            fiote: None,
            speech: None,
        });
        let mut canvas = Presentation::new_with_cell(
            [640, 360],
            directory.path().join("assets"),
            WakeSignal::new(|| {}),
            Some((cell, directory.path().into())),
        )
        .unwrap();
        for _ in 0..10 {
            canvas.probe_frame().unwrap();
        }
        assert!(canvas.records.is_some());
        assert!(canvas.app.world().contains_resource::<CellHandle>());
        assert!(canvas.app.world().get_non_send::<native::cell_bridge::CellBridge>().is_some());
        outputs::probe(&mut canvas).unwrap();
        canvas.finish().unwrap();
    }

    #[test]
    fn switching_existing_output_targets_refreshes_camera_dimensions() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<bevy::render::texture::ManualTextureViews>()
            .add_message::<bevy::asset::AssetEvent<Image>>()
            .add_message::<bevy::window::WindowResized>()
            .add_message::<bevy::window::WindowCreated>()
            .add_message::<bevy::window::WindowScaleFactorChanged>()
            .add_systems(
                PostUpdate,
                (refresh_camera_targets, bevy::render::camera::camera_system).chain(),
            );
        let first = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(output_image([640, 424]));
        let second = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(output_image([800, 600]));
        let camera = app
            .world_mut()
            .spawn((
                Camera::default(),
                Projection::Orthographic(OrthographicProjection::default_3d()),
                RenderTarget::Image(first.clone().into()),
            ))
            .id();
        app.update();
        app.update();
        for (target, size) in [
            (second.clone(), UVec2::new(800, 600)),
            (first, UVec2::new(640, 424)),
            (second, UVec2::new(800, 600)),
        ] {
            app.world_mut()
                .entity_mut(camera)
                .insert(RenderTarget::Image(target.into()));
            app.update();
            assert_eq!(
                app.world()
                    .get::<Camera>(camera)
                    .unwrap()
                    .physical_target_size(),
                Some(size),
            );
        }
    }

    #[test]
    fn leaving_native_focus_cancels_pending_input_and_content_grabs() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            InputPlugin,
            WindowPlugin {
                primary_window: None,
                ..default()
            },
        ))
        .add_message::<PointerInput>();
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.world_mut().spawn((
            native::topology::input::CONTENT_POINTER,
            PointerLocation::new(Location {
                target: bevy::camera::NormalizedRenderTarget::Image(
                    Handle::<Image>::default().into(),
                ),
                position: Vec2::ONE,
            }),
        ));
        let keyboard = KeyboardInput {
            key_code: KeyCode::ControlLeft,
            logical_key: Key::Control,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        };
        let button = MouseButtonInput {
            button: MouseButton::Left,
            state: ButtonState::Pressed,
            window,
        };
        app.world_mut().write_message(keyboard.clone());
        app.world_mut().write_message(button.clone());
        app.update();
        assert!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::ControlLeft)
        );
        app.world_mut().write_message(keyboard.clone());
        app.world_mut()
            .write_message(WindowEvent::KeyboardInput(keyboard));
        app.world_mut().write_message(button.clone());
        app.world_mut()
            .write_message(WindowEvent::MouseButtonInput(button));
        cancel_input(app.world_mut(), window);
        app.update();
        assert_eq!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .get_pressed()
                .count(),
            0
        );
        assert_eq!(
            app.world()
                .resource::<ButtonInput<Key>>()
                .get_pressed()
                .count(),
            0
        );
        assert_eq!(
            app.world()
                .resource::<ButtonInput<MouseButton>>()
                .get_pressed()
                .count(),
            0
        );
        assert!(
            MessageCursor::<PointerInput>::default()
                .read(app.world().resource::<Messages<PointerInput>>())
                .any(|event| matches!(event.action, PointerAction::Cancel))
        );
    }
}
