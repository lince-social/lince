use lince_interface::host::{OutputId, SurfaceId, WindowRequest, WindowState};
use smithay::{
    delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm,
    delegate_viewporter,
    input::{Seat, SeatHandler, SeatState, pointer::CursorImageStatus},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::wayland_server::{
        Client, Display, DisplayHandle, ListeningSocket, Resource,
        backend::{ClientData, ClientId, DisconnectReason, protocol::ProtocolError},
        protocol::{
            wl_buffer::WlBuffer,
            wl_callback::WlCallback,
            wl_seat::WlSeat,
            wl_surface::{self, WlSurface},
        },
    },
    utils::{Logical, Rectangle, Serial, Transform},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
            RegionAttributes, SubsurfaceCachedState, SurfaceAttributes, TraversalAction,
            get_parent, is_sync_subsurface, with_states, with_surface_tree_downward,
        },
        output::OutputHandler,
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
                set_data_device_focus,
            },
        },
        shell::xdg::{
            self, PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
        shm::{ShmHandler, ShmState},
        viewporter::{ViewportCachedState, ViewporterState, ensure_viewport_valid},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    ffi::OsStr,
    io,
    os::{
        fd::OwnedFd,
        unix::{fs::MetadataExt, net::UnixStream},
    },
    path::PathBuf,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

const MAX_CLIENTS: usize = 128;
const MAX_SURFACES: usize = 4096;
const MAX_WINDOWS: usize = 512;
const MAX_BUFFER_BYTES: u64 = 1024 * 1024 * 1024;

pub use input::KeyEvent;
pub use layers::LayerSnapshot;

mod decorations;
mod outputs;
pub use outputs::Monitor;
mod lock;
mod popups;
mod positioner;
pub use popups::PopupSnapshot;
mod activation;
mod foreign;
mod management;
mod workspaces;
pub use workspaces::{WorkspaceEntry, WorkspaceSnapshot};
mod output_control;
mod capture;
pub use capture::{CaptureJob, CaptureTarget};
pub use output_control::{OutputHead, OutputMode, OutputSettings, OutputRequest};
#[cfg(feature = "session")]
pub(crate) use output_control::validate_catalog as validate_output_catalog;
mod extensions;
mod constraints;
mod damage;
mod idle;
mod ime;

pub struct ClientBuffer {
    backing: Arc<BufferBacking>,
    released: AtomicBool,
    pub version: u64,
    pub size: [i32; 2],
    #[cfg(feature = "native-presentation")]
    cache: Mutex<Option<Arc<dyn std::any::Any + Send + Sync>>>,
}

impl ClientBuffer {
    #[cfg(feature = "native-presentation")]
    pub(crate) fn cached<T: std::any::Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()?
            .clone()
            .downcast()
            .ok()
    }

    #[cfg(feature = "native-presentation")]
    pub(crate) fn cache<T: std::any::Any + Send + Sync>(&self, content: T) {
        *self.cache.lock().unwrap_or_else(|error| error.into_inner()) = Some(Arc::new(content));
    }

    #[cfg(feature = "native-presentation")]
    pub(crate) fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
    pub fn buffer(&self) -> &WlBuffer {
        &self.backing.buffer
    }

    pub fn release(&self) {
        if !self.released.swap(true, Ordering::AcqRel) {
            let mut uses = self
                .backing
                .uses
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            *uses -= 1;
            if *uses == 0 && self.backing.buffer.is_alive() {
                self.backing.buffer.release();
            }
        }
    }
}

struct BufferBacking {
    buffer: WlBuffer,
    uses: Mutex<usize>,
}

impl Drop for ClientBuffer {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Clone)]
pub struct SurfaceNode {
    pub damage: smithay::backend::renderer::utils::DamageSnapshot<i32, smithay::utils::Buffer>,
    pub opaque: Option<RegionAttributes>,
    pub surface: WlSurface,
    pub content: Arc<ClientBuffer>,
    pub origin: [i32; 2],
    pub scale: i32,
    pub transform: Transform,
    pub input: Option<RegionAttributes>,
    pub layout: SurfaceLayout,
    pub generation: u64,
}

impl SurfaceNode {
    pub fn hit(&self, point: [f64; 2]) -> Option<[f64; 2]> {
        let local = [
            point[0] - f64::from(self.origin[0]),
            point[1] - f64::from(self.origin[1]),
        ];
        (local.iter().all(|value| value.is_finite())
            && self.layout.contains(local)
            && self
                .input
                .as_ref()
                .is_none_or(|region| input_contains(region, local)))
        .then_some(local)
    }
}

struct Callbacks {
    generation: u64,
    callbacks: Vec<WlCallback>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceLayout {
    pub size: [f64; 2],
    pub uv_origin: [f32; 2],
    pub uv_x: [f32; 2],
    pub uv_y: [f32; 2],
}

impl SurfaceLayout {
    fn new(
        size: [i32; 2],
        scale: i32,
        transform: Transform,
        viewport: ViewportCachedState,
    ) -> Option<Self> {
        if scale <= 0 {
            return None;
        }
        let buffer_size: smithay::utils::Size<f64, Logical> =
            (f64::from(size[0]), f64::from(size[1])).into();
        let transformed = transform.transform_size(buffer_size);
        let logical = transformed / f64::from(scale);
        let source = viewport
            .src
            .unwrap_or_else(|| Rectangle::from_size(logical));
        if source.size.w <= 0.0
            || source.size.h <= 0.0
            || !Rectangle::from_size(logical).contains_rect(source)
        {
            return None;
        }
        let destination = viewport
            .dst
            .map(|size| size.to_f64())
            .unwrap_or(source.size);
        if destination.w <= 0.0 || destination.h <= 0.0 {
            return None;
        }
        let uv = |x: f64, y: f64| {
            let point: smithay::utils::Point<f64, Logical> = (
                source.loc.x + source.size.w * x,
                source.loc.y + source.size.h * y,
            )
                .into();
            let point = point.to_buffer(f64::from(scale), transform, &logical);
            [
                (point.x / buffer_size.w) as f32,
                (point.y / buffer_size.h) as f32,
            ]
        };
        let uv_origin = uv(0.0, 0.0);
        let x = uv(1.0, 0.0);
        let y = uv(0.0, 1.0);
        Some(Self {
            size: [destination.w, destination.h],
            uv_origin,
            uv_x: [x[0] - uv_origin[0], x[1] - uv_origin[1]],
            uv_y: [y[0] - uv_origin[0], y[1] - uv_origin[1]],
        })
    }

