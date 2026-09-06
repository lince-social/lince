use cef::wrapper::{
    byte_read_handler::{ByteReadHandler, ByteStream},
    stream_resource_handler::StreamResourceHandler,
};
use cef::*;
use lince_interface::cef_vulkan::{VulkanCopyFacts, VulkanCopyTarget};
use lince_interface::html::{
    BridgeDecision, BridgeRequest, BridgeSession, CefInputCommand, CefPointerButton,
    HtmlSurfaceManifest, InstalledGrants, MediaGrant, route_cef_input,
};
use lince_interface::input::{
    AffineTransform, ButtonState, InputEnvelope, InputModifiers, InputSource, InputSurface,
    InputTarget, NormalizedInput, Point as InputPoint, PointerButton, Rect as InputRect,
    ScrollUnit,
};
use lince_interface::primitive_gallery::{gallery_mount_envelope, installed_gallery_asset};
#[cfg(feature = "joined-runtime")]
use lince_interface::{
    bevy_host::BevyHost,
    bevy_layer::BevyLayer,
    composition::{
        COMPOSITION_SCHEMA_VERSION, CompositionRuntimeFacts, CompositionWorkbenchState,
        WorkbenchOperation, composition_workbench_package,
    },
    configuration::{
        CONFIGURATION_DEFINITION_COUNT, CONFIGURATION_SCHEMA_VERSION, ConfigurationFacts,
        ConfigurationOperation, ConfigurationWorkbenchState, configuration_sand_package,
        default_configuration_path,
    },
    domain::{NativeDomainClient, NativeDomainFacts},
    frame::FrameAssembly,
    joined_accessibility::JoinedAccessibility,
    joined_panel::JoinedPanel,
    node_layer::NodeLayer,
    official_runtime::{
        OfficialRuntimeIntent, SharedOfficialRuntimeFacts, SharedOfficialRuntimeState,
    },
    official_sands::{
        OFFICIAL_SAND_COUNT, OFFICIAL_SANDS, OfficialMigrationState, OfficialSandGalleryState,
        official_sand_package,
    },
    primitive_gallery::{PRIMITIVE_COUNT, PrimitiveGalleryState, primitive_gallery_package},
    retained_ui::{RetainedRect, RetainedScene, RetainedTextLayer},
    sand::{SAND_ABI_VERSION, SAND_SCHEMA_VERSION},
    spatial::{FieldSolver, PhysicsAdapter},
    style::{ResolvedStyle, STYLE_CONTRACT_VERSION, StyleGalleryState, StyleScope, StyleValue},
};
use lince_interface::{git_dirty, raw_git_revision, source_fingerprint};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use wgpu::{
    Backends, CompositeAlphaMode, DeviceDescriptor, Instance, InstanceDescriptor, LoadOp,
    Operations, PresentMode, RenderPassColorAttachment, RenderPassDescriptor,
    RequestAdapterOptions, StoreOp, SurfaceConfiguration, TextureUsages, TextureViewDescriptor,
};
#[cfg(target_os = "linux")]
use winit::platform::wayland::EventLoopBuilderExtWayland;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{
        ElementState, Ime, KeyEvent as WinitKeyEvent, MouseButton as WinitMouseButton,
        MouseScrollDelta, WindowEvent,
    },
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

const INSTALLED_URL: &str = "lince-sand://installed/index.html";
const INSTALLED_ORIGIN: &str = "lince-sand://installed";
const DEFAULT_WEBSITE_URL: &str = "https://example.com/";
const BRIDGE_MESSAGE: &str = "lince.bridge";
#[cfg(feature = "joined-runtime")]
const ACTIVE_BODY_COUNT: usize = 1_000;
#[cfg(feature = "joined-runtime")]
const RESIDENT_NODE_COUNT: usize = 10_000;
#[cfg(feature = "joined-runtime")]
const VISIBLE_NATIVE_SAND_COUNT: usize = 200;
#[cfg(feature = "joined-runtime")]
const FIXED_STEP_HZ: u32 = 120;
#[cfg(feature = "joined-runtime")]
const FIXED_STEP: Duration = Duration::from_nanos(1_000_000_000 / FIXED_STEP_HZ as u64);
#[cfg(feature = "joined-runtime")]
const MAX_FIXED_STEPS_PER_FRAME: u32 = 8;
#[cfg(feature = "joined-runtime")]
const SCENARIOS: [&str; 8] = [
    "Composition and character",
    "Joined compositor",
    "External authority",
    "Box motion",
    "Camera invariance",
    "Coordinate and specialised pass",
    "Definition replay and browser parity",
    "Accessibility and recovery",
];
static CEF_CONTEXT_READY: AtomicBool = AtomicBool::new(false);
static CEF_NOTICES_BUNDLED: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
struct SurfaceShared {
    label: &'static str,
    url: String,
    manifest: HtmlSurfaceManifest,
    size: Arc<Mutex<[i32; 2]>>,
    scale: Arc<Mutex<f64>>,
    gpu: Arc<Mutex<VulkanCopyTarget>>,
    bridge: Arc<Mutex<Option<BridgeSession>>>,
    bridge_allowed: Arc<AtomicU64>,
    events_allowed: Arc<AtomicU64>,
    actions_allowed: Arc<AtomicU64>,
    bridge_refused: Arc<AtomicU64>,
    latest_bridge_sequence: Arc<AtomicU64>,
    cpu_paints: Arc<AtomicU64>,
    external_network_requests: Arc<AtomicU64>,
    load_completions: Arc<AtomicU64>,
    load_failures: Arc<AtomicU64>,
    renderer_restarts: Arc<AtomicU64>,
    popup_refusals: Arc<AtomicU64>,
    context_menu_refusals: Arc<AtomicU64>,
    media_allowed: Arc<AtomicU64>,
    media_refused: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
    latest_status: Arc<Mutex<String>>,
    latest_page_status: Arc<Mutex<String>>,
}

impl SurfaceShared {
    fn installed(device: wgpu::Device, index: usize) -> Result<Self, String> {
        let manifest = HtmlSurfaceManifest::Installed {
            partition_key: format!("installed-diagnostic-{index}"),
            package_id: format!("installed-{index}"),
            origin: INSTALLED_ORIGIN.into(),
            grants: InstalledGrants {
                protein_read: true,
                events: BTreeSet::from(["record-clicked".into()]),
                actions: BTreeSet::from(["record.open".into()]),
                persistent_storage: true,
                media: BTreeSet::new(),
            },
        };
        Self::new("installed", INSTALLED_URL.into(), manifest, device)
    }

    fn website(device: wgpu::Device, index: usize, website_url: &str) -> Result<Self, String> {
        let origin = observed_origin(website_url)
            .ok_or_else(|| "Website Sand requires an HTTP or HTTPS origin".to_string())?;
        let manifest = HtmlSurfaceManifest::Website {
            partition_key: format!("website-diagnostic-{index}"),
            origin,
            persistent_storage: false,
            media: BTreeSet::new(),
        };
        Self::new("website", website_url.into(), manifest, device)
    }

    fn new(
        label: &'static str,
        url: String,
        manifest: HtmlSurfaceManifest,
        device: wgpu::Device,
    ) -> Result<Self, String> {
        manifest.validate().map_err(|error| error.to_string())?;
        Ok(Self {
            label,
            url,
            manifest,
            size: Arc::new(Mutex::new([640, 640])),
            scale: Arc::new(Mutex::new(1.0)),
            gpu: Arc::new(Mutex::new(
                VulkanCopyTarget::new(device).map_err(|error| error.to_string())?,
            )),
            bridge: Arc::new(Mutex::new(None)),
            bridge_allowed: Arc::new(AtomicU64::new(0)),
            events_allowed: Arc::new(AtomicU64::new(0)),
            actions_allowed: Arc::new(AtomicU64::new(0)),
            bridge_refused: Arc::new(AtomicU64::new(0)),
            latest_bridge_sequence: Arc::new(AtomicU64::new(0)),
            cpu_paints: Arc::new(AtomicU64::new(0)),
            external_network_requests: Arc::new(AtomicU64::new(0)),
            load_completions: Arc::new(AtomicU64::new(0)),
            load_failures: Arc::new(AtomicU64::new(0)),
            renderer_restarts: Arc::new(AtomicU64::new(0)),
            popup_refusals: Arc::new(AtomicU64::new(0)),
            context_menu_refusals: Arc::new(AtomicU64::new(0)),
            media_allowed: Arc::new(AtomicU64::new(0)),
            media_refused: Arc::new(AtomicU64::new(0)),
            closed: Arc::new(AtomicBool::new(false)),
            latest_status: Arc::new(Mutex::new("waiting for accelerated CEF paint".into())),
            latest_page_status: Arc::new(Mutex::new("waiting for CEF navigation".into())),
        })
    }

    fn expected_url(&self) -> &str {
        &self.url
    }

    fn is_installed(&self) -> bool {
        matches!(self.manifest, HtmlSurfaceManifest::Installed { .. })
    }

    fn set_status(&self, status: impl Into<String>) {
        if let Ok(mut latest) = self.latest_status.lock() {
            *latest = status.into();
        }
    }

    fn set_page_status(&self, status: impl Into<String>) {
        if let Ok(mut latest) = self.latest_page_status.lock() {
            *latest = status.into();
        }
    }
}

wrap_render_handler! {
    struct DiagnosticRenderHandler {
        shared: SurfaceShared,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            let Some(rect) = rect else { return };
            let size = self.shared.size.lock().map(|value| *value).unwrap_or([1, 1]);
            rect.x = 0;
            rect.y = 0;
            rect.width = size[0].max(1);
            rect.height = size[1].max(1);
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> i32 {
            let Some(screen_info) = screen_info else { return 0 };
            let size = self.shared.size.lock().map(|value| *value).unwrap_or([1, 1]);
            let scale = self.shared.scale.lock().map(|value| *value).unwrap_or(1.0);
            let rect = Rect { x: 0, y: 0, width: size[0].max(1), height: size[1].max(1) };
            screen_info.device_scale_factor = scale as f32;
            screen_info.depth = 24;
            screen_info.depth_per_component = 8;
            screen_info.is_monochrome = 0;
            screen_info.rect = rect.clone();
            screen_info.available_rect = rect;
            1
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            _type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            _buffer: *const u8,
            _width: i32,
            _height: i32,
        ) {
            self.shared.cpu_paints.fetch_add(1, Ordering::Relaxed);
            self.shared.set_status("REFUSED: CEF attempted a CPU paint path");
        }

        fn on_accelerated_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            info: Option<&AcceleratedPaintInfo>,
        ) {
            if type_ != PaintElementType::VIEW {
                self.shared.set_status("REFUSED: unexpected CEF popup texture");
                return;
            }
            let Some(info) = info else {
                self.shared.set_status("REFUSED: accelerated CEF paint omitted metadata");
                return;
            };
            let result = self.shared.gpu.lock().map_err(|_| "CEF GPU target lock failed".to_string()).and_then(|mut target| target.copy_from_cef(info).map_err(|error| error.to_string()));
            match result {
                Ok(()) => self.shared.set_status("accelerated DMA-BUF copied into Lince-owned GPU memory"),
                Err(error) => self.shared.set_status(format!("REFUSED: {error}")),
            }
        }
    }
}

wrap_load_handler! {
    struct DiagnosticLoadHandler {
        shared: SurfaceShared,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: i32,
        ) {
            let Some(frame) = frame else { return };
            if frame.is_main() == 0 {
                return;
            }
            self.shared.load_completions.fetch_add(1, Ordering::Relaxed);
            let url_value = frame.url();
            let url = CefString::from(&url_value).to_string();
            self.shared.set_page_status(format!("loaded {url} with status {http_status_code}"));
            if self.shared.is_installed() {
                let mount_envelope = gallery_mount_envelope();
                let mut invalid_mount_envelope = mount_envelope.clone();
                invalid_mount_envelope["unexpected"] = serde_json::json!(true);
                let mount = serde_json::to_string(&mount_envelope)
                    .unwrap_or_else(|_| "null".into());
                let invalid_mount = serde_json::to_string(&invalid_mount_envelope)
                    .unwrap_or_else(|_| "null".into());
                let code = format!(
                    "window.__linceMount?.({invalid_mount});window.__linceMount?.({mount});window.__linceRefusalProbe?.();window.__linceProbe?.();document.querySelector('[data-popup-probe]')?.focus();navigator.mediaDevices?.getUserMedia({{audio:true}}).catch(()=>{{}})"
                );
                frame.execute_java_script(
                    Some(&CefString::from(code.as_str())),
                    Some(&CefString::from(url.as_str())),
                    0,
                );
                if let Some(host) = browser.and_then(|browser| browser.host()) {
                    host.send_key_event(Some(&KeyEvent {
                        type_: KeyEventType::RAWKEYDOWN,
                        windows_key_code: 13,
                        ..KeyEvent::default()
                    }));
                    host.send_key_event(Some(&KeyEvent {
                        type_: KeyEventType::KEYUP,
                        windows_key_code: 13,
                        ..KeyEvent::default()
                    }));
                }
            } else {
                frame.execute_java_script(
                    Some(&CefString::from(
                        "let linceWebsiteStorage='unavailable';try{localStorage.setItem('lince-website-probe','available');linceWebsiteStorage=localStorage.getItem('lince-website-probe')}catch(error){linceWebsiteStorage=`refused:${error.name}`}document.title=`${document.title} · storage ${linceWebsiteStorage} · bridge ${typeof window.linceBridge==='undefined'?'absent':'PRESENT'}`",
                    )),
                    Some(&CefString::from(url.as_str())),
                    0,
                );
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if frame.is_some_and(|frame| frame.is_main() == 0) {
                return;
            }
            self.shared.load_failures.fetch_add(1, Ordering::Relaxed);
            self.shared.set_page_status(format!(
                "load refused {:?} {} {}",
                error_code,
                failed_url.map(ToString::to_string).unwrap_or_default(),
                error_text.map(ToString::to_string).unwrap_or_default(),
            ));
        }
    }
}

wrap_resource_request_handler! {
    struct DiagnosticResourceRequestHandler {
        shared: SurfaceShared,
    }

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let url = request
                .map(|request| {
                    let value = request.url();
                    CefString::from(&value).to_string()
                })
                .unwrap_or_default();
            if url.starts_with("https://") || url.starts_with("http://") {
                self.shared
                    .external_network_requests
                    .fetch_add(1, Ordering::Relaxed);
            }
            ReturnValue::CONTINUE
        }
    }
}

wrap_display_handler! {
    struct DiagnosticDisplayHandler {
        shared: SurfaceShared,
    }

    impl DisplayHandler {
        fn on_title_change(
            &self,
            _browser: Option<&mut Browser>,
            title: Option<&CefString>,
        ) {
            let title = title.map(ToString::to_string).unwrap_or_default();
            self.shared.set_page_status(format!("title: {title}"));
        }

        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: i32,
        ) -> i32 {
            self.shared.set_page_status(format!(
                "console {:?}: {} at {}:{line}",
                level,
                message.map(ToString::to_string).unwrap_or_default(),
                source.map(ToString::to_string).unwrap_or_default(),
            ));
            0
        }
    }
}

wrap_life_span_handler! {
    struct DiagnosticLifeSpanHandler {
        shared: SurfaceShared,
    }

    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            self.shared.popup_refusals.fetch_add(1, Ordering::Relaxed);
            self.shared.set_status("popup refused at the CEF life-span boundary");
            1
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            self.shared.closed.store(true, Ordering::Release);
        }
    }
}

wrap_request_handler! {
    struct DiagnosticRequestHandler {
        shared: SurfaceShared,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            if frame.is_some_and(|frame| frame.is_main() == 0) {
                return 0;
            }
            let Some(request) = request else { return 1 };
            let url_value = request.url();
            let url = CefString::from(&url_value).to_string();
            let allowed = if self.shared.is_installed() {
                observed_origin(&url).as_deref() == Some(INSTALLED_ORIGIN)
            } else {
                !url.starts_with("lince-sand:")
            };
            if !allowed {
                self.shared.set_status(format!("REFUSED navigation: {url}"));
            }
            i32::from(!allowed)
        }

        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _target_url: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
        ) -> i32 {
            self.shared.popup_refusals.fetch_add(1, Ordering::Relaxed);
            1
        }

        fn on_render_process_terminated(
            &self,
            browser: Option<&mut Browser>,
            _status: TerminationStatus,
            _error_code: i32,
            error_string: Option<&CefString>,
        ) {
            self.shared.renderer_restarts.fetch_add(1, Ordering::Relaxed);
            self.shared.set_status(format!(
                "CEF renderer restarted after {}",
                error_string.map(ToString::to_string).unwrap_or_else(|| "unknown termination".into())
            ));
            if let Some(frame) = browser.and_then(|browser| browser.main_frame()) {
                frame.load_url(Some(&CefString::from(self.shared.expected_url())));
            }
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: i32,
            _is_download: i32,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut i32>,
        ) -> Option<ResourceRequestHandler> {
            Some(DiagnosticResourceRequestHandler::new(self.shared.clone()))
        }
    }
}

wrap_permission_handler! {
    struct DiagnosticPermissionHandler {
        shared: SurfaceShared,
    }

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> i32 {
            let origin = requesting_origin.map(ToString::to_string).unwrap_or_default();
            let allowed = media_is_allowed(&self.shared.manifest, &origin, requested_permissions);
            if let Some(callback) = callback {
                if allowed {
                    callback.cont(requested_permissions);
                    self.shared.media_allowed.fetch_add(1, Ordering::Relaxed);
                } else {
                    callback.cancel();
                    self.shared.media_refused.fetch_add(1, Ordering::Relaxed);
                }
            }
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            _prompt_id: u64,
            _requesting_origin: Option<&CefString>,
            _requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> i32 {
            if let Some(callback) = callback {
                callback.cont(PermissionRequestResult::DENY);
            }
            self.shared.media_refused.fetch_add(1, Ordering::Relaxed);
            1
        }
    }
}

