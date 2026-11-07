use super::schema as wire;
use super::*;

fn allocation<T>(value: &wire::Allocation<T>, maximum: usize) -> io::Result<()> {
    if value.capacity > maximum || value.values.len() > value.capacity {
        return Err(invalid("Checkpoint allocation exceeds limits"));
    }
    Ok(())
}

struct Graph<'a> {
    interfaces: BTreeMap<ObjectKey, &'a str>,
    frames: BTreeSet<ObjectKey>,
    parents: BTreeMap<ObjectKey, Option<ObjectKey>>,
}

impl Graph<'_> {
    fn reference(
        &self,
        key: ObjectKey,
        interface: &str,
        owner: Option<ObjectKey>,
    ) -> io::Result<()> {
        if self.interfaces.get(&key).copied() != Some(interface)
            || owner.is_some_and(|owner| owner.client != key.client)
        {
            return Err(invalid(
                "Checkpoint resource reference is absent, stale or belongs to a different interface or client",
            ));
        }
        Ok(())
    }

    fn frame(&mut self, key: ObjectKey, owner: ObjectKey) -> io::Result<()> {
        self.reference(key, "wl_callback", Some(owner))?;
        if !self.frames.insert(key) {
            return Err(invalid(
                "Frame callback appears in more than one retained state",
            ));
        }
        Ok(())
    }

    fn attributes(&mut self, value: &wire::Attributes, owner: ObjectKey) -> io::Result<()> {
        if value.scale <= 0 || !value.client_scale.is_finite() || value.client_scale <= 0.0 {
            return Err(invalid("Invalid surface scale"));
        }
        ws::protocol::wl_output::Transform::try_from(value.transform)
            .map_err(|_| invalid("Unknown surface transform"))?;
        if let Some(Some(buffer)) = value.buffer {
            self.reference(buffer, "wl_buffer", Some(owner))?;
        }
        if let Some(region) = &value.opaque {
            allocation(&region.rectangles, 16384)?;
        }
        if let Some(region) = &value.input {
            allocation(&region.rectangles, 16384)?;
        }
        allocation(&value.damage, 16384)?;
        allocation(&value.frames, 1024)?;
        for key in &value.frames.values {
            self.frame(*key, owner)?;
        }
        Ok(())
    }

    fn toplevel_state(&self, value: &wire::ToplevelState, owner: ObjectKey) -> io::Result<()> {
        use wayland_protocols::xdg::{
            decoration::zv1::server::zxdg_toplevel_decoration_v1, shell::server::xdg_toplevel,
        };
        allocation(&value.states, 4096)?;
        allocation(&value.capabilities, 4096)?;
        let mut seen = BTreeSet::new();
        for state in &value.states.values {
            xdg_toplevel::State::try_from(*state).map_err(|_| invalid("Unknown toplevel state"))?;
            if !seen.insert(*state) {
                return Err(invalid("Duplicate toplevel state"));
            }
        }
        seen.clear();
        for capability in &value.capabilities.values {
            xdg_toplevel::WmCapabilities::try_from(*capability)
                .map_err(|_| invalid("Unknown toplevel capability"))?;
            if !seen.insert(*capability) {
                return Err(invalid("Duplicate toplevel capability"));
            }
        }
        if let Some(output) = value.fullscreen_output {
            self.reference(output, "wl_output", Some(owner))?;
        }
        if let Some(mode) = value.decoration {
            zxdg_toplevel_decoration_v1::Mode::try_from(mode)
                .map_err(|_| invalid("Unknown decoration mode"))?;
        }
        Ok(())
    }

    fn role(&mut self, value: &wire::ToplevelRole, owner: ObjectKey) -> io::Result<()> {
        allocation(&value.pending_configures, 65536)?;
        let mut serials = BTreeSet::new();
        for (serial, state) in &value.pending_configures.values {
            if !serials.insert(*serial) {
                return Err(invalid("Duplicate pending configure serial"));
            }
            self.toplevel_state(state, owner)?;
        }
        self.toplevel_state(&value.current, owner)?;
        if let Some(state) = &value.server_pending {
            self.toplevel_state(state, owner)?;
        }
        if let Some(state) = &value.last_acked {
            self.toplevel_state(state, owner)?;
        }
        for text in value.title.iter().chain(&value.app_id) {
            if text.capacity > 16 * 1024 * 1024 || text.value.len() > text.capacity {
                return Err(invalid("Invalid toplevel text allocation"));
            }
        }
        if let Some(parent) = value.parent {
            self.reference(parent, "wl_surface", Some(owner))?;
        }
        if self.parents.insert(owner, value.parent).is_some() {
            return Err(invalid("Duplicate toplevel role data"));
        }
        Ok(())
    }

    fn surface(&mut self, value: &wire::Surface, owner: ObjectKey) -> io::Result<()> {
        if value
            .role
            .as_deref()
            .is_some_and(|role| role != xdg::XDG_TOPLEVEL_ROLE)
        {
            return Err(unsupported(
                "This proof only transfers isolated XDG toplevel surface roles",
            ));
        }
        allocation(&value.pre_hooks, 65536)?;
        allocation(&value.post_hooks, 65536)?;
        allocation(&value.destruction_hooks, 65536)?;
        if !value.pre_hooks.values.is_empty() || !value.destruction_hooks.values.is_empty() {
            return Err(unsupported("Unhandled surface callback phase"));
        }
        let mut caches = BTreeSet::new();
        for state in &value.caches {
            let kind = match state {
                wire::SurfaceCache::Attributes { state } => {
                    allocation(&state.commits, 65536)?;
                    self.attributes(&state.pending, owner)?;
                    self.attributes(&state.current, owner)?;
                    for (_, value) in &state.commits.values {
                        self.attributes(value, owner)?;
                    }
                    0
                }
                wire::SurfaceCache::Xdg { state } => {
                    allocation(&state.commits, 65536)?;
                    1
                }
                wire::SurfaceCache::Viewport { state } => {
                    allocation(&state.commits, 65536)?;
                    for value in std::iter::once(&state.pending)
                        .chain(std::iter::once(&state.current))
                        .chain(state.commits.values.iter().map(|(_, value)| value))
                    {
                        if value
                            .source
                            .is_some_and(|source| source.iter().any(|value| !value.is_finite()))
                        {
                            return Err(invalid("Nonfinite viewport state"));
                        }
                    }
                    2
                }
                wire::SurfaceCache::Color { state } => {
                    allocation(&state.commits, 65536)?;
                    3
                }
                wire::SurfaceCache::Subsurface { state } => {
                    allocation(&state.commits, 65536)?;
                    4
                }
            };
            if !caches.insert(kind) {
                return Err(invalid("Duplicate surface cache type"));
            }
        }
        let mut data = BTreeSet::new();
        for value in &value.user_data {
            let kind = match value {
                wire::UserData::Toplevel { role } => {
                    self.role(role, owner)?;
                    0
                }
                wire::UserData::Suggested { scale, transform } => {
                    if *scale <= 0 {
                        return Err(invalid("Invalid preferred surface scale"));
                    }
                    ws::protocol::wl_output::Transform::try_from(*transform)
                        .map_err(|_| invalid("Unknown preferred surface transform"))?;
                    1
                }
                wire::UserData::Fractional { object, scale } => {
                    if scale.is_some_and(|scale| !scale.is_finite() || scale <= 0.0) {
                        return Err(invalid("Invalid fractional scale"));
                    }
                    if let Some(object) = object {
                        self.reference(*object, "wp_fractional_scale_v1", Some(owner))?;
                    }
                    2
                }
            };
            if !data.insert(kind) {
                return Err(invalid("Duplicate surface user-data type"));
            }
        }
        if !value.post_hooks.values.is_empty()
            && (value.role.as_deref() != Some(xdg::XDG_TOPLEVEL_ROLE) || !data.contains(&0))
        {
            return Err(invalid("Toplevel callbacks have no matching role data"));
        }
        Ok(())
    }
}

