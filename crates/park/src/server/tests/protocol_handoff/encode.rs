use super::schema as wire;
use super::*;
use serde::Serialize;
use smithay::{
    reexports::wayland_server::{
        self as ws,
        protocol::{wl_compositor, wl_region, wl_shm, wl_shm_pool},
    },
    wayland::{
        compositor::{
            CachedState, RectangleKind, RegionUserData,
            handoff::{self as compositor_handoff, SuggestedSurfaceState},
        },
        fractional_scale::FractionalScaleState,
        shell::xdg::{
            XdgShellSurfaceUserData, XdgSurfaceUserData, XdgToplevelSurfaceRoleAttributes,
            XdgWmBaseUserData, handoff as xdg_handoff,
        },
        shm::{ShmBufferUserData, ShmPoolUserData, handoff::SharedPool},
    },
};
use std::{any::Any, cell::RefCell};
use wayland_backend::handoff::ConnectionEncoder;
use wayland_protocols::xdg::shell::server::{xdg_surface, xdg_toplevel, xdg_wm_base};

pub(super) struct Encoder {
    pub handle: DisplayHandle,
    pub clients: Arc<AtomicUsize>,
    pub pools: Vec<SharedPool>,
    pub pool_records: Vec<wire::Pool>,
}

pub(super) fn serialize(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(invalid("Protocol resource payload exceeds byte limit"));
    }
    Ok(bytes)
}

pub(super) fn rect<K>(rectangle: &Rectangle<i32, K>) -> [i32; 4] {
    [
        rectangle.loc.x,
        rectangle.loc.y,
        rectangle.size.w,
        rectangle.size.h,
    ]
}

fn region(value: &RegionAttributes) -> wire::Region {
    wire::Region {
        rectangles: wire::Allocation::new(
            value.rects.capacity(),
            value
                .rects
                .iter()
                .map(|(kind, rectangle)| (matches!(kind, RectangleKind::Add), rect(rectangle)))
                .collect(),
        ),
    }
}

fn text(value: &String) -> wire::Text {
    wire::Text {
        capacity: value.capacity(),
        value: value.clone(),
    }
}

fn cache<T, U>(
    state: &CachedState<T>,
    map: impl FnMut(&T) -> io::Result<U>,
) -> io::Result<wire::Cache<U>> {
    let snapshot = state.handoff_map(map)?;
    Ok(wire::Cache {
        pending: snapshot.pending,
        current: snapshot.current,
        commits: wire::Allocation::new(
            snapshot.cache.capacity(),
            snapshot
                .cache
                .into_iter()
                .map(|(serial, state)| (serial.into(), state))
                .collect(),
        ),
    })
}

fn attributes(value: &SurfaceAttributes) -> io::Result<wire::Attributes> {
    if !value.handoff_client_scale().is_finite() || value.handoff_client_scale() <= 0.0 {
        return Err(invalid("Invalid captured client scale"));
    }
    Ok(wire::Attributes {
        buffer: value.buffer.as_ref().map(|buffer| match buffer {
            BufferAssignment::Removed => None,
            BufferAssignment::NewBuffer(buffer) => Some(buffer.id().handoff_key()),
        }),
        delta: value.buffer_delta.map(|point| [point.x, point.y]),
        scale: value.buffer_scale,
        transform: value.buffer_transform.into(),
        opaque: value.opaque_region.as_ref().map(region),
        input: value.input_region.as_ref().map(region),
        damage: wire::Allocation::new(
            value.damage.capacity(),
            value
                .damage
                .iter()
                .map(|damage| match damage {
                    smithay::wayland::compositor::Damage::Buffer(rectangle) => {
                        (true, rect(rectangle))
                    }
                    smithay::wayland::compositor::Damage::Surface(rectangle) => {
                        (false, rect(rectangle))
                    }
                })
                .collect(),
        ),
        frames: wire::Allocation::new(
            value.frame_callbacks.capacity(),
            value
                .frame_callbacks
                .iter()
                .map(|frame| frame.id().handoff_key())
                .collect(),
        ),
        client_scale: value.handoff_client_scale(),
    })
}

