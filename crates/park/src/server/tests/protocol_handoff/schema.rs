use serde::{Deserialize, Serialize};
use wayland_backend::handoff::{ConnectionState, ObjectKey};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Allocation<T> {
    pub capacity: usize,
    pub values: Vec<T>,
}

impl<T> Allocation<T> {
    pub fn new(capacity: usize, values: Vec<T>) -> Self {
        Self { capacity, values }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cache<T> {
    pub pending: T,
    pub current: T,
    pub commits: Allocation<(u32, T)>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Text {
    pub capacity: usize,
    pub value: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Region {
    pub rectangles: Allocation<(bool, [i32; 4])>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Attributes {
    pub buffer: Option<Option<ObjectKey>>,
    pub delta: Option<[i32; 2]>,
    pub scale: i32,
    pub transform: u32,
    pub opaque: Option<Region>,
    pub input: Option<Region>,
    pub damage: Allocation<(bool, [i32; 4])>,
    pub frames: Allocation<ObjectKey>,
    pub client_scale: f64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct XdgCached {
    pub geometry: Option<[i32; 4]>,
    pub minimum: [i32; 2],
    pub maximum: [i32; 2],
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Viewport {
    pub source: Option<[f64; 4]>,
    pub destination: Option<[i32; 2]>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum SurfaceCache {
    Attributes {
        state: Cache<Attributes>,
    },
    Xdg {
        state: Cache<XdgCached>,
    },
    Viewport {
        state: Cache<Viewport>,
    },
    Color {
        state: Cache<Option<lince_interface::host::color::Description>>,
    },
    Subsurface {
        state: Cache<[i32; 2]>,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToplevelState {
    pub size: Option<[i32; 2]>,
    pub bounds: Option<[i32; 2]>,
    pub states: Allocation<u32>,
    pub fullscreen_output: Option<ObjectKey>,
    pub decoration: Option<u32>,
    pub capabilities: Allocation<u32>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToplevelRole {
    pub configured: bool,
    pub configure_serial: Option<u32>,
    pub initial_sent: bool,
    pub pending_configures: Allocation<(u32, ToplevelState)>,
    pub server_pending: Option<ToplevelState>,
    pub last_acked: Option<ToplevelState>,
    pub current: ToplevelState,
    pub current_serial: Option<u32>,
    pub has_buffer: bool,
    pub parent: Option<ObjectKey>,
    pub modal: bool,
    pub title: Option<Text>,
    pub app_id: Option<Text>,
    pub decoration_initial_sent: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum UserData {
    Toplevel {
        role: Box<ToplevelRole>,
    },
    Suggested {
        scale: i32,
        transform: u32,
    },
    Fractional {
        object: Option<ObjectKey>,
        scale: Option<f64>,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Hook {
    XdgToplevelCommit,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Surface {
    pub role: Option<String>,
    pub next_transaction: u32,
    pub pre_hooks: Allocation<Hook>,
    pub post_hooks: Allocation<Hook>,
    pub destruction_hooks: Allocation<Hook>,
    pub caches: Vec<SurfaceCache>,
    pub user_data: Vec<UserData>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Client {
    pub connected: bool,
    pub counted: bool,
    pub native_canvas: bool,
    pub unrestricted_clipboard: bool,
    pub capabilities: Vec<crate::launch::Capability>,
    pub scale: f64,
    pub queue: Option<(usize, usize, Vec<u32>)>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Object {
    Compositor,
    Shm,
    Callback,
    Region {
        region: Region,
    },
    Surface {
        surface: Box<Surface>,
    },
    Pool {
        pool: usize,
    },
    Buffer {
        pool: usize,
        offset: i32,
        width: i32,
        height: i32,
        stride: i32,
        format: u32,
    },
    WmBase {
        ping: Option<u32>,
        known_surfaces: Allocation<ObjectKey>,
    },
    XdgSurface {
        surface: ObjectKey,
        base: ObjectKey,
        active_role: bool,
    },
    Toplevel {
        surface: ObjectKey,
        base: ObjectKey,
        xdg: ObjectKey,
        decoration: Option<ObjectKey>,
        dialog: Option<ObjectKey>,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pool {
    pub descriptor: usize,
    pub size: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Window {
    pub id: u64,
    pub role: ObjectKey,
    pub age_ns: u64,
    pub minimized: bool,
    pub mapped: bool,
    pub needs_initial: bool,
    pub configured_size: [i32; 2],
    pub reply_requested: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Content {
    pub surface: ObjectKey,
    pub buffer: ObjectKey,
    pub version: u64,
    pub size: [i32; 2],
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Damage {
    pub surface: ObjectKey,
    pub limit: usize,
    pub counter: usize,
    pub sets: Allocation<Allocation<[i32; 4]>>,
    pub layout: ([f64; 2], [f32; 2], [f32; 2], [f32; 2]),
    pub size: [i32; 2],
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ServerState {
    pub size: [i32; 2],
    pub clock_ns: u64,
    pub started_age_ns: u64,
    pub serial: u32,
    pub clients: usize,
    pub next_id: u64,
    pub revision: u64,
    pub window_revision: u64,
    pub input_epoch: u64,
    pub shell_toplevels: Allocation<ObjectKey>,
    pub shell_popup_capacity: usize,
    pub shell_capabilities: Allocation<u32>,
    pub windows: Vec<Window>,
    pub surfaces: Allocation<ObjectKey>,
    pub content: Allocation<Content>,
    pub backing: Allocation<(ObjectKey, bool)>,
    pub generations: Allocation<(ObjectKey, u64)>,
    pub callbacks: Allocation<(ObjectKey, Allocation<(u64, Allocation<ObjectKey>)>)>,
    pub visuals: Allocation<ObjectKey>,
    pub damage: Allocation<Damage>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub schema: u32,
    pub connections: ConnectionState,
    pub pools: Vec<Pool>,
    pub server: ServerState,
}
