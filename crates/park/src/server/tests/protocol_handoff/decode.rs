use super::schema as wire;
use super::*;
use serde::de::DeserializeOwned;
use smithay::{
    backend::renderer::utils::DamageBag,
    reexports::wayland_server::{
        ResourceData,
        protocol::{wl_compositor, wl_output, wl_region, wl_shm, wl_shm_pool},
    },
    wayland::{
        compositor::{
            CacheCheckpoint, RectangleKind, RegionUserData, SurfaceUserData,
            handoff::{
                self as compositor_handoff, ClientCheckpoint, EmptyQueueCheckpoint,
                IsolatedSurfaceCheckpoint, SuggestedSurfaceState,
            },
        },
        fractional_scale::FractionalScaleState,
        shell::xdg::{ToplevelConfigure, XdgToplevelSurfaceRoleAttributes, handoff as xdg_handoff},
        shm::{BufferData, handoff::SharedPool},
    },
};
use std::{cell::RefCell, num::NonZeroUsize};
use wayland_backend::{
    handoff::{ClientKey, ConnectionDecoder, RestoreContext},
    protocol::{Interface, ObjectInfo},
};
use wayland_protocols::{
    wp::fractional_scale::v1::server::wp_fractional_scale_v1,
    xdg::{
        decoration::zv1::server::zxdg_toplevel_decoration_v1,
        dialog::v1::server::xdg_dialog_v1,
        shell::server::{xdg_surface, xdg_toplevel, xdg_wm_base},
    },
};

pub(super) struct Decoder<'a> {
    source: &'a wire::State,
    state: &'a mut State,
    count: Arc<AtomicUsize>,
    clients: Vec<Arc<ClientState>>,
    pools: Vec<SharedPool>,
    objects: BTreeMap<ObjectKey, wire::Object>,
    registered: Vec<WlSurface>,
    committed: bool,
}

impl Drop for Decoder<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for content in self.state.buffers.values() {
            content.released.store(true, Ordering::Release);
        }
        self.state.buffers.clear();
        self.state.backing.clear();
        self.state.windows.clear();
        self.state.surfaces.clear();
        self.state.callbacks.clear();
        self.state.visuals.clear();
        for surface in &self.registered {
            if let Some(data) = surface.data::<SurfaceUserData>() {
                data.discard_handoff();
            }
        }
    }
}

fn parse<T: DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(invalid("Protocol resource payload exceeds byte limit"));
    }
    deserialize(bytes)
}

fn resource<I: Resource>(context: &RestoreContext, key: ObjectKey) -> io::Result<I> {
    I::from_id(
        &DisplayHandle::from(context.backend_handle().clone()),
        context.object(key)?,
    )
    .map_err(io::Error::other)
}

fn allocated<T>(value: wire::Allocation<T>, maximum: usize) -> io::Result<Vec<T>> {
    if value.capacity > maximum || value.values.len() > value.capacity {
        return Err(invalid("Invalid restored resource allocation"));
    }
    let mut result = Vec::with_capacity(value.capacity);
    result.extend(value.values);
    Ok(result)
}

fn rectangle<K>(value: [i32; 4]) -> Rectangle<i32, K> {
    Rectangle::new((value[0], value[1]).into(), (value[2], value[3]).into())
}

fn region(value: wire::Region) -> io::Result<RegionAttributes> {
    let values = allocated(value.rectangles, 16384)?;
    let mut rects = Vec::with_capacity(values.capacity());
    rects.extend(values.into_iter().map(|(add, value)| {
        (
            if add {
                RectangleKind::Add
            } else {
                RectangleKind::Subtract
            },
            rectangle(value),
        )
    }));
    Ok(RegionAttributes { rects })
}

fn text(value: wire::Text) -> io::Result<String> {
    if value.capacity > 16 * 1024 * 1024 || value.value.len() > value.capacity {
        return Err(invalid("Invalid restored text allocation"));
    }
    let mut text = String::with_capacity(value.capacity);
    text.push_str(&value.value);
    Ok(text)
}

