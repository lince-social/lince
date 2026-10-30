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
mod gestures;
mod focus_policy;
mod opening;
mod control;
mod capture;
pub(crate) mod accessibility;
mod mapping;
mod absolute;
mod tablet;
pub mod manipulation;
mod outputs;
mod records;
#[cfg(test)]
mod color_tests;
#[cfg(feature = "native-client-gate")]
pub use client_probe::client_probe;
pub use outputs::Output;

pub struct Presentation {
    app: App,
    renderer: Rc<VulkanRenderer>,
    client_formats: Arc<Vec<smithay::backend::allocator::Fourcc>>,
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
    linear_output: bool,
    last_geometry: Option<lince_interface::host::GeometrySnapshot>,
    window_requests: std::collections::HashMap<
        lince_interface::host::SurfaceId,
        lince_interface::host::WindowRequest,
    >,
    output_state: outputs::State,
    application_focus: Option<lince_interface::host::SurfaceId>,
    canonical_root: Option<Entity>,
    manipulation: Option<manipulation::Active>,
    records: Option<records::Records>,
    accessibility: Option<accessibility::Bridge>,
    magnification_caret: Option<lince_interface::host::magnification::Caret>,
    magnification_pointer_epoch: Option<u64>,
    input_activity_revision: Option<u64>,
}

pub struct Frame {
    pub client_formats: Arc<Vec<smithay::backend::allocator::Fourcc>>,
    pub encoding: lince_interface::host::color::NativeEncoding,
    pub output: Option<lince_interface::host::OutputId>,
    image: Option<ExportedImage>,
    pub geometry: Option<Arc<lince_interface::host::GeometrySnapshot>>,
    pub input: InputSnapshot,
    pub windows: Vec<WindowConfiguration>,
    pub previewed: Vec<lince_interface::host::SurfaceId>,
    pub fullscreen: bool,
    pub origin: [f64; 2],
    pub outputs: Vec<OutputFrame>,
    pub(crate) capture: Option<capture::CaptureFrame>,
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

#[derive(Default)]
pub struct InputSnapshot {
    pub(crate) accessibility: Option<accessibility::Receipt>,
    pub mappings: Arc<lince_interface::host::mapping::Snapshot>,
    pub control: lince_interface::host::control::Catalog,
    pub workspaces: crate::server::WorkspaceSnapshot,
    pub editor: Option<crate::native_input::Editor>,
    pub(crate) native_pointer: crate::native_input::pointer::Snapshot,
    pub drop_target: Option<crate::native_input::transfer::Target>,
    pub controls: Vec<lince_interface::host::Rectangle>,
    pub focus_blocks: Vec<lince_interface::host::Rectangle>,
    pub modal: bool,
    pub keyboard: bool,
    pub keyboard_modal: bool,
    pub keyboard_capture: bool,
    pub keyboard_modal_triggers: Vec<lince_interface::host::Rectangle>,
    pub bindings: Vec<native::shortcuts::DesktopBinding>,
    pub hardware_bindings: Vec<native::shortcuts::DesktopBinding>,
    pub pointer_bindings: Vec<native::shortcuts::PointerBinding>,
    pub pointer_targets: Vec<native::layout::desktop::manipulation::pointer::Target>,
    pub manipulation: Option<manipulation::Active>,
    pub gestures: lince_interface::host::input::GestureSettings,
    pub overview: bool,
    pub focus_policy: lince_interface::host::input::FocusPolicy,
    pub cursor_warp: Option<lince_interface::host::input::CursorWarp>,
    pub text_composing: bool,
    pub opening_ready: bool,
}

#[derive(Resource)]
struct FrameFailure(String);

#[derive(Resource)]
struct MainUpdateStart(std::time::Instant);

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
    #[cfg(feature = "nested")]
    pub(crate) fn owns_modal_pointer(&self, point: [f64; 2]) -> bool {
        self.input.modal && self.geometry.as_ref().is_none_or(|geometry| {
            geometry.outputs.iter().any(|output| Some(output.id) == self.output && output.bounds.contains(point))
        })
    }

    pub fn ready(&self) -> Result<bool, ProbeFailure> {
        if let Some(image) = &self.image && !image.ready()? { return Ok(false); }
        for output in &self.outputs {
            if !output.image.ready()? { return Ok(false); }
        }
        Ok(true)
    }

    pub fn wait_for_probe(&self) -> Result<(), ProbeFailure> {
        if let Some(image) = &self.image { image.wait_for_probe()?; }
        for output in &self.outputs { output.image.wait_for_probe()?; }
        Ok(())
    }