pub(super) fn graph(snapshot: &Snapshot) -> io::Result<()> {
    let state = &snapshot.state;
    let mut interfaces = BTreeMap::new();
    for client in &state.connections.clients {
        for object in &client.objects {
            interfaces.insert(
                ObjectKey {
                    client: client.key,
                    id: object.id,
                    serial: object.serial,
                },
                object.interface.as_str(),
            );
        }
    }
    let mut graph = Graph {
        interfaces,
        frames: BTreeSet::new(),
        parents: BTreeMap::new(),
    };
    let mut surfaces = BTreeSet::new();
    let mut toplevels = BTreeSet::new();
    for client in &state.connections.clients {
        let _: wire::Client = deserialize(&client.payload)?;
        for object in &client.objects {
            if matches!(object.interface.as_str(), "wl_display" | "wl_registry") {
                continue;
            }
            if object.payload.len() > 16 * 1024 * 1024 {
                return Err(invalid("Protocol resource payload exceeds byte limit"));
            }
            let value: wire::Object = deserialize(&object.payload)?;
            let key = ObjectKey {
                client: client.key,
                id: object.id,
                serial: object.serial,
            };
            let expected = match value {
                wire::Object::Compositor => "wl_compositor",
                wire::Object::Shm => "wl_shm",
                wire::Object::Callback => "wl_callback",
                wire::Object::Region { region } => {
                    allocation(&region.rectangles, 16384)?;
                    "wl_region"
                }
                wire::Object::Surface { surface } => {
                    graph.surface(&surface, key)?;
                    surfaces.insert(key);
                    "wl_surface"
                }
                wire::Object::Pool { pool } => {
                    if pool >= state.pools.len() {
                        return Err(invalid("Missing SHM pool"));
                    }
                    "wl_shm_pool"
                }
                wire::Object::Buffer {
                    pool,
                    offset,
                    width,
                    height,
                    stride,
                    format,
                } => {
                    let pool = state
                        .pools
                        .get(pool)
                        .ok_or_else(|| invalid("Missing buffer pool"))?;
                    let format = ws::protocol::wl_shm::Format::try_from(format)
                        .map_err(|_| invalid("Unknown SHM buffer format"))?;
                    let bytes = smithay::wayland::shm::wl_bytes_per_pixel(ws::WEnum::Value(format));
                    let end = stride
                        .checked_mul(height)
                        .and_then(|length| offset.checked_add(length));
                    if offset < 0
                        || width <= 0
                        || height <= 0
                        || stride <= 0
                        || bytes <= 0
                        || stride / bytes < width
                        || end.is_none_or(|end| end < 0 || end as usize > pool.size)
                    {
                        return Err(invalid("Invalid buffer layout"));
                    }
                    "wl_buffer"
                }
                wire::Object::WmBase { known_surfaces, .. } => {
                    allocation(&known_surfaces, 65536)?;
                    let mut seen = BTreeSet::new();
                    for surface in known_surfaces.values {
                        graph.reference(surface, "xdg_surface", Some(key))?;
                        if !seen.insert(surface) {
                            return Err(invalid("Duplicate XDG surface binding"));
                        }
                    }
                    "xdg_wm_base"
                }
                wire::Object::XdgSurface { surface, base, .. } => {
                    graph.reference(surface, "wl_surface", Some(key))?;
                    graph.reference(base, "xdg_wm_base", Some(key))?;
                    "xdg_surface"
                }
                wire::Object::Toplevel {
                    surface,
                    base,
                    xdg,
                    decoration,
                    dialog,
                } => {
                    graph.reference(surface, "wl_surface", Some(key))?;
                    graph.reference(base, "xdg_wm_base", Some(key))?;
                    graph.reference(xdg, "xdg_surface", Some(key))?;
                    if let Some(decoration) = decoration {
                        graph.reference(decoration, "zxdg_toplevel_decoration_v1", Some(key))?;
                    }
                    if let Some(dialog) = dialog {
                        graph.reference(dialog, "xdg_dialog_v1", Some(key))?;
                    }
                    toplevels.insert(key);
                    "xdg_toplevel"
                }
            };
            if expected != object.interface {
                return Err(invalid("Protocol payload does not match its interface"));
            }
        }
    }
    let root = &state.server;
    if surfaces != root.surfaces.values.iter().copied().collect()
        || toplevels != root.shell_toplevels.values.iter().copied().collect()
        || toplevels != root.windows.iter().map(|window| window.role).collect()
    {
        return Err(invalid(
            "Incomplete production surface, shell or window graph",
        ));
    }
    for surface in graph.parents.keys() {
        let mut seen = BTreeSet::new();
        let mut current = Some(*surface);
        while let Some(surface) = current {
            if !seen.insert(surface) {
                return Err(invalid("Cyclic toplevel parent graph"));
            }
            current = *graph
                .parents
                .get(&surface)
                .ok_or_else(|| invalid("Toplevel parent has no role state"))?;
        }
    }
    let mut callback_surfaces = BTreeSet::new();
    for (surface, queue) in &root.callbacks.values {
        graph.reference(*surface, "wl_surface", None)?;
        if !callback_surfaces.insert(*surface) {
            return Err(invalid("Duplicate callback owner"));
        }
        allocation(queue, 1024)?;
        let mut count = 0;
        for (generation, frames) in &queue.values {
            if *generation > root.revision {
                return Err(invalid("Callback generation is from the future"));
            }
            allocation(frames, 1024)?;
            count += frames.values.len();
            if count > 256 {
                return Err(invalid("Excessive pending frame callbacks"));
            }
            for frame in &frames.values {
                graph.frame(*frame, *surface)?;
            }
        }
    }
    let mut descriptor_owners = BTreeSet::new();
    for client in &state.connections.clients {
        descriptor_owners.insert(client.socket.descriptor);
        descriptor_owners.extend(client.socket.incoming_fds.iter().copied());
        descriptor_owners.extend(client.socket.outgoing_fds.iter().copied());
    }
    for pool in &state.pools {
        if !descriptor_owners.insert(pool.descriptor) {
            return Err(invalid(
                "SHM descriptor ownership overlaps another retained resource",
            ));
        }
    }
    if descriptor_owners.len() != snapshot.descriptors.len() {
        return Err(invalid("Unowned protocol checkpoint descriptors"));
    }
    Ok(())
}
