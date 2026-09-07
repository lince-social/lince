use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
#[cfg(feature = "bevy-preflight")]
use lince_interface::bevy_host::BevyHost;
#[cfg(feature = "bevy-preflight")]
use lince_interface::bevy_layer::BevyLayer;
use lince_interface::dependency_graph::DependencyAudit;
use lince_interface::frame::FrameAssembly;
use lince_interface::input::{
    ButtonState as InputButtonState, InputEnvelope, InputEvidence, InputModifiers, InputSource,
    InputSurface, InputTarget, NormalizedInput, Point, PointerButton, ScrollUnit,
    TouchPhase as InputTouchPhase,
};
use lince_interface::window_stress::WindowStress;
use lince_interface::{HostFacts, LaboratoryReport};
use lince_interface::{
    node_layer::NodeLayer,
    official_sands::official_sand_package,
    retained_ui::{RetainedPlacement, RetainedRect, RetainedScene, RetainedTextLayer},
    sand::{BehaviorBinding, DeclarativeBehavior, PortDirection, SandPackage, SandValue},
    style::{ResolvedStyle, StyleGalleryState},
    update_panel::UpdatePanel,
    workspace::{NativeWorkspaceKind, NativeWorkspaceManager},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};
use wgpu::{
    CommandEncoderDescriptor, CompositeAlphaMode, DeviceDescriptor, Instance, InstanceDescriptor,
    LoadOp, MultisampleState, Operations, PresentMode, RenderPassColorAttachment,
    RenderPassDescriptor, RequestAdapterOptions, StoreOp, SurfaceConfiguration, TextureUsages,
    TextureViewDescriptor,
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::{
        ElementState, Ime, KeyEvent, MouseButton as WinitMouseButton, MouseScrollDelta, TouchPhase,
        WindowEvent,
    },
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

const LATO_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Lato/Lato-Regular.ttf");

pub fn run_legacy_laboratory() {
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            let message = error.to_string();
            let report = LaboratoryReport::ownership_preflight(HostFacts::unavailable(
                1.0,
                0,
                0,
                message.clone(),
            ));
            match report.write_default() {
                Ok(path) => eprintln!(
                    "Lince Interface Laboratory unavailable: {message}. Report: {}",
                    path.display()
                ),
                Err(export_error) => eprintln!(
                    "Lince Interface Laboratory unavailable: {message}. Report export failed: {export_error}"
                ),
            }
            return;
        }
    };
    let mut application = LaboratoryApplication {
        state: None,
        export_on_first_frame: std::env::args()
            .any(|argument| argument == "--export-report-and-exit"),
        stress_resize_and_exit: std::env::args()
            .any(|argument| argument == "--stress-resize-and-exit"),
    };
    event_loop
        .run_app(&mut application)
        .expect("run interface laboratory");
}

#[allow(dead_code)]
fn main() {
    run_legacy_laboratory();
}

struct LaboratoryApplication {
    state: Option<LaboratoryWindow>,
    export_on_first_frame: bool,
    stress_resize_and_exit: bool,
}

enum LaboratoryWindow {
    Ready(Box<RenderState>),
    Unavailable(Box<UnavailableState>),
}

struct UnavailableState {
    report: LaboratoryReport,
    window: Arc<Window>,
}

struct RenderState {
    #[cfg(feature = "bevy-preflight")]
    bevy_host: BevyHost,
    #[cfg(feature = "bevy-preflight")]
    bevy_layer: BevyLayer,
    instance: Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: SurfaceConfiguration,
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_buffer: Buffer,
    node_layer: NodeLayer,
    retained_text_layer: RetainedTextLayer,
    retained_style: ResolvedStyle,
    sand_package: SandPackage,
    box_scene: RetainedScene,
    workspaces: NativeWorkspaceManager,
    information_open: bool,
    focused_control: usize,
    editing_workspace_name: bool,
    replace_workspace_name_on_input: bool,
    workspace_status: String,
    animation_frame: u64,
    revision: String,
    last_updated: String,
    update: UpdatePanel,
    report: LaboratoryReport,
    input_evidence: InputEvidence,
    input_sequence: u64,
    input_cursor: Point,
    input_buttons: BTreeSet<PointerButton>,
    input_modifiers: InputModifiers,
    window_stress: Option<WindowStress>,
    recovery_probe_complete: bool,
    teardown_prepared: bool,
    report_export_requested: bool,
    last_export: Option<String>,
    pending_input_started: Option<Instant>,
    scale_factor: f64,
    window: Arc<Window>,
}

