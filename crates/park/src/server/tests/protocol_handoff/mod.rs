use super::super::*;
use smithay::reexports::wayland_server::{self as ws, protocol::wl_callback::WlCallback};
use wayland_backend::handoff::{Checkpoint, ObjectKey};

mod decode;
mod encode;
mod exec;
mod fixture;
mod schema;
mod validate;

pub(in crate::server) fn revision_step() -> u64 {
    if option_env!("LINCE_PROTOCOL_HANDOFF_GENERATION") == Some("next") {
        10
    } else {
        1
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn unsupported(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message)
}
fn would_block(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, message)
}

fn deserialize<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    serde_json::from_slice(bytes).map_err(|error| {
        invalid(&format!(
            "Protocol checkpoint schema mismatch at {}:{}",
            error.line(),
            error.column()
        ))
    })
}

fn clock_ns() -> io::Result<u64> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let seconds = u64::try_from(time.tv_sec).map_err(|_| invalid("Invalid monotonic clock"))?;
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|value| value.checked_add(time.tv_nsec as u64))
        .ok_or_else(|| invalid("Monotonic clock overflow"))
}

fn nanos(duration: std::time::Duration) -> io::Result<u64> {
    duration
        .as_nanos()
        .try_into()
        .map_err(|_| invalid("Checkpoint duration exceeds clock range"))
}

pub(super) struct Snapshot {
    pub state: schema::State,
    pub descriptors: Vec<OwnedFd>,
}

impl Snapshot {
    fn duplicate(&self) -> io::Result<Self> {
        Ok(Self {
            state: self.state.clone(),
            descriptors: self
                .descriptors
                .iter()
                .map(OwnedFd::try_clone)
                .collect::<io::Result<_>>()?,
        })
    }

    fn encode(&self) -> io::Result<Vec<u8>> {
        let bytes = serde_json::to_vec(&self.state)?;
        if bytes.len() > lince_refresh::checkpoint::MAX_STATE_BYTES {
            return Err(invalid("Protocol checkpoint exceeds byte limit"));
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8], descriptors: Vec<OwnedFd>) -> io::Result<Self> {
        if bytes.len() > lince_refresh::checkpoint::MAX_STATE_BYTES {
            return Err(invalid("Protocol checkpoint exceeds byte limit"));
        }
        let state = deserialize(bytes)?;
        let snapshot = Self { state, descriptors };
        snapshot.validate()?;
        Ok(snapshot)
    }

    fn validate(&self) -> io::Result<()> {
        let state = &self.state;
        state.connections.validate(self.descriptors.len())?;
        if state.schema != 1
            || state.server.clients != state.connections.clients.len()
            || state.server.clients > MAX_CLIENTS
            || state.server.windows.len() > MAX_WINDOWS
            || state.pools.len() > 65536
            || state.server.next_id == 0
        {
            return Err(invalid("Invalid production protocol checkpoint header"));
        }
        validate_size(state.server.size)?;
        for pool in &state.pools {
            if pool.descriptor >= self.descriptors.len()
                || pool.size == 0
                || pool.size > i32::MAX as usize
            {
                return Err(invalid("Invalid SHM pool checkpoint"));
            }
        }
        for (length, capacity, maximum) in [
            (
                state.server.surfaces.values.len(),
                state.server.surfaces.capacity,
                MAX_SURFACES * 4,
            ),
            (
                state.server.content.values.len(),
                state.server.content.capacity,
                MAX_SURFACES * 4,
            ),
            (
                state.server.backing.values.len(),
                state.server.backing.capacity,
                65536,
            ),
            (
                state.server.generations.values.len(),
                state.server.generations.capacity,
                MAX_SURFACES * 4,
            ),
            (
                state.server.callbacks.values.len(),
                state.server.callbacks.capacity,
                MAX_SURFACES * 4,
            ),
            (
                state.server.visuals.values.len(),
                state.server.visuals.capacity,
                MAX_SURFACES * 4,
            ),
            (
                state.server.damage.values.len(),
                state.server.damage.capacity,
                MAX_SURFACES * 4,
            ),
            (
                state.server.shell_toplevels.values.len(),
                state.server.shell_toplevels.capacity,
                65536,
            ),
            (
                state.server.shell_capabilities.values.len(),
                state.server.shell_capabilities.capacity,
                4096,
            ),
        ] {
            if capacity < length || capacity > maximum {
                return Err(invalid("Invalid production state allocation"));
            }
        }
        let keys = state
            .connections
            .clients
            .iter()
            .flat_map(|client| {
                client.objects.iter().map(move |object| ObjectKey {
                    client: client.key,
                    id: object.id,
                    serial: object.serial,
                })
            })
            .collect::<BTreeSet<_>>();
        let mut seen = BTreeSet::new();
        for surface in &state.server.surfaces.values {
            if !keys.contains(surface) || !seen.insert(*surface) {
                return Err(invalid("Missing or duplicate Park surface"));
            }
        }
        let mut windows = BTreeSet::new();
        for window in &state.server.windows {
            if window.id == 0
                || window.id >= state.server.next_id
                || !windows.insert(window.id)
                || !keys.contains(&window.role)
            {
                return Err(invalid("Invalid Park window identity"));
            }
        }
        for content in &state.server.content.values {
            if !seen.contains(&content.surface)
                || !keys.contains(&content.buffer)
                || content.size.iter().any(|dimension| *dimension <= 0)
            {
                return Err(invalid("Invalid retained Park buffer"));
            }
        }
        validate::graph(self)
    }
}