fn cache<T, U>(
    value: wire::Cache<T>,
    mut map: impl FnMut(T) -> io::Result<U>,
) -> io::Result<CacheCheckpoint<U>> {
    if value.commits.capacity > 65536 || value.commits.values.len() > value.commits.capacity {
        return Err(invalid("Invalid restored cache queue"));
    }
    let pending = map(value.pending)?;
    let current = map(value.current)?;
    let mut cache = VecDeque::with_capacity(value.commits.capacity);
    for (serial, value) in value.commits.values {
        cache.push_back((serial.into(), map(value)?));
    }
    Ok(CacheCheckpoint {
        pending,
        current,
        cache,
    })
}

fn attributes(context: &RestoreContext, value: wire::Attributes) -> io::Result<SurfaceAttributes> {
    if value.scale <= 0 {
        return Err(invalid("Invalid restored buffer scale"));
    }
    let mut result = SurfaceAttributes::default();
    result.buffer = value
        .buffer
        .map(|buffer| match buffer {
            None => Ok(BufferAssignment::Removed),
            Some(buffer) => resource::<WlBuffer>(context, buffer).map(BufferAssignment::NewBuffer),
        })
        .transpose()?;
    result.buffer_delta = value.delta.map(|point| (point[0], point[1]).into());
    result.buffer_scale = value.scale;
    result.buffer_transform = wl_output::Transform::try_from(value.transform)
        .map_err(|_| invalid("Unknown restored buffer transform"))?;
    result.opaque_region = value.opaque.map(region).transpose()?;
    result.input_region = value.input.map(region).transpose()?;
    let damage = allocated(value.damage, 16384)?;
    let damage_capacity = damage.capacity();
    result.damage = Vec::with_capacity(damage_capacity);
    result
        .damage
        .extend(damage.into_iter().map(|(buffer, value)| {
            if buffer {
                smithay::wayland::compositor::Damage::Buffer(rectangle(value))
            } else {
                smithay::wayland::compositor::Damage::Surface(rectangle(value))
            }
        }));
    let frames = allocated(value.frames, 1024)?;
    result.frame_callbacks = Vec::with_capacity(frames.capacity());
    for frame in frames {
        result
            .frame_callbacks
            .push(resource::<WlCallback>(context, frame)?);
    }
    result.restore_client_scale(value.client_scale)?;
    Ok(result)
}