    pub fn contains(&self, point: [f64; 2]) -> bool {
        (0..2).all(|axis| point[axis] >= 0.0 && point[axis] < self.size[axis])
    }
}

#[derive(Clone)]
pub struct WindowSnapshot {
    pub id: SurfaceId,
    pub surface: WlSurface,
    pub mapped: bool,
    pub title: String,
    pub app_id: String,
    pub parent: Option<SurfaceId>,
    pub geometry: Option<Rectangle<i32, Logical>>,
    pub minimum: [i32; 2],
    pub maximum: [i32; 2],
    pub nodes: Vec<SurfaceNode>,
    pub popups: Vec<PopupSnapshot>,
}

impl WindowSnapshot {
    pub fn bounds(&self) -> Option<Rectangle<f64, Logical>> {
        let mut nodes = self.nodes.iter();
        let node = nodes.next()?;
        let rectangle = |node: &SurfaceNode| {
            Rectangle::new(
                (f64::from(node.origin[0]), f64::from(node.origin[1])).into(),
                (node.layout.size[0], node.layout.size[1]).into(),
            )
        };
        let bounds = nodes.fold(rectangle(node), |bounds, node| {
            bounds.merge(rectangle(node))
        });
        self.geometry
            .map(|geometry| geometry.to_f64())
            .map_or(Some(bounds), |geometry| geometry.intersection(bounds))
    }