impl RenderState {
    async fn new(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
        stress_resize: bool,
    ) -> Result<Self, String> {
        let size = nonzero_size(window.inner_size());
        let scale_factor = window.scale_factor();
        window.set_ime_allowed(true);
        window.set_ime_cursor_area(
            LogicalPosition::new(28.0, 28.0),
            LogicalSize::new(480.0, 24.0),
        );
        let instance = Instance::new(InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        )));
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| error.to_string())?;
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..RequestAdapterOptions::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        let adapter_info = adapter.get_info();
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| "the selected adapter exposes no surface format".to_owned())?;
        let present_mode = if capabilities.present_modes.contains(&PresentMode::Fifo) {
            PresentMode::Fifo
        } else {
            *capabilities
                .present_modes
                .first()
                .ok_or_else(|| "the selected adapter exposes no present mode".to_owned())?
        };
        let alpha_mode = if capabilities
            .alpha_modes
            .contains(&CompositeAlphaMode::Opaque)
        {
            CompositeAlphaMode::Opaque
        } else {
            *capabilities
                .alpha_modes
                .first()
                .ok_or_else(|| "the selected adapter exposes no alpha mode".to_owned())?
        };
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("lince-interface-host"),
                ..DeviceDescriptor::default()
            })
            .await
            .map_err(|error| error.to_string())?;
        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        #[cfg(feature = "bevy-preflight")]
        let bevy_host = BevyHost::new(
            instance.clone(),
            adapter.clone(),
            adapter_info.clone(),
            device.clone(),
            queue.clone(),
        )?;
        #[cfg(feature = "bevy-preflight")]
        let bevy_layer = BevyLayer::new(&device, format, bevy_host.output_view());

        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, &device, MultisampleState::default(), None);
        let mut font_system = FontSystem::new();
        font_system.db_mut().load_font_data(LATO_REGULAR.to_vec());
        let swash_cache = SwashCache::new();
        let text_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(scaled(15.0, scale_factor), scaled(21.0, scale_factor)),
        );
        let node_layer = NodeLayer::new(&device, format);
        let retained_text_layer = RetainedTextLayer::new(
            &device,
            &queue,
            format,
            size.width,
            size.height,
            scale_factor,
        );
        let retained_style = StyleGalleryState::new()
            .and_then(|gallery| gallery.resolved())
            .map_err(|error| error.to_string())?;
        let sand_package = official_sand_package();
        sand_package.validate().map_err(|error| error.to_string())?;
        let workspaces = NativeWorkspaceManager::from_process();
        let revision = if utils::build_info::revision_is_stamped() {
            utils::build_info::revision().to_string()
        } else {
            format!(
                "{} (no revision stamp — development build)",
                lince_interface::git_revision()
            )
        };
        let last_updated = executable_modified_at();
        let report = LaboratoryReport::ownership_preflight(HostFacts {
            operating_system: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            event_loop_owner: "lince-winit-host".into(),
            rendering_device_owner: "lince-wgpu-host".into(),
            final_compositor_owner: "lince-wgpu-host".into(),
            gpu_name: adapter_info.name,
            gpu_backend: format!("{:?}", adapter_info.backend),
            gpu_device_type: format!("{:?}", adapter_info.device_type),
            gpu_driver: adapter_info.driver,
            gpu_driver_info: adapter_info.driver_info,
            surface_format: format!("{format:?}"),
            present_mode: format!("{present_mode:?}"),
            scale_factor,
            surface_width: size.width,
            surface_height: size.height,
            initialization_error: None,
        });
        #[cfg(feature = "bevy-preflight")]
        let report = {
            let mut report = report;
            report.mark_bevy_host_running(bevy_host.updates(), 0, true);
            report
        };

        let mut state = Self {
            #[cfg(feature = "bevy-preflight")]
            bevy_host,
            #[cfg(feature = "bevy-preflight")]
            bevy_layer,
            instance,
            device,
            queue,
            surface,
            surface_config,
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            text_buffer,
            node_layer,
            retained_text_layer,
            retained_style,
            sand_package,
            box_scene: RetainedScene::default(),
            workspaces,
            information_open: false,
            focused_control: 0,
            editing_workspace_name: false,
            replace_workspace_name_on_input: false,
            workspace_status: "Ready".into(),
            animation_frame: 0,
            revision,
            last_updated,
            update: UpdatePanel::new(),
            report,
            input_evidence: InputEvidence::default(),
            input_sequence: 0,
            input_cursor: Point::new(0.0, 0.0),
            input_buttons: BTreeSet::new(),
            input_modifiers: InputModifiers::default(),
            window_stress: stress_resize.then(WindowStress::new),
            recovery_probe_complete: false,
            teardown_prepared: false,
            report_export_requested: false,
            last_export: None,
            pending_input_started: None,
            scale_factor,
            window,
        };
        state.request_next_window_stress();
        state.refresh_box_scene()?;
        state.refresh_text();
        Ok(state)
    }

    fn refresh_text(&mut self) {
        self.report.host.scale_factor = self.scale_factor;
        self.report.host.surface_width = self.surface_config.width;
        self.report.host.surface_height = self.surface_config.height;
        self.text_buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(
                scaled(15.0, self.scale_factor),
                scaled(21.0, self.scale_factor),
            ),
        );
        self.text_buffer.set_size(
            &mut self.font_system,
            Some(self.surface_config.width as f32),
            Some(self.surface_config.height as f32),
        );
        let workspace_text = match &self.workspaces.active().kind {
            NativeWorkspaceKind::Laboratory { run_directory } => format!(
                "{}\n\nWORKSPACE  {}\nEVIDENCE   {}",
                self.report.panel_text(self.last_export.as_deref()),
                self.workspaces.active().name,
                run_directory.display()
            ),
            NativeWorkspaceKind::Empty => format!(
                "LINCE\n\nWORKSPACE  {}\n\nThe workspace is empty. The square control in the lower-right opens Lince information.",
                self.workspaces.active().name
            ),
        };
        self.text_buffer.set_text(
            &mut self.font_system,
            &workspace_text,
            &Attrs::new().family(Family::Name("Lato")),
            Shaping::Advanced,
            None,
        );
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.window.request_redraw();
    }

    fn refresh_box_scene(&mut self) -> Result<(), String> {
        let width = self.surface_config.width as f32;
        let height = self.surface_config.height as f32;
        let mut placements = Vec::new();
        if matches!(
            self.workspaces.active().kind,
            NativeWorkspaceKind::Laboratory { .. }
        ) {
            let phase = self.animation_frame as f32 * 0.012;
            for index in 0..200 {
                let column = (index % 20) as f32;
                let row = (index / 20) as f32;
                let travel = (phase + index as f32 * 0.17).sin() * 8.0;
                placements.push(RetainedPlacement {
                    key: format!("laboratory/sand-{index}"),
                    definition_uid: "panel".into(),
                    inputs: BTreeMap::new(),
                    rect: RetainedRect {
                        x: 18.0 + column * ((width - 56.0).max(400.0) / 20.0) + travel,
                        y: 150.0 + row * 30.0 + travel * 0.35,
                        width: 30.0,
                        height: 20.0,
                    },
                });
            }
        }
        placements.push(RetainedPlacement {
            key: "box/workspace-controls".into(),
            definition_uid: "workspace-controls".into(),
            inputs: self.workspace_control_inputs(),
            rect: RetainedRect {
                x: (width - 300.0).max(0.0),
                y: (height - 36.0).max(0.0),
                width: 292.0,
                height: 28.0,
            },
        });
        if self.information_open {
            let panel_width = width.min(460.0).max(1.0);
            let panel_height = height.min(396.0).max(1.0);
            placements.push(RetainedPlacement {
                key: "box/information".into(),
                definition_uid: "information".into(),
                inputs: self.information_inputs(),
                rect: RetainedRect {
                    x: (width - panel_width - 8.0).max(0.0),
                    y: (height - panel_height - 46.0).max(0.0),
                    width: panel_width,
                    height: panel_height,
                },
            });
        }
        self.box_scene = RetainedScene::from_placements(
            &self.sand_package,
            "box-workspace",
            placements,
            self.focused_control,
        )?;
        Ok(())
    }

    fn information_inputs(&self) -> BTreeMap<String, SandValue> {
        BTreeMap::from([
            (
                "port".into(),
                SandValue::Text(self.workspaces.port().to_string()),
            ),
            (
                "data-directory".into(),
                SandValue::Text(self.workspaces.data_directory().display().to_string()),
            ),
            (
                "tests-directory".into(),
                SandValue::Text(self.workspaces.tests_directory().display().to_string()),
            ),
            (
                "version".into(),
                SandValue::Text(env!("CARGO_PKG_VERSION").into()),
            ),
            ("revision".into(), SandValue::Text(self.revision.clone())),
            (
                "last-updated".into(),
                SandValue::Text(self.last_updated.clone()),
            ),
            (
                "updates".into(),
                SandValue::Text(self.update.updates_line()),
            ),
            (
                "self-apply".into(),
                SandValue::Text(self.update.self_apply_line()),
            ),
            (
                "status".into(),
                SandValue::Text(self.workspace_status.clone()),
            ),
        ])
    }

    fn workspace_control_inputs(&self) -> BTreeMap<String, SandValue> {
        BTreeMap::from([(
            "workspace-name".into(),
            SandValue::Text(self.workspaces.active().name.clone()),
        )])
    }

    fn focus_next_control(&mut self, backwards: bool) {
        let count = self.box_scene.interactive_count();
        if count == 0 {
            return;
        }
        self.focused_control = if backwards {
            self.focused_control.checked_sub(1).unwrap_or(count - 1)
        } else {
            (self.focused_control + 1) % count
        };
        self.box_scene.focus_interactive(self.focused_control);
        self.window.request_redraw();
    }

    fn focus_at(&mut self, index: usize) {
        self.focused_control = index;
        self.box_scene.focus_interactive(index);
        self.window.request_redraw();
    }

    fn activate_focused_control(&mut self) {
        let Some((key, editable)) = self
            .box_scene
            .focused()
            .map(|node| (node.key.clone(), node.editable))
        else {
            return;
        };
        if editable {
            self.editing_workspace_name = true;
            self.replace_workspace_name_on_input = true;
            self.workspace_status = "Editing workspace name".into();
        } else {
            for behavior in self.behaviors_for_control(&key) {
                match behavior {
                    DeclarativeBehavior::EmitEvent { event, .. } => {
                        self.dispatch_box_event(&event);
                    }
                    DeclarativeBehavior::RequestAction { action, .. } => {
                        self.dispatch_action(&action);
                    }
                    DeclarativeBehavior::SetLocalState { .. }
                    | DeclarativeBehavior::TogglePresentationOnEvent { .. } => {}
                }
            }
        }
        self.focused_control = 0;
        if let Err(error) = self.refresh_box_scene() {
            self.workspace_status = error;
        }
        self.window.request_redraw();
    }

    fn behaviors_for_control(&self, key: &str) -> Vec<DeclarativeBehavior> {
        for (root_key, definition_uid) in [
            ("box/workspace-controls", "workspace-controls"),
            ("box/information", "information"),
        ] {
            let Some(child_path) = key
                .strip_prefix(root_key)
                .and_then(|path| path.strip_prefix('/'))
            else {
                continue;
            };
            let Some(child_uid) = child_path.split('/').next() else {
                continue;
            };
            let Some(definition) = self.sand_package.graph.definitions.get(definition_uid) else {
                continue;
            };
            let mut behaviors = Vec::new();
            if let Some(child) = definition
                .children
                .iter()
                .find(|child| child.local_uid == child_uid)
                && let Some(child_definition) = self
                    .sand_package
                    .graph
                    .definitions
                    .get(&child.definition.uid)
            {
                behaviors.extend(child_definition.behaviors.iter().filter_map(|binding| {
                    match binding {
                        BehaviorBinding::Declarative { behavior }
                            if matches!(
                                behavior,
                                DeclarativeBehavior::EmitEvent {
                                    source_output,
                                    ..
                                } | DeclarativeBehavior::RequestAction {
                                    source_output,
                                    ..
                                } if source_output == "pressed"
                            ) =>
                        {
                            Some(behavior.clone())
                        }
                        _ => None,
                    }
                }));
            }
            if let Some(source_output) = definition.exports.iter().find(|export| {
                export.direction == PortDirection::Output
                    && export.child_uid == child_uid
                    && export.child_port == "pressed"
            }) {
                behaviors.extend(
                    definition
                        .behaviors
                        .iter()
                        .filter_map(|binding| match binding {
                            BehaviorBinding::Declarative { behavior }
                                if matches!(
                                    behavior,
                                    DeclarativeBehavior::EmitEvent {
                                        source_output: source,
                                        ..
                                    } | DeclarativeBehavior::RequestAction {
                                        source_output: source,
                                        ..
                                    } if source == &source_output.name
                                ) =>
                            {
                                Some(behavior.clone())
                            }
                            _ => None,
                        }),
                );
            }
            return behaviors;
        }
        Vec::new()
    }

    fn dispatch_box_event(&mut self, event: &str) {
        let toggles_information = self
            .sand_package
            .graph
            .definitions
            .get("information")
            .is_some_and(|definition| {
                definition.behaviors.iter().any(|binding| {
                    matches!(
                        binding,
                        BehaviorBinding::Declarative {
                            behavior: DeclarativeBehavior::TogglePresentationOnEvent {
                                event: listened_event,
                            },
                        } if listened_event == event
                    )
                })
            });
        if toggles_information {
            self.information_open = !self.information_open;
            self.editing_workspace_name = false;
            self.replace_workspace_name_on_input = false;
            self.workspace_status = if self.information_open {
                "Information open".into()
            } else {
                "Ready".into()
            };
        } else if event == "deleteWorkspace" {
            let _ = self.checkpoint_laboratory();
            let removed = self.workspaces.delete_active();
            self.editing_workspace_name = false;
            self.replace_workspace_name_on_input = false;
            self.animation_frame = 0;
            self.workspace_status = format!("Deleted {}", removed.name);
            self.refresh_text();
        }
    }

    fn dispatch_action(&mut self, action: &str) {
        match action {
            "interface.workspace.laboratory.create" => {
                self.editing_workspace_name = false;
                self.replace_workspace_name_on_input = false;
                match self.workspaces.start_laboratory() {
                    Ok(workspace) => {
                        self.workspace_status = format!("Running {}", workspace.name);
                        self.animation_frame = 0;
                        self.refresh_text();
                        let _ = self.checkpoint_laboratory();
                    }
                    Err(error) => self.workspace_status = format!("Laboratory failed: {error}"),
                }
            }
            "interface.update.check" => {
                self.update.start_check();
                self.workspace_status = "Checking for updates".into();
            }
            "interface.update.install" => {
                self.update.start_install();
                self.workspace_status = "Installing update".into();
            }
            _ => {}
        }
    }

    fn append_workspace_name(&mut self, text: &str) {
        if !self.editing_workspace_name {
            return;
        }
        let mut name = if self.replace_workspace_name_on_input {
            String::new()
        } else {
            self.workspaces.active().name.clone()
        };
        self.replace_workspace_name_on_input = false;
        for character in text.chars().filter(|character| !character.is_control()) {
            if name.chars().count() >= 80 {
                break;
            }
            name.push(character);
        }
        if let Err(error) = self.workspaces.rename_active(&name) {
            self.workspace_status = error;
        } else {
            self.workspace_status = "Workspace name changed".into();
            self.refresh_text();
        }
        let _ = self.refresh_box_scene();
    }

    fn backspace_workspace_name(&mut self) {
        if !self.editing_workspace_name {
            return;
        }
        self.replace_workspace_name_on_input = false;
        let mut name = self.workspaces.active().name.clone();
        name.pop();
        if !name.is_empty() {
            let _ = self.workspaces.rename_active(&name);
            self.refresh_text();
            let _ = self.refresh_box_scene();
        }
    }

    fn checkpoint_laboratory(&mut self) -> Result<(), String> {
        self.report.record_input_evidence(&self.input_evidence);
        #[cfg(feature = "bevy-preflight")]
        self.report.mark_bevy_host_running(
            self.bevy_host.updates(),
            self.bevy_host.handed_off_command_buffers(),
            true,
        );
        if let Some(path) = self
            .workspaces
            .write_active_laboratory_report(&self.report)?
        {
            self.last_export = Some(path.display().to_string());
        }
        Ok(())
    }

    fn handle_interface_key(&mut self, key: &Key, text: Option<&str>) -> bool {
        if self.editing_workspace_name {
            match key {
                Key::Named(NamedKey::Backspace) => self.backspace_workspace_name(),
                Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Escape) => {
                    self.editing_workspace_name = false;
                    self.replace_workspace_name_on_input = false;
                    self.workspace_status = "Workspace name saved".into();
                    let _ = self.refresh_box_scene();
                }
                Key::Character(_) => {
                    if let Some(text) = text {
                        self.append_workspace_name(text);
                    }
                }
                _ => {}
            }
            return true;
        }
        match key {
            Key::Named(NamedKey::Tab) => {
                self.focus_next_control(self.input_modifiers.shift);
                true
            }
            Key::Named(NamedKey::Enter) => {
                self.activate_focused_control();
                true
            }
            Key::Character(value) if value == " " => {
                self.activate_focused_control();
                true
            }
            _ => false,
        }
    }

    fn activate_at_cursor(&mut self) {
        let point = [self.input_cursor.x, self.input_cursor.y];
        let Some(index) = self.box_scene.hit_test(point) else {
            return;
        };
        self.focus_at(index);
        self.activate_focused_control();
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        let size = nonzero_size(size);
        self.surface_config.width = size.width;
        self.surface_config.height = size.height;
        self.surface.configure(&self.device, &self.surface_config);
        self.retained_text_layer
            .resize(&self.queue, size.width, size.height, self.scale_factor);
        let _ = self.refresh_box_scene();
        self.refresh_text();
    }

    fn export_report(&mut self) {
        self.report.record_input_evidence(&self.input_evidence);
        #[cfg(feature = "bevy-preflight")]
        self.report.mark_bevy_host_running(
            self.bevy_host.updates(),
            self.bevy_host.handed_off_command_buffers(),
            true,
        );
        self.last_export = Some(match self.report.write_default() {
            Ok(path) => path.display().to_string(),
            Err(error) => format!("EXPORT FAILED: {error}"),
        });
        self.refresh_text();
    }

    fn request_report_export(&mut self) {
        self.report_export_requested = true;
        self.window.request_redraw();
    }

    fn export_dependency_audit(&mut self) {
        self.last_export = Some(
            DependencyAudit::collect()
                .and_then(|audit| audit.write_default())
                .map_or_else(
                    |error| format!("AUDIT FAILED: {error}"),
                    |path| path.display().to_string(),
                ),
        );
        self.refresh_text();
    }

    fn mark_input_started(&mut self) {
        self.pending_input_started.get_or_insert_with(Instant::now);
        self.window.request_redraw();
    }

    fn accept_input(&mut self, event: NormalizedInput) {
        self.input_sequence = self.input_sequence.saturating_add(1);
        let surface = InputSurface::new(
            "box-main",
            self.surface_config.width,
            self.surface_config.height,
            self.scale_factor,
        );
        let target = InputTarget::full_surface("interface-laboratory", "lince-winit", &surface);
        let input = InputEnvelope::new(
            self.input_sequence,
            InputSource::LinceWinit,
            surface,
            target,
            event,
        );
        if let Err(error) = self.input_evidence.record(input) {
            self.window.set_title(&format!(
                "Lince Interface Laboratory — input refused: {error}"
            ));
        } else {
            self.mark_input_started();
        }
        self.report.record_input_evidence(&self.input_evidence);
    }

    fn start_window_stress(&mut self) {
        if self.window_stress.is_none() {
            self.window_stress = Some(WindowStress::new());
        }
        self.request_next_window_stress();
        self.refresh_text();
    }

    fn request_next_window_stress(&mut self) {
        let request = self
            .window_stress
            .as_mut()
            .and_then(|stress| stress.request_next(self.scale_factor));
        if let Some(request) = request {
            if self.window.is_maximized() {
                self.window.set_maximized(false);
            }
            let _ = self.window.request_inner_size(LogicalSize::new(
                f64::from(request.logical_width),
                f64::from(request.logical_height),
            ));
        }
        if let Some(stress) = &self.window_stress {
            self.report.record_window_stress(stress);
        }
        self.window.request_redraw();
    }

    fn advance_window_stress(&mut self) {
        let observed = self.window_stress.as_mut().is_some_and(|stress| {
            stress.observe(
                self.surface_config.width,
                self.surface_config.height,
                self.scale_factor,
            )
        });
        if observed {
            self.request_next_window_stress();
            self.refresh_text();
        } else if let Some(stress) = &self.window_stress {
            self.report.record_window_stress(stress);
            if !stress.is_complete() {
                self.window.request_redraw();
            }
        }
    }

    fn window_stress_complete(&self) -> bool {
        self.window_stress
            .as_ref()
            .is_some_and(WindowStress::is_complete)
    }

    fn prepare_teardown(&mut self) {
        if self.teardown_prepared {
            return;
        }
        self.teardown_prepared = true;
        let gpu_idle = self
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(2)),
            })
            .is_ok();
        self.report.record_teardown(
            gpu_idle,
            u64::from(!gpu_idle),
            Some(gpu_idle),
            "not applicable to Lince-owned host",
            true,
        );
    }

    fn recover_surface(&mut self) -> bool {
        match self.instance.create_surface(self.window.clone()) {
            Ok(surface) => {
                self.report.record_surface_recovery(true);
                self.surface = surface;
                self.surface.configure(&self.device, &self.surface_config);
                self.window.request_redraw();
                true
            }
            Err(error) => {
                self.report.record_surface_recovery(false);
                self.window.set_title(&format!(
                    "Lince Interface Laboratory — surface recovery failed: {error}"
                ));
                false
            }
        }
    }

    fn run_surface_recovery_probe(&mut self) {
        self.recovery_probe_complete = true;
        self.recover_surface();
        self.refresh_text();
    }

    fn render(&mut self) {
        let frame_started = Instant::now();
        self.report.record_host_redraw();
        #[cfg(feature = "bevy-preflight")]
        self.bevy_host.prepare_frame();
        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.surface_config.width,
                height: self.surface_config.height,
            },
        );
        if matches!(
            self.workspaces.active().kind,
            NativeWorkspaceKind::Laboratory { .. }
        ) {
            self.animation_frame = self.animation_frame.saturating_add(1);
            if let Err(error) = self.refresh_box_scene() {
                self.workspace_status = error;
            }
        }
        if let Err(error) = self.node_layer.update_retained(
            &self.queue,
            &self.box_scene,
            [self.surface_config.width, self.surface_config.height],
            &self.retained_style,
        ) {
            self.window
                .set_title(&format!("Lince native Interface — Sand error: {error}"));
            return;
        }
        if let Err(error) = self.retained_text_layer.prepare(
            &self.device,
            &self.queue,
            Some(&self.box_scene),
            &self.retained_style,
        ) {
            self.window
                .set_title(&format!("Lince native Interface — text error: {error}"));
            return;
        }

        let margin = scaled(28.0, self.scale_factor);
        let right = i32::try_from(self.surface_config.width).unwrap_or(i32::MAX);
        let bottom = i32::try_from(self.surface_config.height).unwrap_or(i32::MAX);
        if let Err(error) = self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            [TextArea {
                buffer: &self.text_buffer,
                left: margin,
                top: margin,
                scale: 1.0,
                bounds: TextBounds {
                    left: margin as i32,
                    top: margin as i32,
                    right,
                    bottom,
                },
                default_color: Color::rgb(248, 250, 252),
                custom_glyphs: &[],
            }],
            &mut self.swash_cache,
        ) {
            self.window
                .set_title(&format!("Lince Interface Laboratory — text error: {error}"));
            return;
        }

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                drop(frame);
                self.surface.configure(&self.device, &self.surface_config);
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.recover_surface();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                self.window
                    .set_title("Lince Interface Laboratory — validation failure");
                return;
            }
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("lince-interface-frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("lince-interface-panel"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color {
                            r: 18.0 / 255.0,
                            g: 18.0 / 255.0,
                            b: 20.0 / 255.0,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            #[cfg(feature = "bevy-preflight")]
            self.bevy_layer.render(&mut pass);
            self.node_layer.render(&mut pass);
            if let Err(error) = self.retained_text_layer.render(&mut pass) {
                self.window.set_title(&format!(
                    "Lince native Interface — retained text error: {error}"
                ));
                return;
            }
            if let Err(error) = self
                .text_renderer
                .render(&self.atlas, &self.viewport, &mut pass)
            {
                self.window.set_title(&format!(
                    "Lince Interface Laboratory — render error: {error}"
                ));
                return;
            }
        }
        #[cfg(feature = "bevy-preflight")]
        let assembly = {
            let mut assembly = FrameAssembly::new();
            for commands in self.bevy_host.take_output_commands() {
                assembly.contribute("bevy-world", commands);
            }
            assembly
        };
        #[cfg(not(feature = "bevy-preflight"))]
        let assembly = FrameAssembly::new();
        let submission = assembly.seal("lince-compositor", encoder.finish());
        let submitted_command_buffers = submission.command_buffer_count();
        let submission_participants = submission.participants();
        self.queue.submit(submission.into_commands());
        if let Some(started) = self.pending_input_started.take() {
            self.report.record_input_to_submit(started.elapsed());
        }
        frame.present();
        self.report.record_host_frame(
            frame_started.elapsed(),
            submitted_command_buffers,
            submission_participants,
        );
        self.atlas.trim();
        self.retained_text_layer.trim();
        if self.animation_frame > 0 && self.animation_frame.is_multiple_of(120) {
            if let Err(error) = self.checkpoint_laboratory() {
                self.workspace_status = format!("Report failed: {error}");
            }
        }
        if self.report_export_requested {
            self.report_export_requested = false;
            self.export_report();
        }
        self.advance_window_stress();
        #[cfg(feature = "bevy-preflight")]
        {
            if self.report.runtime.host_frames_presented.is_multiple_of(60) {
                self.refresh_text();
            } else {
                self.window.request_redraw();
            }
        }
    }
}