fn toplevel_state(value: &xdg::ToplevelState) -> wire::ToplevelState {
    let (states, state_capacity) = xdg_handoff::toplevel_states_parts(&value.states);
    let (capabilities, capability_capacity) = xdg_handoff::capabilities_parts(&value.capabilities);
    wire::ToplevelState {
        size: value.size.map(|size| [size.w, size.h]),
        bounds: value.bounds.map(|size| [size.w, size.h]),
        states: wire::Allocation::new(
            state_capacity,
            states.iter().map(|state| u32::from(*state)).collect(),
        ),
        fullscreen_output: value
            .fullscreen_output
            .as_ref()
            .map(|output| output.id().handoff_key()),
        decoration: value.decoration_mode.map(u32::from),
        capabilities: wire::Allocation::new(
            capability_capacity,
            capabilities.into_iter().map(u32::from).collect(),
        ),
    }
}

fn role(value: &XdgToplevelSurfaceRoleAttributes) -> wire::ToplevelRole {
    wire::ToplevelRole {
        configured: value.configured,
        configure_serial: value.configure_serial.map(u32::from),
        initial_sent: value.initial_configure_sent,
        pending_configures: wire::Allocation::new(
            value.pending_configures.capacity(),
            value
                .pending_configures
                .iter()
                .map(|configure| (configure.serial.into(), toplevel_state(&configure.state)))
                .collect(),
        ),
        server_pending: value.server_pending.as_ref().map(toplevel_state),
        last_acked: value.last_acked.as_ref().map(toplevel_state),
        current: toplevel_state(&value.current),
        current_serial: value.current_serial.map(u32::from),
        has_buffer: value.has_buffer,
        parent: value
            .parent
            .as_ref()
            .map(|surface| surface.id().handoff_key()),
        modal: value.modal,
        title: value.title.as_ref().map(text),
        app_id: value.app_id.as_ref().map(text),
        decoration_initial_sent: value.initial_decoration_configure_sent,
    }
}

fn hook(value: smithay::utils::CallbackIdentity) -> io::Result<wire::Hook> {
    if value.capture_size == 0 && value == xdg_handoff::toplevel_commit_identity::<State>() {
        Ok(wire::Hook::XdgToplevelCommit)
    } else {
        Err(unsupported(
            "This protocol proof has no adapter for that compositor callback",
        ))
    }
}

fn capture_caches(value: &dyn Any, result: &mut Vec<wire::SurfaceCache>) -> io::Result<()> {
    if let Some(value) = value.downcast_ref::<Mutex<CachedState<SurfaceAttributes>>>() {
        result.push(wire::SurfaceCache::Attributes {
            state: cache(
                &value
                    .lock()
                    .map_err(|_| invalid("Poisoned surface attribute cache"))?,
                attributes,
            )?,
        });
    } else if let Some(value) = value.downcast_ref::<Mutex<CachedState<xdg::SurfaceCachedState>>>()
    {
        result.push(wire::SurfaceCache::Xdg {
            state: cache(
                &value.lock().map_err(|_| invalid("Poisoned XDG cache"))?,
                |value| {
                    Ok(wire::XdgCached {
                        geometry: value.geometry.as_ref().map(rect),
                        minimum: [value.min_size.w, value.min_size.h],
                        maximum: [value.max_size.w, value.max_size.h],
                    })
                },
            )?,
        });
    } else if let Some(value) = value.downcast_ref::<Mutex<CachedState<ViewportCachedState>>>() {
        result.push(wire::SurfaceCache::Viewport {
            state: cache(
                &value
                    .lock()
                    .map_err(|_| invalid("Poisoned viewport cache"))?,
                |value| {
                    Ok(wire::Viewport {
                        source: value.src.map(|rectangle| {
                            [
                                rectangle.loc.x,
                                rectangle.loc.y,
                                rectangle.size.w,
                                rectangle.size.h,
                            ]
                        }),
                        destination: value.dst.map(|size| [size.w, size.h]),
                    })
                },
            )?,
        });
    } else if let Some(value) = value.downcast_ref::<Mutex<CachedState<color::SurfaceColor>>>() {
        result.push(wire::SurfaceCache::Color {
            state: cache(
                &value.lock().map_err(|_| invalid("Poisoned color cache"))?,
                |value| Ok(value.description),
            )?,
        });
    } else if let Some(value) = value.downcast_ref::<Mutex<CachedState<SubsurfaceCachedState>>>() {
        result.push(wire::SurfaceCache::Subsurface {
            state: cache(
                &value
                    .lock()
                    .map_err(|_| invalid("Poisoned subsurface location cache"))?,
                |value| Ok([value.location.x, value.location.y]),
            )?,
        });
    } else {
        return Err(unsupported(
            "This protocol proof has no adapter for that surface cache",
        ));
    }
    Ok(())
}