    pub fn hit_content(&self, point: [f64; 2]) -> Option<(WlSurface, [f64; 2])> {
        if !self.mapped || point.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let bounds = self.bounds()?;
        if !Rectangle::from_size(bounds.size).contains((point[0], point[1])) {
            return None;
        }
        self.nodes.iter().find_map(|node| {
            node.hit([point[0] + bounds.loc.x, point[1] + bounds.loc.y])
                .map(|local| (node.surface.clone(), local))
        })
    }
}

fn input_contains(region: &RegionAttributes, point: [f64; 2]) -> bool {
    let point = point.map(|value| value.floor() as i64);
    let mut contains = false;
    for (kind, rectangle) in &region.rects {
        let origin = [i64::from(rectangle.loc.x), i64::from(rectangle.loc.y)];
        let size = [i64::from(rectangle.size.w), i64::from(rectangle.size.h)];
        if (0..2).all(|axis| point[axis] >= origin[axis] && point[axis] < origin[axis] + size[axis])
        {
            contains = matches!(kind, smithay::wayland::compositor::RectangleKind::Add);
        }
    }
    contains
}

struct Window {
    minimized: bool,
    role: ToplevelSurface,
    mapped: bool,
    needs_initial: bool,
    configured_size: [i32; 2],
    reply_requested: bool,
}

struct ClientState {
    native_canvas: bool,
    compositor: CompositorClientState,
    native_clipboard: bool,
    capabilities: Vec<crate::launch::Capability>,
    count: Arc<AtomicUsize>,
    counted: AtomicBool,
}

impl ClientState {
    fn release(&self) {
        if self.counted.swap(false, Ordering::AcqRel) {
            self.count.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

impl ClientData for ClientState {
    fn initialized(&self, _: ClientId) {}
    fn disconnected(&self, _: ClientId, _: DisconnectReason) {
        self.release();
    }
}

impl Drop for ClientState {
    fn drop(&mut self) {
        self.release();
    }
}

struct State {
    capture: capture::Capture,
    output_control: output_control::Control,
    workspaces: workspaces::Workspaces,
    management: management::Management,
    ime: ime::Ime,
    native_surface: Option<WlSurface>,
    idle: idle::Idle,
    damage: HashMap<WlSurface, damage::Tracked>,
    handle: DisplayHandle,
    compositor: CompositorState,
    shell: XdgShellState,
    _decorations: smithay::wayland::shell::xdg::decoration::XdgDecorationState,
    shm: ShmState,
    data: DataDeviceState,
    seats: SeatState<Self>,
    seat: Seat<Self>,
    output: Output,
    outputs: BTreeMap<OutputId, outputs::Registered>,
    _outputs: smithay::wayland::output::OutputManagerState,
    surfaces: HashSet<WlSurface>,
    windows: BTreeMap<SurfaceId, Window>,
    buffers: HashMap<WlSurface, Arc<ClientBuffer>>,
    backing: HashMap<WlBuffer, Weak<BufferBacking>>,
    next_id: u64,
    revision: u64,
    window_revision: u64,
    initial_size: [i32; 2],
    cursor: CursorImageStatus,
    _viewporter: ViewporterState,
    layers: layers::Layers,
    dmabuf: buffers::Buffers,
    buttons: BTreeSet<u32>,
    generations: HashMap<WlSurface, u64>,
    callbacks: HashMap<WlSurface, VecDeque<Callbacks>>,
    requests: BTreeMap<SurfaceId, WindowRequest>,
    popups: popups::Popups,
    authority: VecDeque<input::Authority>,
    presented_geometry: Option<lince_interface::host::GeometrySnapshot>,
    presented_focus: Option<OutputId>,
    activation: smithay::wayland::xdg_activation::XdgActivationState,
    activations: VecDeque<(SurfaceId, Instant, u64)>,
    input_epoch: u64,
    lock: lock::Lock,
    drag_icon: Option<WlSurface>,
    dragging: bool,
    data_control: smithay::wayland::selection::wlr_data_control::DataControlState,
    foreign: smithay::wayland::foreign_toplevel_list::ForeignToplevelListState,
    foreign_handles: BTreeMap<SurfaceId, smithay::wayland::foreign_toplevel_list::ForeignToplevelHandle>,
    _relative_pointer: smithay::wayland::relative_pointer::RelativePointerManagerState,
    shortcuts: smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitState,
    shortcut_inhibitors: Vec<smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitor>,
    _pointer_constraints: smithay::wayland::pointer_constraints::PointerConstraintsState,
    controls_suppressed: Option<WlSurface>,
    _fractional: smithay::wayland::fractional_scale::FractionalScaleManagerState,
}

pub struct Server {
    #[cfg(feature = "nested")]
    native_client: Option<crate::native_input::client::Client>,
    display: Display<State>,
    state: State,
    listener: Option<ListeningSocket>,
    clients: Arc<AtomicUsize>,
    started: Instant,
}

impl Server {
    #[cfg(feature = "nested")]
    pub(crate) fn event_fds(&mut self) -> io::Result<Vec<OwnedFd>> {
        use std::os::fd::AsFd;
        let mut fds = vec![self.display.backend().poll_fd().try_clone_to_owned()?];
        if let Some(listener) = &self.listener {
            fds.push(listener.as_fd().try_clone_to_owned()?);
        }
        if let Some(client) = &self.native_client { fds.extend(client.descriptors()?); }
        Ok(fds)
    }

    pub fn output(&self, id: OutputId) -> Option<Output> {
        self.state
            .outputs
            .get(&id)
            .map(|registered| registered.output.clone())
    }

    pub fn new(size: [i32; 2]) -> io::Result<Self> {
        validate_size(size)?;
        let display =
            Display::<State>::new().map_err(|error| io::Error::other(error.to_string()))?;
        let handle = display.handle();
        let data_control = smithay::wayland::selection::wlr_data_control::DataControlState::new::<
            State,
            _,
        >(&handle, None, |client| {
            client
                .get_data::<ClientState>()
                .is_some_and(|data| data.native_clipboard || data.capabilities.contains(&crate::launch::Capability::Clipboard))
        });
        let mut seats = SeatState::new();
        let mut seat = seats.new_wl_seat(&handle, "park");
        seat.add_keyboard(Default::default(), 25, 500)
            .map_err(|error| io::Error::other(error.to_string()))?;
        seat.add_pointer();
        let output = Output::new(
            "Park".into(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Lince".into(),
                model: "Park".into(),
            },
        );
        let mode = Mode {
            size: (size[0], size[1]).into(),
            refresh: 60_000,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            None,
            Some((0, 0).into()),
        );
        output.set_preferred(mode);
        let global = output.create_global::<State>(&handle);
        let outputs = BTreeMap::from([(
            OutputId(1),
            outputs::Registered {
                output: output.clone(),
                global,
            },
        )]);
        let state = State {
            capture: capture::Capture::new(&display.handle()),
            output_control: output_control::Control::new(&display.handle()),
            workspaces: workspaces::Workspaces::new(&display.handle()),
            management: management::Management::new(&display.handle()),
            damage: HashMap::new(),
            compositor: CompositorState::new::<State>(&handle),
            shell: XdgShellState::new::<State>(&handle),
            _decorations: smithay::wayland::shell::xdg::decoration::XdgDecorationState::new::<State>(
                &handle,
            ),
            _outputs: smithay::wayland::output::OutputManagerState::new_with_xdg_output::<State>(
                &handle,
            ),
            shm: ShmState::new::<State>(&handle, vec![]),
            data: DataDeviceState::new::<State>(&handle),
            handle,
            seats,
            seat,
            output,
            outputs,
            surfaces: HashSet::new(),
            windows: BTreeMap::new(),
            buffers: HashMap::new(),
            backing: HashMap::new(),
            next_id: 1,
            revision: 1,
            window_revision: 1,
            initial_size: size,
            cursor: CursorImageStatus::default_named(),
            _viewporter: ViewporterState::new::<State>(&display.handle()),
            layers: layers::Layers::new(&display.handle()),
            dmabuf: buffers::Buffers::new(),
            buttons: BTreeSet::new(),
            generations: HashMap::new(),
            callbacks: HashMap::new(),
            requests: BTreeMap::new(),
            popups: Default::default(),
            authority: VecDeque::new(),
            presented_geometry: None,
            presented_focus: None,
            activation: smithay::wayland::xdg_activation::XdgActivationState::new::<State>(
                &display.handle(),
            ),
            activations: VecDeque::new(),
            input_epoch: 0,
            lock: lock::Lock::new(&display.handle()),
            drag_icon: None,
            dragging: false,
            data_control,
            foreign: smithay::wayland::foreign_toplevel_list::ForeignToplevelListState::new_with_filter::<State>(
                &display.handle(),
                |client| client.get_data::<ClientState>().is_some_and(|data| data.capabilities.contains(&crate::launch::Capability::WindowList)),
            ),
            foreign_handles: BTreeMap::new(),
            _relative_pointer: smithay::wayland::relative_pointer::RelativePointerManagerState::new::<State>(&display.handle()),
            shortcuts: smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitState::new::<State>(&display.handle()),
            shortcut_inhibitors: Vec::new(),
            _pointer_constraints: smithay::wayland::pointer_constraints::PointerConstraintsState::new::<State>(&display.handle()),
            controls_suppressed: None,
            _fractional: smithay::wayland::fractional_scale::FractionalScaleManagerState::new::<State>(&display.handle()),
            idle: idle::Idle::new(&display.handle()),
            ime: ime::Ime::new(&display.handle()),
            native_surface: None,
        };
        let server = Self {
            #[cfg(feature = "nested")]
            native_client: None,
            display,
            state,
            listener: None,
            clients: Arc::new(AtomicUsize::new(0)),
            started: Instant::now(),
        };
        #[cfg(feature = "nested")]
        {
            let mut server = server;
            let (stream, client) = UnixStream::pair()?;
            server.display.handle().insert_client(stream, Arc::new(ClientState {
                native_canvas: true, native_clipboard: false, capabilities: Vec::new(),
                compositor: CompositorClientState::default(), count: server.clients.clone(),
                counted: AtomicBool::new(false),
            }))?;
            server.native_client = Some(crate::native_input::client::Client::new(client)?);
            Ok(server)
        }
        #[cfg(not(feature = "nested"))]
        Ok(server)
    }

    pub fn listen(&mut self) -> io::Result<PathBuf> {
        self.listen_on(None)
    }

    #[cfg(any(feature = "session", feature = "clipboard-gate"))]
    pub(crate) fn listen_named(&mut self, name: &str) -> io::Result<PathBuf> {
        if name.is_empty()
            || name.len() > 100
            || !name
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || value == b'-')
        {
            return Err(io::Error::other("Invalid Park display name"));
        }
        self.listen_on(Some(name))
    }

    fn listen_on(&mut self, name: Option<&str>) -> io::Result<PathBuf> {
        if self.listener.is_some() {
            return Err(io::Error::other("Park is already listening"));
        }
        let directory = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|directory| directory.is_absolute())
            .ok_or_else(|| {
                io::Error::other("XDG_RUNTIME_DIR must be an absolute private directory")
            })?;
        let metadata = std::fs::symlink_metadata(&directory)?;
        if !metadata.is_dir()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o077 != 0
        {
            return Err(io::Error::other(
                "XDG_RUNTIME_DIR must be owned by this user with mode 0700",
            ));
        }
        let listener = match name {
            Some(name) => ListeningSocket::bind(name),
            None => ListeningSocket::bind_auto("wayland-park", 0..64),
        }
        .map_err(|error| io::Error::other(error.to_string()))?;
        let path = directory.join(
            listener
                .socket_name()
                .ok_or_else(|| io::Error::other("the Wayland socket has no name"))?,
        );
        self.listener = Some(listener);
        Ok(path)
    }

    pub fn socket_name(&self) -> Option<&OsStr> {
        self.listener.as_ref()?.socket_name()
    }

    pub fn insert_client(&mut self, stream: UnixStream) -> io::Result<Client> {
        let native_clipboard = rustix::net::sockopt::socket_peercred(&stream).is_ok_and(
            |credentials| credentials.pid.as_raw_nonzero().get() == std::process::id() as i32,
        );
        self.insert_authorized_client(stream, Vec::new(), native_clipboard)
    }

    fn insert_authorized_client(
        &mut self,
        stream: UnixStream,
        capabilities: Vec<crate::launch::Capability>,
        native_clipboard: bool,
    ) -> io::Result<Client> {
        if self.clients.load(Ordering::Acquire) >= MAX_CLIENTS {
            return Err(io::Error::other("Park client limit reached"));
        }
        self.clients.fetch_add(1, Ordering::AcqRel);
        let data = Arc::new(ClientState {
            native_canvas: false,
            compositor: CompositorClientState::default(),
            native_clipboard,
            capabilities,
            count: self.clients.clone(),
            counted: AtomicBool::new(true),
        });
        self.display.handle().insert_client(stream, data)
    }

    pub fn spawn_helper(
        &mut self,
        description: &crate::launch::Description,
        socket: &std::path::Path,
    ) -> io::Result<std::process::Child> {
        let application = description.application()?;
        if description.capabilities.is_empty() {
            return application.spawn(socket);
        }
        let (server, child) = UnixStream::pair()?;
        let client = self.insert_authorized_client(server, description.capabilities.clone(), false)?;
        match application.spawn_connected(socket, child) {
            Ok(child) => Ok(child),
            Err(error) => {
                client.kill(&self.state.handle, ProtocolError {
                    code: 1, object_id: 1, object_interface: "wl_display".into(),
                    message: "Authorized helper failed to start".into(),
                });
                Err(error)
            }
        }
    }

    pub fn dispatch(&mut self) -> io::Result<()> {
        #[cfg(feature = "nested")]
        if let Some(client) = &mut self.native_client { client.dispatch()?; }
        for _ in 0..16 {
            let stream = match &self.listener {
                Some(listener) => listener.accept()?,
                None => None,
            };
            let Some(stream) = stream else {
                break;
            };
            if self.clients.load(Ordering::Acquire) < MAX_CLIENTS {
                self.insert_client(stream)?;
            }
        }
        self.display.dispatch_clients(&mut self.state)?;
        self.state.sync_foreign();
        self.state.refresh_capture();
        self.state.refresh_shortcut_inhibitors();
        self.state.refresh_constraints();
        self.state.refresh_idle();
        self.state.popups.manager.cleanup();
        self.state.reconstrain_popups();
        for output in self.state.outputs.values() {
            smithay::desktop::layer_map_for_output(&output.output).cleanup();
        }
        self.display.flush_clients()?;
        #[cfg(feature = "nested")]
        if let Some(client) = &mut self.native_client { client.dispatch()?; }
        Ok(())
    }

    pub fn revision(&self) -> u64 {
        self.state.revision
    }

    pub fn window_revision(&self) -> u64 {
        self.state.window_revision
    }

    pub fn windows(&self) -> Vec<WindowSnapshot> {
        self.state
            .windows
            .iter()
            .filter(|(_, window)| window.role.wl_surface().is_alive())
            .map(|(id, window)| {
                let surface = window.role.wl_surface();
                let (title, app_id, client_state) = with_states(surface, |states| {
                    let metadata = states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    let state = *states
                        .cached_state
                        .get::<xdg::SurfaceCachedState>()
                        .current();
                    (
                        bounded(metadata.title.as_deref().unwrap_or_default()),
                        bounded(metadata.app_id.as_deref().unwrap_or_default()),
                        state,
                    )
                });
                let nodes = self.state.nodes(surface);
                WindowSnapshot {
                    id: *id,
                    surface: surface.clone(),
                    mapped: window.mapped,
                    title,
                    app_id,
                    parent: window
                        .role
                        .parent()
                        .and_then(|parent| self.state.id(&parent)),
                    geometry: client_state.geometry,
                    minimum: [client_state.min_size.w, client_state.min_size.h],
                    maximum: [client_state.max_size.w, client_state.max_size.h],
                    nodes,
                    popups: self.state.popup_snapshots(surface),
                }
            })
            .collect()
    }

    pub fn configure(&mut self, id: SurfaceId, size: [i32; 2]) -> io::Result<()> {
        validate_size(size)?;
        let window = self
            .state
            .windows
            .get_mut(&id)
            .ok_or_else(|| io::Error::other("the application window is unavailable"))?;
        if window.configured_size != size {
            window.configured_size = size;
            if window.needs_initial {
                return Ok(());
            }
            window
                .role
                .with_pending_state(|state| state.size = Some((size[0], size[1]).into()));
            window.role.send_configure();
        }
        Ok(())
    }

    pub fn close(&mut self, id: SurfaceId) {
        if let Some(window) = self.state.windows.get(&id) {
            window.role.send_close();
        }
    }

    pub fn requests(&mut self) -> Vec<(SurfaceId, WindowRequest)> {
        self.state
            .requests
            .retain(|id, _| self.state.windows.contains_key(id));
        let ready: Vec<_> = self
            .state
            .requests
            .keys()
            .filter(|id| self.state.windows[id].mapped)
            .copied()
            .collect();
        ready
            .into_iter()
            .filter_map(|id| self.state.requests.remove(&id).map(|request| (id, request)))
            .collect()
    }

    pub fn configure_state(
        &mut self,
        id: SurfaceId,
        size: [i32; 2],
        state: WindowState,
    ) -> io::Result<()> {
        validate_size(size)?;
        use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as XdgState;
        let window = self
            .state
            .windows
            .get_mut(&id)
            .ok_or_else(|| io::Error::other("the application window is unavailable"))?;
        window.configured_size = size;
        window.minimized = state.minimized;
        window.role.with_pending_state(|pending| {
            pending.size = Some((size[0], size[1]).into());
            for (flag, enabled) in [
                (XdgState::Fullscreen, state.fullscreen),
                (XdgState::Maximized, state.maximized),
            ] {
                if enabled {
                    pending.states.set(flag);
                } else {
                    pending.states.unset(flag);
                }
            }
        });
        if !window.needs_initial {
            if std::mem::take(&mut window.reply_requested) {
                window.role.send_configure();
            } else {
                window.role.send_pending_configure();
            }
        }
        Ok(())
    }

    pub fn reject_window(&self, id: SurfaceId) {
        if let Some(window) = self.state.windows.get(&id) {
            self.state.reject(
                window.role.wl_surface(),
                "Park could not import this client's content",
            );
        }
    }

    pub fn reject_surface(&self, surface: &WlSurface) {
        self.state
            .reject(surface, "Park could not import this client's content");
    }

    pub fn focus(&mut self, id: Option<SurfaceId>, serial: Serial) {
        let surface = id
            .and_then(|id| self.state.windows.get(&id))
            .filter(|window| window.mapped)
            .map(|window| window.role.wl_surface().clone());
        self.focus_surface(surface, serial);
    }

    pub fn focus_surface(&mut self, surface: Option<WlSurface>, serial: Serial) {
        let surface = surface.filter(|surface| !self.locked() || self.state.lock_allows(surface));
        if self.popup_grab_active() {
            if surface.as_ref() == self.state.popups.root.as_ref() {
                return;
            }
            self.dismiss_popup_grab();
        }
        let id = surface.as_ref().and_then(|surface| self.state.id(surface));
        if let Some(keyboard) = self.state.seat.get_keyboard() {
            if keyboard.current_focus() == surface {
                return;
            }
            #[cfg(feature = "nested")]
            self.suspend_native_drop();
            keyboard.set_focus(&mut self.state, surface.clone(), serial);
            self.state.update_shortcut_focus(surface.as_ref());
            for (window_id, window) in &self.state.windows {
                window.role.with_pending_state(|state| {
                    if Some(*window_id) == id && window.mapped { state.states.set(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated); }
                    else { state.states.unset(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated); }
                });
                if !window.needs_initial {
                    window.role.send_pending_configure();
                }
            }
        }
    }

    pub fn presented(&mut self, windows: &[SurfaceId]) {
        let nodes: Vec<_> = windows
            .iter()
            .filter_map(|id| self.state.windows.get(id).filter(|window| window.mapped))
            .flat_map(|window| self.state.nodes(window.role.wl_surface()))
            .collect();
        self.presented_nodes(&nodes);
    }

    pub fn presented_nodes(&mut self, nodes: &[SurfaceNode]) {
        let time = self.started.elapsed().as_millis() as u32;
        for node in nodes {
            let Some(pending) = self.state.callbacks.get_mut(&node.surface) else {
                continue;
            };
            while pending
                .front()
                .is_some_and(|batch| batch.generation <= node.generation)
            {
                for callback in pending.pop_front().unwrap().callbacks {
                    if callback.is_alive() {
                        callback.done(time);
                    }
                }
            }
            if pending.is_empty() {
                self.state.callbacks.remove(&node.surface);
            }
        }
    }

    pub fn cursor(&self) -> &CursorImageStatus {
        if self.pointer_locked() {
            return &CursorImageStatus::Hidden;
        }
        &self.state.cursor
    }

    pub fn surface_nodes(&self, surface: &WlSurface) -> Vec<SurfaceNode> {
        self.state.nodes(surface)
    }
}

fn validate_size(size: [i32; 2]) -> io::Result<()> {
    if size.iter().any(|size| !(1..=16384).contains(size)) {
        return Err(io::Error::other("invalid Park surface size"));
    }
    Ok(())
}

fn bounded(value: &str) -> String {
    value.chars().take(1024).collect()
}

impl State {
    fn nodes(&self, surface: &WlSurface) -> Vec<SurfaceNode> {
        let mut nodes = Vec::new();
        with_surface_tree_downward(
            surface,
            [0_i32; 2],
            |_, states, offset| {
                let location = states
                    .cached_state
                    .get::<SubsurfaceCachedState>()
                    .current()
                    .location;
                TraversalAction::DoChildren([
                    offset[0].saturating_add(location.x),
                    offset[1].saturating_add(location.y),
                ])
            },
            |surface, states, offset| {
                let location = states
                    .cached_state
                    .get::<SubsurfaceCachedState>()
                    .current()
                    .location;
                let mut cached = states.cached_state.get::<SurfaceAttributes>();
                let attributes = cached.current();
                if let Some(content) = self.buffers.get(surface) {
                    let viewport = *states.cached_state.get::<ViewportCachedState>().current();
                    let Some(layout) = SurfaceLayout::new(
                        content.size,
                        attributes.buffer_scale,
                        attributes.buffer_transform.into(),
                        viewport,
                    ) else {
                        return;
                    };
                    nodes.push(SurfaceNode {
                        damage: self.damage.get(surface).map(|tracked| tracked.damage.snapshot()).unwrap_or_else(|| smithay::backend::renderer::utils::DamageBag::new(16).snapshot()),
                        opaque: attributes.opaque_region.clone(),
                        surface: surface.clone(),
                        content: content.clone(),
                        origin: [
                            offset[0].saturating_add(location.x),
                            offset[1].saturating_add(location.y),
                        ],
                        scale: attributes.buffer_scale,
                        transform: attributes.buffer_transform.into(),
                        input: attributes.input_region.clone(),
                        layout,
                        generation: self.generations.get(surface).copied().unwrap_or(0),
                    });
                }
            },
            |_, _, _| true,
        );
        nodes
    }