impl ApplicationHandler for LaboratoryApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_inner_size(LogicalSize::new(1040.0, 760.0))
            .with_min_inner_size(LogicalSize::new(720.0, 560.0))
            .with_title("Lince Interface Laboratory — starting");
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("create interface laboratory window"),
        );
        let size = nonzero_size(window.inner_size());
        let scale_factor = window.scale_factor();
        self.state = Some(
            match pollster::block_on(RenderState::new(
                window.clone(),
                event_loop,
                self.stress_resize_and_exit,
            )) {
                Ok(state) => LaboratoryWindow::Ready(Box::new(state)),
                Err(error) => {
                    window.set_title(&format!(
                        "Lince Interface Laboratory — unavailable: {error}"
                    ));
                    eprintln!("Lince Interface Laboratory unavailable: {error}");
                    LaboratoryWindow::Unavailable(Box::new(UnavailableState {
                        report: LaboratoryReport::ownership_preflight(HostFacts::unavailable(
                            scale_factor,
                            size.width,
                            size.height,
                            error,
                        )),
                        window,
                    }))
                }
            },
        );
        match &mut self.state {
            Some(LaboratoryWindow::Ready(state)) => state.window.request_redraw(),
            Some(LaboratoryWindow::Unavailable(state)) if self.export_on_first_frame => {
                match state.report.write_default() {
                    Ok(path) => eprintln!("Report: {}", path.display()),
                    Err(error) => eprintln!("Report export failed: {error}"),
                }
                event_loop.exit();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(LaboratoryWindow::Ready(state)) = &mut self.state {
            if state.update.pump() {
                if let Err(error) = state.refresh_box_scene() {
                    state.workspace_status = error;
                }
                state.refresh_text();
            }
            if state.update.busy() {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(150),
                ));
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };
        let expected_window_id = match state {
            LaboratoryWindow::Ready(state) => state.window.id(),
            LaboratoryWindow::Unavailable(state) => state.window.id(),
        };
        if window_id != expected_window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                if let LaboratoryWindow::Ready(state) = state {
                    state.prepare_teardown();
                }
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let logical_key = event.logical_key.clone();
                let pressed = event.state == ElementState::Pressed;
                let interface_handled = match state {
                    LaboratoryWindow::Ready(state) => {
                        state.accept_input(normalize_winit_key(&event, state.input_modifiers));
                        pressed
                            && state.handle_interface_key(
                                &logical_key,
                                event.text.as_ref().map(|text| text.as_str()),
                            )
                    }
                    LaboratoryWindow::Unavailable(state) => {
                        state.report.record_keyboard_input();
                        false
                    }
                };
                if pressed && !interface_handled {
                    match logical_key {
                        Key::Named(NamedKey::Escape) => {
                            if let LaboratoryWindow::Ready(state) = state {
                                state.prepare_teardown();
                            }
                            event_loop.exit();
                        }
                        Key::Character(value) if value.eq_ignore_ascii_case("a") => match state {
                            LaboratoryWindow::Ready(state) => state.export_dependency_audit(),
                            LaboratoryWindow::Unavailable(state) => {
                                let result = DependencyAudit::collect()
                                    .and_then(|audit| audit.write_default())
                                    .map_or_else(
                                        |error| format!("audit failed: {error}"),
                                        |path| format!("audit exported to {}", path.display()),
                                    );
                                state
                                    .window
                                    .set_title(&format!("Lince Interface Laboratory — {result}"));
                            }
                        },
                        Key::Character(value) if value.eq_ignore_ascii_case("e") => match state {
                            LaboratoryWindow::Ready(state) => state.request_report_export(),
                            LaboratoryWindow::Unavailable(state) => {
                                match state.report.write_default() {
                                    Ok(path) => state.window.set_title(&format!(
                                        "Lince Interface Laboratory — report exported to {}",
                                        path.display()
                                    )),
                                    Err(error) => state.window.set_title(&format!(
                                        "Lince Interface Laboratory — report export failed: {error}"
                                    )),
                                }
                            }
                        },
                        Key::Character(value) if value.eq_ignore_ascii_case("r") => {
                            if let LaboratoryWindow::Ready(state) = state {
                                state.start_window_stress();
                            }
                        }
                        Key::Character(value) if value.eq_ignore_ascii_case("d") => {
                            if let LaboratoryWindow::Ready(state) = state {
                                state.run_surface_recovery_probe();
                            }
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => match state {
                LaboratoryWindow::Ready(state) => {
                    state.input_modifiers = normalize_winit_modifiers(modifiers.state());
                    state.accept_input(NormalizedInput::ModifiersChanged {
                        modifiers: state.input_modifiers,
                    });
                }
                LaboratoryWindow::Unavailable(state) => state.report.record_keyboard_input(),
            },
            WindowEvent::CursorMoved { position, .. } => match state {
                LaboratoryWindow::Ready(state) => {
                    state.input_cursor = Point::new(position.x, position.y);
                    state.accept_input(NormalizedInput::PointerMoved {
                        surface_physical: state.input_cursor,
                        buttons: state.input_buttons.iter().copied().collect(),
                        modifiers: state.input_modifiers,
                    });
                }
                LaboratoryWindow::Unavailable(state) => state.report.record_pointer_input(),
            },
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => match state {
                LaboratoryWindow::Ready(state) => {
                    let button = normalize_winit_button(button);
                    if button_state == ElementState::Pressed {
                        state.input_buttons.insert(button);
                    } else {
                        state.input_buttons.remove(&button);
                    }
                    state.accept_input(NormalizedInput::PointerButton {
                        surface_physical: state.input_cursor,
                        button,
                        state: normalize_button_state(button_state),
                        modifiers: state.input_modifiers,
                    });
                    if button == PointerButton::Left && button_state == ElementState::Pressed {
                        state.activate_at_cursor();
                    }
                }
                LaboratoryWindow::Unavailable(state) => state.report.record_pointer_input(),
            },
            WindowEvent::MouseWheel { delta, .. } => match state {
                LaboratoryWindow::Ready(state) => {
                    let (delta, unit) = normalize_winit_scroll(delta);
                    state.accept_input(NormalizedInput::Scroll {
                        surface_physical: state.input_cursor,
                        delta,
                        unit,
                        modifiers: state.input_modifiers,
                    });
                }
                LaboratoryWindow::Unavailable(state) => state.report.record_pointer_input(),
            },
            WindowEvent::Touch(touch) => match state {
                LaboratoryWindow::Ready(state) => {
                    state.accept_input(NormalizedInput::Touch {
                        touch_id: touch.id,
                        surface_physical: Point::new(touch.location.x, touch.location.y),
                        phase: normalize_touch_phase(touch.phase),
                    });
                }
                LaboratoryWindow::Unavailable(state) => state.report.record_pointer_input(),
            },
            WindowEvent::Focused(focused) => match state {
                LaboratoryWindow::Ready(state) => {
                    state.accept_input(NormalizedInput::Focus { focused });
                }
                LaboratoryWindow::Unavailable(state) => state.report.record_focus_event(),
            },
            WindowEvent::Ime(ime) => match state {
                LaboratoryWindow::Ready(state) => {
                    if let Ime::Commit(text) = &ime {
                        state.append_workspace_name(text);
                    }
                    state.accept_input(normalize_winit_ime(ime));
                }
                LaboratoryWindow::Unavailable(state) => state.report.record_ime_event(),
            },
            WindowEvent::Resized(size) => {
                if let LaboratoryWindow::Ready(state) = state {
                    state.report.record_resize();
                    state.resize(size);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let LaboratoryWindow::Ready(state) = state {
                    state.report.record_scale_factor_change();
                    state.scale_factor = scale_factor;
                    state.resize(state.window.inner_size());
                }
            }
            WindowEvent::RedrawRequested => {
                if let LaboratoryWindow::Ready(state) = state {
                    state.render();
                    if self.stress_resize_and_exit && state.window_stress_complete() {
                        if state.recovery_probe_complete {
                            state.prepare_teardown();
                            state.export_report();
                            if let Some(path) = &state.last_export {
                                eprintln!("Report: {path}");
                            }
                            event_loop.exit();
                        } else {
                            state.run_surface_recovery_probe();
                        }
                    } else if self.export_on_first_frame {
                        state.prepare_teardown();
                        state.export_report();
                        if let Some(path) = &state.last_export {
                            eprintln!("Report: {path}");
                        }
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }
}

fn nonzero_size(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
}

fn scaled(value: f32, scale_factor: f64) -> f32 {
    value * scale_factor as f32
}

fn executable_modified_at() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| std::fs::metadata(path).ok())
        .and_then(|metadata| metadata.modified().ok())
        .map(chrono::DateTime::<chrono::Utc>::from)
        .map(|updated| updated.to_rfc3339())
        .unwrap_or_else(|| "Unknown".into())
}

fn normalize_winit_key(event: &KeyEvent, modifiers: InputModifiers) -> NormalizedInput {
    NormalizedInput::Key {
        physical_key: Some(format!("{:?}", event.physical_key)),
        logical_key: format!("{:?}", event.logical_key),
        text: event.text.as_ref().map(ToString::to_string),
        state: normalize_button_state(event.state),
        repeat: event.repeat,
        modifiers,
    }
}

fn normalize_winit_modifiers(modifiers: ModifiersState) -> InputModifiers {
    InputModifiers {
        control: modifiers.control_key(),
        alt: modifiers.alt_key(),
        shift: modifiers.shift_key(),
        platform: modifiers.super_key(),
        function: false,
    }
}

fn normalize_winit_button(button: WinitMouseButton) -> PointerButton {
    match button {
        WinitMouseButton::Left => PointerButton::Left,
        WinitMouseButton::Right => PointerButton::Right,
        WinitMouseButton::Middle => PointerButton::Middle,
        WinitMouseButton::Back => PointerButton::Back,
        WinitMouseButton::Forward => PointerButton::Forward,
        WinitMouseButton::Other(number) => PointerButton::Other(number),
    }
}

fn normalize_button_state(state: ElementState) -> InputButtonState {
    match state {
        ElementState::Pressed => InputButtonState::Pressed,
        ElementState::Released => InputButtonState::Released,
    }
}

fn normalize_winit_scroll(delta: MouseScrollDelta) -> (Point, ScrollUnit) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => {
            (Point::new(f64::from(x), f64::from(y)), ScrollUnit::Lines)
        }
        MouseScrollDelta::PixelDelta(position) => (
            Point::new(position.x, position.y),
            ScrollUnit::PhysicalPixels,
        ),
    }
}

fn normalize_touch_phase(phase: TouchPhase) -> InputTouchPhase {
    match phase {
        TouchPhase::Started => InputTouchPhase::Started,
        TouchPhase::Moved => InputTouchPhase::Moved,
        TouchPhase::Ended => InputTouchPhase::Ended,
        TouchPhase::Cancelled => InputTouchPhase::Cancelled,
    }
}

fn normalize_winit_ime(ime: Ime) -> NormalizedInput {
    match ime {
        Ime::Enabled => NormalizedInput::ImeEnabled,
        Ime::Preedit(text, cursor) => NormalizedInput::ImePreedit {
            text,
            cursor: cursor.map(|(start, end)| [start, end]),
        },
        Ime::Commit(text) => NormalizedInput::ImeCommit { text },
        Ime::Disabled => NormalizedInput::ImeDisabled,
    }
}