wrap_context_menu_handler! {
    struct DiagnosticContextMenuHandler {
        shared: SurfaceShared,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            if let Some(model) = model {
                model.clear();
            }
            self.shared
                .context_menu_refusals
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

wrap_client! {
    struct DiagnosticClient {
        shared: SurfaceShared,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(DiagnosticRenderHandler::new(self.shared.clone()))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(DiagnosticLifeSpanHandler::new(self.shared.clone()))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(DiagnosticRequestHandler::new(self.shared.clone()))
        }

        fn permission_handler(&self) -> Option<PermissionHandler> {
            Some(DiagnosticPermissionHandler::new(self.shared.clone()))
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(DiagnosticContextMenuHandler::new(self.shared.clone()))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(DiagnosticLoadHandler::new(self.shared.clone()))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(DiagnosticDisplayHandler::new(self.shared.clone()))
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> i32 {
            if source_process != ProcessId::RENDERER {
                return 0;
            }
            let Some(message) = message else { return 0 };
            let message_name = message.name();
            if CefString::from(&message_name).to_string() != BRIDGE_MESSAGE {
                return 0;
            }
            let bytes = message.argument_list().map(|arguments| {
                let value = arguments.string(0);
                CefString::from(&value).to_string()
            }).unwrap_or_default();
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&bytes)
                && let Some(sequence) = value.get("sequence").and_then(serde_json::Value::as_u64)
            {
                self.shared.latest_bridge_sequence.fetch_max(sequence, Ordering::Relaxed);
            }
            let browser_id = browser.as_ref().map(|browser| browser.identifier()).unwrap_or_default();
            let origin = frame.as_ref().and_then(|frame| {
                let value = frame.url();
                observed_origin(&CefString::from(&value).to_string())
            }).unwrap_or_default();
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
            let decision = self.shared.bridge.lock().map_err(|_| "bridge lock failed".to_string()).and_then(|mut session| {
                if session.is_none() {
                    *session = Some(BridgeSession::new(self.shared.label, browser_id, self.shared.manifest.clone()).map_err(|error| error.to_string())?);
                }
                session.as_mut().map(|session| session.handle_json(bytes.as_bytes(), &origin, now)).ok_or_else(|| "bridge session initialization produced no session".to_string())
            });
            let decision = match decision {
                Ok(decision) => decision,
                Err(reason) => BridgeDecision::Refused { surface_id: self.shared.label.into(), browser_id, reason },
            };
            match &decision {
                BridgeDecision::Allowed {
                    request: BridgeRequest::RequestAction { .. },
                    ..
                } => {
                    self.shared.bridge_allowed.fetch_add(1, Ordering::Relaxed);
                    self.shared.actions_allowed.fetch_add(1, Ordering::Relaxed);
                }
                BridgeDecision::Allowed {
                    request: BridgeRequest::EmitEvent { .. },
                    ..
                } => {
                    self.shared.bridge_allowed.fetch_add(1, Ordering::Relaxed);
                    self.shared.events_allowed.fetch_add(1, Ordering::Relaxed);
                }
                BridgeDecision::Allowed { .. } => { self.shared.bridge_allowed.fetch_add(1, Ordering::Relaxed); }
                BridgeDecision::Refused { .. } => { self.shared.bridge_refused.fetch_add(1, Ordering::Relaxed); }
            }
            self.shared.set_status(serde_json::to_string(&decision).unwrap_or_else(|error| error.to_string()));
            if let Some(frame) = frame {
                let serialized = serde_json::to_string(&decision).unwrap_or_else(|_| "null".into());
                let quoted = serde_json::to_string(&serialized).unwrap_or_else(|_| "\"null\"".into());
                let code = format!("window.__linceReply?.(JSON.parse({quoted}))");
                let url_value = frame.url();
                let url = CefString::from(&url_value);
                frame.execute_java_script(Some(&CefString::from(code.as_str())), Some(&url), 0);
            }
            1
        }
    }
}

wrap_v8_handler! {
    struct InstalledBridgeV8Handler;

    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            exception: Option<&mut CefString>,
        ) -> i32 {
            let Some(argument) = arguments.and_then(|values| values.first()).and_then(Option::as_ref) else {
                if let Some(exception) = exception { *exception = CefString::from("linceBridge requires one JSON string") }
                return 1;
            };
            if argument.is_string() == 0 {
                if let Some(exception) = exception { *exception = CefString::from("linceBridge requires one JSON string") }
                return 1;
            }
            let Some(context) = v8_context_get_current_context() else { return 1 };
            let Some(frame) = context.frame() else { return 1 };
            let Some(mut message) = process_message_create(Some(&CefString::from(BRIDGE_MESSAGE))) else { return 1 };
            let Some(arguments) = message.argument_list() else { return 1 };
            let value = argument.string_value();
            let value = CefString::from(&value);
            arguments.set_string(0, Some(&value));
            frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
            1
        }
    }
}

wrap_render_process_handler! {
    struct DiagnosticRenderProcessHandler;

    impl RenderProcessHandler {
        fn on_context_created(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            let Some(frame) = frame else { return };
            let url_value = frame.url();
            let url = CefString::from(&url_value).to_string();
            if frame.is_main() == 0 || observed_origin(&url).as_deref() != Some(INSTALLED_ORIGIN) {
                return;
            }
            let Some(context) = context else { return };
            let Some(global) = context.global() else { return };
            let mut handler = InstalledBridgeV8Handler::new();
            let Some(mut function) = v8_value_create_function(Some(&CefString::from("linceBridge")), Some(&mut handler)) else { return };
            if global.set_value_bykey(Some(&CefString::from("linceBridge")), Some(&mut function), V8Propertyattribute::default()) == 0 {
                eprintln!("CEF renderer refused the installed bridge binding");
            }
        }
    }
}

wrap_browser_process_handler! {
    struct DiagnosticBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            CEF_CONTEXT_READY.store(true, Ordering::Release);
        }
    }
}

wrap_app! {
    struct DiagnosticCefApp;

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(command_line) = command_line else { return };
            command_line.append_switch_with_value(
                Some(&CefString::from("ozone-platform")),
                Some(&CefString::from("wayland")),
            );
            command_line.append_switch(Some(&CefString::from("no-first-run")));
            command_line.append_switch(Some(&CefString::from("no-default-browser-check")));
            command_line.append_switch_with_value(
                Some(&CefString::from("use-angle")),
                Some(&CefString::from("gl-egl")),
            );
        }

        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            let Some(registrar) = registrar else { return };
            let options = SchemeOptions::STANDARD.get_raw()
                | SchemeOptions::SECURE.get_raw()
                | SchemeOptions::CORS_ENABLED.get_raw()
                | SchemeOptions::FETCH_ENABLED.get_raw();
            registrar.add_custom_scheme(Some(&CefString::from("lince-sand")), options as i32);
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(DiagnosticRenderProcessHandler::new())
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(DiagnosticBrowserProcessHandler::new())
        }
    }
}

wrap_scheme_handler_factory! {
    struct InstalledSchemeFactory;

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = request.map(|request| {
                let value = request.url();
                CefString::from(&value).to_string()
            }).unwrap_or_default();
            if observed_origin(&url).as_deref() != Some(INSTALLED_ORIGIN) {
                return None;
            }
            let path = url::Url::parse(&url).ok()?.path().to_string();
            let (bytes, media_type) = installed_gallery_asset(&path)?;
            let stream = Arc::new(Mutex::new(ByteStream::new(bytes.to_vec())));
            let mut reader = ByteReadHandler::new(stream);
            let stream_reader = stream_reader_create_for_handler(Some(&mut reader))?;
            Some(StreamResourceHandler::new_with_stream(
                media_type.into(),
                stream_reader,
            ))
        }
    }
}

wrap_request_context_handler! {
    struct DiagnosticRequestContextHandler {
        ready: Arc<AtomicBool>,
    }

    impl RequestContextHandler {
        fn on_request_context_initialized(&self, _request_context: Option<&mut RequestContext>) {
            self.ready.store(true, Ordering::Release);
        }
    }
}

fn media_is_allowed(manifest: &HtmlSurfaceManifest, origin: &str, requested: u32) -> bool {
    if origin != manifest.origin() || requested == 0 {
        return false;
    }
    let grants = match manifest {
        HtmlSurfaceManifest::Website { media, .. } => media,
        HtmlSurfaceManifest::Installed { grants, .. } => &grants.media,
    };
    let camera = MediaAccessPermissionTypes::DEVICE_VIDEO_CAPTURE.get_raw();
    let microphone = MediaAccessPermissionTypes::DEVICE_AUDIO_CAPTURE.get_raw();
    let display = MediaAccessPermissionTypes::DESKTOP_VIDEO_CAPTURE.get_raw()
        | MediaAccessPermissionTypes::DESKTOP_AUDIO_CAPTURE.get_raw();
    let known = camera | microphone | display;
    requested & !known == 0
        && (requested & camera == 0 || grants.contains(&MediaGrant::Camera))
        && (requested & microphone == 0 || grants.contains(&MediaGrant::Microphone))
        && (requested & display == 0 || grants.contains(&MediaGrant::DisplayCapture))
}

fn observed_origin(value: &str) -> Option<String> {
    let parsed = url::Url::parse(value).ok()?;
    let host = parsed.host_str()?;
    let mut origin = format!("{}://{host}", parsed.scheme());
    if let Some(port) = parsed.port() {
        origin.push(':');
        origin.push_str(&port.to_string());
    }
    Some(origin)
}

fn initialize_cef() -> Result<Option<CefGuard>, String> {
    if api_hash(cef::sys::CEF_API_VERSION_LAST, 0).is_null()
        || api_version() != cef::sys::CEF_API_VERSION_LAST
    {
        return Err("CEF runtime rejected the compiled stable API version".into());
    }
    let args = cef::args::Args::new();
    let mut app = DiagnosticCefApp::new();
    let subprocess_code =
        execute_process(Some(args.as_main_args()), Some(&mut app), ptr::null_mut());
    if subprocess_code >= 0 {
        std::process::exit(subprocess_code);
    }
    let runtime = cef::sys::get_cef_dir()
        .ok_or_else(|| "CEF runtime directory is unavailable".to_string())?;
    bundle_cef_notices(&runtime)?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let root_cache = PathBuf::from("target/interface-laboratory/cef/root");
    fs::create_dir_all(&root_cache).map_err(|error| error.to_string())?;
    let root_cache = root_cache
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let settings = Settings {
        no_sandbox: 0,
        browser_subprocess_path: CefString::from(executable.to_string_lossy().as_ref()),
        external_message_pump: 0,
        windowless_rendering_enabled: 1,
        root_cache_path: CefString::from(root_cache.to_string_lossy().as_ref()),
        resources_dir_path: CefString::from(runtime.to_string_lossy().as_ref()),
        locales_dir_path: CefString::from(runtime.join("locales").to_string_lossy().as_ref()),
        log_file: CefString::from("target/interface-laboratory/cef/cef.log"),
        log_severity: LogSeverity::INFO,
        ..Settings::default()
    };
    eprintln!("CEF initializing from {}", runtime.display());
    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        ptr::null_mut(),
    ) == 0
    {
        return Err(
            "CEF initialization failed or another laboratory process owns its cache".into(),
        );
    }
    eprintln!("CEF initialized");
    let mut factory = InstalledSchemeFactory::new();
    if register_scheme_handler_factory(
        Some(&CefString::from("lince-sand")),
        Some(&CefString::from("installed")),
        Some(&mut factory),
    ) == 0
    {
        shutdown();
        return Err("CEF refused the installed-Sand scheme factory".into());
    }
    Ok(Some(CefGuard))
}

fn bundle_cef_notices(runtime: &std::path::Path) -> Result<(), String> {
    let destination = PathBuf::from("target/interface-laboratory/cef/notices");
    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    for name in ["LICENSE.txt", "CREDITS.html"] {
        let runtime_source = runtime.join(name);
        let packaged_source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("licenses/cef")
            .join(name);
        let source = if runtime_source.is_file() {
            runtime_source
        } else {
            packaged_source
        };
        if !source.is_file() {
            return Err(format!("CEF runtime omits required notice {name}"));
        }
        fs::copy(&source, destination.join(name)).map_err(|error| error.to_string())?;
    }
    CEF_NOTICES_BUNDLED.store(true, Ordering::Release);
    Ok(())
}

struct CefGuard;

impl Drop for CefGuard {
    fn drop(&mut self) {
        clear_scheme_handler_factories();
        shutdown();
    }
}

struct DiagnosticApplication {
    state: Option<DiagnosticWindow>,
    export_and_exit: bool,
    report_path: PathBuf,
    sample_seconds: u64,
    warmup_seconds: u64,
    cef_count: usize,
    recovery_probe: bool,
    accessibility_probe: bool,
    process_started: Instant,
    website_url: String,
    domain_url: Option<String>,
}

struct DiagnosticWindow {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: SurfaceConfiguration,
    surfaces: Vec<BrowserSurface>,
    cursor: InputPoint,
    modifiers: InputModifiers,
    sequence: u64,
    focused: usize,
    closing: bool,
    popup_probe_sent: bool,
    #[cfg(feature = "joined-runtime")]
    renderer_probe_sent: bool,
    started: Instant,
    next_redraw: Instant,
    frames: u64,
    export_and_exit: bool,
    last_export: Option<PathBuf>,
    report_path: PathBuf,
    sample_seconds: u64,
    warmup_seconds: u64,
    gpu_name: String,
    gpu_backend: String,
    gpu_driver: String,
    gpu_driver_info: String,
    present_mode: String,
    recovery_probe: bool,
    accessibility_probe: bool,
    startup_to_window_ready_millis: f64,
    first_cef_frame_millis: Option<f64>,
    #[cfg(feature = "joined-runtime")]
    instance: Instance,
    #[cfg(feature = "joined-runtime")]
    adapter: wgpu::Adapter,
    #[cfg(feature = "joined-runtime")]
    device_recovery_attempted: bool,
    #[cfg(feature = "joined-runtime")]
    device_recovery_completed: bool,
    #[cfg(feature = "joined-runtime")]
    device_recovery_millis: Option<f64>,
    #[cfg(feature = "joined-runtime")]
    bevy_host: BevyHost,
    #[cfg(feature = "joined-runtime")]
    bevy_layer: BevyLayer,
    #[cfg(feature = "joined-runtime")]
    node_layer: NodeLayer,
    #[cfg(feature = "joined-runtime")]
    joined_panel: JoinedPanel,
    #[cfg(feature = "joined-runtime")]
    retained_text_layer: RetainedTextLayer,
    #[cfg(feature = "joined-runtime")]
    field_solver: FieldSolver,
    #[cfg(feature = "joined-runtime")]
    semantic_instances: usize,
    #[cfg(feature = "joined-runtime")]
    composition_workbench: CompositionWorkbenchState,
    #[cfg(feature = "joined-runtime")]
    composition_workbench_active: bool,
    #[cfg(feature = "joined-runtime")]
    composition_package_sha256: String,
    #[cfg(feature = "joined-runtime")]
    configuration_workbench: ConfigurationWorkbenchState,
    #[cfg(feature = "joined-runtime")]
    configuration_workbench_active: bool,
    #[cfg(feature = "joined-runtime")]
    configuration_package_sha256: String,
    #[cfg(feature = "joined-runtime")]
    official_sand_gallery: OfficialSandGalleryState,
    #[cfg(feature = "joined-runtime")]
    official_sand_gallery_active: bool,
    #[cfg(feature = "joined-runtime")]
    official_sand_package_sha256: String,
    #[cfg(feature = "joined-runtime")]
    official_runtime: SharedOfficialRuntimeState,
    #[cfg(feature = "joined-runtime")]
    official_runtime_scene: Option<RetainedScene>,
    #[cfg(feature = "joined-runtime")]
    native_domain: NativeDomainClient,
    #[cfg(feature = "joined-runtime")]
    joined_cpu_frame_nanos: Vec<u64>,
    #[cfg(feature = "joined-runtime")]
    joined_frame_interval_nanos: Vec<u64>,
    #[cfg(feature = "joined-runtime")]
    joined_last_presented: Option<Instant>,
    #[cfg(feature = "joined-runtime")]
    joined_fixed_nanos: Vec<u64>,
    #[cfg(feature = "joined-runtime")]
    simulation_last: Instant,
    #[cfg(feature = "joined-runtime")]
    simulation_accumulator: Duration,
    #[cfg(feature = "joined-runtime")]
    simulation_ticks: u64,
    #[cfg(feature = "joined-runtime")]
    joined_input_to_present_call_nanos: Vec<u64>,
    #[cfg(feature = "joined-runtime")]
    pending_input_started: Option<Instant>,
    #[cfg(feature = "joined-runtime")]
    next_input_probe: Instant,
    #[cfg(feature = "joined-runtime")]
    last_panel_refresh: Instant,
    #[cfg(feature = "joined-runtime")]
    scenario_index: usize,
    #[cfg(feature = "joined-runtime")]
    primitive_gallery: PrimitiveGalleryState,
    #[cfg(feature = "joined-runtime")]
    native_gallery_focused: bool,
    #[cfg(feature = "joined-runtime")]
    sand_package_sha256: String,
    #[cfg(feature = "joined-runtime")]
    style_gallery: StyleGalleryState,
    #[cfg(feature = "joined-runtime")]
    resolved_style: ResolvedStyle,
    #[cfg(feature = "joined-runtime")]
    last_style_css: String,
    #[cfg(feature = "joined-runtime")]
    last_style_load_completions: u64,
    #[cfg(feature = "joined-runtime")]
    style_projection_updates: u64,
    #[cfg(feature = "joined-runtime")]
    token_probe_applied: bool,
    #[cfg(feature = "joined-runtime")]
    token_probe_load_before: Option<u64>,
    #[cfg(feature = "joined-runtime")]
    token_probe_load_after: Option<u64>,
    #[cfg(feature = "joined-runtime")]
    configuration_probe_applied: bool,
    #[cfg(feature = "joined-runtime")]
    configuration_probe_reverted: bool,
    #[cfg(feature = "joined-runtime")]
    configuration_probe_load_before: Option<u64>,
    #[cfg(feature = "joined-runtime")]
    configuration_probe_load_after: Option<u64>,
    #[cfg(feature = "joined-runtime")]
    joined_host_submissions: u64,
    #[cfg(feature = "joined-runtime")]
    off_camera_copy_start: Option<u64>,
    #[cfg(feature = "joined-runtime")]
    off_camera_bridge_start: Option<u64>,
    #[cfg(feature = "joined-runtime")]
    off_camera_copy_delta: Option<u64>,
    #[cfg(feature = "joined-runtime")]
    off_camera_bridge_delta: Option<u64>,
    #[cfg(feature = "joined-runtime")]
    accessibility: JoinedAccessibility,
}

struct BrowserSurface {
    browser: Browser,
    shared: SurfaceShared,
    rect: [u32; 4],
}

impl DiagnosticApplication {
    fn ensure_state(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() || !CEF_CONTEXT_READY.load(Ordering::Acquire) {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Lince CEF seam · starting")
            .with_inner_size(LogicalSize::new(1280.0, 760.0))
            .with_visible(!cfg!(feature = "joined-runtime"));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("CEF diagnostic window failed: {error}");
                event_loop.exit();
                return;
            }
        };
        window.set_ime_allowed(true);
        match pollster::block_on(DiagnosticWindow::new(
            window.clone(),
            event_loop,
            self.export_and_exit,
            self.report_path.clone(),
            self.sample_seconds,
            self.warmup_seconds,
            self.cef_count,
            self.recovery_probe,
            self.accessibility_probe,
            self.process_started,
            &self.website_url,
            self.domain_url.clone(),
        )) {
            Ok(state) => {
                window.set_visible(true);
                self.state = Some(state);
            }
            Err(error) => {
                eprintln!("CEF diagnostic unavailable: {error}");
                event_loop.exit();
            }
        }
    }
}