fn capture_user_data(value: &dyn Any, result: &mut Vec<wire::UserData>) -> io::Result<()> {
    if let Some(value) = value.downcast_ref::<Mutex<XdgToplevelSurfaceRoleAttributes>>() {
        result.push(wire::UserData::Toplevel {
            role: Box::new(role(
                &value
                    .lock()
                    .map_err(|_| invalid("Poisoned toplevel attributes"))?,
            )),
        });
    } else if let Some(value) = value.downcast_ref::<RefCell<SuggestedSurfaceState>>() {
        let value = value
            .try_borrow()
            .map_err(|_| would_block("Suggested surface state is borrowed"))?;
        let transform: ws::protocol::wl_output::Transform = value.transform.into();
        result.push(wire::UserData::Suggested {
            scale: value.scale,
            transform: transform.into(),
        });
    } else if let Some(value) = value.downcast_ref::<RefCell<FractionalScaleState>>() {
        let value = value
            .try_borrow()
            .map_err(|_| would_block("Fractional scale state is borrowed"))?;
        let (object, scale) = value.handoff_parts();
        result.push(wire::UserData::Fractional {
            object: object.map(|object| object.id().handoff_key()),
            scale,
        });
    } else {
        return Err(unsupported(
            "This protocol proof has no adapter for that surface user data",
        ));
    }
    Ok(())
}

fn surface(value: &WlSurface) -> io::Result<wire::Surface> {
    let root = compositor_handoff::isolated_surface(value)?;
    let hooks = compositor_handoff::surface_hooks(value)?;
    if !hooks.pre_commit.is_empty() || !hooks.destruction.is_empty() {
        return Err(unsupported(
            "This protocol proof only restores the XDG post-commit hook",
        ));
    }
    let (caches, user_data) = with_states(value, |states| {
        let mut caches = Vec::new();
        let mut user_data = Vec::new();
        states
            .cached_state
            .handoff_visit(|value| capture_caches(value, &mut caches))?;
        states
            .data_map
            .handoff_visit(|value| capture_user_data(value, &mut user_data))?;
        Ok::<_, io::Error>((caches, user_data))
    })?;
    Ok(wire::Surface {
        role: root.role,
        next_transaction: root.next_transaction,
        pre_hooks: wire::Allocation::new(
            hooks.capacities[0],
            hooks
                .pre_commit
                .into_iter()
                .map(hook)
                .collect::<io::Result<_>>()?,
        ),
        post_hooks: wire::Allocation::new(
            hooks.capacities[1],
            hooks
                .post_commit
                .into_iter()
                .map(hook)
                .collect::<io::Result<_>>()?,
        ),
        destruction_hooks: wire::Allocation::new(
            hooks.capacities[2],
            hooks
                .destruction
                .into_iter()
                .map(hook)
                .collect::<io::Result<_>>()?,
        ),
        caches,
        user_data,
    })
}

impl Encoder {
    fn resource<I: Resource>(&self, id: ws::backend::ObjectId) -> io::Result<I> {
        I::from_id(&self.handle, id).map_err(io::Error::other)
    }

    fn pool(&mut self, pool: SharedPool, descriptors: &mut Vec<OwnedFd>) -> io::Result<usize> {
        if let Some(index) = self.pools.iter().position(|known| known.same_pool(&pool)) {
            return Ok(index);
        }
        let (descriptor, size) = pool.checkpoint()?;
        let index = self.pools.len();
        self.pool_records.push(wire::Pool {
            descriptor: descriptors.len(),
            size: size.get(),
        });
        descriptors.push(descriptor);
        self.pools.push(pool);
        Ok(index)
    }
}

fn data<I: Resource, U: 'static>(object: &I) -> io::Result<&U> {
    object
        .data::<U>()
        .ok_or_else(|| unsupported("Resource has an unhandled typed dispatch payload"))
}

impl ConnectionEncoder<State> for Encoder {
    fn client(
        &mut self,
        _: ClientId,
        value: &Arc<dyn ClientData>,
        _: &mut Vec<OwnedFd>,
    ) -> io::Result<Vec<u8>> {
        let value = value
            .downcast_ref::<ClientState>()
            .ok_or_else(|| unsupported("Unknown Park client data"))?;
        if !Arc::ptr_eq(&value.count, &self.clients)
            || !value.connected.load(Ordering::Acquire)
            || !value.counted.load(Ordering::Acquire)
            || value.native_canvas
        {
            return Err(unsupported(
                "This protocol proof only transfers live counted external clients",
            ));
        }
        let compositor = value.compositor.handoff_state()?;
        serialize(&wire::Client {
            connected: true,
            counted: true,
            native_canvas: false,
            unrestricted_clipboard: value.unrestricted_clipboard,
            capabilities: value.capabilities.clone(),
            scale: compositor.scale,
            queue: compositor.queue.map(|queue| {
                (
                    queue.transaction_capacity,
                    queue.seen_capacity,
                    queue.seen_surfaces,
                )
            }),
        })
    }