    pub fn dmabuf(&self) -> Option<&smithay::backend::allocator::dmabuf::Dmabuf> {
        self.image.as_ref().map(|image| &image.dmabuf)
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
    let before = probe_pixels(frame.as_ref().unwrap().dmabuf().ok_or_else(|| input_failure("The probe output image is unavailable"))?)?;
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
    let after = probe_pixels(frame.as_ref().unwrap().dmabuf().ok_or_else(|| input_failure("The probe output image is unavailable"))?)?;
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
    if frame.as_ref().unwrap().dmabuf().ok_or_else(|| input_failure("The probe output image is unavailable"))?.size() != (800, 480).into() {
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
        if self.suspended() && action != lince_interface::host::DesktopAction::Logout { return Ok(()); }
        if let lince_interface::host::DesktopAction::CommitRecent { request } = action {
            let root = self.canvas_root()?;
            let Some((workspace, item)) = native::layout::desktop::recent::selection(self.app.world(), root, request) else {
                native::layout::desktop::recent::acknowledge(self.app.world_mut(), root, request, false);
                return Ok(());
            };
            let prepared = native::layout::desktop::transfers::prepare_identity(self.app.world(), root, workspace);
            self.apply_workspace_transfer(prepared, None, true)?;
            if self.app.world().get::<native::workspace::Workspaces>(root).is_some_and(|spaces| spaces.active == workspace)
                && native::layout::desktop::overview::valid_selection(self.app.world(), root, workspace, Some(item)) {
                self.application_focus = None;
                native::layout::desktop::recent::activate(self.app.world_mut(), root, request);
            } else {
                native::layout::desktop::recent::acknowledge(self.app.world_mut(), root, request, false);
            }
            return Ok(());
        }
        if let lince_interface::host::DesktopAction::SelectPlacement { workspace, item } = action {
            let root = self.canvas_root()?;
            let previous = native::workspace::navigation::capture(self.app.world(), root);
            if !native::layout::desktop::overview::valid_selection(self.app.world(), root, workspace, item) {
                native::layout::desktop::overview::activate(self.app.world_mut(), root, workspace, item);
                return Ok(());
            }
            if let Some(output) = self.output_state.assignments.focused() {
                self.output_state.assignments.select(output, workspace).map_err(|_| input_failure("could not select the overview workspace"))?;
                self.select_output_workspace()?;
                self.refresh_outputs()?;
            }
            self.application_focus = None;
            native::layout::desktop::overview::activate(self.app.world_mut(), root, workspace, item);
            if let Some(previous) = previous { previous.remember(self.app.world_mut()); }
            return Ok(());
        }
        if action == lince_interface::host::DesktopAction::ReapplyOpeningRules {
            let root = self.canvas_root()?;
            use native::actions::Action;
            action.apply(self.app.world_mut(), root);
            return Ok(());
        }
        if action == lince_interface::host::DesktopAction::InspectWindowRecord {
            let root = self.canvas_root()?;
            action.apply(self.app.world_mut(), root);
            return Ok(());
        }
        if self.output_action(action)? {
            return Ok(());
        }
        self.application_focus = None;
        let root = self.canvas_root()?;
        action.apply(self.app.world_mut(), root);
        Ok(())
    }

    pub(crate) fn file_drag(&mut self, context: Option<lince_interface::host::FileDragContext>) -> Result<(), ProbeFailure> {
        let window = self.input_window()?;
        let context = context.filter(|_| !self.suspended());
        native::layout::desktop::overview::file_drag(self.app.world_mut(), window, context);
        Ok(())
    }

    pub(crate) fn overview_hover(&mut self, request: lince_interface::host::OverviewHoverRequest, accepted: bool) -> Result<(), ProbeFailure> {
        let accepted = accepted && !self.suspended();
        if native::layout::desktop::overview::acknowledge_hover(self.app.world_mut(), request, accepted) {
            self.activate_workspace(request.drag.owner, request.workspace)?;
        }
        Ok(())
    }

    pub fn activate_workspace(&mut self, owner: u64, workspace: u64) -> Result<(), ProbeFailure> {
        if self.suspended() { return Ok(()); }
        match self.try_activate_workspace(owner, workspace) {
            Ok(()) => Ok(()),
            Err(outputs::ActionFailure::Refused(message)) => { native::notifications::report(self.app.world_mut(), "Park", &message); Ok(()) }
            Err(outputs::ActionFailure::Backend(error)) => Err(error),
        }
    }

    fn try_activate_workspace(&mut self, owner: u64, workspace: u64) -> Result<(), outputs::ActionFailure> {
        if self.suspended() { return Err(outputs::ActionFailure::Refused("The Park view is inactive.".into())); }
        let root = self.canvas_root()?;
        if root.to_bits() != owner || native::laboratory::temporary(self.app.world(), root, workspace) { return Err(outputs::ActionFailure::Refused("The captured workspace is unavailable.".into())); }
        let prepared = native::layout::desktop::transfers::prepare_identity(self.app.world(), root, workspace);
        native::layout::desktop::overview::dismiss(self.app.world_mut());
        self.try_apply_workspace_transfer(prepared, None, true)
    }

    pub fn create_workspace(&mut self, owner: u64) -> Result<(), ProbeFailure> {
        if self.suspended() { return Ok(()); }
        let root = self.canvas_root()?;
        if root.to_bits() != owner { return Ok(()); }
        let prepared = native::layout::desktop::transfers::prepare_creation(self.app.world(), root);
        native::layout::desktop::overview::dismiss(self.app.world_mut());
        self.apply_workspace_transfer(prepared, None, true)
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
                    active: !self.suspended() && views.map_or(spaces.active == space.id, |views| views.outputs.iter().any(|output| output.workspace == space.id)),
                }).take(4096).collect(),
        }
    }

    pub fn focus_application(&mut self, surface: lince_interface::host::SurfaceId) {
        if self.suspended() { return; }
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
        if self.suspended() { return Ok(()); }
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
        let local = lince_interface::host::OutputViewport { rotation: Default::default(),
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
            .insert_resource(native::layout::desktop::HostRequests(sender))
            .insert_resource(lince_interface::host::FileDragSupport { available: true, reason: None });
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
        let client_formats = Arc::new(renderer.client_import_formats());
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
        if crate::runtime::tracing_frames() {
            app.add_systems(First, |mut commands: Commands| {
                commands.insert_resource(MainUpdateStart(std::time::Instant::now()));
            }).add_systems(Last, |started: Res<MainUpdateStart>| {
                crate::runtime::trace_frame("native-main", started.0);
            });
        }
        app.insert_resource(bevy::render::error_handler::RenderErrorHandler(
            |error, world, _| {
                world.insert_resource(FrameFailure(format!("{error:?}")));
                bevy::render::error_handler::RenderErrorPolicy::StopRendering
            },
        ));
        app.insert_resource(native::layout::desktop::DefaultLayout(
            lince_interface::host::WorkspaceLayout::Tiled,
        ));
        app.sub_app_mut(RenderApp).insert_resource(wake.clone());
        app.insert_resource(wake)
            .add_systems(Startup, native::app::canvas)
            .add_systems(
                First,
                route_pointer_target.in_set(bevy::picking::PickingSystems::PostInput),
            )
            .add_systems(
                PostUpdate,
                refresh_camera_targets.before(bevy::camera::CameraUpdateSystems),
            );
        clients::install(&mut app)?;
        outputs::install_suspension(&mut app);
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
            client_formats,
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
            linear_output: false,
            last_geometry: None,
            window_requests: Default::default(),
            output_state: Default::default(),
            application_focus: None,
            canonical_root: None,
            accessibility: None,
            magnification_caret: None,
            magnification_pointer_epoch: None,
            input_activity_revision: None,
            manipulation: None,
            records,
        })
    }

    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    pub(crate) fn prepare_linear_output(&mut self, requested: bool) -> Result<(), ProbeFailure> {
        if !requested { return Ok(()); }
        if self.app.world().contains_resource::<native::topology::presentation::SceneCamera>() {
            return Err(input_failure("Native color precision must be selected before the first interface frame"));
        }
        if let Err(failure) = self.renderer.linear_scene_support() {
            eprintln!("Park native linear color is unavailable: {failure}");
            return Ok(());
        }
        let target = self.app.world_mut().resource_mut::<Assets<Image>>().add(output_image_format(self.render_size, true));
        let previous = std::mem::replace(&mut self.target, target.clone());
        self.app.world_mut().insert_resource(OutputTarget(RenderTarget::Image(target.into())));
        self.app.world_mut().insert_resource(native::app::LinearOutput);
        self.app.world_mut().resource_mut::<Assets<Image>>().remove(previous.id());
        self.linear_output = true;
        Ok(())
    }

    fn native_encoding(&self) -> lince_interface::host::color::NativeEncoding {
        if self.linear_output { lince_interface::host::color::NativeEncoding::LinearRelative }
        else { lince_interface::host::color::NativeEncoding::Srgb }
    }

    #[cfg(all(test, feature = "server-tests"))]
    pub(crate) fn published_geometry(&self) -> Option<&lince_interface::host::GeometrySnapshot> {
        self.last_geometry.as_ref()
    }

    #[cfg(all(test, feature = "nested", feature = "server-tests"))]
    pub(crate) fn native_test_points(&mut self, entity: Entity) -> Option<(lince_interface::host::OutputId, [[f64; 2]; 4])> {
        let geometry = self.last_geometry.clone();
        let focused = self.output_state.assignments.focused();
        let mut views = self.accessibility_views(geometry.as_ref());
        views.sort_by_key(|view| Some(view.output) != focused);
        views.into_iter().find_map(|view| {
            crate::native_input::projection::accessibility_points(self.app.world(), &view, geometry.as_ref(), entity, None)
                .map(|points| (view.output, points))
        })
    }

    pub(crate) fn native_pointer_current(&mut self, target: &crate::native_input::PointerTarget) -> bool {
        if self.suspended() { return false; }
        let geometry = self.frame_geometry();
        let views = self.accessibility_views(geometry.as_ref());
        crate::native_input::pointer::current(self.app.world(), &views, geometry.as_ref(), target)
    }

    pub fn pointer(&mut self, position: [f32; 2]) -> Result<(), ProbeFailure> {
        if self.suspended() { return Ok(()); }
        if position.iter().any(|value| !value.is_finite()) {
            return Err(input_failure("pointer coordinates must be finite"));
        }
        let mapping = self.input_mapping();
        let origin = self.output_view.as_ref().map_or([0.0; 2], |output|
            std::array::from_fn(|axis| output.bounds.origin()[axis] + self.render_origin[axis]));
        let window = self.input_window()?;
        let world = self.app.world_mut();
        let scale = f64::from(world.get::<Window>(window).unwrap().resolution.scale_factor());
        let physical = mapping.point(std::array::from_fn(|axis| f64::from(position[axis]) + origin[axis]));
        let position = Vec2::from_array(physical.map(|value| (value / scale) as f32));
        let mut state = world.get_mut::<Window>(window).unwrap();
        let delta = state.cursor_position().map(|last| position - last);
        state.set_cursor_position(Some(position));
        self.manipulate_pointer(position);
        let world = self.app.world_mut();
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
        if self.suspended() { return Ok(()); }
        if pressed { self.app.world_mut().remove_resource::<native::output_views::HandFocusRequest>(); }
        let window = self.input_window()?;
        if pressed {
            let Some(position) = self.app.world().get::<Window>(window).and_then(Window::physical_cursor_position) else { return Ok(()); };
            let point = self.input_mapping().logical_point(position.as_dvec2().to_array());
            let geometry = self.frame_geometry();
            let views = self.accessibility_views(geometry.as_ref());
            let target = crate::native_input::pointer::capture(self.app.world(), &views, geometry.as_ref()).target(point);
            let Some(target) = target else { return Ok(()); };
            if !self.app.world_mut().resource_mut::<crate::native_input::pointer::Held>().press(button, target) { return Ok(()); }
        } else {
            let Ok(target) = self.app.world_mut().resource_mut::<crate::native_input::pointer::Held>().release(button) else { return Ok(()); };
            if let Some(target) = target {
                let geometry = self.frame_geometry();
                let views = self.accessibility_views(geometry.as_ref());
                if !crate::native_input::pointer::held_current(self.app.world(), &views, geometry.as_ref(), &target) {
                    cancel_pointer(self.app.world_mut());
                    return Ok(());
                }
            }
        }
        if pressed {
            self.application_focus = None;
        }
        let event = bevy::input::mouse::MouseButtonInput {
            window,
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
        if self.suspended() { return Ok(()); }
        if pressed && physical == KeyCode::Escape {
            self.app.world_mut().remove_resource::<native::output_views::HandFocusRequest>();
        }
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
        if self.suspended() { return Ok(()); }
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
        let focused = focused && !self.suspended();
        if !focused { self.cancel_touch(); self.cancel_tablet(); }
        if let Some(bridge) = &mut self.accessibility { bridge.focus(focused); }
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
            if native::shortcuts::keyboard_owned(self.app.world())
                || self.app.world().get_resource::<lince_interface::host::input::InputActivity>()
                    .is_none_or(|activity| !activity.active())
            {
                native::layout::desktop::cancel_scratch_interactions(self.app.world_mut());
            }
            native::layout::desktop::overview::file_drag(self.app.world_mut(), window, None);
            self.manipulation = None;
            native::layout::desktop::manipulation::pointer::cancel(self.app.world_mut());
            crate::native_input::cancel(self.app.world_mut());
            cancel_input(self.app.world_mut(), window);
        }
        Ok(())
    }

    pub(crate) fn modifiers(&mut self, pressed: &[(KeyCode, bevy::input::keyboard::Key)]) -> Result<(), ProbeFailure> {
        if self.suspended() { return Ok(()); }
        let window = self.input_window()?;
        native::synchronize_keyboard_modifiers(self.app.world_mut(), window, pressed);
        Ok(())
    }

    pub fn text(&mut self, value: String) -> Result<(), ProbeFailure> {
        if self.suspended() { return Ok(()); }
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
                if !requested && native::app::text_ready_to_close(self.app.world_mut())
                    && self.records.as_ref().is_none_or(|records| !records.pending())
                {
                    if let Some(records) = &mut self.records {
                        records.flush().map_err(|error| input_failure(&format!(
                            "Logout cancelled because Park window customizations could not be saved: {error}"
                        )))?;
                    }
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
        if self.suspended() || !self.output_state.outputs.is_empty() {
            return Err(input_failure("Resize assigned monitors through output configuration, so their render targets remain synchronized."));
        }
        let world = self.app.world_mut();
        let target = world
            .resource_mut::<Assets<Image>>()
            .add(output_image_format(size, self.linear_output));
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

    pub(crate) fn synchronize_input_activity(&mut self) -> Result<(), ProbeFailure> {
        if let Some(revision) = self.app.world_mut().get_resource_mut::<lince_interface::host::input::InputActivity>().map(|mut activity| activity.begin_frame()) {
            if self.input_activity_revision != Some(revision) {
                let previous = self.input_activity_revision.replace(revision);
                if previous.is_some() || revision & 1 == 0 {
                    self.focus(false)?;
                }
            }
        }
        Ok(())
    }

    pub fn frame(&mut self) -> Result<Frame, ProbeFailure> {
        self.synchronize_input_activity()?;
        let hand_focus = self.app.world_mut().remove_resource::<native::output_views::HandFocusRequest>();
        if let Some(request) = hand_focus
            && !self.suspended()
            && request.fresh()
            && self.app.world().get_resource::<lince_interface::host::input::InputActivity>().is_none_or(|activity| activity.active())
            && self.canvas_root().ok().is_some_and(|root| self.output_state.assignments.outputs().iter().any(|output| request.matches(root, output.id, output.active)))
        {
            self.focus_output(request.output)?;
        }
        let started = std::time::Instant::now();
        let references = self.records.as_ref().map(|records| records.opening_references()).unwrap_or_default();
        native::layout::desktop::opening::window_references(self.app.world_mut(), references);
        if let Some(records) = &mut self.records {
            records.tick(self.app.world_mut(), &self.windows, &self.identities);
        }
        if let Some(inspection) = self
            .records
            .as_mut()
            .and_then(records::Records::take_inspection)
        {
            self.open_inspection(inspection);
        }
        self.refresh_inspections();
        self.apply_window_requests();
        self.refresh_viewport()?;
        self.accessibility_requests();
        self.reconcile_touch();
        self.reconcile_tablet();
        let input_view = self.native_input_view(None);
        crate::native_input::projection::prepare(self.app.world_mut(), input_view);
        self.prepare_capture_targets();
        self.app.update();
        self.capture_settings();
        self.publish_capture_policy();
        self.retain_capture_windows();
        crate::runtime::trace_frame("native-update", started);
        let started = std::time::Instant::now();
        if let Some(error) = self.app.world().get_resource::<FrameFailure>() {
            return Err(ProbeFailure {
                stage: "native_frame".into(),
                detail: error.0.clone(),
            });
        }
        if self.suspended() {
            let geometry = self.frame_geometry().map(Arc::new);
            let input = self.finish_input(geometry.as_deref())?;
            self.release_capture_uploads();
            return Ok(Frame {
                client_formats: self.client_formats.clone(),
                encoding: self.native_encoding(),
                output: None,
                image: None,
                geometry,
                input,
                windows: self.window_configurations(),
                previewed: Vec::new(),
                fullscreen: false,
                origin: [0.0; 2],
                outputs: Vec::new(),
                capture: None,
            });
        }
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
        let geometry = self.frame_geometry().map(Arc::new);
        let input = self.finish_input(geometry.as_deref())?;
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
        let mut frame = Frame {
            client_formats: self.client_formats.clone(),
            encoding: self.native_encoding(),
            output: self.output_state.assignments.focused(),
            image: Some(self.renderer.export_copy(&texture)?),
            geometry,
            input,
            windows,
            previewed: native::layout::desktop::overview::previewed_applications(self.app.world_mut()),
            fullscreen: self.render_fullscreen,
            origin: self.render_origin,
            outputs,
            capture: None,
        };
        frame.capture = self.capture_frame();
        self.release_capture_uploads();
        crate::runtime::trace_frame("export", started);
        Ok(frame)
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
                let item = native::canvas::presented(world, window.entity)?;
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
                    state: native::layout::desktop::manipulation::window_state(world, window.entity).unwrap_or(lince_interface::host::WindowState {
                        minimized: workspace.desktop.minimized(placement),
                        fullscreen: workspace.desktop.protocol_fullscreen(placement),
                        windowed_fullscreen: workspace.desktop.windowed_fullscreen(placement),
                        maximized: workspace.desktop.maximized(placement),
                        resizing: native::layout::desktop::manipulation::resizing(world, window.entity),
                    }),
                })
            })
            .collect()
    }

    fn frame_geometry(&mut self) -> Option<lince_interface::host::GeometrySnapshot> {
        if self.suspended() {
            return Some(self.remember_geometry(lince_interface::host::GeometrySnapshot {
                revision: 0,
                outputs: Vec::new(),
                items: Vec::new(),
                focused: None,
            }));
        }
        let mut geometry = self.geometry()?.clone();
        let root = self.canvas_root().ok()?;
        geometry.focused = self.app.world().get::<native::workspace::Workspaces>(root).and_then(|spaces| {
            let id = spaces.entries.iter().find(|space| space.id == spaces.active)?.desktop.focused()?;
            self.app.world().get::<Children>(root)?.iter().find_map(|entity| {
                let world = self.app.world();
                (world.get::<native::layout::desktop::Managed>(entity)?.id == id).then(|| world.get::<native::layout::desktop::ApplicationWindow>(entity).map_or(lince_interface::host::ItemId::Native(id.0), |window| lince_interface::host::ItemId::Application(window.surface)))
            })
        });
        let outputs = self
            .app
            .world()
            .get_resource::<native::output_views::OutputViews>()
            .filter(|views| views.applies_to(root))
            .cloned()
            .map_or_else(|| geometry.outputs.clone(), |views| views.outputs);
        geometry.items = outputs.iter().filter(|output| output.usable.is_some()).flat_map(|output| {
            native::topology::output_views::geometry(self.app.world_mut(), root, output)
        }).collect();
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
            let current = self.windows.get(&surface).is_some_and(|window| {
                let world = self.app.world();
                world.get::<native::layout::desktop::Managed>(window.entity).is_none_or(|managed| {
                    world.get::<native::workspace::Workspaces>(root).is_some_and(|spaces| spaces.entries.iter().find(|space| space.id == spaces.active).is_some_and(|space| space.desktop.focused() == Some(managed.id)))
                })
            });
            let focused = lince_interface::host::ItemId::Application(surface);
            if current && geometry.items.iter().any(|item| {
                item.id == focused
                    && self
                        .output_state
                        .assignments
                        .focused()
                        .is_none_or(|output| output == item.output)
            }) {
                geometry.focused = Some(focused);
            } else if !current || self.windows.get(&surface).is_none_or(|window| {
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
                if let Some(polygons) = &mut item.clip {
                    for point in polygons.iter_mut().flatten() {
                        for axis in 0..2 { point[axis] += offset[axis]; }
                    }
                }
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
        Some(self.remember_geometry(geometry))
    }

    fn remember_geometry(&mut self, mut geometry: lince_interface::host::GeometrySnapshot) -> lince_interface::host::GeometrySnapshot {
        geometry.revision = self.last_geometry.as_ref().map_or(1, |previous| {
            previous.revision
                + u64::from(
                    previous.outputs != geometry.outputs
                        || previous.items != geometry.items
                        || previous.focused != geometry.focused,
                )
        });
        self.last_geometry = Some(geometry.clone());
        geometry
    }

    fn native_input_view(&mut self, geometry: Option<&lince_interface::host::GeometrySnapshot>) -> Option<crate::native_input::projection::View> {
        if self.suspended() { return None; }
        let root = self.canvas_root().ok();
        let output = self.output_state.assignments.focused()
            .or_else(|| self.output_view.as_ref().map(|view| view.id))
            .unwrap_or(lince_interface::host::OutputId(1));
        let mut viewport = geometry.and_then(|geometry| geometry.outputs.iter().find(|view| view.id == output)).cloned()
            .or_else(|| self.output_view.clone())
            .or_else(|| root.and_then(|root| self.app.world().get::<native::layout::desktop::HostViewport>(root)).map(|view| view.0.clone()));
        if let Some(viewport) = &mut viewport {
            if let Some(spaces) = root.and_then(|root| self.app.world().get::<native::workspace::Workspaces>(root)) {
                viewport.workspace = spaces.active;
            }
        }
        Some(crate::native_input::projection::View {
            root, output, image: self.target.id(), raster: self.input_mapping(),
            origin: self.output_state.targets.get(&output).map_or_else(
                || self.output_view.as_ref().map_or(self.render_origin, |view| std::array::from_fn(|axis| view.bounds.origin()[axis] + self.render_origin[axis])),
                |target| target.origin,
            ),
            viewport,
        })
    }

    fn input_snapshot(&mut self, geometry: Option<&lince_interface::host::GeometrySnapshot>) -> InputSnapshot {
        use bevy::input_focus::tab_navigation::TabGroup;
        let workspaces = self.workspace_snapshot();
        if self.suspended() {
            return InputSnapshot {
                workspaces,
                hardware_bindings: native::shortcuts::binding_snapshot(self.app.world()).1,
                ..default()
            };
        }
        let manipulation = self.manipulation_snapshot();
        let pointer_targets = self.canvas_root().ok().map(|root|
            native::layout::desktop::manipulation::pointer::targets(self.app.world(), root)
        ).unwrap_or_default();
        let mapping = self.input_mapping();
        let input_view = self.native_input_view(geometry);
        let pointer_views = self.accessibility_views(geometry);
        let world = self.app.world_mut();
        crate::native_input::projection::prepare(world, input_view);
        let output = self.target.id();
        let window = world.query_filtered::<Entity, With<PrimaryWindow>>().iter(world).next();
        let output_id = self.output_view.as_ref().map_or(lince_interface::host::OutputId(1), |output| output.id);
        let drop_target = window.and_then(|window| crate::native_input::transfer::capture(world, window, output, output_id, mapping));
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
                    mapping.logical_rectangle(lince_interface::host::Rectangle::new(
                        low.as_dvec2().to_array(), (high - low).as_dvec2().to_array(),
                    )?)
                } else {
                    None
                };
                let mut focus_block = control;
                let mut ancestor = Some(entity);
                for _ in 0..native::layout::MAX_DEPTH {
                    let Some(entity) = ancestor else { break; };
                    if world.get::<native::canvas::CanvasItem>(entity).is_some() {
                        focus_block = false;
                        break;
                    }
                    ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
                }
                Some((rectangle, modal.is_some_and(|group| group.modal), world.get::<native::shortcuts::KeyboardModalTrigger>(entity).is_some(), focus_block))
            })
            .collect();
        let keyboard_modal = native::shortcuts::keyboard_owned(world);
        let modal = nodes.iter().any(|(_, modal, _, _)| *modal)
            || world
                .query::<&native::edit_mode::EditMode>()
                .iter(world)
                .any(|mode| mode.enabled);
        let mut focused = world
            .get_resource::<bevy::input_focus::InputFocus>()
            .and_then(bevy::input_focus::InputFocus::get);
        let mut keyboard = modal || keyboard_modal || native::accessibility::owns_keyboard(world);
        for _ in 0..native::layout::MAX_DEPTH {
            let Some(entity) = focused else {
                break;
            };
            keyboard |= world.get::<bevy::text::EditableText>(entity).is_some();
            focused = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        let (bindings, hardware_bindings) = native::shortcuts::binding_snapshot(world);
        let native_pointer = crate::native_input::pointer::capture(world, &pointer_views, geometry);
        crate::native_input::pointer::publish_status(world, &native_pointer);
        InputSnapshot {
            accessibility: None,
            mappings: Default::default(),
            control: self.canonical_root.map(|root| native::layout::desktop::control::catalog(world, root)).unwrap_or_default(),
            workspaces,
            drop_target,
            editor: crate::native_input::capture(world, geometry),
            native_pointer,
            controls: nodes
                .iter()
                .filter_map(|(rectangle, _, _, _)| *rectangle)
                .collect(),
            focus_blocks: nodes.iter().filter_map(|(rectangle, _, _, blocked)| blocked.then_some(*rectangle).flatten()).collect(),
            modal,
            keyboard,
            keyboard_modal,
            keyboard_capture: native::shortcuts::keyboard_capturing(world),
            keyboard_modal_triggers: nodes.into_iter().filter_map(|(rectangle, _, trigger, _)| trigger.then_some(rectangle).flatten()).collect(),
            bindings,
            hardware_bindings,
            pointer_bindings: native::shortcuts::pointer_bindings(world),
            pointer_targets,
            manipulation,
            gestures: native::layout::desktop::gestures::settings(world),
            overview: native::layout::desktop::overview::active(world),
            focus_policy: native::layout::desktop::focus::policy(world),
            cursor_warp: native::layout::desktop::focus::pending_warp(world),
            text_composing: world.get_resource::<bevy::input_focus::InputFocus>()
                .and_then(|focus| focus.get())
                .and_then(|input| world.get::<bevy::text::EditableText>(input))
                .is_some_and(|input| input.is_composing()),
            opening_ready: native::layout::desktop::opening::ready(world),
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

fn route_pointer_target(
    output: Res<OutputTarget>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut inputs: MessageMutator<bevy::picking::pointer::PointerInput>,
    mut pointers: Query<(&bevy::picking::pointer::PointerId, &mut bevy::picking::pointer::PointerLocation)>,
    mut previous: Local<Option<bevy::camera::NormalizedRenderTarget>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(target) = output.0.normalize(Some(window)) else {
        return;
    };
    let window_target = RenderTarget::Window(bevy::window::WindowRef::Entity(window))
        .normalize(Some(window))
        .unwrap();
    for input in inputs.read() {
        if input.location.target == window_target {
            input.location.target = target.clone();
        }
    }
    for (id, mut pointer) in &mut pointers {
        if let Some(location) = &mut pointer.location
            && !((matches!(id, bevy::picking::pointer::PointerId::Touch(_)) || crate::native_input::absolute::is_tablet_pointer(id)) && location.target != window_target)
            && (location.target == window_target || previous.as_ref() == Some(&location.target))
        {
            location.target = target.clone();
        }
    }
    *previous = Some(target);
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

fn output_image(size: [u32; 2]) -> Image { output_image_format(size, false) }

fn output_image_format(size: [u32; 2], linear: bool) -> Image {
    let mut image =
        Image::new_target_texture(size[0], size[1], if linear { wgpu::TextureFormat::Rgba16Float } else { wgpu::TextureFormat::Bgra8UnormSrgb }, None);
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

fn cancel_pointer(world: &mut World) {
    use bevy::{ecs::message::Messages, input::mouse::{MouseButton, MouseButtonInput},
        picking::pointer::{PointerAction, PointerId, PointerInput, PointerLocation}, window::WindowEvent};
    if let Some(mut held) = world.get_resource_mut::<crate::native_input::pointer::Held>() { held.cancel(); }
    world.resource_mut::<Messages<MouseButtonInput>>().clear();
    let retained = world.resource_mut::<Messages<WindowEvent>>().drain()
        .filter(|event| !matches!(event, WindowEvent::MouseButtonInput(_))).collect::<Vec<_>>();
    world.write_message_batch(retained);
    world.resource_mut::<ButtonInput<MouseButton>>().release_all();
    let locations = world.query::<(&PointerId, &PointerLocation)>().iter(world)
        .filter(|(id, _)| **id == PointerId::Mouse || **id == native::topology::input::CONTENT_POINTER)
        .filter_map(|(id, location)| Some((*id, location.location.clone()?))).collect::<Vec<_>>();
    for (pointer_id, location) in locations { world.write_message(PointerInput { pointer_id, location, action: PointerAction::Cancel }); }
    if let Some(mut state) = world.get_resource_mut::<native::topology::input::PointerState>() { *state = Default::default(); }
}

fn cancel_input(world: &mut World, window: Entity) {
    use bevy::{
        ecs::message::Messages,
        input::{
            mouse::{MouseButton, MouseButtonInput},
        },
        picking::pointer::{PointerAction, PointerId, PointerInput, PointerLocation},
        window::{CursorLeft, WindowEvent},
    };
    world.resource_mut::<Messages<MouseButtonInput>>().clear();
    if let Some(mut held) = world.get_resource_mut::<crate::native_input::pointer::Held>() { held.cancel(); }
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
    world
        .resource_mut::<ButtonInput<MouseButton>>()
        .release_all();
    native::cancel_keyboard_input(world);
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
    #[test]
    fn fullscreen_and_tiled_physics_behavior_checks() {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let cases: Vec<_> = lince_interface::native::laboratory::catalogue()
                    .into_iter()
                    .filter(|case| {
                        case.name.ends_with("tiling_and_fullscreen_suppression_remove_physics_until_released")
                            || case.name.ends_with("fullscreen_restore_keeps_free_geometry_despite_presentation_changes")
                            || case.name.ends_with("mapping_an_unrelated_window_preserves_fullscreen_and_restores_every_window")
                            || case.name.ends_with("dialog_stays_above_its_parent_and_preserves_parent_fullscreen")
                    })
                    .collect();
                assert_eq!(cases.len(), 4);
                for case in cases {
                    let result = case.execute(&runtime);
                    assert!(result.error.is_none(), "{}: {:?}", result.name, result.error);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    use super::*;
    mod showoff;
    mod pointer;
    mod pointer_bindings;
    mod pointer_settings;
    mod external_commands;
    mod shortcuts;
    mod configuration;
    mod control;
    mod capture;
    mod monitors;
    mod overview;
    mod tiling;
    mod transfers;
    mod workspace_navigation;
    mod widths;
    mod layout_preferences;
    mod displays;
    mod suspension;
    mod layers;
    mod shaders;
    mod appearance;
    #[cfg(not(debug_assertions))]
    mod appearance_cost;
    mod stacking;
    mod commands;
    mod logout;
    mod recent;
    mod opening;
    mod manipulation;
    mod keyboard;
    mod focus_handoff;
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
    fn window_state_and_geometry_are_exported_from_the_same_layout() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _entered = runtime.enter();
        let directory = tempfile::tempdir().unwrap();
        let mut canvas = Presentation::new(
            [800, 600],
            directory.path().into(),
            WakeSignal::new(|| {}),
        ).unwrap();
        for _ in 0..4 { canvas.probe_frame().unwrap(); }
        canvas.desktop_action(lince_interface::host::DesktopAction::SetLayout(
            lince_interface::host::WorkspaceLayout::Free,
        )).unwrap();
        let surface = lince_interface::host::SurfaceId(1);
        canvas.open_window(surface, "Lifecycle", "validation").unwrap();
        canvas.window_request(surface, lince_interface::host::WindowRequest {
            fullscreen: Some(true),
            ..Default::default()
        });
        for _ in 0..4 {
            let frame = canvas.probe_frame().unwrap();
            let window = frame.windows.iter().find(|window| window.surface == surface).unwrap();
            assert_eq!(window.size, if window.state.fullscreen { [800, 600] } else { [388, 480] });
        }
        assert!(canvas.window_requests.is_empty());
        canvas.window_request(surface, lince_interface::host::WindowRequest {
            fullscreen: Some(false),
            ..Default::default()
        });
        for _ in 0..4 {
            let frame = canvas.probe_frame().unwrap();
            let window = frame.windows.iter().find(|window| window.surface == surface).unwrap();
            assert!(!window.state.fullscreen);
            assert_eq!(window.size, [388, 480]);
        }
    }

    #[test]
    #[ignore = "requires hardware Vulkan and EGL"]
    fn connected_cell_attaches_before_render_plugins_and_protects_failed_saves() {
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
        let surface = lince_interface::host::SurfaceId(1);
        let identity = canvas.open_window(surface, "Save retry", "test.generic").unwrap();
        for _ in 0..100 {
            canvas.probe_frame().unwrap();
            if canvas.records.as_ref().unwrap().profile(identity).is_some_and(|record| record.uid.is_some()) { break; }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let uid = canvas.records.as_ref().unwrap().profile(identity).unwrap().uid.clone().unwrap();
        let path = directory.path().join("park-windows.json");
        if path.exists() { std::fs::remove_file(&path).unwrap(); }
        std::fs::create_dir(&path).unwrap();
        let launch = serde_json::from_value(serde_json::json!({"program":"test-generic","arguments":[]})).unwrap();
        assert!(canvas.window_launch_description(surface, launch).is_err());
        let failure = canvas.finish().unwrap_err();
        assert!(failure.detail.contains("window customizations could not be saved"));
        assert!(canvas.app.should_exit().is_none());
        std::fs::remove_dir(&path).unwrap();
        canvas.finish().unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let saved = saved.as_array().unwrap().iter()
            .find(|record| record["uid"].as_str() == Some(uid.as_str())).unwrap();
        assert_eq!(saved["launch"]["program"], "test-generic");
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
    fn cancelling_a_replaced_pointer_target_preserves_keyboard_focus_and_pending_keys() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin, WindowPlugin { primary_window: None, ..default() }))
            .add_message::<PointerInput>()
            .init_resource::<bevy::input_focus::InputFocus>();
        let window = app.world_mut().spawn((Window::default(), PrimaryWindow)).id();
        let field = app.world_mut().spawn(bevy::text::EditableText::new("Keep this editor")).id();
        app.world_mut().resource_mut::<bevy::input_focus::InputFocus>().set(field, bevy::input_focus::FocusCause::Navigated);
        for pointer in [bevy::picking::pointer::PointerId::Mouse, native::topology::input::CONTENT_POINTER] {
            app.world_mut().spawn((pointer, PointerLocation::new(Location {
                target: bevy::camera::NormalizedRenderTarget::Image(Handle::<Image>::default().into()), position: Vec2::ONE,
            })));
        }
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ControlLeft);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.world_mut().write_message(KeyboardInput { key_code: KeyCode::KeyX, logical_key: Key::Character("x".into()),
            state: ButtonState::Pressed, text: Some("x".into()), repeat: false, window });
        cancel_pointer(app.world_mut());
        assert_eq!(app.world().resource::<bevy::input_focus::InputFocus>().get(), Some(field));
        assert_eq!(app.world().get::<bevy::text::EditableText>(field).unwrap().value(), "Keep this editor");
        assert_eq!(MessageCursor::<PointerInput>::default().read(app.world().resource::<Messages<PointerInput>>())
            .filter(|event| matches!(event.action, PointerAction::Cancel)).count(), 2);
        app.update();
        assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::ControlLeft));
        assert!(app.world().resource::<ButtonInput<KeyCode>>().pressed(KeyCode::KeyX));
        assert!(!app.world().resource::<ButtonInput<MouseButton>>().pressed(MouseButton::Left));
        assert_eq!(app.world().resource::<bevy::input_focus::InputFocus>().get(), Some(field));
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