fn capture(server: &mut Server) -> io::Result<Snapshot> {
    if server.listener.is_some()
        || server.locked()
        || server.state.native_surface.is_some()
        || !server.state.requests.is_empty()
        || !server.state.activations.is_empty()
        || !server.state.buttons.is_empty()
        || !server.state.touches.is_empty()
        || server.state.dragging
        || server.state.presented_geometry.is_some()
        || !server.state.presented_mappings.is_empty()
        || !server.state.security_contexts.is_empty()
    {
        return Err(unsupported(
            "This private proof does not checkpoint live session, input or control authorities",
        ));
    }
    let instant = Instant::now();
    let clock = clock_ns()?;
    let mut uses = HashMap::<WlBuffer, usize>::new();
    for content in server.state.buffers.values() {
        if content.released.load(Ordering::Acquire) || Arc::strong_count(content) != 1 {
            return Err(unsupported(
                "Transfer renderer-held buffer generations with the graphics adapter",
            ));
        }
        *uses.entry(content.buffer().clone()).or_default() += 1;
    }
    let mut backing = Vec::new();
    for (buffer, known) in &server.state.backing {
        let live = if let Some(known) = known.upgrade() {
            let actual = *known
                .uses
                .lock()
                .map_err(|_| invalid("Poisoned buffer-use checkpoint lock"))?;
            if actual == 0 || Some(&actual) != uses.get(buffer) {
                return Err(unsupported("Unhandled retained buffer backing owner"));
            }
            true
        } else {
            false
        };
        backing.push((buffer.id().handoff_key(), live));
    }
    let mut encoder = encode::Encoder {
        handle: server.display.handle(),
        clients: server.clients.clone(),
        pools: Vec::new(),
        pool_records: Vec::new(),
    };
    let connections = server
        .display
        .backend()
        .checkpoint_connections(&mut encoder)?;
    let (shell_capacity, capabilities, capability_capacity) =
        server.state.shell.handoff_manager_layout();
    let mut windows = Vec::new();
    for (id, window) in &server.state.windows {
        windows.push(schema::Window {
            id: id.0,
            role: window.role.xdg_toplevel().id().handoff_key(),
            age_ns: nanos(
                instant
                    .checked_duration_since(window.created)
                    .ok_or_else(|| invalid("Window creation time is in the future"))?,
            )?,
            minimized: window.minimized,
            mapped: window.mapped,
            needs_initial: window.needs_initial,
            configured_size: window.configured_size,
            reply_requested: window.reply_requested,
        });
    }
    let mut damage = Vec::new();
    for (surface, tracked) in &server.state.damage {
        let (limit, counter, capacity, capacities) = tracked.damage.handoff_layout();
        let sets = tracked
            .damage
            .raw()
            .zip(capacities)
            .map(|(set, capacity)| {
                schema::Allocation::new(capacity, set.map(encode::rect).collect())
            })
            .collect();
        damage.push(schema::Damage {
            surface: surface.id().handoff_key(),
            limit,
            counter,
            sets: schema::Allocation::new(capacity, sets),
            layout: (
                tracked.layout.size,
                tracked.layout.uv_origin,
                tracked.layout.uv_x,
                tracked.layout.uv_y,
            ),
            size: tracked.size,
        });
    }
    let snapshot = Snapshot {
        state: schema::State {
            schema: 1,
            connections: connections.state,
            pools: encoder.pool_records,
            server: schema::ServerState {
                size: server.state.initial_size,
                clock_ns: clock,
                started_age_ns: nanos(
                    instant
                        .checked_duration_since(server.started)
                        .ok_or_else(|| invalid("Server start time is in the future"))?,
                )?,
                serial: smithay::utils::SERIAL_COUNTER.handoff_value(),
                clients: server.clients.load(Ordering::Acquire),
                next_id: server.state.next_id,
                revision: server.state.revision,
                window_revision: server.state.window_revision,
                input_epoch: server.state.input_epoch,
                shell_toplevels: schema::Allocation::new(
                    shell_capacity[0],
                    server
                        .state
                        .shell
                        .toplevel_surfaces()
                        .iter()
                        .map(|role| role.xdg_toplevel().id().handoff_key())
                        .collect(),
                ),
                shell_popup_capacity: shell_capacity[1],
                shell_capabilities: schema::Allocation::new(
                    capability_capacity,
                    capabilities.into_iter().map(u32::from).collect(),
                ),
                windows,
                surfaces: schema::Allocation::new(
                    server.state.surfaces.capacity(),
                    server
                        .state
                        .surfaces
                        .iter()
                        .map(|surface| surface.id().handoff_key())
                        .collect(),
                ),
                content: schema::Allocation::new(
                    server.state.buffers.capacity(),
                    server
                        .state
                        .buffers
                        .iter()
                        .map(|(surface, content)| schema::Content {
                            surface: surface.id().handoff_key(),
                            buffer: content.buffer().id().handoff_key(),
                            version: content.version,
                            size: content.size,
                        })
                        .collect(),
                ),
                backing: schema::Allocation::new(server.state.backing.capacity(), backing),
                generations: schema::Allocation::new(
                    server.state.generations.capacity(),
                    server
                        .state
                        .generations
                        .iter()
                        .map(|(surface, generation)| (surface.id().handoff_key(), *generation))
                        .collect(),
                ),
                callbacks: schema::Allocation::new(
                    server.state.callbacks.capacity(),
                    server
                        .state
                        .callbacks
                        .iter()
                        .map(|(surface, queue)| {
                            (
                                surface.id().handoff_key(),
                                schema::Allocation::new(
                                    queue.capacity(),
                                    queue
                                        .iter()
                                        .map(|batch| {
                                            (
                                                batch.generation,
                                                schema::Allocation::new(
                                                    batch.callbacks.capacity(),
                                                    batch
                                                        .callbacks
                                                        .iter()
                                                        .map(|callback| callback.id().handoff_key())
                                                        .collect(),
                                                ),
                                            )
                                        })
                                        .collect(),
                                ),
                            )
                        })
                        .collect(),
                ),
                visuals: schema::Allocation::new(
                    server.state.visuals.capacity(),
                    server
                        .state
                        .visuals
                        .keys()
                        .map(|surface| surface.id().handoff_key())
                        .collect(),
                ),
                damage: schema::Allocation::new(server.state.damage.capacity(), damage),
            },
        },
        descriptors: connections.descriptors,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

fn restore(snapshot: &Snapshot, restore_serial: bool) -> io::Result<Server> {
    snapshot.validate()?;
    let now = Instant::now();
    let elapsed = clock_ns()?
        .checked_sub(snapshot.state.server.clock_ns)
        .ok_or_else(|| invalid("Checkpoint monotonic time is in the future"))?;
    let age = snapshot
        .state
        .server
        .started_age_ns
        .checked_add(elapsed)
        .ok_or_else(|| invalid("Server start age overflow"))?;
    let started = now
        .checked_sub(std::time::Duration::from_nanos(age))
        .ok_or_else(|| invalid("Server start time is outside clock range"))?;
    let mut server = Server::new(snapshot.state.server.size)?;
    let connections = Checkpoint {
        state: snapshot.state.connections.clone(),
        descriptors: snapshot
            .descriptors
            .iter()
            .map(OwnedFd::try_clone)
            .collect::<io::Result<_>>()?,
    };
    let mut decoder = decode::Decoder::new(
        &snapshot.state,
        &snapshot.descriptors,
        &mut server.state,
        server.clients.clone(),
    )?;
    server
        .display
        .backend()
        .restore_connections(&connections, &mut decoder)?;
    decoder.commit_clients();
    drop(decoder);
    server.started = started;
    if restore_serial {
        smithay::utils::SERIAL_COUNTER.restore_handoff(snapshot.state.server.serial);
    }
    Ok(server)
}