fn toplevel_state(
    context: &RestoreContext,
    value: wire::ToplevelState,
) -> io::Result<xdg::ToplevelState> {
    let states_capacity = value.states.capacity;
    let states = allocated(value.states, 4096)?
        .into_iter()
        .map(|value| {
            xdg_toplevel::State::try_from(value).map_err(|_| invalid("Unknown toplevel state"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let capabilities_capacity = value.capabilities.capacity;
    let capabilities = allocated(value.capabilities, 4096)?
        .into_iter()
        .map(|value| {
            xdg_toplevel::WmCapabilities::try_from(value)
                .map_err(|_| invalid("Unknown toplevel capability"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(xdg::ToplevelState {
        size: value.size.map(|size| (size[0], size[1]).into()),
        bounds: value.bounds.map(|size| (size[0], size[1]).into()),
        states: xdg_handoff::toplevel_states_data(states, states_capacity)?,
        fullscreen_output: value
            .fullscreen_output
            .map(|key| resource(context, key))
            .transpose()?,
        decoration_mode: value
            .decoration
            .map(|value| {
                zxdg_toplevel_decoration_v1::Mode::try_from(value)
                    .map_err(|_| invalid("Unknown decoration mode"))
            })
            .transpose()?,
        capabilities: xdg_handoff::capabilities_data(capabilities, capabilities_capacity)?,
    })
}

fn role(
    context: &RestoreContext,
    owner: ObjectKey,
    value: wire::ToplevelRole,
) -> io::Result<XdgToplevelSurfaceRoleAttributes> {
    if value
        .parent
        .is_some_and(|parent| parent.client != owner.client || parent == owner)
    {
        return Err(invalid("Invalid toplevel parent identity"));
    }
    let pending = allocated(value.pending_configures, 65536)?;
    let mut pending_configures = Vec::with_capacity(pending.capacity());
    for (serial, state) in pending {
        pending_configures.push(ToplevelConfigure {
            serial: serial.into(),
            state: toplevel_state(context, state)?,
        });
    }
    Ok(XdgToplevelSurfaceRoleAttributes {
        configured: value.configured,
        configure_serial: value.configure_serial.map(Into::into),
        initial_configure_sent: value.initial_sent,
        pending_configures,
        server_pending: value
            .server_pending
            .map(|state| toplevel_state(context, state))
            .transpose()?,
        last_acked: value
            .last_acked
            .map(|state| toplevel_state(context, state))
            .transpose()?,
        current: toplevel_state(context, value.current)?,
        current_serial: value.current_serial.map(Into::into),
        has_buffer: value.has_buffer,
        parent: value
            .parent
            .map(|parent| resource(context, parent))
            .transpose()?,
        modal: value.modal,
        title: value.title.map(text).transpose()?,
        app_id: value.app_id.map(text).transpose()?,
        initial_decoration_configure_sent: value.decoration_initial_sent,
    })
}

fn restore_surface(
    value: wire::Surface,
    key: ObjectKey,
    context: &RestoreContext,
) -> io::Result<()> {
    let surface = resource::<WlSurface>(context, key)?;
    compositor_handoff::restore_isolated_surface(
        &surface,
        IsolatedSurfaceCheckpoint {
            role: value.role.clone(),
            next_transaction: value.next_transaction,
        },
        |role| (role == xdg::XDG_TOPLEVEL_ROLE).then_some(xdg::XDG_TOPLEVEL_ROLE),
    )?;
    if !value.pre_hooks.values.is_empty() || !value.destruction_hooks.values.is_empty() {
        return Err(unsupported("Unhandled restored compositor hook phase"));
    }
    let has_toplevel_data = value
        .user_data
        .iter()
        .any(|data| matches!(data, wire::UserData::Toplevel { .. }));
    if !value.post_hooks.values.is_empty()
        && (!has_toplevel_data || value.role.as_deref() != Some(xdg::XDG_TOPLEVEL_ROLE))
    {
        return Err(invalid("XDG commit hooks require matching role data"));
    }
    compositor_handoff::reserve_hooks(
        &surface,
        [
            value.pre_hooks.capacity,
            value.post_hooks.capacity,
            value.destruction_hooks.capacity,
        ],
    )?;
    let hooks = allocated(value.post_hooks, 65536)?;
    with_states(&surface, |states| {
        for data in value.user_data {
            let inserted = match data {
                wire::UserData::Toplevel { role: value } => {
                    let restored = role(context, key, *value)?;
                    states
                        .data_map
                        .insert_if_missing_threadsafe(|| Mutex::new(restored))
                }
                wire::UserData::Suggested { scale, transform } => {
                    let transform = wl_output::Transform::try_from(transform)
                        .map_err(|_| invalid("Unknown preferred surface transform"))?;
                    states.data_map.insert_if_missing(|| {
                        RefCell::new(SuggestedSurfaceState {
                            scale,
                            transform: transform.into(),
                        })
                    })
                }
                wire::UserData::Fractional { object, scale } => {
                    let object = object
                        .map(|object| {
                            resource::<wp_fractional_scale_v1::WpFractionalScaleV1>(context, object)
                        })
                        .transpose()?;
                    let restored = FractionalScaleState::from_handoff(object, scale)?;
                    states.data_map.insert_if_missing(|| RefCell::new(restored))
                }
            };
            if !inserted {
                return Err(invalid("Duplicate restored surface user data"));
            }
        }
        for data in value.caches {
            match data {
                wire::SurfaceCache::Attributes { state } => states
                    .cached_state
                    .restore_handoff(cache(state, |value| attributes(context, value))?)?,
                wire::SurfaceCache::Xdg { state } => {
                    states.cached_state.restore_handoff(cache(state, |value| {
                        Ok(xdg::SurfaceCachedState {
                            geometry: value.geometry.map(rectangle),
                            min_size: (value.minimum[0], value.minimum[1]).into(),
                            max_size: (value.maximum[0], value.maximum[1]).into(),
                        })
                    })?)?
                }
                wire::SurfaceCache::Viewport { state } => {
                    states.cached_state.restore_handoff(cache(state, |value| {
                        if value
                            .source
                            .is_some_and(|source| source.iter().any(|value| !value.is_finite()))
                        {
                            return Err(invalid("Invalid viewport source"));
                        }
                        Ok(ViewportCachedState {
                            src: value.source.map(|source| {
                                Rectangle::new(
                                    (source[0], source[1]).into(),
                                    (source[2], source[3]).into(),
                                )
                            }),
                            dst: value.destination.map(|size| (size[0], size[1]).into()),
                        })
                    })?)?
                }
                wire::SurfaceCache::Color { state } => {
                    states
                        .cached_state
                        .restore_handoff(cache(state, |description| {
                            Ok(color::SurfaceColor { description })
                        })?)?
                }
                wire::SurfaceCache::Subsurface { state } => {
                    states
                        .cached_state
                        .restore_handoff(cache(state, |location| {
                            Ok(SubsurfaceCachedState {
                                location: (location[0], location[1]).into(),
                            })
                        })?)?
                }
            }
        }
        Ok::<_, io::Error>(())
    })?;
    for hook in hooks {
        match hook {
            wire::Hook::XdgToplevelCommit => {
                xdg_handoff::restore_toplevel_commit_hook::<State>(&surface);
            }
        }
    }
    Ok(())
}

impl<'a> Decoder<'a> {
    pub fn new(
        source: &'a wire::State,
        descriptors: &[OwnedFd],
        state: &'a mut State,
        count: Arc<AtomicUsize>,
    ) -> io::Result<Self> {
        let mut pools = Vec::new();
        for pool in &source.pools {
            let descriptor = descriptors
                .get(pool.descriptor)
                .ok_or_else(|| invalid("Missing SHM descriptor"))?
                .try_clone()?;
            pools.push(SharedPool::restore(
                descriptor,
                NonZeroUsize::new(pool.size).ok_or_else(|| invalid("Empty restored SHM pool"))?,
            )?);
        }
        Ok(Self {
            source,
            state,
            count,
            clients: Vec::new(),
            pools,
            objects: BTreeMap::new(),
            registered: Vec::new(),
            committed: false,
        })
    }

    pub fn commit_clients(&mut self) {
        self.count.store(self.clients.len(), Ordering::Release);
        for client in &self.clients {
            client.counted.store(true, Ordering::Release);
        }
        self.committed = true;
    }

    fn install<I: Resource + 'static, U: Send + Sync + 'static>(
        &self,
        context: &RestoreContext,
        key: ObjectKey,
        value: U,
    ) -> io::Result<()>
    where
        State: ws::Dispatch<I, U>,
    {
        context
            .backend_handle()
            .set_object_data::<State>(
                context.object(key)?,
                Arc::new(ResourceData::<I, U>::new(value)),
            )
            .map_err(io::Error::other)
    }

    fn restore_owners(&mut self, context: &RestoreContext) -> io::Result<()> {
        let root = &self.source.server;
        let now = Instant::now();
        let elapsed = clock_ns()?
            .checked_sub(root.clock_ns)
            .ok_or_else(|| invalid("Checkpoint is from the future"))?;
        let mut surfaces = HashSet::with_capacity(root.surfaces.capacity);
        for key in &root.surfaces.values {
            if !surfaces.insert(resource::<WlSurface>(context, *key)?) {
                return Err(invalid("Duplicate Park surface"));
            }
        }
        let mut windows = BTreeMap::new();
        let mut window_roles = BTreeSet::new();
        for window in &root.windows {
            let role_resource = resource::<xdg_toplevel::XdgToplevel>(context, window.role)?;
            let role = self
                .state
                .shell
                .get_toplevel(&role_resource)
                .ok_or_else(|| invalid("Window role was not registered"))?;
            if !surfaces.contains(role.wl_surface()) || !window_roles.insert(window.role) {
                return Err(invalid("Window role differs from its surface owner"));
            }
            let age = window
                .age_ns
                .checked_add(elapsed)
                .ok_or_else(|| invalid("Window age overflow"))?;
            let created = now
                .checked_sub(std::time::Duration::from_nanos(age))
                .ok_or_else(|| invalid("Window creation time exceeds clock range"))?;
            if windows
                .insert(
                    SurfaceId(window.id),
                    super::super::super::Window {
                        created,
                        minimized: window.minimized,
                        role,
                        mapped: window.mapped,
                        needs_initial: window.needs_initial,
                        configured_size: window.configured_size,
                        reply_requested: window.reply_requested,
                    },
                )
                .is_some()
            {
                return Err(invalid("Duplicate Park window identity"));
            }
        }
        let expected_roles = self
            .objects
            .iter()
            .filter_map(|(key, object)| {
                matches!(object, wire::Object::Toplevel { .. }).then_some(*key)
            })
            .collect::<BTreeSet<_>>();
        if window_roles != expected_roles {
            return Err(invalid("Incomplete Park window graph"));
        }
        let mut backing = HashMap::<ObjectKey, Arc<BufferBacking>>::new();
        let mut buffers = HashMap::with_capacity(root.content.capacity);
        for content in &root.content.values {
            let surface = resource::<WlSurface>(context, content.surface)?;
            let buffer = resource::<WlBuffer>(context, content.buffer)?;
            if !surfaces.contains(&surface) || content.version > root.revision {
                return Err(invalid("Invalid retained buffer generation"));
            }
            let data = buffer
                .data::<smithay::wayland::shm::ShmBufferUserData>()
                .ok_or_else(|| invalid("Retained buffer has no SHM data"))?;
            let (_, layout) = SharedPool::from_buffer(data)?;
            if content.size != [layout.width, layout.height] {
                return Err(invalid("Retained buffer size differs from its layout"));
            }
            let backing = backing
                .entry(content.buffer)
                .or_insert_with(|| {
                    Arc::new(BufferBacking {
                        buffer: buffer.clone(),
                        uses: Mutex::new(0),
                    })
                })
                .clone();
            *backing
                .uses
                .lock()
                .map_err(|_| invalid("Poisoned restored buffer count"))? += 1;
            let content = Arc::new(ClientBuffer {
                backing,
                released: AtomicBool::new(false),
                version: content.version,
                size: content.size,
                #[cfg(feature = "native-presentation")]
                cache: Mutex::new(None),
            });
            if buffers.insert(surface, content).is_some() {
                return Err(invalid("Duplicate retained surface content"));
            }
        }
        let mut backing_table = HashMap::with_capacity(root.backing.capacity);
        for (key, live) in &root.backing.values {
            let buffer = resource::<WlBuffer>(context, *key)?;
            let value = if *live {
                Arc::downgrade(
                    backing
                        .get(key)
                        .ok_or_else(|| invalid("Missing live buffer backing"))?,
                )
            } else {
                if backing.contains_key(key) {
                    return Err(invalid("Dead backing has retained users"));
                }
                Weak::new()
            };
            if backing_table.insert(buffer, value).is_some() {
                return Err(invalid("Duplicate buffer backing"));
            }
        }
        if backing.keys().any(|key| {
            !root
                .backing
                .values
                .iter()
                .any(|(known, live)| known == key && *live)
        }) {
            return Err(invalid("Incomplete buffer backing table"));
        }
        let mut generations = HashMap::with_capacity(root.generations.capacity);
        for (key, generation) in &root.generations.values {
            let surface = resource::<WlSurface>(context, *key)?;
            if !surfaces.contains(&surface)
                || *generation > root.revision
                || generations.insert(surface, *generation).is_some()
            {
                return Err(invalid("Invalid surface generation"));
            }
        }
        let mut callbacks = HashMap::with_capacity(root.callbacks.capacity);
        for (key, saved) in &root.callbacks.values {
            let surface = resource::<WlSurface>(context, *key)?;
            if !surfaces.contains(&surface)
                || saved.capacity > 1024
                || saved.values.len() > saved.capacity
            {
                return Err(invalid("Invalid callback queue"));
            }
            let mut queue = VecDeque::with_capacity(saved.capacity);
            let mut count = 0_usize;
            for (generation, frames) in &saved.values {
                if *generation > root.revision
                    || frames.capacity > 1024
                    || frames.values.len() > frames.capacity
                {
                    return Err(invalid("Invalid callback batch"));
                }
                count += frames.values.len();
                if count > 256 {
                    return Err(invalid("Too many retained frame callbacks"));
                }
                let mut restored = Vec::with_capacity(frames.capacity);
                for frame in &frames.values {
                    if frame.client != key.client {
                        return Err(invalid("Frame callback belongs to another client"));
                    }
                    restored.push(resource::<WlCallback>(context, *frame)?);
                }
                queue.push_back(Callbacks {
                    generation: *generation,
                    callbacks: restored,
                });
            }
            if callbacks.insert(surface, queue).is_some() {
                return Err(invalid("Duplicate callback owner"));
            }
        }
        let mut damage = HashMap::with_capacity(root.damage.capacity);
        for saved in &root.damage.values {
            let surface = resource::<WlSurface>(context, saved.surface)?;
            if !surfaces.contains(&surface)
                || saved
                    .layout
                    .0
                    .iter()
                    .any(|value| !value.is_finite() || *value <= 0.0)
                || saved
                    .layout
                    .1
                    .iter()
                    .chain(&saved.layout.2)
                    .chain(&saved.layout.3)
                    .any(|value| !value.is_finite())
            {
                return Err(invalid("Invalid damage owner or layout"));
            }
            let sets = saved
                .sets
                .values
                .iter()
                .map(|set| {
                    (
                        set.capacity,
                        set.values.iter().map(|value| rectangle(*value)).collect(),
                    )
                })
                .collect();
            let tracked = damage::Tracked {
                damage: DamageBag::from_handoff(
                    saved.limit,
                    saved.counter,
                    saved.sets.capacity,
                    sets,
                )?,
                layout: SurfaceLayout {
                    size: saved.layout.0,
                    uv_origin: saved.layout.1,
                    uv_x: saved.layout.2,
                    uv_y: saved.layout.3,
                },
                size: saved.size,
            };
            if damage.insert(surface, tracked).is_some() {
                return Err(invalid("Duplicate damage owner"));
            }
        }
        self.state.surfaces = surfaces;
        self.state.windows = windows;
        self.state.buffers = buffers;
        self.state.backing = backing_table;
        self.state.generations = generations;
        self.state.callbacks = callbacks;
        self.state.damage = damage;
        self.state.next_id = root.next_id;
        self.state.revision = root.revision;
        self.state.window_revision = root.window_revision;
        self.state.input_epoch = root.input_epoch;
        let mut visuals = HashMap::with_capacity(root.visuals.capacity);
        for key in &root.visuals.values {
            let surface = resource::<WlSurface>(context, *key)?;
            if !self.state.surfaces.contains(&surface)
                || visuals
                    .insert(surface.clone(), self.state.visual(&surface))
                    .is_some()
            {
                return Err(invalid("Invalid visual owner"));
            }
        }
        self.state.visuals = visuals;
        for window in self.state.windows.values().filter(|window| window.mapped) {
            self.state.output.enter(window.role.wl_surface());
        }
        Ok(())
    }
}

impl ConnectionDecoder<State> for Decoder<'_> {
    fn interface(&self, name: &str) -> Option<&'static Interface> {
        match name {
            "wl_compositor" => Some(wl_compositor::WlCompositor::interface()),
            "wl_shm" => Some(wl_shm::WlShm::interface()),
            "wl_callback" => Some(WlCallback::interface()),
            "wl_region" => Some(wl_region::WlRegion::interface()),
            "wl_surface" => Some(WlSurface::interface()),
            "wl_shm_pool" => Some(wl_shm_pool::WlShmPool::interface()),
            "wl_buffer" => Some(WlBuffer::interface()),
            "xdg_wm_base" => Some(xdg_wm_base::XdgWmBase::interface()),
            "xdg_surface" => Some(xdg_surface::XdgSurface::interface()),
            "xdg_toplevel" => Some(xdg_toplevel::XdgToplevel::interface()),
            _ => None,
        }
    }

    fn client(
        &mut self,
        _: ClientKey,
        payload: &[u8],
        _: &[OwnedFd],
    ) -> io::Result<Arc<dyn ClientData>> {
        let value: wire::Client = parse(payload)?;
        if !value.connected
            || !value.counted
            || value.native_canvas
            || value.capabilities.len() > 1024
        {
            return Err(invalid("Invalid external Park client authority"));
        }
        let compositor = CompositorClientState::from_handoff(ClientCheckpoint {
            scale: value.scale,
            queue: value
                .queue
                .map(
                    |(transaction_capacity, seen_capacity, seen_surfaces)| EmptyQueueCheckpoint {
                        transaction_capacity,
                        seen_capacity,
                        seen_surfaces,
                    },
                ),
        })?;
        let data = Arc::new(ClientState {
            connected: Arc::new(AtomicBool::new(true)),
            native_canvas: false,
            compositor,
            unrestricted_clipboard: value.unrestricted_clipboard,
            capabilities: value.capabilities,
            count: self.count.clone(),
            counted: AtomicBool::new(false),
        });
        self.clients.push(data.clone());
        Ok(data)
    }

    fn object(
        &mut self,
        key: ObjectKey,
        info: ObjectInfo,
        payload: &[u8],
        _: &[OwnedFd],
    ) -> io::Result<Arc<dyn ws::backend::ObjectData<State>>> {
        let value: wire::Object = parse(payload)?;
        let data: Arc<dyn ws::backend::ObjectData<State>> = match (info.interface.name, &value) {
            ("wl_compositor", wire::Object::Compositor) => {
                Arc::new(ResourceData::<wl_compositor::WlCompositor, ()>::new(()))
            }
            ("wl_shm", wire::Object::Shm) => Arc::new(ResourceData::<wl_shm::WlShm, ()>::new(())),
            ("wl_callback", wire::Object::Callback) => {
                Arc::new(ResourceData::<WlCallback, ()>::new(()))
            }
            ("wl_region", wire::Object::Region { region: value }) => {
                Arc::new(ResourceData::<wl_region::WlRegion, _>::new(
                    RegionUserData::from_handoff(region(value.clone())?),
                ))
            }
            ("wl_surface", wire::Object::Surface { .. }) => Arc::new(
                ResourceData::<WlSurface, _>::new(SurfaceUserData::for_handoff::<State>()),
            ),
            ("wl_shm_pool", wire::Object::Pool { pool }) => {
                Arc::new(ResourceData::<wl_shm_pool::WlShmPool, _>::new(
                    self.pools
                        .get(*pool)
                        .ok_or_else(|| invalid("Unknown restored SHM pool"))?
                        .pool_data(),
                ))
            }
            (
                "wl_buffer",
                wire::Object::Buffer {
                    pool,
                    offset,
                    width,
                    height,
                    stride,
                    format,
                },
            ) => {
                let format =
                    wl_shm::Format::try_from(*format).map_err(|_| invalid("Unknown SHM format"))?;
                let data = self
                    .pools
                    .get(*pool)
                    .ok_or_else(|| invalid("Unknown buffer SHM pool"))?
                    .buffer_data(
                        &self.state.shm,
                        BufferData {
                            offset: *offset,
                            width: *width,
                            height: *height,
                            stride: *stride,
                            format,
                        },
                    )?;
                Arc::new(ResourceData::<WlBuffer, _>::new(data))
            }
            ("xdg_wm_base", wire::Object::WmBase { ping, .. }) => {
                Arc::new(ResourceData::<xdg_wm_base::XdgWmBase, _>::new(
                    xdg_handoff::wm_base_data(ping.map(Into::into), |_| Ok(()))?,
                ))
            }
            ("xdg_surface", wire::Object::XdgSurface { .. })
            | ("xdg_toplevel", wire::Object::Toplevel { .. }) => {
                wayland_backend::handoff::deferred_object_data()
            }
            _ => {
                return Err(invalid(
                    "Protocol payload differs from its resource interface",
                ));
            }
        };
        if self.objects.insert(key, value).is_some() {
            return Err(invalid("Duplicate restored protocol resource"));
        }
        Ok(data)
    }

    fn finish(&mut self, context: &RestoreContext, _: &[OwnedFd]) -> io::Result<()> {
        let keys = self.objects.keys().copied().collect::<Vec<_>>();
        let expected_surfaces = self
            .objects
            .iter()
            .filter_map(|(key, value)| {
                matches!(value, wire::Object::Surface { .. }).then_some(*key)
            })
            .collect::<BTreeSet<_>>();
        if expected_surfaces
            != self
                .source
                .server
                .surfaces
                .values
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
        {
            return Err(invalid("Incomplete compositor surface graph"));
        }
        for key in &keys {
            if matches!(self.objects.get(key), Some(wire::Object::Surface { .. })) {
                let surface = resource::<WlSurface>(context, *key)?;
                self.state
                    .compositor
                    .register_handoff_surface::<State>(&surface)?;
                self.registered.push(surface);
            }
        }
        for key in &keys {
            if let Some(wire::Object::XdgSurface {
                surface,
                base,
                active_role,
            }) = self.objects.get(key).cloned()
            {
                let data = xdg_handoff::surface_data(xdg_handoff::SurfaceLinks {
                    wl_surface: resource(context, surface)?,
                    wm_base: resource(context, base)?,
                    active_role,
                })?;
                self.install::<xdg_surface::XdgSurface, _>(context, *key, data)?;
            }
        }
        for key in &keys {
            if let Some(wire::Object::Toplevel {
                surface,
                base,
                xdg,
                decoration,
                dialog,
            }) = self.objects.get(key).cloned()
            {
                let data = xdg_handoff::shell_surface_data(xdg_handoff::ShellSurfaceLinks {
                    wl_surface: resource(context, surface)?,
                    wm_base: resource(context, base)?,
                    xdg_surface: resource(context, xdg)?,
                    decoration: decoration
                        .map(|key| {
                            resource::<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1>(
                                context, key,
                            )
                        })
                        .transpose()?,
                    dialog: dialog
                        .map(|key| resource::<xdg_dialog_v1::XdgDialogV1>(context, key))
                        .transpose()?,
                })?;
                self.install::<xdg_toplevel::XdgToplevel, _>(context, *key, data)?;
            }
        }
        let shell = &self.source.server;
        let capabilities = shell
            .shell_capabilities
            .values
            .iter()
            .map(|value| {
                xdg_toplevel::WmCapabilities::try_from(*value)
                    .map_err(|_| invalid("Unknown shell capability"))
            })
            .collect::<io::Result<Vec<_>>>()?;
        self.state.shell.restore_handoff_manager_layout(
            [shell.shell_toplevels.capacity, shell.shell_popup_capacity],
            capabilities,
            shell.shell_capabilities.capacity,
        )?;
        let mut roles = BTreeSet::new();
        for key in &shell.shell_toplevels.values {
            if !matches!(self.objects.get(key), Some(wire::Object::Toplevel { .. }))
                || !roles.insert(*key)
            {
                return Err(invalid("Invalid shell toplevel list"));
            }
            self.state
                .shell
                .register_handoff_toplevel(&resource::<xdg_toplevel::XdgToplevel>(
                    context, *key,
                )?)?;
        }
        for key in &keys {
            if let Some(wire::Object::Surface { surface }) = self.objects.get(key).cloned() {
                restore_surface(*surface, *key, context)?;
            }
        }
        for key in &keys {
            if let Some(wire::Object::WmBase { known_surfaces, .. }) =
                self.objects.get(key).cloned()
            {
                let capacity = known_surfaces.capacity;
                let known = allocated(known_surfaces, 65536)?
                    .into_iter()
                    .map(|key| {
                        resource::<xdg_surface::XdgSurface>(context, key)
                            .map(|surface| surface.downgrade())
                    })
                    .collect::<io::Result<Vec<_>>>()?;
                xdg_handoff::restore_known_surfaces(
                    &resource::<xdg_wm_base::XdgWmBase>(context, *key)?,
                    capacity,
                    known,
                )?;
            }
        }
        self.restore_owners(context)?;
        if self.clients.len() != self.source.server.clients {
            return Err(invalid("Restored client authority count differs"));
        }
        Ok(())
    }
}