    fn id(&self, surface: &WlSurface) -> Option<SurfaceId> {
        self.windows
            .iter()
            .find(|(_, window)| window.role.wl_surface() == surface)
            .map(|(id, _)| *id)
    }

    fn changed(&mut self) {
        self.presentation_changed();
        self.window_revision = self.revision;
    }

    fn presentation_changed(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    fn valid_surface(&self, surface: &WlSurface, size: [i32; 2]) -> bool {
        with_states(surface, |states| {
            let location = states
                .cached_state
                .get::<SubsurfaceCachedState>()
                .current()
                .location;
            if !(-32768..=32768).contains(&location.x) || !(-32768..=32768).contains(&location.y) {
                surface.post_error(
                    wl_surface::Error::InvalidSize,
                    "Subsurface position exceeds Park's supported bounds",
                );
                return false;
            }
            let mut cache = states.cached_state.get::<SurfaceAttributes>();
            let attributes = cache.current();
            let scale = attributes.buffer_scale;
            if scale <= 0 || size.iter().any(|dimension| dimension % scale != 0) {
                surface.post_error(
                    wl_surface::Error::InvalidSize,
                    "buffer dimensions must be integer multiples of its scale",
                );
                return false;
            }
            let transform: Transform = attributes.buffer_transform.into();
            let dimensions: smithay::utils::Size<i32, smithay::utils::Buffer> =
                (size[0], size[1]).into();
            ensure_viewport_valid(states, dimensions.to_logical(scale, transform))
        })
    }

    fn reject(&self, surface: &WlSurface, message: &str) {
        if let Some(client) = surface.client() {
            client.kill(
                &self.handle,
                ProtocolError {
                    code: 1,
                    object_id: 1,
                    object_interface: "wl_display".into(),
                    message: message.into(),
                },
            );
        }
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, buffer: &WlBuffer) {
        self.backing.remove(buffer);
    }
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        if surface.client().and_then(|client| client.get_data::<ClientState>().map(|data| data.native_canvas)).unwrap_or(false) {
            if self.native_surface.as_ref().is_some_and(Resource::is_alive) {
                self.reject(surface, "The Park native canvas already has a surface");
                return;
            }
            self.native_surface = Some(surface.clone());
        }
        if self.surfaces.len() >= MAX_SURFACES {
            self.reject(surface, "Park surface limit reached");
        } else {
            self.surfaces.insert(surface.clone());
            self.update_surface_scale(surface);
        }
    }