impl DiagnosticWindow {
    async fn new(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
        export_and_exit: bool,
        report_path: PathBuf,
        sample_seconds: u64,
        warmup_seconds: u64,
        cef_count: usize,
        recovery_probe: bool,
        accessibility_probe: bool,
        process_started: Instant,
        website_url: &str,
        domain_url: Option<String>,
    ) -> Result<Self, String> {
        #[cfg(feature = "joined-runtime")]
        let accessibility = JoinedAccessibility::new(event_loop, &window, cef_count);
        let size = nonzero_size(window.inner_size());
        let mut instance_descriptor = InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        ));
        instance_descriptor.backends = Backends::VULKAN;
        let instance = Instance::new(instance_descriptor);
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
        let capabilities = surface.get_capabilities(&adapter);
        let adapter_info = adapter.get_info();
        if !capabilities.usages.contains(TextureUsages::COPY_DST) {
            return Err("host swapchain does not support Vulkan transfer composition".into());
        }
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| "adapter exposes no surface format".to_string())?;
        let present_mode = if capabilities.present_modes.contains(&PresentMode::Mailbox) {
            PresentMode::Mailbox
        } else if capabilities.present_modes.contains(&PresentMode::Fifo) {
            PresentMode::Fifo
        } else {
            capabilities.present_modes[0]
        };
        let alpha_mode = if capabilities
            .alpha_modes
            .contains(&CompositeAlphaMode::Opaque)
        {
            CompositeAlphaMode::Opaque
        } else {
            capabilities.alpha_modes[0]
        };
        let (device, queue) = open_interop_device(&adapter)?;
        #[cfg(feature = "joined-runtime")]
        let mut bevy_host = BevyHost::new(
            instance.clone(),
            adapter.clone(),
            adapter_info.clone(),
            device.clone(),
            queue.clone(),
        )?;
        #[cfg(feature = "joined-runtime")]
        bevy_host.prepare_frame();
        #[cfg(feature = "joined-runtime")]
        let bevy_layer = BevyLayer::new(&device, format, bevy_host.output_view());
        #[cfg(feature = "joined-runtime")]
        let node_layer = NodeLayer::new(&device, format);
        #[cfg(feature = "joined-runtime")]
        let joined_panel = JoinedPanel::new(
            &device,
            &queue,
            format,
            size.width,
            size.height,
            window.scale_factor(),
        );
        #[cfg(feature = "joined-runtime")]
        let retained_text_layer = RetainedTextLayer::new(
            &device,
            &queue,
            format,
            size.width,
            size.height,
            window.scale_factor(),
        );
        #[cfg(feature = "joined-runtime")]
        let field_solver = FieldSolver::deterministic_fixture(ACTIVE_BODY_COUNT, 0x4c494e4345);
        #[cfg(feature = "joined-runtime")]
        let style_gallery = StyleGalleryState::new().map_err(|error| error.to_string())?;
        #[cfg(feature = "joined-runtime")]
        let resolved_style = style_gallery
            .resolved()
            .map_err(|error| error.to_string())?;
        #[cfg(feature = "joined-runtime")]
        let (primitive_gallery, sand_package_sha256) = {
            let package = primitive_gallery_package();
            package.validate().map_err(|error| error.to_string())?;
            let hash = package.graph_sha256().map_err(|error| error.to_string())?;
            let mut gallery = PrimitiveGalleryState::new();
            gallery.exercise();
            (gallery, hash)
        };
        #[cfg(feature = "joined-runtime")]
        let (composition_workbench, composition_package_sha256, semantic_instances) = {
            let package = composition_workbench_package();
            package.validate().map_err(|error| error.to_string())?;
            let hash = package.graph_sha256().map_err(|error| error.to_string())?;
            let mut workbench =
                CompositionWorkbenchState::new().map_err(|error| error.to_string())?;
            workbench.exercise().map_err(|error| error.to_string())?;
            let placements = workbench.facts().mounted_placements;
            (workbench, hash, placements)
        };
        #[cfg(feature = "joined-runtime")]
        let (configuration_workbench, configuration_package_sha256) = {
            let package = configuration_sand_package();
            package.validate().map_err(|error| error.to_string())?;
            let hash = package.graph_sha256().map_err(|error| error.to_string())?;
            let path = if export_and_exit {
                report_path.with_file_name("configuration.json")
            } else {
                default_configuration_path().map_err(|error| error.to_string())?
            };
            let mut workbench = if export_and_exit {
                ConfigurationWorkbenchState::fixture(path)
            } else {
                ConfigurationWorkbenchState::open(path)
            }
            .map_err(|error| error.to_string())?;
            if export_and_exit {
                workbench.exercise().map_err(|error| error.to_string())?;
            }
            (workbench, hash)
        };
        #[cfg(feature = "joined-runtime")]
        let (official_sand_gallery, official_sand_package_sha256) = {
            let package = official_sand_package();
            package.validate().map_err(|error| error.to_string())?;
            let hash = package.graph_sha256().map_err(|error| error.to_string())?;
            (
                OfficialSandGalleryState::new().map_err(|error| error.to_string())?,
                hash,
            )
        };
        #[cfg(feature = "joined-runtime")]
        let mut official_runtime = {
            let mut runtime = SharedOfficialRuntimeState::new();
            runtime.exercise()?;
            runtime
        };
        #[cfg(feature = "joined-runtime")]
        let native_domain = {
            let client = NativeDomainClient::connect(domain_url);
            if client.facts().endpoint.is_some() {
                official_runtime.set_domain_status(client.status_line());
            }
            client
        };
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_DST,
            format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let mut surfaces = Vec::with_capacity(cef_count);
        for index in 0..cef_count {
            let installed = index % 2 == 0;
            let shared = if installed {
                SurfaceShared::installed(device.clone(), index)?
            } else {
                SurfaceShared::website(device.clone(), index, website_url)?
            };
            surfaces.push(create_browser(shared, &device, installed)?);
        }
        let mut state = Self {
            window,
            device,
            queue,
            surface,
            config,
            surfaces,
            cursor: InputPoint::new(0.0, 0.0),
            modifiers: InputModifiers::default(),
            sequence: 0,
            focused: 0,
            closing: false,
            popup_probe_sent: false,
            #[cfg(feature = "joined-runtime")]
            renderer_probe_sent: false,
            started: Instant::now(),
            next_redraw: Instant::now(),
            frames: 0,
            export_and_exit,
            last_export: None,
            report_path,
            sample_seconds,
            warmup_seconds,
            gpu_name: adapter_info.name,
            gpu_backend: format!("{:?}", adapter_info.backend),
            gpu_driver: adapter_info.driver,
            gpu_driver_info: adapter_info.driver_info,
            present_mode: format!("{present_mode:?}"),
            recovery_probe,
            accessibility_probe,
            startup_to_window_ready_millis: process_started.elapsed().as_secs_f64() * 1_000.0,
            first_cef_frame_millis: None,
            #[cfg(feature = "joined-runtime")]
            instance: instance.clone(),
            #[cfg(feature = "joined-runtime")]
            adapter: adapter.clone(),
            #[cfg(feature = "joined-runtime")]
            device_recovery_attempted: false,
            #[cfg(feature = "joined-runtime")]
            device_recovery_completed: false,
            #[cfg(feature = "joined-runtime")]
            device_recovery_millis: None,
            #[cfg(feature = "joined-runtime")]
            bevy_host,
            #[cfg(feature = "joined-runtime")]
            bevy_layer,
            #[cfg(feature = "joined-runtime")]
            node_layer,
            #[cfg(feature = "joined-runtime")]
            joined_panel,
            #[cfg(feature = "joined-runtime")]
            retained_text_layer,
            #[cfg(feature = "joined-runtime")]
            field_solver,
            #[cfg(feature = "joined-runtime")]
            semantic_instances,
            #[cfg(feature = "joined-runtime")]
            composition_workbench,
            #[cfg(feature = "joined-runtime")]
            composition_workbench_active: false,
            #[cfg(feature = "joined-runtime")]
            composition_package_sha256,
            #[cfg(feature = "joined-runtime")]
            configuration_workbench,
            #[cfg(feature = "joined-runtime")]
            configuration_workbench_active: false,
            #[cfg(feature = "joined-runtime")]
            configuration_package_sha256,
            #[cfg(feature = "joined-runtime")]
            official_sand_gallery,
            #[cfg(feature = "joined-runtime")]
            official_sand_gallery_active: false,
            #[cfg(feature = "joined-runtime")]
            official_sand_package_sha256,
            #[cfg(feature = "joined-runtime")]
            official_runtime,
            #[cfg(feature = "joined-runtime")]
            official_runtime_scene: None,
            #[cfg(feature = "joined-runtime")]
            native_domain,
            #[cfg(feature = "joined-runtime")]
            joined_cpu_frame_nanos: Vec::new(),
            #[cfg(feature = "joined-runtime")]
            joined_frame_interval_nanos: Vec::new(),
            #[cfg(feature = "joined-runtime")]
            joined_last_presented: None,
            #[cfg(feature = "joined-runtime")]
            joined_fixed_nanos: Vec::new(),
            #[cfg(feature = "joined-runtime")]
            simulation_last: Instant::now(),
            #[cfg(feature = "joined-runtime")]
            simulation_accumulator: Duration::ZERO,
            #[cfg(feature = "joined-runtime")]
            simulation_ticks: 0,
            #[cfg(feature = "joined-runtime")]
            joined_input_to_present_call_nanos: Vec::new(),
            #[cfg(feature = "joined-runtime")]
            pending_input_started: None,
            #[cfg(feature = "joined-runtime")]
            next_input_probe: Instant::now(),
            #[cfg(feature = "joined-runtime")]
            last_panel_refresh: Instant::now() - Duration::from_secs(1),
            #[cfg(feature = "joined-runtime")]
            scenario_index: 0,
            #[cfg(feature = "joined-runtime")]
            primitive_gallery,
            #[cfg(feature = "joined-runtime")]
            native_gallery_focused: false,
            #[cfg(feature = "joined-runtime")]
            sand_package_sha256,
            #[cfg(feature = "joined-runtime")]
            style_gallery,
            #[cfg(feature = "joined-runtime")]
            resolved_style,
            #[cfg(feature = "joined-runtime")]
            last_style_css: String::new(),
            #[cfg(feature = "joined-runtime")]
            last_style_load_completions: 0,
            #[cfg(feature = "joined-runtime")]
            style_projection_updates: 0,
            #[cfg(feature = "joined-runtime")]
            token_probe_applied: false,
            #[cfg(feature = "joined-runtime")]
            token_probe_load_before: None,
            #[cfg(feature = "joined-runtime")]
            token_probe_load_after: None,
            #[cfg(feature = "joined-runtime")]
            configuration_probe_applied: false,
            #[cfg(feature = "joined-runtime")]
            configuration_probe_reverted: false,
            #[cfg(feature = "joined-runtime")]
            configuration_probe_load_before: None,
            #[cfg(feature = "joined-runtime")]
            configuration_probe_load_after: None,
            #[cfg(feature = "joined-runtime")]
            joined_host_submissions: 0,
            #[cfg(feature = "joined-runtime")]
            off_camera_copy_start: None,
            #[cfg(feature = "joined-runtime")]
            off_camera_bridge_start: None,
            #[cfg(feature = "joined-runtime")]
            off_camera_copy_delta: None,
            #[cfg(feature = "joined-runtime")]
            off_camera_bridge_delta: None,
            #[cfg(feature = "joined-runtime")]
            accessibility,
        };
        state.update_layout();
        state
            .surfaces
            .first()
            .and_then(|surface| surface.browser.host())
            .map(|host| host.set_focus(1));
        state.update_title();
        Ok(state)
    }

    fn update_layout(&mut self) {
        let rects = browser_rects(
            self.config.width,
            self.config.height,
            self.surfaces.len(),
            cfg!(feature = "joined-runtime"),
        );
        for (surface, rect) in self.surfaces.iter_mut().zip(rects) {
            surface.rect = rect;
        }
        #[cfg(feature = "joined-runtime")]
        self.publish_accessibility();
        let scale = self.window.scale_factor();
        for surface in &self.surfaces {
            if let Ok(mut value) = surface.shared.scale.lock() {
                *value = scale;
            }
            if let Ok(mut value) = surface.shared.size.lock() {
                *value = [
                    (f64::from(surface.rect[2]) / scale).round().max(1.0) as i32,
                    (f64::from(surface.rect[3]) / scale).round().max(1.0) as i32,
                ];
            }
            if let Some(host) = surface.browser.host() {
                host.notify_screen_info_changed();
                host.was_resized();
            }
        }
    }

    #[cfg(feature = "joined-runtime")]
    fn publish_accessibility(&mut self) {
        let installed = self
            .installed()
            .map(|surface| surface.rect)
            .unwrap_or([0, 0, 0, 0]);
        let website = self
            .website()
            .map(|surface| surface.rect)
            .unwrap_or([0, 0, 0, 0]);
        self.accessibility.publish(
            [self.config.width, self.config.height],
            installed,
            website,
            self.primitive_gallery.focused_index(),
            self.configuration_workbench_active,
            self.configuration_workbench.selected_index(),
            self.official_sand_gallery_active,
            self.official_sand_gallery.selected_index(),
            self.official_runtime_scene.as_ref(),
        );
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        let size = nonzero_size(size);
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        #[cfg(feature = "joined-runtime")]
        self.joined_panel.resize(
            &self.queue,
            size.width,
            size.height,
            self.window.scale_factor(),
        );
        #[cfg(feature = "joined-runtime")]
        self.retained_text_layer.resize(
            &self.queue,
            size.width,
            size.height,
            self.window.scale_factor(),
        );
        #[cfg(feature = "joined-runtime")]
        if self.official_runtime.active().is_some() {
            self.official_runtime_scene = self
                .official_runtime
                .scene(self.official_runtime_viewport())
                .ok();
        }
        self.update_layout();
    }

    fn render(&mut self) -> Result<(), String> {
        #[cfg(feature = "joined-runtime")]
        let joined_frame_started = Instant::now();
        #[cfg(feature = "joined-runtime")]
        let joined_sampling = self.started.elapsed() >= Duration::from_secs(self.warmup_seconds);
        #[cfg(feature = "joined-runtime")]
        {
            if self.native_domain.poll() {
                let facts = self.native_domain.facts();
                if facts.snapshots > 0 || facts.updates > 0 {
                    self.official_runtime.bind_domain_records(
                        self.native_domain.records(),
                        self.native_domain.status_line(),
                    );
                    self.official_runtime
                        .bind_domain_conversations(self.native_domain.conversations());
                    self.official_runtime
                        .bind_domain_message_drafts(self.native_domain.message_drafts());
                } else if facts.endpoint.is_some() {
                    self.official_runtime
                        .set_domain_status(self.native_domain.status_line());
                }
                self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
            }
            for receipt in self.native_domain.take_action_receipts() {
                self.official_runtime.apply_action_receipt(receipt);
                self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
            }
            for intent in self.official_runtime.take_intents() {
                let result = match &intent {
                    OfficialRuntimeIntent::SendMessage { thread, body, .. } => {
                        self.native_domain.request_message(thread, body)
                    }
                    OfficialRuntimeIntent::CreateDraft {
                        conversation,
                        thread,
                        body,
                        pinned,
                        timing,
                        position,
                        ..
                    } => self.native_domain.request_create_draft(
                        conversation,
                        thread,
                        body,
                        *pinned,
                        (*timing).into(),
                        *position,
                    ),
                    OfficialRuntimeIntent::ReviseDraft {
                        draft,
                        body,
                        pinned,
                        timing,
                        position,
                        ..
                    } => self.native_domain.request_revise_draft(
                        draft,
                        body,
                        *pinned,
                        (*timing).into(),
                        *position,
                    ),
                    OfficialRuntimeIntent::DeleteDraft { draft } => {
                        self.native_domain.request_delete_draft(draft)
                    }
                    OfficialRuntimeIntent::SendDraft { draft, .. } => {
                        self.native_domain.request_send_draft(draft)
                    }
                    OfficialRuntimeIntent::CreateRecord {
                        title,
                        description,
                        quantity,
                        ..
                    } => self
                        .native_domain
                        .request_create_record(title, description, *quantity),
                    OfficialRuntimeIntent::SetRecordQuantity { record, value, .. } => self
                        .native_domain
                        .request_set_record_quantity(record, *value),
                };
                match result {
                    Ok(id) => self.official_runtime.domain_action_submitted(id, intent),
                    Err(error) => self.official_runtime.domain_action_refused(&intent, error),
                }
                self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
            }
            if let Some(index) = self.accessibility.take_requested_primitive() {
                self.focus_native_gallery();
                self.primitive_gallery.focus_at(
                    index,
                    lince_interface::primitive_gallery::GalleryVisualState::FocusVisible,
                );
                self.primitive_gallery.activate();
                self.publish_accessibility();
            }
            if self.accessibility.take_requested_composition() {
                self.composition_workbench_active = true;
                self.configuration_workbench_active = false;
                self.official_sand_gallery_active = false;
                let _ = self.composition_workbench.activate();
                self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
            }
            if let Some(index) = self.accessibility.take_requested_configuration() {
                self.configuration_workbench_active = true;
                self.composition_workbench_active = false;
                self.official_sand_gallery_active = false;
                self.configuration_workbench.focus_at(index);
                if self.configuration_workbench.activate().is_ok()
                    && let Ok(style) = self.configuration_workbench.resolved_style()
                {
                    self.resolved_style = style;
                }
                self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                self.publish_accessibility();
            }
            if let Some(index) = self.accessibility.take_requested_official() {
                self.official_runtime.close();
                self.official_sand_gallery_active = true;
                self.composition_workbench_active = false;
                self.configuration_workbench_active = false;
                self.native_gallery_focused = false;
                self.official_sand_gallery.focus_at(index);
                self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                self.publish_accessibility();
            }
            if let Some(index) = self.accessibility.take_requested_official_runtime()
                && let Some(scene) = self.official_runtime_scene.clone()
            {
                self.official_runtime.focus_at(index, &scene);
                if let Ok(focused_scene) = self
                    .official_runtime
                    .scene(self.official_runtime_viewport())
                {
                    let _ = self.official_runtime.activate(&focused_scene);
                    self.official_runtime_scene = Some(focused_scene);
                }
                self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                self.publish_accessibility();
            }
            let simulation_now = Instant::now();
            self.simulation_accumulator += simulation_now.duration_since(self.simulation_last);
            self.simulation_last = simulation_now;
            let mut fixed_steps = 0;
            while self.simulation_accumulator >= FIXED_STEP
                && fixed_steps < MAX_FIXED_STEPS_PER_FRAME
            {
                let fixed_started = Instant::now();
                self.field_solver.step(1.0 / FIXED_STEP_HZ as f32);
                self.simulation_accumulator -= FIXED_STEP;
                self.simulation_ticks = self.simulation_ticks.saturating_add(1);
                fixed_steps += 1;
                if joined_sampling {
                    self.joined_fixed_nanos.push(elapsed_nanos(fixed_started));
                }
            }
            self.bevy_host.prepare_frame();
            let positions = self
                .field_solver
                .positions()
                .iter()
                .take(VISIBLE_NATIVE_SAND_COUNT)
                .map(|position| [position.x, position.y])
                .collect::<Vec<_>>();
            self.official_runtime_scene = if self.official_runtime.active().is_some() {
                Some(
                    self.official_runtime
                        .scene(self.official_runtime_viewport())?,
                )
            } else {
                None
            };
            if let Some(scene) = self.official_runtime_scene.as_ref() {
                self.node_layer
                    .update_retained(
                        &self.queue,
                        scene,
                        [self.config.width, self.config.height],
                        &self.resolved_style,
                    )
                    .map_err(|error| error.to_string())?;
            } else {
                self.node_layer
                    .update(
                        &self.queue,
                        &positions,
                        &self.resolved_style,
                        self.primitive_gallery.focused_index(),
                        self.primitive_gallery.state(),
                        self.native_gallery_focused,
                    )
                    .map_err(|error| error.to_string())?;
            }
            self.drive_external_begin_frames();
            self.drive_input_probe();
            self.drive_token_probe();
            self.drive_configuration_probe();
            self.drive_style_projection();
            let refresh_panel = self.last_panel_refresh.elapsed() >= Duration::from_millis(200);
            let panel_text = refresh_panel.then(|| self.panel_text());
            self.joined_panel.prepare(
                &self.device,
                &self.queue,
                panel_text.as_deref(),
                &self.resolved_style,
            )?;
            self.retained_text_layer.prepare(
                &self.device,
                &self.queue,
                self.official_runtime_scene.as_ref(),
                &self.resolved_style,
            )?;
            if refresh_panel {
                self.last_panel_refresh = Instant::now();
            }
        }
        #[cfg(not(feature = "joined-runtime"))]
        for surface in &self.surfaces {
            if let Some(host) = surface.browser.host() {
                host.send_external_begin_frame();
            }
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return Err("host presentation surface was lost".into());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("host presentation surface validation failed".into());
            }
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut clear = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("lince-cef-background"),
            });
        #[cfg(feature = "joined-runtime")]
        let background_color = {
            let color = self
                .resolved_style
                .color_linear("--lynx-surface-canvas")
                .map_err(|error| error.to_string())?;
            wgpu::Color {
                r: f64::from(color[0]),
                g: f64::from(color[1]),
                b: f64::from(color[2]),
                a: f64::from(color[3]),
            }
        };
        #[cfg(not(feature = "joined-runtime"))]
        let background_color = wgpu::Color {
            r: 0.025,
            g: 0.035,
            b: 0.03,
            a: 1.0,
        };
        {
            let mut pass = clear.begin_render_pass(&RenderPassDescriptor {
                label: Some("lince-cef-background-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(background_color),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            #[cfg(feature = "joined-runtime")]
            self.bevy_layer.render(&mut pass);
            #[cfg(not(feature = "joined-runtime"))]
            let _ = &mut pass;
        }
        #[cfg(feature = "joined-runtime")]
        {
            let mut assembly = FrameAssembly::new();
            for command in self.bevy_host.take_output_commands() {
                assembly.contribute("bevy-world", command);
            }
            let submission = assembly.seal("lince-world-compositor", clear.finish());
            self.queue.submit(submission.into_commands());
            self.joined_host_submissions += 1;
        }
        #[cfg(not(feature = "joined-runtime"))]
        self.queue.submit([clear.finish()]);
        #[cfg(feature = "joined-runtime")]
        let native_surface_covers_cef = self.official_runtime.active().is_some();
        #[cfg(not(feature = "joined-runtime"))]
        let native_surface_covers_cef = false;
        for surface in &mut self.surfaces {
            if native_surface_covers_cef {
                continue;
            }
            let result = surface
                .shared
                .gpu
                .lock()
                .map_err(|_| "CEF GPU target lock failed".to_string())
                .and_then(|mut gpu| {
                    gpu.compose_into(
                        &frame.texture,
                        [self.config.width, self.config.height],
                        surface.rect,
                    )
                    .map_err(|error| error.to_string())
                });
            if let Err(error) = result {
                if !error.contains("has not supplied") {
                    surface
                        .shared
                        .set_status(format!("REFUSED composition: {error}"));
                }
            }
        }
        let mut overlay = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("lince-cef-overlay"),
            });
        {
            let mut pass = overlay.begin_render_pass(&RenderPassDescriptor {
                label: Some("lince-cef-overlay-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            #[cfg(feature = "joined-runtime")]
            self.node_layer.render(&mut pass);
            #[cfg(feature = "joined-runtime")]
            self.joined_panel.render(&mut pass)?;
            #[cfg(feature = "joined-runtime")]
            self.retained_text_layer.render(&mut pass)?;
            #[cfg(not(feature = "joined-runtime"))]
            let _ = &mut pass;
        }
        self.queue.submit([overlay.finish()]);
        #[cfg(feature = "joined-runtime")]
        {
            self.joined_host_submissions += 1;
        }
        frame.present();
        #[cfg(feature = "joined-runtime")]
        {
            let presented = Instant::now();
            if let Some(started) = self.pending_input_started.take()
                && joined_sampling
            {
                self.joined_input_to_present_call_nanos
                    .push(elapsed_nanos(started));
            }
            if joined_sampling {
                self.joined_cpu_frame_nanos
                    .push(elapsed_nanos(joined_frame_started));
                if let Some(previous) = self.joined_last_presented {
                    self.joined_frame_interval_nanos
                        .push(elapsed_nanos(previous));
                }
            }
            self.joined_last_presented = Some(presented);
            self.joined_panel.trim();
            self.retained_text_layer.trim();
            if self.first_cef_frame_millis.is_none() && self.surfaces_have_results() {
                self.first_cef_frame_millis = Some(
                    self.startup_to_window_ready_millis
                        + self.started.elapsed().as_secs_f64() * 1_000.0,
                );
            }
        }
        self.frames += 1;
        self.drive_popup_probe();
        #[cfg(feature = "joined-runtime")]
        self.drive_renderer_recovery_probe();
        #[cfg(feature = "joined-runtime")]
        self.drive_device_recovery_probe()?;
        self.update_title();
        let seam_ready = self.surfaces_have_results()
            && self.installed().is_none_or(|surface| {
                surface.shared.bridge_allowed.load(Ordering::Relaxed) > 0
                    && surface.shared.bridge_refused.load(Ordering::Relaxed) > 0
                    && surface.shared.popup_refusals.load(Ordering::Relaxed) > 0
                    && surface.shared.media_refused.load(Ordering::Relaxed) > 0
                    && surface
                        .shared
                        .external_network_requests
                        .load(Ordering::Relaxed)
                        > 0
            });
        let evidence_deadline = self.started.elapsed()
            >= Duration::from_secs(self.warmup_seconds + self.sample_seconds);
        let evidence_ready = if cfg!(feature = "joined-runtime") {
            evidence_deadline
        } else {
            seam_ready || evidence_deadline
        };
        if self.export_and_exit && !self.closing && self.frames > 20 && evidence_ready {
            self.begin_close();
        }
        Ok(())
    }

    #[cfg(feature = "joined-runtime")]
    fn drive_external_begin_frames(&mut self) {
        let elapsed = self.started.elapsed();
        let off_camera = elapsed >= Duration::from_secs(2) && elapsed < Duration::from_secs(4);
        if elapsed >= Duration::from_secs(2) && self.off_camera_copy_start.is_none() {
            self.off_camera_copy_start = self
                .installed()
                .map(|surface| copy_facts(&surface.shared).copied_frames);
            self.off_camera_bridge_start = self
                .installed()
                .map(|surface| surface.shared.bridge_allowed.load(Ordering::Relaxed));
        }
        if elapsed >= Duration::from_secs(4) && self.off_camera_copy_delta.is_none() {
            self.off_camera_copy_delta = self.off_camera_copy_start.and_then(|start| {
                self.installed().map(|surface| {
                    copy_facts(&surface.shared)
                        .copied_frames
                        .saturating_sub(start)
                })
            });
            self.off_camera_bridge_delta = self.off_camera_bridge_start.and_then(|start| {
                self.installed().map(|surface| {
                    surface
                        .shared
                        .bridge_allowed
                        .load(Ordering::Relaxed)
                        .saturating_sub(start)
                })
            });
        }
        for surface in &self.surfaces {
            if (!surface.shared.is_installed() || !off_camera)
                && let Some(host) = surface.browser.host()
            {
                host.send_external_begin_frame();
            }
        }
    }

    #[cfg(feature = "joined-runtime")]
    fn drive_input_probe(&mut self) {
        let now = Instant::now();
        let elapsed = self.started.elapsed();
        let sampling_started = elapsed >= Duration::from_secs(self.warmup_seconds);
        let sampling_finished =
            elapsed >= Duration::from_secs(self.warmup_seconds.saturating_add(self.sample_seconds));
        if !sampling_started
            || sampling_finished
            || now < self.next_input_probe
            || self.pending_input_started.is_some()
            || self.surfaces.is_empty()
        {
            return;
        }
        let surface = &self.surfaces[0];
        let phase = self.sequence % 17;
        self.cursor = InputPoint::new(
            f64::from(surface.rect[0] + surface.rect[2] / 3) + phase as f64,
            f64::from(surface.rect[1] + surface.rect[3] / 2),
        );
        self.pending_input_started = Some(now);
        self.dispatch(
            NormalizedInput::PointerMoved {
                surface_physical: self.cursor,
                buttons: Vec::new(),
                modifiers: self.modifiers,
            },
            0,
        );
        self.next_input_probe = now + Duration::from_millis(100);
    }

    #[cfg(feature = "joined-runtime")]
    fn drive_token_probe(&mut self) {
        let elapsed = self.started.elapsed();
        if !self.token_probe_applied && elapsed >= Duration::from_millis(3_000) {
            self.token_probe_load_before = self
                .installed()
                .map(|surface| surface.shared.load_completions.load(Ordering::Relaxed));
            self.style_gallery.cycle_palette();
            self.style_gallery.cycle_density();
            self.style_gallery.cycle_radius();
            if self.refresh_style().is_ok() {
                self.token_probe_applied = true;
                self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
            }
        }
        if self.token_probe_applied
            && self.token_probe_load_after.is_none()
            && elapsed >= Duration::from_millis(4_000)
        {
            self.token_probe_load_after = self
                .installed()
                .map(|surface| surface.shared.load_completions.load(Ordering::Relaxed));
        }
    }

    #[cfg(feature = "joined-runtime")]
    fn drive_configuration_probe(&mut self) {
        if !self.export_and_exit {
            return;
        }
        let elapsed = self.started.elapsed();
        if !self.configuration_probe_applied && elapsed >= Duration::from_millis(4_250) {
            self.configuration_probe_load_before = self
                .installed()
                .map(|surface| surface.shared.load_completions.load(Ordering::Relaxed));
            if let Ok(style) = self.configuration_workbench.resolved_style() {
                self.resolved_style = style;
                self.configuration_workbench_active = true;
                self.composition_workbench_active = false;
                self.official_sand_gallery_active = false;
                self.configuration_probe_applied = true;
                self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                self.publish_accessibility();
            }
        }
        if self.configuration_probe_applied
            && !self.configuration_probe_reverted
            && elapsed >= Duration::from_millis(4_750)
        {
            self.configuration_probe_load_after = self
                .installed()
                .map(|surface| surface.shared.load_completions.load(Ordering::Relaxed));
            self.configuration_workbench_active = false;
            let _ = self.refresh_style();
            self.configuration_probe_reverted = true;
            self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
            self.publish_accessibility();
        }
    }

    #[cfg(feature = "joined-runtime")]
    fn refresh_style(&mut self) -> Result<(), String> {
        let next = self
            .style_gallery
            .resolved()
            .map_err(|error| error.to_string())?;
        self.resolved_style = next;
        Ok(())
    }

    #[cfg(feature = "joined-runtime")]
    fn drive_style_projection(&mut self) {
        let Some(installed) = self.installed() else {
            return;
        };
        let load_completions = installed.shared.load_completions.load(Ordering::Acquire);
        if load_completions == 0 {
            return;
        }
        let css = if self.configuration_workbench_active {
            self.configuration_workbench
                .css_declarations()
                .unwrap_or_else(|_| self.resolved_style.css_declarations())
        } else {
            self.resolved_style.css_declarations()
        };
        if css == self.last_style_css && load_completions == self.last_style_load_completions {
            return;
        }
        let Some(frame) = installed.browser.main_frame() else {
            return;
        };
        let quoted = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".into());
        let code = format!("window.__linceApplyStyle?.({STYLE_CONTRACT_VERSION},{quoted})");
        let url_value = frame.url();
        let url = CefString::from(&url_value);
        frame.execute_java_script(Some(&CefString::from(code.as_str())), Some(&url), 0);
        self.last_style_css = css;
        self.last_style_load_completions = load_completions;
        self.style_projection_updates = self.style_projection_updates.saturating_add(1);
    }

    #[cfg(feature = "joined-runtime")]
    fn panel_text(&self) -> String {
        if let Some(scene) = self.official_runtime_scene.as_ref() {
            return self.official_runtime.panel_text(scene);
        }
        if self.official_sand_gallery_active {
            return self.official_sand_panel_text();
        }
        if self.configuration_workbench_active {
            return self.configuration_panel_text();
        }
        if self.composition_workbench_active {
            return self.composition_panel_text();
        }
        let frame_p95 = percentile_millis(&self.joined_frame_interval_nanos, 0.95);
        let fixed_p95 = percentile_millis(&self.joined_fixed_nanos, 0.95);
        let input_p95 = percentile_millis(&self.joined_input_to_present_call_nanos, 0.95);
        let accent_origin = self
            .resolved_style
            .origin("--lynx-accent")
            .map(|origin| origin.label.as_str())
            .unwrap_or("unavailable");
        let density_origin = self
            .resolved_style
            .origin("--lynx-density-scale")
            .map(|origin| origin.label.as_str())
            .unwrap_or("unavailable");
        let radius_origin = self
            .resolved_style
            .origin("--lynx-radius-control")
            .map(|origin| origin.label.as_str())
            .unwrap_or("unavailable");
        let primitives = self
            .primitive_gallery
            .definitions()
            .chunks(2)
            .map(|pair| {
                pair.iter()
                    .map(|primitive| {
                        let marker = if primitive.uid == self.primitive_gallery.focused().uid {
                            "▸"
                        } else {
                            " "
                        };
                        format!("{marker} {:<20}", primitive.label)
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "LINCE · PRIMITIVE SAND GALLERY\n\n{} native focus · F5 switch native/HTML · Tab/Shift-Tab focus · Enter/Space activate · F9 state · F8 report\n\n{}\n\n{}\n\nSand schema v1 · ABI v1 · {} definitions\npackage {}\nStyle v{} · {} / {}\naccent {} · {} · density {} · {} · radius {} · {}\n{} resolved tokens · {} HTML updates\n\nScenario {}/8 · {}\nLIVE · frame {:>5.2} ms · fixed {:>5.2} ms · input {:>5.2} ms\n{} bodies · {} resident · {} visible · {} HTML",
            if self.native_gallery_focused {
                "ACTIVE"
            } else {
                "inactive"
            },
            primitives,
            self.primitive_gallery.status_line(),
            PRIMITIVE_COUNT,
            self.sand_package_sha256,
            STYLE_CONTRACT_VERSION,
            self.style_gallery.theme(),
            self.style_gallery.mode(),
            self.style_gallery.palette_index() + 1,
            accent_origin,
            self.style_gallery.density_index() + 1,
            density_origin,
            self.style_gallery.radius_index() + 1,
            radius_origin,
            self.resolved_style.values.len(),
            self.style_projection_updates,
            self.scenario_index + 1,
            SCENARIOS[self.scenario_index],
            frame_p95,
            fixed_p95,
            input_p95,
            self.field_solver.positions().len(),
            RESIDENT_NODE_COUNT,
            self.node_layer.count(),
            self.surfaces.len(),
        )
    }

    #[cfg(feature = "joined-runtime")]
    fn official_sand_panel_text(&self) -> String {
        let selected = self.official_sand_gallery.selected();
        let sands = OFFICIAL_SANDS
            .chunks(2)
            .map(|pair| {
                pair.iter()
                    .map(|spec| {
                        let marker = if spec.uid == selected.uid { "▸" } else { " " };
                        format!("{marker} {:<20}", spec.display_name)
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let tree = self
            .official_sand_gallery
            .tree_lines()
            .into_iter()
            .take(24)
            .collect::<Vec<_>>()
            .join("\n");
        let boundary = self.official_sand_gallery.boundary_lines().join("\n");
        format!(
            "LINCE · OFFICIAL SANDS · C4 MIGRATION\n\nACTIVE native catalog · F12 Gallery/catalog · Tab/Shift-Tab Sand · Enter/Space operate available runtime · pointer or AccessKit select · F8 report\nStructure readiness is not Behavior completion.\n\n{sands}\n\nSELECTED {}/{} · {}\n{}\nlegacy {}\n{}\n\nRUST DEFINITION TREE\n{tree}\n\nPUBLIC TYPED BOUNDARY\n{boundary}\n\n{} total Rust definitions\npackage {}",
            self.official_sand_gallery.selected_index() + 1,
            OFFICIAL_SAND_COUNT,
            selected.display_name,
            selected.state.label(),
            selected.legacy_source,
            selected.purpose,
            self.official_sand_gallery.package().graph.definitions.len(),
            self.official_sand_package_sha256,
        )
    }

    #[cfg(feature = "joined-runtime")]
    fn configuration_panel_text(&self) -> String {
        let operations = ConfigurationOperation::ALL
            .iter()
            .enumerate()
            .map(|(index, operation)| {
                let marker = if index == self.configuration_workbench.selected_index() {
                    "▸"
                } else {
                    " "
                };
                format!("{marker} {}", operation.label())
            })
            .collect::<Vec<_>>()
            .join("\n");
        let origins = self.configuration_workbench.origin_lines().join("\n");
        let artifact = self.configuration_workbench.artifact();
        format!(
            "LINCE · CONFIGURATION SAND\n\nACTIVE native Sand · F11 Gallery/Configuration · Tab/Shift-Tab operation · Enter apply · F8 report\nEvery accepted edit previews live and persists. REFUSED edits leave the last good state active.\n\nOPERATIONS\n{operations}\n\nRESOLVED VALUE · ORIGIN\n{origins}\n\n{}\n\nConfiguration schema v{} · {} definitions · {} launch receipt\npackage {}",
            self.configuration_workbench.status_line(),
            CONFIGURATION_SCHEMA_VERSION,
            artifact.composition.catalog.active.len(),
            artifact.configuration.launches.len(),
            self.configuration_package_sha256,
        )
    }

    #[cfg(feature = "joined-runtime")]
    fn composition_panel_text(&self) -> String {
        let operations = WorkbenchOperation::ALL
            .iter()
            .enumerate()
            .map(|(index, operation)| {
                let marker = if index == self.composition_workbench.selected_index() {
                    "▸"
                } else {
                    " "
                };
                format!("{marker} {}", operation.label())
            })
            .collect::<Vec<_>>()
            .join("\n");
        let tree = self.composition_workbench.tree_lines().join("\n");
        let arrows = self.composition_workbench.arrow_lines().join("\n");
        let button = self
            .composition_workbench
            .host()
            .catalog
            .active_ref("button")
            .map(|reference| reference.revision)
            .unwrap_or_default();
        let room = self
            .composition_workbench
            .host()
            .catalog
            .active_ref("video-call-room")
            .map(|reference| reference.revision)
            .unwrap_or_default();
        format!(
            "LINCE · RECURSIVE SAND COMPOSITION\n\nACTIVE native workbench · F10 Gallery/workbench · Tab/Shift-Tab operation · Enter apply · F8 report\n\nOPERATIONS\n{operations}\n\nTREE · stable local identities\n{tree}\n\nTYPED ROUTES\n{arrows}\n\n{}\n\nComposition schema v{} · Button r{} · room r{}\npackage {}",
            self.composition_workbench.status_line(),
            COMPOSITION_SCHEMA_VERSION,
            button,
            room,
            self.composition_package_sha256,
        )
    }

    fn drive_popup_probe(&mut self) {
        let Some(installed) = self.installed() else {
            return;
        };
        if self.popup_probe_sent
            || self.started.elapsed() < Duration::from_millis(800)
            || installed.shared.load_completions.load(Ordering::Acquire) == 0
        {
            return;
        }
        let Some(host) = installed.browser.host() else {
            return;
        };
        let event = MouseEvent {
            x: 24,
            y: 24,
            modifiers: 0,
        };
        host.send_mouse_move_event(Some(&event), 0);
        host.send_mouse_click_event(Some(&event), MouseButtonType::LEFT, 0, 1);
        host.send_mouse_click_event(Some(&event), MouseButtonType::LEFT, 1, 1);
        let context_event = MouseEvent {
            x: 120,
            y: 120,
            modifiers: 0,
        };
        host.send_mouse_click_event(Some(&context_event), MouseButtonType::RIGHT, 0, 1);
        host.send_mouse_click_event(Some(&context_event), MouseButtonType::RIGHT, 1, 1);
        self.popup_probe_sent = true;
    }

    #[cfg(feature = "joined-runtime")]
    fn drive_renderer_recovery_probe(&mut self) {
        if !self.recovery_probe
            || self.renderer_probe_sent
            || self.started.elapsed() < Duration::from_millis(4_500)
        {
            return;
        }
        let Some(website) = self.website() else {
            return;
        };
        let Some(frame) = website.browser.main_frame() else {
            return;
        };
        frame.load_url(Some(&CefString::from("chrome://crash")));
        self.renderer_probe_sent = true;
    }

    #[cfg(feature = "joined-runtime")]
    fn drive_device_recovery_probe(&mut self) -> Result<(), String> {
        if !self.recovery_probe
            || self.device_recovery_attempted
            || self.started.elapsed() < Duration::from_secs(7)
        {
            return Ok(());
        }
        self.device_recovery_attempted = true;
        let started = Instant::now();
        let (new_device, new_queue) = open_interop_device(&self.adapter)?;
        let mut new_bevy_host = BevyHost::new(
            self.instance.clone(),
            self.adapter.clone(),
            self.adapter.get_info(),
            new_device.clone(),
            new_queue.clone(),
        )?;
        new_bevy_host.prepare_frame();
        let new_bevy_layer =
            BevyLayer::new(&new_device, self.config.format, new_bevy_host.output_view());
        let new_node_layer = NodeLayer::new(&new_device, self.config.format);
        let new_joined_panel = JoinedPanel::new(
            &new_device,
            &new_queue,
            self.config.format,
            self.config.width,
            self.config.height,
            self.window.scale_factor(),
        );
        let new_retained_text_layer = RetainedTextLayer::new(
            &new_device,
            &new_queue,
            self.config.format,
            self.config.width,
            self.config.height,
            self.window.scale_factor(),
        );
        for surface in &self.surfaces {
            let replacement =
                VulkanCopyTarget::new(new_device.clone()).map_err(|error| error.to_string())?;
            let mut target = surface
                .shared
                .gpu
                .lock()
                .map_err(|_| "CEF GPU target lock failed during recovery".to_string())?;
            *target = replacement;
        }
        self.bevy_host = new_bevy_host;
        self.bevy_layer = new_bevy_layer;
        self.node_layer = new_node_layer;
        self.joined_panel = new_joined_panel;
        self.retained_text_layer = new_retained_text_layer;
        let old_device = std::mem::replace(&mut self.device, new_device);
        self.queue = new_queue;
        self.surface.configure(&self.device, &self.config);
        old_device.destroy();
        self.update_layout();
        self.device_recovery_completed = true;
        self.device_recovery_millis = Some(started.elapsed().as_secs_f64() * 1_000.0);
        Ok(())
    }

    fn surfaces_have_results(&self) -> bool {
        self.surfaces.iter().all(|surface| {
            surface
                .shared
                .gpu
                .lock()
                .map(|gpu| {
                    let facts = gpu.facts();
                    facts.copied_frames > 0 || facts.refused_frames > 0
                })
                .unwrap_or(true)
        })
    }

    fn update_title(&self) {
        let copied = self
            .surfaces
            .iter()
            .map(|surface| copy_facts(&surface.shared).copied_frames)
            .sum::<u64>();
        let refused = self
            .surfaces
            .iter()
            .map(|surface| copy_facts(&surface.shared).refused_frames)
            .sum::<u64>();
        let bridge_allowed = self
            .surfaces
            .iter()
            .map(|surface| surface.shared.bridge_allowed.load(Ordering::Relaxed))
            .sum::<u64>();
        let bridge_refused = self
            .surfaces
            .iter()
            .map(|surface| surface.shared.bridge_refused.load(Ordering::Relaxed))
            .sum::<u64>();
        let suffix = self
            .last_export
            .as_ref()
            .map(|path| format!(" · report {}", path.display()))
            .unwrap_or_default();
        self.window.set_title(&format!(
            "Lince native Interface · {} CEF Sands · GPU {copied}/{refused} · bridge {bridge_allowed}/{bridge_refused} · F10 compose · F11 configure · F12 official Sands · F8 export{suffix}",
            self.surfaces.len(),
        ));
    }

    fn dispatch(&mut self, event: NormalizedInput, target_index: usize) {
        self.sequence += 1;
        let surface = InputSurface::new(
            "cef-diagnostic-window",
            self.config.width,
            self.config.height,
            self.window.scale_factor(),
        );
        let Some(browser_surface) = self.surfaces.get(target_index) else {
            return;
        };
        let scale = self.window.scale_factor();
        let target = InputTarget {
            semantic_id: format!("{}-cef-surface", browser_surface.shared.label),
            adapter: "cef".into(),
            local_from_surface: AffineTransform {
                xx: 1.0 / scale,
                xy: 0.0,
                yx: 0.0,
                yy: 1.0 / scale,
                tx: -f64::from(browser_surface.rect[0]) / scale,
                ty: -f64::from(browser_surface.rect[1]) / scale,
            },
            local_clip: InputRect {
                origin: InputPoint::new(0.0, 0.0),
                extent: InputPoint::new(
                    f64::from(browser_surface.rect[2]) / scale,
                    f64::from(browser_surface.rect[3]) / scale,
                ),
            },
        };
        let envelope = InputEnvelope::new(
            self.sequence,
            InputSource::LinceWinit,
            surface,
            target,
            event,
        );
        if let Ok(command) = route_cef_input(&envelope) {
            send_cef_command(&browser_surface.browser, command);
        }
    }

    fn target_at_cursor(&self) -> Option<usize> {
        #[cfg(feature = "joined-runtime")]
        if self.official_runtime.active().is_some() {
            return None;
        }
        self.surfaces
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, surface)| point_in_rect(self.cursor, surface.rect).then_some(index))
    }

    #[cfg(feature = "joined-runtime")]
    fn official_runtime_viewport(&self) -> RetainedRect {
        let width = self.config.width as f32;
        let height = self.config.height as f32;
        RetainedRect {
            x: width * 0.5 + 12.0,
            y: 18.0,
            width: (width * 0.5 - 30.0).max(1.0),
            height: (height - 36.0).max(1.0),
        }
    }

    fn focus(&mut self, index: usize) {
        #[cfg(feature = "joined-runtime")]
        {
            self.native_gallery_focused = false;
        }
        self.focused = index;
        for (surface_index, surface) in self.surfaces.iter().enumerate() {
            if let Some(host) = surface.browser.host() {
                host.set_focus(i32::from(surface_index == index));
            }
        }
    }

    #[cfg(feature = "joined-runtime")]
    fn focus_native_gallery(&mut self) {
        self.native_gallery_focused = true;
        self.composition_workbench_active = false;
        self.configuration_workbench_active = false;
        self.official_sand_gallery_active = false;
        for surface in &self.surfaces {
            if let Some(host) = surface.browser.host() {
                host.set_focus(0);
            }
        }
        self.last_panel_refresh = Instant::now() - Duration::from_secs(1);
    }

    #[cfg(feature = "joined-runtime")]
    fn gallery_index_at_cursor(&self) -> Option<usize> {
        let panel_width = f64::from(self.config.width) * 0.47;
        if self.composition_workbench_active
            || self.configuration_workbench_active
            || self.official_sand_gallery_active
            || self.cursor.x >= panel_width
            || self.cursor.y < 100.0
        {
            return None;
        }
        let row = ((self.cursor.y - 100.0) / 21.0).floor().max(0.0) as usize;
        let column = usize::from(self.cursor.x >= panel_width * 0.5);
        let index = row.saturating_mul(2).saturating_add(column);
        (index < PRIMITIVE_COUNT).then_some(index)
    }

    #[cfg(feature = "joined-runtime")]
    fn configuration_index_at_cursor(&self) -> Option<usize> {
        let panel_width = f64::from(self.config.width) * 0.47;
        if !self.configuration_workbench_active
            || self.cursor.x >= panel_width
            || self.cursor.y < 100.0
        {
            return None;
        }
        let index = ((self.cursor.y - 100.0) / 21.0).floor().max(0.0) as usize;
        (index < ConfigurationOperation::ALL.len()).then_some(index)
    }

    #[cfg(feature = "joined-runtime")]
    fn official_index_at_cursor(&self) -> Option<usize> {
        let panel_width = f64::from(self.config.width) * 0.47;
        if !self.official_sand_gallery_active
            || self.cursor.x >= panel_width
            || self.cursor.y < 100.0
        {
            return None;
        }
        let row = ((self.cursor.y - 100.0) / 21.0).floor().max(0.0) as usize;
        let column = usize::from(self.cursor.x >= panel_width * 0.5);
        let index = row.saturating_mul(2).saturating_add(column);
        (index < OFFICIAL_SAND_COUNT).then_some(index)
    }

    fn export_report(&mut self) {
        let report = CefDiagnosticReport::from_window(self);
        let path = self.report_path.clone();
        let result = path
            .parent()
            .map(fs::create_dir_all)
            .transpose()
            .and_then(|_| serde_json::to_vec_pretty(&report).map_err(std::io::Error::other))
            .and_then(|bytes| fs::write(&path, bytes));
        match result {
            Ok(()) => self.last_export = Some(path),
            Err(error) => {
                if let Some(surface) = self.surfaces.first() {
                    surface
                        .shared
                        .set_status(format!("report export failed: {error}"));
                } else {
                    eprintln!("report export failed: {error}");
                }
            }
        }
        self.update_title();
    }

    fn begin_close(&mut self) {
        if self.closing {
            return;
        }
        self.closing = true;
        for surface in &self.surfaces {
            if let Some(host) = surface.browser.host() {
                host.close_browser(1);
            }
        }
    }

    fn close_complete(&self) -> bool {
        self.closing
            && self
                .surfaces
                .iter()
                .all(|surface| surface.shared.closed.load(Ordering::Acquire))
    }

    fn installed(&self) -> Option<&BrowserSurface> {
        self.surfaces
            .iter()
            .find(|surface| surface.shared.is_installed())
    }

    #[cfg(feature = "joined-runtime")]
    fn website(&self) -> Option<&BrowserSurface> {
        self.surfaces
            .iter()
            .find(|surface| !surface.shared.is_installed())
    }
}

impl ApplicationHandler for DiagnosticApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        do_message_loop_work();
        self.ensure_state(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if state.window.id() != window_id {
            return;
        }
        #[cfg(feature = "joined-runtime")]
        state.accessibility.process_event(&state.window, &event);
        match event {
            WindowEvent::CloseRequested
                if cfg!(feature = "joined-runtime")
                    && state.export_and_exit
                    && state.started.elapsed()
                        < Duration::from_secs(state.warmup_seconds + state.sample_seconds) => {}
            WindowEvent::CloseRequested => state.begin_close(),
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::ScaleFactorChanged { .. } => {
                state.resize(state.window.inner_size());
                state.dispatch(NormalizedInput::Focus { focused: true }, state.focused);
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = state.render() {
                    if let Some(surface) = state.surfaces.first() {
                        surface
                            .shared
                            .set_status(format!("host render error: {error}"));
                    } else {
                        eprintln!("host render error: {error}");
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor = InputPoint::new(position.x, position.y);
                if let Some(target) = state.target_at_cursor() {
                    state.dispatch(
                        NormalizedInput::PointerMoved {
                            surface_physical: state.cursor,
                            buttons: Vec::new(),
                            modifiers: state.modifiers,
                        },
                        target,
                    );
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                if let Some(target) = state.target_at_cursor() {
                    if button_state == ElementState::Pressed {
                        state.focus(target);
                    }
                    if let Some(button) = pointer_button(button) {
                        state.dispatch(
                            NormalizedInput::PointerButton {
                                surface_physical: state.cursor,
                                button,
                                state: input_button_state(button_state),
                                modifiers: state.modifiers,
                            },
                            target,
                        );
                    }
                } else {
                    #[cfg(feature = "joined-runtime")]
                    if button == WinitMouseButton::Left && button_state == ElementState::Pressed {
                        if let Some(scene) = state.official_runtime_scene.clone()
                            && let Some(index) = scene.hit_test([state.cursor.x, state.cursor.y])
                        {
                            state.official_runtime.focus_at(index, &scene);
                            if let Ok(focused_scene) = state
                                .official_runtime
                                .scene(state.official_runtime_viewport())
                            {
                                let _ = state.official_runtime.activate(&focused_scene);
                                state.official_runtime_scene = Some(focused_scene);
                            }
                            state.accessibility.record_keyboard_action();
                            state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            state.publish_accessibility();
                        } else if let Some(index) = state.official_index_at_cursor() {
                            state.official_sand_gallery.focus_at(index);
                            state.accessibility.record_keyboard_action();
                            state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            state.publish_accessibility();
                        } else if let Some(index) = state.configuration_index_at_cursor() {
                            state.configuration_workbench.focus_at(index);
                            if state.configuration_workbench.activate().is_ok()
                                && let Ok(style) = state.configuration_workbench.resolved_style()
                            {
                                state.resolved_style = style;
                            }
                            state.accessibility.record_keyboard_action();
                            state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            state.publish_accessibility();
                        } else if let Some(index) = state.gallery_index_at_cursor() {
                            state.focus_native_gallery();
                            state.primitive_gallery.focus_at(
                                index,
                                lince_interface::primitive_gallery::GalleryVisualState::FocusVisible,
                            );
                            state.primitive_gallery.activate();
                            state.accessibility.record_keyboard_action();
                            state.publish_accessibility();
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(target) = state.target_at_cursor() else {
                    return;
                };
                let (delta, unit) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (
                        InputPoint::new(f64::from(x), f64::from(y)),
                        ScrollUnit::Lines,
                    ),
                    MouseScrollDelta::PixelDelta(position) => (
                        InputPoint::new(position.x, position.y),
                        ScrollUnit::PhysicalPixels,
                    ),
                };
                state.dispatch(
                    NormalizedInput::Scroll {
                        surface_physical: state.cursor,
                        delta,
                        unit,
                        modifiers: state.modifiers,
                    },
                    target,
                );
            }
            WindowEvent::KeyboardInput { event, .. } => {
                #[cfg(feature = "joined-runtime")]
                if event.state == ElementState::Pressed {
                    if event.logical_key == Key::Named(NamedKey::Escape)
                        && state.official_runtime.active().is_some()
                    {
                        state.official_runtime.close();
                        state.official_runtime_scene = None;
                        state.official_sand_gallery_active = true;
                        state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                        state.publish_accessibility();
                        return;
                    }
                    if event.logical_key == Key::Named(NamedKey::F10) {
                        state.official_runtime.close();
                        state.official_runtime_scene = None;
                        state.composition_workbench_active = !state.composition_workbench_active;
                        state.configuration_workbench_active = false;
                        state.official_sand_gallery_active = false;
                        state.native_gallery_focused = false;
                        state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                        state.publish_accessibility();
                        return;
                    }
                    if event.logical_key == Key::Named(NamedKey::F11) {
                        state.official_runtime.close();
                        state.official_runtime_scene = None;
                        state.configuration_workbench_active =
                            !state.configuration_workbench_active;
                        state.composition_workbench_active = false;
                        state.official_sand_gallery_active = false;
                        state.native_gallery_focused = false;
                        if state.configuration_workbench_active {
                            if let Ok(style) = state.configuration_workbench.resolved_style() {
                                state.resolved_style = style;
                            }
                        } else {
                            let _ = state.refresh_style();
                        }
                        state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                        state.publish_accessibility();
                        return;
                    }
                    if event.logical_key == Key::Named(NamedKey::F12) {
                        if state.official_runtime.active().is_some() {
                            state.official_runtime.close();
                            state.official_runtime_scene = None;
                            state.official_sand_gallery_active = true;
                        } else {
                            state.official_sand_gallery_active =
                                !state.official_sand_gallery_active;
                        }
                        state.composition_workbench_active = false;
                        state.configuration_workbench_active = false;
                        state.native_gallery_focused = false;
                        state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                        state.publish_accessibility();
                        return;
                    }
                    if state.official_runtime.active().is_some()
                        && let Some(scene) = state.official_runtime_scene.clone()
                    {
                        let handled = match &event.logical_key {
                            Key::Named(NamedKey::Tab) => {
                                state
                                    .official_runtime
                                    .focus_next(state.modifiers.shift, &scene);
                                true
                            }
                            Key::Named(NamedKey::Enter) => {
                                let _ = state.official_runtime.activate(&scene);
                                state.accessibility.record_keyboard_action();
                                true
                            }
                            Key::Named(NamedKey::Backspace) => {
                                state.official_runtime.backspace(&scene);
                                true
                            }
                            Key::Character(value) if value.as_str() == " " => {
                                let _ = state.official_runtime.activate(&scene);
                                state.accessibility.record_keyboard_action();
                                true
                            }
                            Key::Character(value) => {
                                state.official_runtime.input_text(value.as_str(), &scene);
                                true
                            }
                            _ => false,
                        };
                        if handled {
                            state.official_runtime_scene = state
                                .official_runtime
                                .scene(state.official_runtime_viewport())
                                .ok();
                            state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            state.publish_accessibility();
                            return;
                        }
                    }
                    if state.official_sand_gallery_active {
                        if event.logical_key == Key::Named(NamedKey::Tab) {
                            state
                                .official_sand_gallery
                                .move_focus(state.modifiers.shift);
                            state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            state.publish_accessibility();
                            return;
                        }
                        if event.logical_key == Key::Named(NamedKey::Enter)
                            || matches!(&event.logical_key, Key::Character(value) if value.as_str() == " ")
                        {
                            let selected = state.official_sand_gallery.selected();
                            if state.official_runtime.open(selected.uid) {
                                state.official_sand_gallery_active = false;
                                state.official_runtime_scene = state
                                    .official_runtime
                                    .scene(state.official_runtime_viewport())
                                    .ok();
                            }
                            state.accessibility.record_keyboard_action();
                            state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            state.publish_accessibility();
                            return;
                        }
                    }
                    if state.configuration_workbench_active {
                        match &event.logical_key {
                            Key::Named(NamedKey::Tab) => {
                                state
                                    .configuration_workbench
                                    .focus_next(state.modifiers.shift);
                                state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                                state.publish_accessibility();
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                if state.configuration_workbench.activate().is_ok()
                                    && let Ok(style) =
                                        state.configuration_workbench.resolved_style()
                                {
                                    state.resolved_style = style;
                                }
                                state.accessibility.record_keyboard_action();
                                state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                                state.publish_accessibility();
                                return;
                            }
                            _ => {}
                        }
                    }
                    if state.composition_workbench_active {
                        match &event.logical_key {
                            Key::Named(NamedKey::Tab) => {
                                state
                                    .composition_workbench
                                    .focus_next(state.modifiers.shift);
                                state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                                return;
                            }
                            Key::Named(NamedKey::Enter) => {
                                let _ = state.composition_workbench.activate();
                                state.accessibility.record_keyboard_action();
                                state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                                return;
                            }
                            _ => {}
                        }
                    }
                    if event.logical_key == Key::Named(NamedKey::F5) {
                        if state.native_gallery_focused {
                            state.focus(state.focused);
                        } else {
                            state.focus_native_gallery();
                        }
                        return;
                    }
                    if state.native_gallery_focused {
                        match &event.logical_key {
                            Key::Named(NamedKey::Tab) => {
                                state.primitive_gallery.focus_next(state.modifiers.shift);
                            }
                            Key::Named(NamedKey::Enter) => {
                                state.primitive_gallery.activate();
                                state.accessibility.record_keyboard_action();
                            }
                            Key::Named(NamedKey::F9) => {
                                state.primitive_gallery.cycle_state();
                            }
                            Key::Character(value) if value.as_str() == " " => {
                                state.primitive_gallery.activate();
                                state.accessibility.record_keyboard_action();
                            }
                            Key::Character(value) => {
                                state.primitive_gallery.input_text(value.as_str());
                            }
                            _ => {}
                        }
                        state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                        state.publish_accessibility();
                        return;
                    }
                    match event.logical_key {
                        Key::Named(NamedKey::F1) => {
                            state.style_gallery.cycle_palette();
                            if state.refresh_style().is_ok() {
                                state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            }
                            return;
                        }
                        Key::Named(NamedKey::F2) => {
                            state.style_gallery.cycle_density();
                            if state.refresh_style().is_ok() {
                                state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            }
                            return;
                        }
                        Key::Named(NamedKey::F3) => {
                            state.style_gallery.cycle_radius();
                            if state.refresh_style().is_ok() {
                                state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            }
                            return;
                        }
                        Key::Named(NamedKey::F4) => {
                            state.style_gallery.toggle_selected_theme();
                            if state.refresh_style().is_ok() {
                                state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            }
                            return;
                        }
                        Key::Named(NamedKey::F6) => {
                            state.scenario_index = (state.scenario_index + 1) % SCENARIOS.len();
                            state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            return;
                        }
                        Key::Named(NamedKey::F7) => {
                            state.style_gallery.toggle_mode();
                            if state.refresh_style().is_ok() {
                                state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                            }
                            return;
                        }
                        Key::Named(NamedKey::Enter) => {
                            state.accessibility.record_keyboard_action();
                        }
                        _ => {}
                    }
                }
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::F8)
                {
                    state.export_report();
                    return;
                }
                state.dispatch(normalized_key(event, state.modifiers), state.focused);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = input_modifiers(modifiers.state());
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                state.dispatch(
                    NormalizedInput::ImePreedit {
                        text,
                        cursor: cursor.map(|(start, end)| [start, end]),
                    },
                    state.focused,
                );
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                #[cfg(feature = "joined-runtime")]
                if let Some(scene) = state.official_runtime_scene.clone() {
                    state.official_runtime.input_text(text.as_str(), &scene);
                    state.official_runtime_scene = state
                        .official_runtime
                        .scene(state.official_runtime_viewport())
                        .ok();
                    state.last_panel_refresh = Instant::now() - Duration::from_secs(1);
                    return;
                }
                state.dispatch(NormalizedInput::ImeCommit { text }, state.focused);
            }
            WindowEvent::Ime(Ime::Enabled) => {}
            WindowEvent::Ime(Ime::Disabled) => {
                if let Some(host) = focused_browser(state).and_then(Browser::host) {
                    host.ime_cancel_composition();
                }
            }
            WindowEvent::Focused(focused) => {
                state.dispatch(NormalizedInput::Focus { focused }, state.focused);
            }
            _ => {}
        }
        if state.close_complete() {
            if state.export_and_exit && state.last_export.is_none() {
                state.export_report();
            }
            let _ = state.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(2)),
            });
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        do_message_loop_work();
        self.ensure_state(event_loop);
        let mut wake_at = Instant::now() + Duration::from_nanos(8_333_333);
        if let Some(state) = self.state.as_mut() {
            let now = Instant::now();
            if now >= state.next_redraw {
                state.window.request_redraw();
                state.next_redraw = now + Duration::from_nanos(8_333_333);
            }
            wake_at = state.next_redraw;
            if state.close_complete() {
                if state.export_and_exit && state.last_export.is_none() {
                    state.export_report();
                }
                event_loop.exit();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake_at));
    }
}

fn open_interop_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), String> {
    let descriptor = DeviceDescriptor {
        label: Some("lince-cef-diagnostic-host"),
        ..DeviceDescriptor::default()
    };
    let hal_device = {
        let hal_adapter = unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }
            .ok_or_else(|| "CEF interop requires a Vulkan WGPU adapter".to_string())?;
        let supported = unsafe {
            hal_adapter
                .shared_instance()
                .raw_instance()
                .enumerate_device_extension_properties(hal_adapter.raw_physical_device())
        }
        .map_err(|error| format!("enumerate Vulkan device extensions: {error:?}"))?;
        let required = ash::ext::image_drm_format_modifier::NAME;
        if !supported.iter().any(|extension| {
            extension
                .extension_name_as_c_str()
                .is_ok_and(|name| name == required)
        }) {
            return Err(format!(
                "physical Vulkan adapter is missing {}",
                required.to_string_lossy()
            ));
        }
        unsafe {
            hal_adapter.open_with_callback(
                descriptor.required_features,
                &descriptor.required_limits,
                &descriptor.memory_hints,
                Some(Box::new(move |arguments| {
                    arguments.extensions.push(required);
                })),
            )
        }
        .map_err(|error| format!("open Vulkan interop device: {error:?}"))?
    };
    unsafe { adapter.create_device_from_hal::<wgpu::hal::api::Vulkan>(hal_device, &descriptor) }
        .map_err(|error| format!("adopt Vulkan interop device into WGPU: {error}"))
}

fn create_browser(
    shared: SurfaceShared,
    _device: &wgpu::Device,
    installed: bool,
) -> Result<BrowserSurface, String> {
    let mut client = DiagnosticClient::new(shared.clone());
    let mut window_info = WindowInfo::default().set_as_windowless(0);
    window_info.shared_texture_enabled = 1;
    window_info.external_begin_frame_enabled = 1;
    window_info.runtime_style = RuntimeStyle::ALLOY;
    let browser_settings = BrowserSettings {
        windowless_frame_rate: 60,
        local_storage: State::ENABLED,
        javascript: State::ENABLED,
        webgl: State::ENABLED,
        background_color: 0xff101814,
        ..BrowserSettings::default()
    };
    let mut context_settings = RequestContextSettings::default();
    if installed {
        let cache = PathBuf::from("target/interface-laboratory/cef/root")
            .join(&shared.manifest.request_context().partition_key);
        fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        let cache = cache.canonicalize().map_err(|error| error.to_string())?;
        context_settings.cache_path = CefString::from(cache.to_string_lossy().as_ref());
        context_settings.persist_session_cookies = 0;
        context_settings.cookieable_schemes_list = CefString::from("lince-sand");
    }
    let context_ready = Arc::new(AtomicBool::new(false));
    let mut context_handler = DiagnosticRequestContextHandler::new(context_ready.clone());
    let mut context =
        request_context_create_context(Some(&context_settings), Some(&mut context_handler))
            .ok_or_else(|| format!("create {} request context", shared.label))?;
    let context_deadline = Instant::now() + Duration::from_secs(5);
    while !context_ready.load(Ordering::Acquire) {
        if Instant::now() >= context_deadline {
            return Err(format!("initialize {} request context", shared.label));
        }
        do_message_loop_work();
        std::thread::yield_now();
    }
    if installed {
        let mut factory = InstalledSchemeFactory::new();
        if context.register_scheme_handler_factory(
            Some(&CefString::from("lince-sand")),
            Some(&CefString::from("installed")),
            Some(&mut factory),
        ) == 0
        {
            return Err("register installed scheme on its request context".into());
        }
    }
    let url = shared.expected_url().to_owned();
    let browser = browser_host_create_browser_sync(
        Some(&window_info),
        Some(&mut client),
        Some(&CefString::from(url.as_str())),
        Some(&browser_settings),
        None,
        Some(&mut context),
    )
    .ok_or_else(|| format!("create {} CEF browser", shared.label))?;
    if let Some(host) = browser.host() {
        host.set_accessibility_state(State::ENABLED);
    }
    Ok(BrowserSurface {
        browser,
        shared,
        rect: [0, 0, 1, 1],
    })
}

fn send_cef_command(browser: &Browser, command: CefInputCommand) {
    let Some(host) = browser.host() else { return };
    match command {
        CefInputCommand::PointerMoved { x, y, modifiers } => {
            host.send_mouse_move_event(
                Some(&MouseEvent {
                    x,
                    y,
                    modifiers: cef_modifiers(modifiers),
                }),
                0,
            );
        }
        CefInputCommand::PointerButton {
            x,
            y,
            button,
            state,
            modifiers,
        } => {
            let button = match button {
                CefPointerButton::Left => MouseButtonType::LEFT,
                CefPointerButton::Right => MouseButtonType::RIGHT,
                CefPointerButton::Middle => MouseButtonType::MIDDLE,
            };
            host.send_mouse_click_event(
                Some(&MouseEvent {
                    x,
                    y,
                    modifiers: cef_modifiers(modifiers),
                }),
                button,
                i32::from(state == ButtonState::Released),
                1,
            );
        }
        CefInputCommand::Scroll {
            x,
            y,
            delta_x,
            delta_y,
            modifiers,
        } => {
            host.send_mouse_wheel_event(
                Some(&MouseEvent {
                    x,
                    y,
                    modifiers: cef_modifiers(modifiers),
                }),
                delta_x,
                delta_y,
            );
        }
        CefInputCommand::Key {
            logical_key,
            text,
            state,
            modifiers,
        } => {
            let character = text
                .as_deref()
                .and_then(|value| value.encode_utf16().next())
                .unwrap_or_default();
            let event = KeyEvent {
                type_: if state == ButtonState::Pressed {
                    KeyEventType::RAWKEYDOWN
                } else {
                    KeyEventType::KEYUP
                },
                modifiers: cef_modifiers(modifiers),
                windows_key_code: virtual_key(&logical_key, character),
                character,
                unmodified_character: character,
                ..KeyEvent::default()
            };
            host.send_key_event(Some(&event));
            if state == ButtonState::Pressed && character != 0 {
                host.send_key_event(Some(&KeyEvent {
                    type_: KeyEventType::CHAR,
                    ..event
                }));
            }
        }
        CefInputCommand::Focus { focused } => host.set_focus(i32::from(focused)),
        CefInputCommand::ImePreedit { text, cursor } => {
            let selection = cursor
                .and_then(|[start, end]| utf16_range(&text, start, end))
                .unwrap_or_default();
            host.ime_set_composition(
                Some(&CefString::from(text.as_str())),
                None,
                None,
                Some(&selection),
            );
        }
        CefInputCommand::ImeCommit { text } => {
            host.ime_commit_text(Some(&CefString::from(text.as_str())), None, 0);
        }
    }
}

fn cef_modifiers(modifiers: InputModifiers) -> u32 {
    let mut flags = 0;
    if modifiers.shift {
        flags |= cef::sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0;
    }
    if modifiers.control {
        flags |= cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0;
    }
    if modifiers.alt {
        flags |= cef::sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN.0;
    }
    if modifiers.platform {
        flags |= cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0;
    }
    flags
}

fn virtual_key(logical: &str, character: u16) -> i32 {
    match logical {
        "Enter" => 13,
        "Tab" => 9,
        "Backspace" => 8,
        "Delete" => 46,
        "Escape" => 27,
        "ArrowLeft" => 37,
        "ArrowUp" => 38,
        "ArrowRight" => 39,
        "ArrowDown" => 40,
        _ if character <= 0x7f => i32::from((character as u8).to_ascii_uppercase()),
        _ => i32::from(character),
    }
}

fn utf16_range(text: &str, start: usize, end: usize) -> Option<Range> {
    if start > end || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return None;
    }
    Some(Range {
        from: text[..start].encode_utf16().count().try_into().ok()?,
        to: text[..end].encode_utf16().count().try_into().ok()?,
    })
}

fn focused_browser(state: &DiagnosticWindow) -> Option<&Browser> {
    state
        .surfaces
        .get(state.focused)
        .map(|surface| &surface.browser)
}

fn normalized_key(event: WinitKeyEvent, modifiers: InputModifiers) -> NormalizedInput {
    NormalizedInput::Key {
        physical_key: Some(format!("{:?}", event.physical_key)),
        logical_key: logical_key(&event.logical_key),
        text: event.text.map(|text| text.to_string()),
        state: input_button_state(event.state),
        repeat: event.repeat,
        modifiers,
    }
}

fn logical_key(key: &Key) -> String {
    match key {
        Key::Character(value) => value.to_string(),
        Key::Named(value) => format!("{value:?}"),
        Key::Dead(value) => value
            .map(|character| character.to_string())
            .unwrap_or_else(|| "Dead".into()),
        Key::Unidentified(_) => "Unidentified".into(),
    }
}

fn input_modifiers(modifiers: ModifiersState) -> InputModifiers {
    InputModifiers {
        control: modifiers.control_key(),
        alt: modifiers.alt_key(),
        shift: modifiers.shift_key(),
        platform: modifiers.super_key(),
        function: false,
    }
}

fn pointer_button(button: WinitMouseButton) -> Option<PointerButton> {
    match button {
        WinitMouseButton::Left => Some(PointerButton::Left),
        WinitMouseButton::Right => Some(PointerButton::Right),
        WinitMouseButton::Middle => Some(PointerButton::Middle),
        WinitMouseButton::Back => Some(PointerButton::Back),
        WinitMouseButton::Forward => Some(PointerButton::Forward),
        WinitMouseButton::Other(value) => Some(PointerButton::Other(value)),
    }
}

fn input_button_state(state: ElementState) -> ButtonState {
    match state {
        ElementState::Pressed => ButtonState::Pressed,
        ElementState::Released => ButtonState::Released,
    }
}

fn point_in_rect(point: InputPoint, rect: [u32; 4]) -> bool {
    point.x >= f64::from(rect[0])
        && point.y >= f64::from(rect[1])
        && point.x < f64::from(rect[0] + rect[2])
        && point.y < f64::from(rect[1] + rect[3])
}

fn browser_rects(width: u32, height: u32, count: usize, joined: bool) -> Vec<[u32; 4]> {
    if joined && count == 2 {
        let margin = 18;
        let surface_width = (width * 43 / 100).max(1);
        let surface_height = (height * 46 / 100).max(1);
        return vec![
            [
                width.saturating_sub(surface_width + margin),
                margin,
                surface_width,
                surface_height,
            ],
            [
                width.saturating_sub(surface_width + margin + 70),
                height.saturating_sub(surface_height + margin),
                surface_width,
                surface_height,
            ],
        ];
    }
    if count == 0 {
        return Vec::new();
    }
    let columns = match count {
        1 => 1,
        2..=4 => 2,
        _ => 4,
    };
    let rows = count.div_ceil(columns);
    let margin = 18_u32;
    let gap = 12_u32;
    let columns_u32 = columns as u32;
    let rows_u32 = rows as u32;
    let usable_width = width.saturating_sub(margin * 2 + gap * columns_u32.saturating_sub(1));
    let usable_height = height.saturating_sub(margin * 2 + gap * rows_u32.saturating_sub(1));
    let cell_width = (usable_width / columns_u32).max(1);
    let cell_height = (usable_height / rows_u32).max(1);
    (0..count)
        .map(|index| {
            let column = (index % columns) as u32;
            let row = (index / columns) as u32;
            [
                margin + column * (cell_width + gap),
                margin + row * (cell_height + gap),
                cell_width,
                cell_height,
            ]
        })
        .collect()
}

fn nonzero_size(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
}

fn copy_facts(shared: &SurfaceShared) -> VulkanCopyFacts {
    shared.gpu.lock().map(|gpu| gpu.facts()).unwrap_or_default()
}

#[derive(Serialize)]
struct CefDiagnosticReport {
    schema_version: u32,
    gate: &'static str,
    status: &'static str,
    created_unix_millis: u128,
    git_revision: String,
    git_dirty: bool,
    source_fingerprint_sha256: String,
    profile: &'static str,
    frames_presented: u64,
    runtime_seconds: f64,
    warmup_seconds: u64,
    sample_seconds: u64,
    cef_notices_bundled: bool,
    cef_count: usize,
    host: CefHostReport,
    workload: CefWorkloadReport,
    surfaces: Vec<CefSurfaceReport>,
    assertions: Vec<CefAssertion>,
    #[cfg(feature = "joined-runtime")]
    joined: JoinedReport,
}

#[derive(Serialize)]
struct CefHostReport {
    operating_system: &'static str,
    window_system: &'static str,
    architecture: &'static str,
    cpu_model: String,
    memory_kib: Option<u64>,
    power_profile: String,
    gpu_name: String,
    gpu_backend: String,
    gpu_driver: String,
    gpu_driver_info: String,
    surface_format: String,
    present_mode: String,
    scale_factor: f64,
    surface_width: u32,
    surface_height: u32,
    physical_presentation_timing: &'static str,
    process_to_interactive_window_millis: f64,
    process_to_first_cef_frame_millis: Option<f64>,
    executable_bytes: Option<u64>,
    cef_runtime_bytes: Option<u64>,
    bundled_notice_bytes: Option<u64>,
}

#[derive(Serialize)]
struct CefWorkloadReport {
    version: u32,
    seed: u64,
    visible_interactive_sands: usize,
    continuously_eligible_bodies: usize,
    resident_light_nodes: usize,
    native_sand_half_extent_ndc: [f32; 2],
    body_shape: &'static str,
    moving_ratio: f32,
    area_set: &'static str,
    cef_surface_rects: Vec<[u32; 4]>,
    cef_content: &'static str,
    instrumentation_overlay: &'static str,
}

#[cfg(feature = "joined-runtime")]
#[derive(Serialize)]
struct JoinedReport {
    semantic_instances: usize,
    active_bodies: usize,
    resident_nodes: usize,
    visible_native_sands: u32,
    bevy_updates: u64,
    bevy_command_buffers_handed_to_host: u64,
    host_queue_submissions: u64,
    frame_samples: usize,
    frame_p50_millis: f64,
    frame_p95_millis: f64,
    frame_p99_millis: f64,
    frame_hitches_over_50_millis: usize,
    cpu_frame_samples: usize,
    cpu_frame_p50_millis: f64,
    cpu_frame_p95_millis: f64,
    cpu_frame_p99_millis: f64,
    fixed_step_p95_millis: f64,
    fixed_step_hz: u32,
    fixed_step_ticks: u64,
    fixed_step_backlog_steps: u64,
    input_to_present_call_samples: usize,
    input_to_present_call_p50_millis: f64,
    input_to_present_call_p95_millis: f64,
    input_to_present_call_p99_millis: f64,
    frame_interval_nanos: Vec<u64>,
    cpu_frame_nanos: Vec<u64>,
    fixed_step_nanos: Vec<u64>,
    input_to_present_call_nanos: Vec<u64>,
    off_camera_copied_frame_delta: Option<u64>,
    off_camera_bridge_event_delta: Option<u64>,
    off_camera_paint_suppression_effective: bool,
    accessibility_nodes: usize,
    accessibility_initial_tree_requests: u64,
    accessibility_actions: u64,
    accessibility_deactivations: u64,
    accessibility_updates: u64,
    accessibility_probe: bool,
    scenario_count: usize,
    visible_panel_controls: [&'static str; 14],
    sand_schema_version: u32,
    sand_abi_version: u32,
    primitive_definition_count: usize,
    primitive_package_sha256: String,
    primitive_gallery_focus: String,
    primitive_gallery_state: String,
    primitive_gallery_actions: u64,
    primitive_gallery_events: u64,
    composition_schema_version: u32,
    composition_definition_count: usize,
    composition_package_sha256: String,
    composition_selected_operation: String,
    composition_facts: CompositionRuntimeFacts,
    composition_arrows: Vec<String>,
    configuration_schema_version: u32,
    configuration_definition_count: usize,
    configuration_package_sha256: String,
    configuration_selected_operation: String,
    configuration_facts: ConfigurationFacts,
    configuration_mode: String,
    configuration_launch_receipts: usize,
    configuration_persisted_bytes: usize,
    configuration_probe_applied: bool,
    configuration_probe_reverted: bool,
    configuration_probe_cef_load_before: Option<u64>,
    configuration_probe_cef_load_after: Option<u64>,
    official_sand_count: usize,
    official_definition_count: usize,
    official_package_sha256: String,
    official_selected: String,
    official_selected_state: String,
    official_runtime_facts: SharedOfficialRuntimeFacts,
    native_domain_facts: NativeDomainFacts,
    token_probe_applied: bool,
    token_probe_cef_load_before: Option<u64>,
    token_probe_cef_load_after: Option<u64>,
    style_contract_version: u32,
    style_theme: String,
    style_mode: String,
    style_projection_updates: u64,
    style_css_projection_bytes: usize,
    resolved_style: ResolvedStyle,
    button_instance_radius_px: f32,
    recovery_probe: bool,
    device_recovery_attempted: bool,
    device_recovery_completed: bool,
    device_recovery_millis: Option<f64>,
    browser_parity_report: &'static str,
    spatial_report: &'static str,
    ownership: [&'static str; 5],
}

#[derive(Serialize)]
struct CefSurfaceReport {
    authority: String,
    origin: String,
    request_context: lince_interface::html::RequestContextPolicy,
    gpu: VulkanCopyReport,
    bridge_allowed: u64,
    events_allowed: u64,
    actions_allowed: u64,
    bridge_refused: u64,
    latest_bridge_sequence: u64,
    cpu_paints: u64,
    external_network_requests: u64,
    load_completions: u64,
    load_failures: u64,
    popup_refusals: u64,
    context_menu_refusals: u64,
    media_allowed: u64,
    media_refused: u64,
    renderer_restarts: u64,
    closed: bool,
    latest_status: String,
    latest_page_status: String,
}

#[derive(Serialize)]
struct VulkanCopyReport {
    received_frames: u64,
    copied_frames: u64,
    composed_frames: u64,
    refused_frames: u64,
    latest_width: u32,
    latest_height: u32,
    latest_modifier: u64,
    latest_error: Option<String>,
}

#[derive(Serialize)]
struct CefAssertion {
    name: &'static str,
    passed: bool,
    detail: String,
}

impl CefDiagnosticReport {
    fn from_window(window: &DiagnosticWindow) -> Self {
        let surfaces = window
            .surfaces
            .iter()
            .map(|surface| surface_report(&surface.shared))
            .collect::<Vec<_>>();
        let installed = surfaces
            .iter()
            .find(|surface| surface.authority == "Installed");
        let website = surfaces
            .iter()
            .find(|surface| surface.authority == "Website");
        let assertions = vec![
            CefAssertion {
                name: "all admitted accelerated paints copied",
                passed: surfaces.iter().all(|surface| {
                    surface.gpu.copied_frames > 0 && surface.gpu.latest_error.is_none()
                }),
                detail: format!("{} admitted CEF Sands", surfaces.len()),
            },
            CefAssertion {
                name: "installed bridge allowed",
                passed: installed.is_none_or(|surface| surface.bridge_allowed > 0),
                detail: installed.map_or_else(
                    || "no installed Sand admitted".into(),
                    |surface| {
                        format!(
                            "allowed {}, refused {}",
                            surface.bridge_allowed, surface.bridge_refused
                        )
                    },
                ),
            },
            CefAssertion {
                name: "unknown installed bridge operation refused",
                passed: installed.is_none_or(|surface| surface.bridge_refused > 0),
                detail: installed.map_or_else(
                    || "no installed Sand admitted".into(),
                    |surface| {
                        format!(
                            "allowed {}, refused {}, latest sequence {}",
                            surface.bridge_allowed,
                            surface.bridge_refused,
                            surface.latest_bridge_sequence
                        )
                    },
                ),
            },
            CefAssertion {
                name: "installed primitive consumed Protein input and emitted declared Sand event",
                passed: installed.is_none_or(|surface| {
                    surface.events_allowed > 0
                        && surface.latest_page_status.contains("ABI v1")
                        && surface.latest_page_status.contains("ABI refusals 1")
                        && surface.latest_page_status.contains("Protein Build the Box")
                }),
                detail: installed.map_or_else(
                    || "no installed Sand admitted".into(),
                    |surface| {
                        format!(
                            "{} events; {}",
                            surface.events_allowed, surface.latest_page_status
                        )
                    },
                ),
            },
            #[cfg(feature = "joined-runtime")]
            CefAssertion {
                name: "recursive composition agrees across native host and Installed HTML",
                passed: {
                    let facts = window.composition_workbench.facts();
                    facts.mounted_placements >= 4
                        && facts.mounted_nodes > facts.mounted_placements
                        && facts.scoped_dom_identities == facts.mounted_nodes
                        && facts.active_behavior_handles > 0
                        && facts.configured_nodes > 0
                        && facts.emitted_events > 0
                        && facts.routed_outputs > 0
                        && facts.action_requests > 0
                        && facts.retired_behavior_handles > 0
                        && facts.rejected_publications > 0
                        && facts.save_reopens > 0
                        && facts.lock_changes > 0
                        && facts.style_override_sets > 0
                        && facts.style_override_resets > 0
                        && facts.shared_publications > 0
                        && facts.saved_definitions > 0
                        && facts.forked_definitions > 0
                        && installed.is_none_or(|surface| {
                            surface
                                .latest_page_status
                                .contains("composition 2/8 scoped unique")
                                && surface.latest_page_status.contains("teardown 1")
                        })
                },
                detail: format!(
                    "{:?}; {}",
                    window.composition_workbench.facts(),
                    installed.map_or("no Installed Sand", |surface| {
                        surface.latest_page_status.as_str()
                    })
                ),
            },
            #[cfg(feature = "joined-runtime")]
            CefAssertion {
                name: "Configuration Sand edits previews persists and launches through one artifact",
                passed: {
                    let workbench = &window.configuration_workbench;
                    let artifact = workbench.artifact();
                    let facts = workbench.facts();
                    let projected = artifact
                        .configuration
                        .projected_styles(&artifact.composition, "room-native");
                    facts.accepted_edits >= 13
                        && facts.refused_edits > 0
                        && facts.undo_count > 0
                        && facts.save_reopens > 0
                        && facts.live_previews > 0
                        && facts.definition_publications == 3
                        && facts.launches_created > 0
                        && facts.launches_focused > 0
                        && facts.persisted_bytes > 0
                        && window.configuration_probe_applied
                        && window.configuration_probe_reverted
                        && window.configuration_probe_load_before
                            == window.configuration_probe_load_after
                        && artifact.composition.catalog.active.len()
                            == CONFIGURATION_DEFINITION_COUNT
                        && artifact
                            .composition
                            .document
                            .placements
                            .iter()
                            .any(|placement| placement.instance_uid == "configuration-sand")
                        && artifact.configuration.launches.len() == 1
                        && !artifact.configuration.developer_mode
                        && artifact.configuration.raw_css.is_empty()
                        && artifact
                            .configuration
                            .resolved_for(&artifact.composition, "room-native")
                            .is_ok_and(|style| {
                                style
                                    .origin("--lynx-margin-sand")
                                    .is_ok_and(|origin| origin.scope == StyleScope::Workspace)
                                    && style
                                        .origin("--lynx-gap-content")
                                        .is_ok_and(|origin| origin.scope == StyleScope::Group)
                                    && style
                                        .origin("--lynx-radius-control")
                                        .is_ok_and(|origin| origin.scope == StyleScope::Instance)
                            })
                        && projected.is_ok_and(|projections| {
                            projections.len() == 4
                                && projections
                                    .windows(2)
                                    .all(|pair| pair[0].declarations == pair[1].declarations)
                        })
                },
                detail: format!(
                    "schema {}, {} definitions, {:?}, {} launch receipt, {} persisted bytes, CEF loads {:?} to {:?}",
                    CONFIGURATION_SCHEMA_VERSION,
                    window
                        .configuration_workbench
                        .artifact()
                        .composition
                        .catalog
                        .active
                        .len(),
                    window.configuration_workbench.facts(),
                    window
                        .configuration_workbench
                        .artifact()
                        .configuration
                        .launches
                        .len(),
                    window.configuration_workbench.facts().persisted_bytes,
                    window.configuration_probe_load_before,
                    window.configuration_probe_load_after,
                ),
            },
            #[cfg(feature = "joined-runtime")]
            CefAssertion {
                name: "official Sand catalog is Rust-owned decomposed and honestly staged",
                passed: window.official_sand_gallery.package().validate().is_ok()
                    && OFFICIAL_SANDS.len() == OFFICIAL_SAND_COUNT
                    && OFFICIAL_SANDS.iter().all(|spec| {
                        window
                            .official_sand_gallery
                            .package()
                            .graph
                            .definitions
                            .get(spec.uid)
                            .is_some_and(|definition| definition.children.len() >= 2)
                    })
                    && OFFICIAL_SANDS
                        .iter()
                        .filter(|spec| spec.state == OfficialMigrationState::Landed)
                        .count()
                        == 1
                    && OFFICIAL_SANDS
                        .iter()
                        .filter(|spec| spec.state == OfficialMigrationState::NativeBehavior)
                        .count()
                        == 7,
                detail: format!(
                    "{} official roots, {} Rust definitions, selected {} is {}",
                    OFFICIAL_SAND_COUNT,
                    window
                        .official_sand_gallery
                        .package()
                        .graph
                        .definitions
                        .len(),
                    window.official_sand_gallery.selected().uid,
                    window.official_sand_gallery.selected().state.label(),
                ),
            },
            #[cfg(feature = "joined-runtime")]
            CefAssertion {
                name: "shared shell Record Conversation and collection definitions render and operate natively",
                passed: {
                    let facts = window.official_runtime.facts(None);
                    facts.edit_mode
                        && facts.group_locked
                        && facts.castle_save_requests > 0
                        && facts.recenter_actions > 0
                        && facts.emitted_record_events >= 3
                        && facts.message_send_requests > 0
                        && facts.conversation_drafts > 1
                        && facts.pinned_conversation_drafts > 0
                        && facts.workflow_action_requests >= 2
                },
                detail: format!("{:?}", window.official_runtime.facts(None)),
            },
            #[cfg(feature = "joined-runtime")]
            CefAssertion {
                name: "configured native domain uses a live Protein subscription",
                passed: {
                    let facts = window.native_domain.facts();
                    facts.endpoint.is_none()
                        || (facts.connection
                            == lince_interface::domain::DomainConnectionState::Live
                            && facts.snapshots >= 3)
                },
                detail: format!("{:?}", window.native_domain.facts()),
            },
            CefAssertion {
                name: "declared installed Action request allowed",
                passed: installed.is_none_or(|surface| surface.actions_allowed > 0),
                detail: format!(
                    "{} Action requests allowed",
                    installed.map_or(0, |surface| surface.actions_allowed)
                ),
            },
            CefAssertion {
                name: "Website bridge absent",
                passed: website.is_none_or(|surface| {
                    surface.bridge_allowed == 0
                        && surface.latest_page_status.contains("bridge absent")
                }),
                detail: website.map_or_else(
                    || "no Website admitted".into(),
                    |surface| {
                        format!(
                            "allowed {}, refused {}",
                            surface.bridge_allowed, surface.bridge_refused
                        )
                    },
                ),
            },
            CefAssertion {
                name: "Website origin storage available without Lince authority",
                passed: website.is_none_or(|surface| {
                    surface.latest_page_status.contains("storage available")
                        && surface.latest_page_status.contains("bridge absent")
                }),
                detail: website.map_or_else(
                    || "no Website admitted".into(),
                    |surface| surface.latest_page_status.clone(),
                ),
            },
            CefAssertion {
                name: "request contexts isolated",
                passed: surfaces
                    .iter()
                    .map(|surface| surface.request_context.partition_key.as_str())
                    .collect::<BTreeSet<_>>()
                    .len()
                    == surfaces.len(),
                detail: format!("{} unique partitions", surfaces.len()),
            },
            CefAssertion {
                name: "CPU paint path unused",
                passed: surfaces.iter().all(|surface| surface.cpu_paints == 0),
                detail: format!(
                    "{} CPU paints",
                    surfaces
                        .iter()
                        .map(|surface| surface.cpu_paints)
                        .sum::<u64>()
                ),
            },
            CefAssertion {
                name: "main documents loaded",
                passed: surfaces
                    .iter()
                    .all(|surface| surface.load_completions > 0 && surface.load_failures == 0),
                detail: format!(
                    "{} completions, {} failures",
                    surfaces
                        .iter()
                        .map(|surface| surface.load_completions)
                        .sum::<u64>(),
                    surfaces
                        .iter()
                        .map(|surface| surface.load_failures)
                        .sum::<u64>()
                ),
            },
            CefAssertion {
                name: "installed persistent storage available",
                passed: installed
                    .is_none_or(|surface| surface.latest_page_status.contains("storage available")),
                detail: installed.map_or_else(
                    || "no installed Sand admitted".into(),
                    |surface| surface.latest_page_status.clone(),
                ),
            },
            CefAssertion {
                name: "installed loopback WebRTC media remains live",
                passed: installed.is_none_or(|surface| {
                    surface.latest_page_status.contains("WebRTC loopback-live")
                }),
                detail: installed.map_or_else(
                    || "no installed Sand admitted".into(),
                    |surface| surface.latest_page_status.clone(),
                ),
            },
            CefAssertion {
                name: "ordinary HTTPS networking exercised",
                passed: surfaces
                    .iter()
                    .all(|surface| surface.external_network_requests > 0),
                detail: format!(
                    "{} requests across {} Sands",
                    surfaces
                        .iter()
                        .map(|surface| surface.external_network_requests)
                        .sum::<u64>(),
                    surfaces.len()
                ),
            },
            CefAssertion {
                name: "installed popup refused",
                passed: installed.is_none_or(|surface| surface.popup_refusals > 0),
                detail: format!(
                    "{} refusals",
                    installed.map_or(0, |surface| surface.popup_refusals)
                ),
            },
            CefAssertion {
                name: "OSR context menus stay inside the Lince-owned surface policy",
                passed: installed.is_none_or(|surface| surface.context_menu_refusals > 0),
                detail: format!(
                    "{} Chromium default menus cleared before native-window fallback",
                    surfaces
                        .iter()
                        .map(|surface| surface.context_menu_refusals)
                        .sum::<u64>()
                ),
            },
            CefAssertion {
                name: "undeclared installed media refused",
                passed: installed
                    .is_none_or(|surface| surface.media_refused > 0 && surface.media_allowed == 0),
                detail: format!(
                    "allowed {}, refused {}",
                    installed.map_or(0, |surface| surface.media_allowed),
                    installed.map_or(0, |surface| surface.media_refused)
                ),
            },
            CefAssertion {
                name: "Website renderer loss restores its document",
                passed: !window.recovery_probe
                    || website.is_none_or(|surface| {
                        surface.renderer_restarts > 0 && surface.load_completions > 1
                    }),
                detail: website.map_or_else(
                    || "no Website admitted".into(),
                    |surface| {
                        format!(
                            "{} renderer restarts, {} load completions",
                            surface.renderer_restarts, surface.load_completions
                        )
                    },
                ),
            },
            CefAssertion {
                name: "CEF notices bundled",
                passed: CEF_NOTICES_BUNDLED.load(Ordering::Acquire),
                detail: "LICENSE.txt and CREDITS.html copied beside laboratory evidence".into(),
            },
            CefAssertion {
                name: "browser processes closed",
                passed: surfaces.iter().all(|surface| surface.closed),
                detail: format!(
                    "{} of {} closed",
                    surfaces.iter().filter(|surface| surface.closed).count(),
                    surfaces.len()
                ),
            },
        ];
        #[cfg(feature = "joined-runtime")]
        let mut assertions = assertions;
        #[cfg(feature = "joined-runtime")]
        {
            let frame_p95 = percentile_millis(&window.joined_frame_interval_nanos, 0.95);
            let cpu_frame_p95 = percentile_millis(&window.joined_cpu_frame_nanos, 0.95);
            let fixed_p95 = percentile_millis(&window.joined_fixed_nanos, 0.95);
            let input_p95 = percentile_millis(&window.joined_input_to_present_call_nanos, 0.95);
            let input_p99 = percentile_millis(&window.joined_input_to_present_call_nanos, 0.99);
            let style_marker =
                window
                    .resolved_style
                    .value("--lynx-accent")
                    .ok()
                    .and_then(|value| match value {
                        StyleValue::Color(value) => {
                            Some(format!("style v{STYLE_CONTRACT_VERSION}:{value}"))
                        }
                        _ => None,
                    });
            assertions.extend([
                CefAssertion {
                    name: "joined Bevy commands handed to Lince host",
                    passed: window.bevy_host.handed_off_command_buffers() > 0,
                    detail: format!(
                        "{} updates, {} command buffers",
                        window.bevy_host.updates(),
                        window.bevy_host.handed_off_command_buffers()
                    ),
                },
                CefAssertion {
                    name: "joined native Sands use one instanced custom WGPU pass",
                    passed: window.node_layer.count() == VISIBLE_NATIVE_SAND_COUNT as u32,
                    detail: format!(
                        "{} visible, {} continuously eligible, {} resident",
                        window.node_layer.count(),
                        window.field_solver.positions().len(),
                        RESIDENT_NODE_COUNT
                    ),
                },
                CefAssertion {
                    name: "joined frame-interval gate",
                    passed: frame_p95 <= 16.67,
                    detail: format!("p95 {frame_p95:.3} ms"),
                },
                CefAssertion {
                    name: "joined redraw-to-present-call CPU gate",
                    passed: cpu_frame_p95 <= 16.67,
                    detail: format!("p95 {cpu_frame_p95:.3} ms"),
                },
                CefAssertion {
                    name: "joined fixed-step gate",
                    passed: fixed_p95 <= 8.0 && window.simulation_accumulator < FIXED_STEP,
                    detail: format!(
                        "{} Hz, {} ticks, {} pending, p95 {fixed_p95:.3} ms",
                        FIXED_STEP_HZ,
                        window.simulation_ticks,
                        window.simulation_accumulator.as_nanos() / FIXED_STEP.as_nanos()
                    ),
                },
                CefAssertion {
                    name: "instrumented input-to-present-call lower-bound gate",
                    passed: window.surfaces.is_empty()
                        || (input_p95 <= 33.4 && input_p99 <= 50.0),
                    detail: format!(
                        "{} samples, p95 {input_p95:.3} ms, p99 {input_p99:.3} ms; physical display timing unavailable",
                        window.joined_input_to_present_call_nanos.len()
                    ),
                },
                CefAssertion {
                    name: "authoritative primitive Sand package and retained Gallery are live",
                    passed: window.primitive_gallery.definitions().len() == PRIMITIVE_COUNT
                        && window.sand_package_sha256.starts_with("sha256:")
                        && window.primitive_gallery.activations() > 0
                        && window.primitive_gallery.emitted_events() > 0,
                    detail: format!(
                        "{} definitions, {}, {} actions, {} events",
                        window.primitive_gallery.definitions().len(),
                        window.sand_package_sha256,
                        window.primitive_gallery.activations(),
                        window.primitive_gallery.emitted_events()
                    ),
                },
                CefAssertion {
                    name: "versioned style cascade updates native and installed HTML projections without reload",
                    passed: window.token_probe_applied
                        && window.token_probe_load_before == window.token_probe_load_after
                        && installed.is_none_or(|surface| {
                            window.style_projection_updates > 0
                                && style_marker.as_ref().is_some_and(|marker| {
                                    surface.latest_page_status.contains(marker)
                                })
                        })
                        && window
                            .resolved_style
                            .origin("--lynx-accent")
                            .is_ok_and(|origin| origin.scope == StyleScope::Workspace)
                        && window
                            .resolved_style
                            .origin("--lynx-density-scale")
                            .is_ok_and(|origin| origin.scope == StyleScope::Group)
                        && window
                            .resolved_style
                            .origin("--lynx-radius-control")
                            .is_ok_and(|origin| origin.scope == StyleScope::Instance),
                    detail: format!(
                        "contract {}, theme {}, mode {}, palette {}, density {}, radius {}, {} resolved tokens, {} HTML updates, CEF loads {:?} to {:?}, HTML {}",
                        STYLE_CONTRACT_VERSION,
                        window.style_gallery.theme(),
                        window.style_gallery.mode(),
                        window.style_gallery.palette_index() + 1,
                        window.style_gallery.density_index() + 1,
                        window.style_gallery.radius_index() + 1,
                        window.resolved_style.values.len(),
                        window.style_projection_updates,
                        window.token_probe_load_before,
                        window.token_probe_load_after,
                        installed.map_or("not admitted", |surface| surface.latest_page_status.as_str())
                    ),
                },
                CefAssertion {
                    name: "off-camera CEF behavior and paint cost are characterized",
                    passed: window.installed().is_none()
                        || (window.off_camera_copy_delta.is_some()
                            && window
                                .off_camera_bridge_delta
                                .is_some_and(|delta| delta > 0)),
                    detail: format!(
                        "copied frames {:?}, bridge events {:?}",
                        window.off_camera_copy_delta, window.off_camera_bridge_delta
                    ),
                },
                CefAssertion {
                    name: "joined native and HTML boundaries publish one AccessKit tree",
                    passed: window.accessibility.node_count()
                        == 10
                            + PRIMITIVE_COUNT
                            + ConfigurationOperation::ALL.len()
                            + OFFICIAL_SAND_COUNT
                            + window.surfaces.len().min(2)
                        && window.accessibility.published_updates() > 0,
                    detail: format!(
                        "{} nodes, {} updates, {} platform tree requests",
                        window.accessibility.node_count(),
                        window.accessibility.published_updates(),
                        window.accessibility.initial_tree_requests()
                    ),
                },
                CefAssertion {
                    name: "external AT-SPI traversal and action probe",
                    passed: !window.accessibility_probe
                        || (window.accessibility.initial_tree_requests() > 0
                            && window.accessibility.actions() > 0),
                    detail: format!(
                        "enabled {}, {} tree requests, {} actions",
                        window.accessibility_probe,
                        window.accessibility.initial_tree_requests(),
                        window.accessibility.actions()
                    ),
                },
                CefAssertion {
                    name: "cold process reaches interactive native window",
                    passed: window.startup_to_window_ready_millis <= 4_000.0,
                    detail: format!("{:.3} ms", window.startup_to_window_ready_millis),
                },
                CefAssertion {
                    name: "induced WGPU device loss rebuilds all host-owned GPU adapters",
                    passed: !window.recovery_probe
                        || (window.device_recovery_attempted && window.device_recovery_completed),
                    detail: format!(
                        "enabled {}, attempted {}, completed {}, {:?} ms",
                        window.recovery_probe,
                        window.device_recovery_attempted,
                        window.device_recovery_completed,
                        window.device_recovery_millis
                    ),
                },
                CefAssertion {
                    name: "joined ownership remains Lince outermost",
                    passed: window.joined_host_submissions >= window.frames.saturating_mul(2),
                    detail: format!(
                        "{} host submissions for {} presented frames",
                        window.joined_host_submissions, window.frames
                    ),
                },
            ]);
        }
        let status = if assertions.iter().all(|assertion| assertion.passed) {
            "passed"
        } else {
            "failed"
        };
        #[cfg(feature = "joined-runtime")]
        let (visible_interactive_sands, continuously_eligible_bodies, resident_light_nodes) = (
            VISIBLE_NATIVE_SAND_COUNT,
            ACTIVE_BODY_COUNT,
            RESIDENT_NODE_COUNT,
        );
        #[cfg(not(feature = "joined-runtime"))]
        let (visible_interactive_sands, continuously_eligible_bodies, resident_light_nodes) =
            (0, 0, 0);
        Self {
            schema_version: 14,
            gate: if cfg!(feature = "joined-runtime") {
                "joined native Interface decision"
            } else {
                "external HTML seam"
            },
            status,
            created_unix_millis: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            git_revision: raw_git_revision(),
            git_dirty: git_dirty(),
            source_fingerprint_sha256: source_fingerprint(),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            frames_presented: window.frames,
            runtime_seconds: window.started.elapsed().as_secs_f64(),
            warmup_seconds: window.warmup_seconds,
            sample_seconds: window.sample_seconds,
            cef_notices_bundled: CEF_NOTICES_BUNDLED.load(Ordering::Acquire),
            cef_count: surfaces.len(),
            host: CefHostReport {
                operating_system: std::env::consts::OS,
                window_system: window_system(),
                architecture: std::env::consts::ARCH,
                cpu_model: cpu_model(),
                memory_kib: memory_kib(),
                power_profile: power_profile(),
                gpu_name: window.gpu_name.clone(),
                gpu_backend: window.gpu_backend.clone(),
                gpu_driver: window.gpu_driver.clone(),
                gpu_driver_info: window.gpu_driver_info.clone(),
                surface_format: format!("{:?}", window.config.format),
                present_mode: window.present_mode.clone(),
                scale_factor: window.window.scale_factor(),
                surface_width: window.config.width,
                surface_height: window.config.height,
                physical_presentation_timing: "Wayland compositor-managed; CPU redraw-to-present-call measured",
                process_to_interactive_window_millis: window.startup_to_window_ready_millis,
                process_to_first_cef_frame_millis: window.first_cef_frame_millis,
                executable_bytes: std::env::current_exe()
                    .ok()
                    .and_then(|path| fs::metadata(path).ok())
                    .map(|metadata| metadata.len()),
                cef_runtime_bytes: cef::sys::get_cef_dir().and_then(|path| directory_bytes(&path)),
                bundled_notice_bytes: directory_bytes(&PathBuf::from(
                    "target/interface-laboratory/cef/notices",
                )),
            },
            workload: CefWorkloadReport {
                version: 1,
                seed: 0x4c494e4345,
                visible_interactive_sands,
                continuously_eligible_bodies,
                resident_light_nodes,
                native_sand_half_extent_ndc: [0.035, 0.024],
                body_shape: "deterministic rectangles and circles",
                moving_ratio: if continuously_eligible_bodies == 0 {
                    0.0
                } else {
                    1.0
                },
                area_set: "force, repel, sort, constraint, immunity, weak centre and mutation preview",
                cef_surface_rects: window.surfaces.iter().map(|surface| surface.rect).collect(),
                cef_content: "installed primitive and recursive-composition package plus ordinary Website HTTPS page",
                instrumentation_overlay: "in-window retained text panel plus window title",
            },
            surfaces,
            assertions,
            #[cfg(feature = "joined-runtime")]
            joined: JoinedReport {
                semantic_instances: window.semantic_instances,
                active_bodies: window.field_solver.positions().len(),
                resident_nodes: RESIDENT_NODE_COUNT,
                visible_native_sands: window.node_layer.count(),
                bevy_updates: window.bevy_host.updates(),
                bevy_command_buffers_handed_to_host: window.bevy_host.handed_off_command_buffers(),
                host_queue_submissions: window.joined_host_submissions,
                frame_samples: window.joined_frame_interval_nanos.len(),
                frame_p50_millis: percentile_millis(&window.joined_frame_interval_nanos, 0.50),
                frame_p95_millis: percentile_millis(&window.joined_frame_interval_nanos, 0.95),
                frame_p99_millis: percentile_millis(&window.joined_frame_interval_nanos, 0.99),
                frame_hitches_over_50_millis: window
                    .joined_frame_interval_nanos
                    .iter()
                    .filter(|nanos| **nanos > 50_000_000)
                    .count(),
                cpu_frame_samples: window.joined_cpu_frame_nanos.len(),
                cpu_frame_p50_millis: percentile_millis(&window.joined_cpu_frame_nanos, 0.50),
                cpu_frame_p95_millis: percentile_millis(&window.joined_cpu_frame_nanos, 0.95),
                cpu_frame_p99_millis: percentile_millis(&window.joined_cpu_frame_nanos, 0.99),
                fixed_step_p95_millis: percentile_millis(&window.joined_fixed_nanos, 0.95),
                fixed_step_hz: FIXED_STEP_HZ,
                fixed_step_ticks: window.simulation_ticks,
                fixed_step_backlog_steps: (window.simulation_accumulator.as_nanos()
                    / FIXED_STEP.as_nanos()) as u64,
                input_to_present_call_samples: window.joined_input_to_present_call_nanos.len(),
                input_to_present_call_p50_millis: percentile_millis(
                    &window.joined_input_to_present_call_nanos,
                    0.50,
                ),
                input_to_present_call_p95_millis: percentile_millis(
                    &window.joined_input_to_present_call_nanos,
                    0.95,
                ),
                input_to_present_call_p99_millis: percentile_millis(
                    &window.joined_input_to_present_call_nanos,
                    0.99,
                ),
                frame_interval_nanos: window.joined_frame_interval_nanos.clone(),
                cpu_frame_nanos: window.joined_cpu_frame_nanos.clone(),
                fixed_step_nanos: window.joined_fixed_nanos.clone(),
                input_to_present_call_nanos: window.joined_input_to_present_call_nanos.clone(),
                off_camera_copied_frame_delta: window.off_camera_copy_delta,
                off_camera_bridge_event_delta: window.off_camera_bridge_delta,
                off_camera_paint_suppression_effective: window
                    .off_camera_copy_delta
                    .is_some_and(|delta| delta <= 2),
                accessibility_nodes: window.accessibility.node_count(),
                accessibility_initial_tree_requests: window.accessibility.initial_tree_requests(),
                accessibility_actions: window.accessibility.actions(),
                accessibility_deactivations: window.accessibility.deactivations(),
                accessibility_updates: window.accessibility.published_updates(),
                accessibility_probe: window.accessibility_probe,
                scenario_count: SCENARIOS.len(),
                visible_panel_controls: [
                    "F1 workspace palette",
                    "F2 group density",
                    "F3 instance radius",
                    "F4 partial theme",
                    "F5 native or HTML focus",
                    "F6 scenario",
                    "F7 mode",
                    "F8 report",
                    "F9 primitive visual state",
                    "F10 recursive composition workbench",
                    "F11 Configuration Sand",
                    "F12 official Sand migration catalog",
                    "Enter opens an available official native runtime",
                    "Tab and Enter primitive or composition interaction",
                ],
                sand_schema_version: SAND_SCHEMA_VERSION,
                sand_abi_version: SAND_ABI_VERSION,
                primitive_definition_count: window.primitive_gallery.definitions().len(),
                primitive_package_sha256: window.sand_package_sha256.clone(),
                primitive_gallery_focus: window.primitive_gallery.focused().uid.into(),
                primitive_gallery_state: format!("{:?}", window.primitive_gallery.state()),
                primitive_gallery_actions: window.primitive_gallery.activations(),
                primitive_gallery_events: window.primitive_gallery.emitted_events(),
                composition_schema_version: COMPOSITION_SCHEMA_VERSION,
                composition_definition_count: window
                    .composition_workbench
                    .host()
                    .catalog
                    .active
                    .len(),
                composition_package_sha256: window.composition_package_sha256.clone(),
                composition_selected_operation: window
                    .composition_workbench
                    .selected()
                    .label()
                    .into(),
                composition_facts: window.composition_workbench.facts(),
                composition_arrows: window.composition_workbench.arrow_lines(),
                configuration_schema_version: CONFIGURATION_SCHEMA_VERSION,
                configuration_definition_count: window
                    .configuration_workbench
                    .artifact()
                    .composition
                    .catalog
                    .active
                    .len(),
                configuration_package_sha256: window.configuration_package_sha256.clone(),
                configuration_selected_operation: window
                    .configuration_workbench
                    .selected()
                    .label()
                    .into(),
                configuration_facts: window.configuration_workbench.facts().clone(),
                configuration_mode: window
                    .configuration_workbench
                    .artifact()
                    .configuration
                    .mode
                    .clone(),
                configuration_launch_receipts: window
                    .configuration_workbench
                    .artifact()
                    .configuration
                    .launches
                    .len(),
                configuration_persisted_bytes: window
                    .configuration_workbench
                    .facts()
                    .persisted_bytes,
                configuration_probe_applied: window.configuration_probe_applied,
                configuration_probe_reverted: window.configuration_probe_reverted,
                configuration_probe_cef_load_before: window.configuration_probe_load_before,
                configuration_probe_cef_load_after: window.configuration_probe_load_after,
                official_sand_count: OFFICIAL_SAND_COUNT,
                official_definition_count: window
                    .official_sand_gallery
                    .package()
                    .graph
                    .definitions
                    .len(),
                official_package_sha256: window.official_sand_package_sha256.clone(),
                official_selected: window.official_sand_gallery.selected().uid.into(),
                official_selected_state: window
                    .official_sand_gallery
                    .selected()
                    .state
                    .label()
                    .into(),
                official_runtime_facts: window.official_runtime.facts(None),
                native_domain_facts: window.native_domain.facts(),
                token_probe_applied: window.token_probe_applied,
                token_probe_cef_load_before: window.token_probe_load_before,
                token_probe_cef_load_after: window.token_probe_load_after,
                style_contract_version: STYLE_CONTRACT_VERSION,
                style_theme: window.style_gallery.theme().into(),
                style_mode: window.style_gallery.mode().into(),
                style_projection_updates: window.style_projection_updates,
                style_css_projection_bytes: window.resolved_style.css_declarations().len(),
                resolved_style: window.resolved_style.clone(),
                button_instance_radius_px: window
                    .resolved_style
                    .length_px("--lynx-radius-control")
                    .unwrap_or_default(),
                recovery_probe: window.recovery_probe,
                device_recovery_attempted: window.device_recovery_attempted,
                device_recovery_completed: window.device_recovery_completed,
                device_recovery_millis: window.device_recovery_millis,
                browser_parity_report: "target/interface-laboratory/parity/report.json",
                spatial_report: "target/interface-laboratory/spatial/report.json",
                ownership: [
                    "winit: event loop and window",
                    "wgpu: host device and final swapchain",
                    "Bevy: manual world command producer",
                    "Lince retained pass: native Sands and chrome",
                    "CEF: isolated accelerated HTML producer",
                ],
            },
        }
    }
}

#[cfg(feature = "joined-runtime")]
fn percentile_millis(values: &[u64], percentile: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index] as f64 / 1_000_000.0
}

#[cfg(feature = "joined-runtime")]
fn elapsed_nanos(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

fn surface_report(shared: &SurfaceShared) -> CefSurfaceReport {
    let facts = copy_facts(shared);
    let latest_error = shared
        .gpu
        .lock()
        .ok()
        .and_then(|gpu| gpu.last_error().map(str::to_owned));
    CefSurfaceReport {
        authority: format!("{:?}", shared.manifest.authority()),
        origin: shared.manifest.origin().into(),
        request_context: shared.manifest.request_context(),
        gpu: VulkanCopyReport {
            received_frames: facts.received_frames,
            copied_frames: facts.copied_frames,
            composed_frames: facts.composed_frames,
            refused_frames: facts.refused_frames,
            latest_width: facts.latest_width,
            latest_height: facts.latest_height,
            latest_modifier: facts.latest_modifier,
            latest_error,
        },
        bridge_allowed: shared.bridge_allowed.load(Ordering::Relaxed),
        events_allowed: shared.events_allowed.load(Ordering::Relaxed),
        actions_allowed: shared.actions_allowed.load(Ordering::Relaxed),
        bridge_refused: shared.bridge_refused.load(Ordering::Relaxed),
        latest_bridge_sequence: shared.latest_bridge_sequence.load(Ordering::Relaxed),
        cpu_paints: shared.cpu_paints.load(Ordering::Relaxed),
        external_network_requests: shared.external_network_requests.load(Ordering::Relaxed),
        load_completions: shared.load_completions.load(Ordering::Relaxed),
        load_failures: shared.load_failures.load(Ordering::Relaxed),
        popup_refusals: shared.popup_refusals.load(Ordering::Relaxed),
        context_menu_refusals: shared.context_menu_refusals.load(Ordering::Relaxed),
        media_allowed: shared.media_allowed.load(Ordering::Relaxed),
        media_refused: shared.media_refused.load(Ordering::Relaxed),
        renderer_restarts: shared.renderer_restarts.load(Ordering::Relaxed),
        closed: shared.closed.load(Ordering::Acquire),
        latest_status: shared
            .latest_status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| "status lock failed".into()),
        latest_page_status: shared
            .latest_page_status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| "page status lock failed".into()),
    }
}

pub fn run_native_interface(website_url: Option<String>, domain_url: Option<String>) {
    let process_started = Instant::now();
    let cef_guard = match initialize_cef() {
        Ok(Some(guard)) => guard,
        Ok(None) => return,
        Err(error) => {
            eprintln!("CEF diagnostic unavailable: {error}");
            return;
        }
    };
    #[cfg(target_os = "linux")]
    let event_loop_result = EventLoop::builder().with_wayland().build();
    #[cfg(not(target_os = "linux"))]
    let event_loop_result = EventLoop::new();
    let event_loop = match event_loop_result {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("CEF diagnostic event loop unavailable: {error}");
            return;
        }
    };
    let export_and_exit = std::env::args().any(|argument| {
        argument == "--export-report-and-exit" || argument == "--joined-report-and-exit"
    });
    let report_path = cli_value("--report").map_or_else(
        || {
            if cfg!(feature = "joined-runtime") {
                PathBuf::from("target/interface-laboratory/joined/report.json")
            } else {
                PathBuf::from("target/interface-laboratory/html/report.json")
            }
        },
        PathBuf::from,
    );
    let sample_seconds = cli_value("--sample-seconds")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(12)
        .max(5);
    let warmup_seconds = cli_value("--warmup-seconds")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(if cfg!(feature = "joined-runtime") {
            2
        } else {
            0
        });
    let cef_count = cli_value("--cef-count")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2)
        .min(12);
    let recovery_probe = std::env::args().any(|argument| argument == "--recovery-probe");
    let accessibility_probe = std::env::args().any(|argument| argument == "--accessibility-probe");
    let mut application = DiagnosticApplication {
        state: None,
        export_and_exit,
        report_path,
        sample_seconds,
        warmup_seconds,
        cef_count,
        recovery_probe,
        accessibility_probe,
        process_started,
        website_url: website_url.unwrap_or_else(|| DEFAULT_WEBSITE_URL.into()),
        domain_url,
    };
    if let Err(error) = event_loop.run_app(&mut application) {
        eprintln!("CEF diagnostic failed: {error}");
    }
    drop(application.state.take());
    drop(cef_guard);
}

fn cli_value(name: &str) -> Option<String> {
    let mut arguments = std::env::args();
    while let Some(argument) = arguments.next() {
        if argument == name {
            return arguments.next();
        }
    }
    None
}

fn directory_bytes(path: &std::path::Path) -> Option<u64> {
    let mut total = 0_u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).ok()? {
            let entry = entry.ok()?;
            let file_type = entry.file_type().ok()?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata().ok()?.len());
            }
        }
    }
    Some(total)
}