    fn object(
        &mut self,
        id: ws::backend::ObjectId,
        _: &Arc<dyn ws::backend::ObjectData<State>>,
        descriptors: &mut Vec<OwnedFd>,
    ) -> io::Result<Vec<u8>> {
        let record = match id.interface().name {
            "wl_compositor" => {
                data::<_, ()>(&self.resource::<wl_compositor::WlCompositor>(id)?)?;
                wire::Object::Compositor
            }
            "wl_shm" => {
                data::<_, ()>(&self.resource::<wl_shm::WlShm>(id)?)?;
                wire::Object::Shm
            }
            "wl_callback" => {
                data::<_, ()>(&self.resource::<WlCallback>(id)?)?;
                wire::Object::Callback
            }
            "wl_region" => {
                let object = self.resource::<wl_region::WlRegion>(id)?;
                data::<_, RegionUserData>(&object)?;
                wire::Object::Region {
                    region: region(&smithay::wayland::compositor::get_region_attributes(
                        &object,
                    )),
                }
            }
            "wl_surface" => wire::Object::Surface {
                surface: Box::new(surface(&self.resource::<WlSurface>(id)?)?),
            },
            "wl_shm_pool" => {
                let object = self.resource::<wl_shm_pool::WlShmPool>(id)?;
                wire::Object::Pool {
                    pool: self.pool(
                        SharedPool::from_pool(data::<_, ShmPoolUserData>(&object)?),
                        descriptors,
                    )?,
                }
            }
            "wl_buffer" => {
                let object = self.resource::<WlBuffer>(id)?;
                let (pool, value) =
                    SharedPool::from_buffer(data::<_, ShmBufferUserData>(&object)?)?;
                wire::Object::Buffer {
                    pool: self.pool(pool, descriptors)?,
                    offset: value.offset,
                    width: value.width,
                    height: value.height,
                    stride: value.stride,
                    format: value.format.into(),
                }
            }
            "xdg_wm_base" => {
                let object = self.resource::<xdg_wm_base::XdgWmBase>(id)?;
                let parts =
                    xdg_handoff::wm_base_parts(data::<_, XdgWmBaseUserData>(&object)?, |_| {
                        Err(unsupported("Unhandled XDG shell client user data"))
                    })?;
                wire::Object::WmBase {
                    ping: parts.pending_ping.map(u32::from),
                    known_surfaces: wire::Allocation::new(
                        parts.known_capacity,
                        parts
                            .known_surfaces
                            .into_iter()
                            .map(|surface| {
                                surface
                                    .upgrade()
                                    .map(|surface| surface.id().handoff_key())
                                    .map_err(io::Error::other)
                            })
                            .collect::<io::Result<_>>()?,
                    ),
                }
            }
            "xdg_surface" => {
                let object = self.resource::<xdg_surface::XdgSurface>(id)?;
                let links = xdg_handoff::surface_links(data::<_, XdgSurfaceUserData>(&object)?);
                wire::Object::XdgSurface {
                    surface: links.wl_surface.id().handoff_key(),
                    base: links.wm_base.id().handoff_key(),
                    active_role: links.active_role,
                }
            }
            "xdg_toplevel" => {
                let object = self.resource::<xdg_toplevel::XdgToplevel>(id)?;
                let links =
                    xdg_handoff::shell_surface_links(data::<_, XdgShellSurfaceUserData>(&object)?)?;
                wire::Object::Toplevel {
                    surface: links.wl_surface.id().handoff_key(),
                    base: links.wm_base.id().handoff_key(),
                    xdg: links.xdg_surface.id().handoff_key(),
                    decoration: links.decoration.map(|object| object.id().handoff_key()),
                    dialog: links.dialog.map(|object| object.id().handoff_key()),
                }
            }
            _ => {
                return Err(unsupported(
                    "This production-protocol proof has no adapter for that bound interface",
                ));
            }
        };
        serialize(&record)
    }
}