    fn new_subsurface(&mut self, surface: &WlSurface, parent: &WlSurface) {
        let mut current = Some(parent.clone());
        for _ in 0..32 {
            let Some(parent) = current else {
                return;
            };
            current = get_parent(&parent);
        }
        self.reject(surface, "Park subsurface nesting limit reached");
    }

    fn commit(&mut self, surface: &WlSurface) {
        if is_sync_subsurface(surface) {
            return;
        }
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        let id = self.id(&root);
        self.popups.manager.commit(&root);
        let mut unmapped = false;
        let mut assignments = Vec::new();
        let mut invalid = Vec::new();
        let mut callbacks = Vec::new();
        let mut damages = Vec::new();
        with_surface_tree_downward(
            &root,
            (),
            |_, _, _| TraversalAction::DoChildren(()),
            |surface, states, _| {
                let mut cached = states.cached_state.get::<SurfaceAttributes>();
                let attributes = cached.current();
                if attributes.damage.len() > 4096 || attributes.frame_callbacks.len() > 256
                    || attributes
                        .input_region
                        .as_ref()
                        .is_some_and(|region| region.rects.len() > 4096)
                    || attributes
                        .opaque_region
                        .as_ref()
                        .is_some_and(|region| region.rects.len() > 4096)
                {
                    invalid.push(surface.clone());
                }
                let mut damage = std::mem::take(&mut attributes.damage);
                damage.truncate(4096);
                damages.push((surface.clone(), damage));
                if let Some(buffer) = attributes.buffer.take() {
                    assignments.push((surface.clone(), buffer));
                }
                callbacks.push((
                    surface.clone(),
                    std::mem::take(&mut attributes.frame_callbacks),
                ));
            },
            |_, _, _| true,
        );
        for surface in invalid {
            self.reject(&surface, "Park surface state limit reached");
        }
        let committed: Vec<_> = callbacks
            .iter()
            .map(|(surface, _)| surface.clone())
            .collect();
        for (surface, new) in callbacks {
            self.generations.insert(surface.clone(), self.revision);
            if !new.is_empty() {
                let pending = self.callbacks.entry(surface.clone()).or_default();
                let count = pending
                    .iter()
                    .map(|batch| batch.callbacks.len())
                    .sum::<usize>();
                if count + new.len() > 256 {
                    self.reject(&surface, "Park frame callback limit reached");
                } else {
                    pending.push_back(Callbacks {
                        generation: self.revision,
                        callbacks: new,
                    });
                }
            }
        }
        for (surface, assignment) in assignments {
            match assignment {
                BufferAssignment::NewBuffer(buffer) => {
                    if let Some(id) = id
                        && !self.windows[&id].role.ensure_configured()
                    {
                        buffer.release();
                        continue;
                    }
                    if !self.layer_configured(&root) {
                        buffer.release();
                        continue;
                    }
                    if self
                        .popups
                        .surfaces
                        .get(&root)
                        .is_some_and(|popup| !popup.ensure_configured())
                    {
                        buffer.release();
                        continue;
                    }
                    let Some(size) = smithay::backend::renderer::buffer_dimensions(&buffer) else {
                        buffer.release();
                        self.reject(&surface, "unsupported Wayland buffer");
                        continue;
                    };
                    if validate_size([size.w, size.h]).is_err()
                        || i64::from(size.w) * i64::from(size.h) * 4 > 256 * 1024 * 1024
                    {
                        buffer.release();
                        surface.post_error(
                            wl_surface::Error::InvalidSize,
                            "unsupported buffer dimensions",
                        );
                        continue;
                    }
                    let bytes = |size: [i32; 2]| size[0] as u64 * size[1] as u64 * 4;
                    let retained = self
                        .buffers
                        .iter()
                        .filter(|(existing, _)| *existing != &surface)
                        .map(|(_, buffer)| bytes(buffer.size))
                        .sum::<u64>();
                    if retained + bytes([size.w, size.h]) > MAX_BUFFER_BYTES {
                        buffer.release();
                        self.reject(&surface, "Park application buffer memory limit reached");
                        continue;
                    }
                    if !self.valid_surface(&surface, [size.w, size.h]) {
                        buffer.release();
                        continue;
                    }
                    let backing = self
                        .backing
                        .get(&buffer)
                        .and_then(Weak::upgrade)
                        .unwrap_or_else(|| {
                            let backing = Arc::new(BufferBacking {
                                buffer: buffer.clone(),
                                uses: Mutex::new(0),
                            });
                            self.backing.insert(buffer, Arc::downgrade(&backing));
                            backing
                        });
                    *backing
                        .uses
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) += 1;
                    self.buffers.insert(
                        surface.clone(),
                        Arc::new(ClientBuffer {
                            backing,
                            released: AtomicBool::new(false),
                            version: self.revision,
                            size: [size.w, size.h],
                            #[cfg(feature = "native-presentation")]
                            cache: Mutex::new(None),
                        }),
                    );
                    if let Some(id) = id
                        && surface == root
                    {
                        let window = self.windows.get_mut(&id).unwrap();
                        window.mapped = true;
                        window.needs_initial = false;
                        if !self.outputs.is_empty() {
                            self.output.enter(&root);
                        }
                    }
                }
                BufferAssignment::Removed => {
                    let attached = self.buffers.remove(&surface).is_some();
                    if surface == root && attached {
                        unmapped = true;
                        self.dismiss_children(&root);
                    }
                    if let Some(id) = id
                        && surface == root
                        && self.windows[&id].mapped
                    {
                        let window = self.windows.get_mut(&id).unwrap();
                        window.mapped = false;
                        window.minimized = false;
                        window.reply_requested = false;
                        self.requests.remove(&id);
                        window.needs_initial = true;
                        window.role.reset_initial_configure_sent();
                        with_states(&root, |states| {
                            *states
                                .data_map
                                .get::<XdgToplevelSurfaceData>()
                                .unwrap()
                                .lock()
                                .unwrap_or_else(|error| error.into_inner()) = Default::default();
                            *states
                                .cached_state
                                .get::<xdg::SurfaceCachedState>()
                                .current() = Default::default();
                            *states
                                .cached_state
                                .get::<xdg::SurfaceCachedState>()
                                .pending() = Default::default();
                        });
                        for output in self.outputs.values() {
                            output.output.leave(&root);
                        }
                    }
                }
            }
        }
        for (surface, damage) in damages { self.update_damage(&surface, damage); }
        for surface in committed {
            if let Some(buffer) = self.buffers.get(&surface) {
                self.valid_surface(&surface, buffer.size);
            }
        }
        if let Some(id) = id {
            let window = self.windows.get_mut(&id).unwrap();
            if window.needs_initial && !window.mapped && !unmapped {
                window.role.with_pending_state(|state| {
                    state.size = Some((window.configured_size[0], window.configured_size[1]).into())
                });
                window.role.send_configure();
                window.needs_initial = false;
            }
        }
        self.layer_commit(&root, unmapped);
        self.popup_commit(&root);
        if id.is_some() || self.popups.surfaces.contains_key(&root) {
            self.changed();
        } else {
            self.presentation_changed();
        }
    }