fn cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("model name\t:")
                    .or_else(|| line.strip_prefix("Model\t\t:"))
                    .map(str::trim)
                    .map(str::to_owned)
            })
        })
        .unwrap_or_else(|| "unavailable".into())
}

fn memory_kib() -> Option<u64> {
    fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemTotal:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse().ok())
        })
}

fn power_profile() -> String {
    [
        "/sys/firmware/acpi/platform_profile",
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
    ]
    .into_iter()
    .find_map(|path| fs::read_to_string(path).ok())
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| "unavailable".into())
}

fn window_system() -> &'static str {
    #[cfg(target_os = "linux")]
    return "wayland";
    #[cfg(target_os = "windows")]
    return "win32";
    #[cfg(target_os = "macos")]
    return "appkit";
    #[allow(unreachable_code)]
    "unsupported"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_are_derived_from_trusted_frame_urls() {
        assert_eq!(
            observed_origin(INSTALLED_URL).as_deref(),
            Some(INSTALLED_ORIGIN)
        );
        assert_eq!(
            observed_origin(DEFAULT_WEBSITE_URL).as_deref(),
            Some("https://example.com")
        );
        assert_eq!(observed_origin("data:text/html,nope"), None);
    }

    #[test]
    fn media_grants_are_exact_and_origin_bound() {
        let manifest = HtmlSurfaceManifest::Installed {
            partition_key: "media-test".into(),
            package_id: "media-test".into(),
            origin: INSTALLED_ORIGIN.into(),
            grants: InstalledGrants {
                protein_read: false,
                events: BTreeSet::new(),
                actions: BTreeSet::new(),
                persistent_storage: false,
                media: BTreeSet::from([MediaGrant::Camera]),
            },
        };
        let camera = MediaAccessPermissionTypes::DEVICE_VIDEO_CAPTURE.get_raw();
        let microphone = MediaAccessPermissionTypes::DEVICE_AUDIO_CAPTURE.get_raw();
        assert!(media_is_allowed(&manifest, INSTALLED_ORIGIN, camera));
        assert!(!media_is_allowed(&manifest, INSTALLED_ORIGIN, microphone));
        assert!(!media_is_allowed(&manifest, "https://example.com", camera));
        assert!(!media_is_allowed(
            &manifest,
            INSTALLED_ORIGIN,
            camera | 1 << 31
        ));
    }

    #[test]
    fn ime_byte_ranges_convert_to_utf16() {
        let range = utf16_range("a🦁b", 1, 5).unwrap();
        assert_eq!(range.from, 1);
        assert_eq!(range.to, 3);
        assert!(utf16_range("a🦁b", 2, 5).is_none());
    }
}