    fn destroyed(&mut self, surface: &WlSurface) {
        if self.native_surface.as_ref() == Some(surface) { self.native_surface = None; }
        self.ime.popups.retain(|popup| popup.wl_surface() != surface && popup.alive());
        self.dismiss_children(surface);
        self.surfaces.remove(surface);
        self.buffers.remove(surface);
        self.damage.remove(surface);
        self.generations.remove(surface);
        self.callbacks.remove(surface);
        self.remove_layer(surface);
        self.popups.surfaces.remove(surface);
        if let Some(id) = self.id(surface) {
            self.windows.remove(&id);
            self.requests.remove(&id);
        }
        self.changed();
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.shell
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        if self.windows.len() >= MAX_WINDOWS {
            self.reject(surface.wl_surface(), "Park window limit reached");
            return;
        }
        let Some(next) = self.next_id.checked_add(1) else {
            self.reject(surface.wl_surface(), "Park surface identities exhausted");
            return;
        };
        let id = SurfaceId(self.next_id);
        self.next_id = next;
        let size = self.initial_size;
        self.windows.insert(
            id,
            Window {
                minimized: false,
                role: surface,
                mapped: false,
                needs_initial: true,
                configured_size: size,
                reply_requested: false,
            },
        );
        self.changed();
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        self.add_popup(surface, positioner);
    }
    fn grab(&mut self, surface: PopupSurface, seat: WlSeat, serial: Serial) {
        self.grab_popup(surface, seat, serial);
    }
    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        if self.position_popup(&surface, positioner) {
            surface.send_repositioned(token);
            self.presentation_changed();
        }
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        self.popups.surfaces.remove(surface.wl_surface());
        self.buffers.remove(surface.wl_surface());
        self.presentation_changed();
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.dismiss_children(surface.wl_surface());
        if let Some(id) = self.id(surface.wl_surface()) {
            self.windows.remove(&id);
            self.requests.remove(&id);
        }
        self.buffers.remove(surface.wl_surface());
        self.changed();
    }

    fn app_id_changed(&mut self, _: ToplevelSurface) {
        self.changed();
    }
    fn title_changed(&mut self, _: ToplevelSurface) {
        self.changed();
    }
    fn parent_changed(&mut self, surface: ToplevelSurface) {
        let parent_size = surface
            .parent()
            .and_then(|parent| self.id(&parent))
            .and_then(|id| self.windows.get(&id))
            .map(|window| window.configured_size);
        if let Some(size) = parent_size
            && let Some(window) = self
                .id(surface.wl_surface())
                .and_then(|id| self.windows.get_mut(&id))
            && window.needs_initial
        {
            window.configured_size = [size[0].saturating_mul(3) / 4, size[1].saturating_mul(3) / 4]
                .map(|size| size.max(1));
        }
        self.changed();
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self.id(surface.wl_surface()) {
            self.windows.get_mut(&id).unwrap().reply_requested = true;
            self.requests.entry(id).or_default().maximized = Some(true);
        }
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self.id(surface.wl_surface()) { self.requests.entry(id).or_default().minimized = Some(true); }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self.id(surface.wl_surface()) {
            self.windows.get_mut(&id).unwrap().reply_requested = true;
            self.requests.entry(id).or_default().maximized = Some(false);
        }
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
    ) {
        if let Some(id) = self.id(surface.wl_surface()) {
            self.windows.get_mut(&id).unwrap().reply_requested = true;
            self.requests.entry(id).or_default().fullscreen = Some(true);
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self.id(surface.wl_surface()) {
            self.windows.get_mut(&id).unwrap().reply_requested = true;
            self.requests.entry(id).or_default().fullscreen = Some(false);
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}

impl OutputHandler for State {}
impl SelectionHandler for State {
    type SelectionUserData = ();
}
impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data
    }
}
impl smithay::wayland::selection::wlr_data_control::DataControlHandler for State {
    fn data_control_state(
        &self,
    ) -> &smithay::wayland::selection::wlr_data_control::DataControlState {
        &self.data_control
    }
}
smithay::delegate_data_control!(State);
impl ClientDndGrabHandler for State {
    fn started(
        &mut self,
        _: Option<smithay::reexports::wayland_server::protocol::wl_data_source::WlDataSource>,
        icon: Option<WlSurface>,
        _: Seat<Self>,
    ) {
        self.drag_icon = icon;
        self.dragging = true;
        self.presentation_changed();
    }

    fn dropped(&mut self, _: Option<WlSurface>, _: bool, _: Seat<Self>) {
        self.drag_icon = None;
        self.dragging = false;
        self.presentation_changed();
    }
}
impl ServerDndGrabHandler for State {
    fn send(&mut self, _: String, _: OwnedFd, _: Seat<Self>) {}
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;
    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seats
    }
    fn focus_changed(&mut self, seat: &Seat<Self>, surface: Option<&WlSurface>) {
        set_data_device_focus(&self.handle, seat, surface.and_then(Resource::client));
        self.update_shortcut_focus(surface);
    }
    fn cursor_image(&mut self, _: &Seat<Self>, image: CursorImageStatus) {
        self.cursor = image;
        self.presentation_changed();
    }
}

delegate_compositor!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_output!(State);
delegate_data_device!(State);
delegate_viewporter!(State);

#[cfg(all(test, feature = "server-tests"))]
mod tests;

mod buffers;
mod input;
mod layers;
