use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read},
    os::unix::net::UnixStream,
    sync::Arc,
};

use rustix::event::epoll;

use super::{
    ClientId, Data, DumbObjectData, InnerBackend, InnerClientId, InnerObjectId, ObjectId,
    client::{Client, ClientStore},
    handle::State,
    registry::{Global, Registry},
};
use crate::{
    core_interfaces::{WL_DISPLAY_INTERFACE, WL_REGISTRY_INTERFACE},
    handoff::{
        Checkpoint, ClientKey, ClientState, ConnectionDecoder, ConnectionEncoder, ConnectionState,
        GlobalState, MAX_BYTES, MAX_CLIENTS, MAX_DESCRIPTORS, MAX_OBJECTS, MAX_SLOTS, ObjectKey,
        ObjectState, RestoreContext, invalid, protocol_identity,
    },
    protocol::{Interface, ObjectInfo},
    rs::{
        map::{Object, ObjectMap},
        socket::{BufferedSocket, Socket},
    },
};

fn client_key(id: &InnerClientId) -> ClientKey {
    ClientKey {
        slot: id.id,
        serial: id.serial,
    }
}

fn object_key(id: &InnerObjectId) -> ObjectKey {
    ObjectKey {
        client: client_key(&id.client_id),
        id: id.id,
        serial: id.serial,
    }
}

impl ClientId {
    #[doc(hidden)]
    pub fn handoff_key(&self) -> ClientKey {
        client_key(&self.id)
    }
}

impl ObjectId {
    #[doc(hidden)]
    pub fn handoff_key(&self) -> ObjectKey {
        object_key(&self.id)
    }
}

fn globals<D: 'static>(state: &State<D>) -> Vec<Option<GlobalState>> {
    state
        .registry
        .globals
        .iter()
        .map(|global| {
            global.as_ref().map(|global| GlobalState {
                serial: global.id.serial,
                interface: global.interface.name.to_owned(),
                protocol: protocol_identity(global.interface),
                version: global.version,
                disabled: global.disabled,
            })
        })
        .collect()
}

fn object_interface<D: 'static>(
    decoder: &impl ConnectionDecoder<D>,
    name: &str,
) -> io::Result<&'static Interface> {
    let interface = match name {
        "wl_display" => Some(&WL_DISPLAY_INTERFACE),
        "wl_registry" => Some(&WL_REGISTRY_INTERFACE),
        _ => decoder.interface(name),
    }
    .ok_or_else(|| invalid("Checkpoint refers to an unknown protocol interface"))?;
    if interface.name != name {
        return Err(invalid("Decoder returned a different protocol interface"));
    }
    Ok(interface)
}

impl<D: 'static> InnerBackend<D> {
    pub(crate) fn checkpoint_connections(
        &self,
        encoder: &mut impl ConnectionEncoder<D>,
    ) -> io::Result<Checkpoint> {
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("Poisoned backend checkpoint lock"))?;
        if !state.pending_destructors.is_empty()
            || state
                .clients
                .clients
                .iter()
                .flatten()
                .any(|client| client.killed)
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Complete backend cleanup before checkpointing",
            ));
        }
        if state.clients.clients.len() > MAX_CLIENTS || state.registry.globals.len() > MAX_OBJECTS {
            return Err(invalid("Backend is too large for this checkpoint format"));
        }
        let mut descriptors = Vec::new();
        let mut clients = Vec::new();
        let mut pending_clients = Vec::new();
        let mut pending_objects = Vec::new();
        let mut total_bytes = 0_usize;
        let mut total_objects = 0_usize;
        let mut total_slots = 0_usize;
        for client in state.clients.clients.iter().flatten() {
            let socket = client.socket.checkpoint(&mut descriptors)?;
            total_bytes = total_bytes
                .checked_add(socket.incoming.len())
                .and_then(|value| value.checked_add(socket.outgoing.len()))
                .ok_or_else(|| invalid("Checkpoint size overflow"))?;
            let (client_slots, server_slots, client_capacity, server_capacity) =
                client.map.checkpoint_layout();
            total_slots = total_slots
                .checked_add(client_capacity)
                .and_then(|value| value.checked_add(server_capacity))
                .ok_or_else(|| invalid("Object layout size overflow"))?;
            if total_slots > MAX_SLOTS || total_bytes > MAX_BYTES {
                return Err(invalid("Backend checkpoint exceeds limits"));
            }
            let mut objects = Vec::new();
            let client_index = clients.len();
            pending_clients.push((
                ClientId {
                    id: client.id.clone(),
                },
                client.data.clone(),
            ));
            for (id, object) in client.map.all_objects() {
                total_objects += 1;
                if total_objects > MAX_OBJECTS {
                    return Err(invalid("Too many protocol objects to checkpoint"));
                }
                let object_id = ObjectId {
                    id: InnerObjectId {
                        id,
                        serial: object.data.serial,
                        client_id: client.id.clone(),
                        interface: object.interface,
                    },
                };
                if id != 1 && object.interface.name != "wl_registry" {
                    pending_objects.push((
                        client_index,
                        objects.len(),
                        object_id,
                        object.data.user_data.clone(),
                    ));
                }
                objects.push(ObjectState {
                    id,
                    serial: object.data.serial,
                    interface: object.interface.name.to_owned(),
                    protocol: protocol_identity(object.interface),
                    version: object.version,
                    payload: Vec::new(),
                });
            }
            clients.push(ClientState {
                key: client_key(&client.id),
                last_serial: client.last_serial,
                debug: client.debug,
                client_slots,
                server_slots,
                client_capacity,
                server_capacity,
                socket,
                objects,
                payload: Vec::new(),
            });
        }
        let metadata = ConnectionState {
            schema: 1,
            little_endian: cfg!(target_endian = "little"),
            client_slots: state.clients.clients.len(),
            client_capacity: state.clients.clients.capacity(),
            client_serial: state.clients.last_serial,
            debug: state.clients.debug,
            default_maximum_buffer: state.default_max_buffer_size,
            globals: globals(&state),
            global_serial: state.registry.last_serial,
            global_capacity: state.registry.globals.capacity(),
            registry_capacity: state.registry.known_registries.capacity(),
            destructor_capacity: state.pending_destructors.capacity(),
            registries: state
                .registry
                .known_registries
                .iter()
                .map(object_key)
                .collect(),
            clients,
            descriptor_count: descriptors.len(),
        };
        drop(state);
        let mut checkpoint = Checkpoint {
            state: metadata,
            descriptors,
        };
        for (index, (id, data)) in pending_clients.into_iter().enumerate() {
            let payload = encoder.client(id, &data, &mut checkpoint.descriptors)?;
            total_bytes = total_bytes
                .checked_add(payload.len())
                .ok_or_else(|| invalid("Client payload size overflow"))?;
            if total_bytes > MAX_BYTES || checkpoint.descriptors.len() > MAX_DESCRIPTORS {
                return Err(invalid("Client payload exceeds checkpoint limits"));
            }
            checkpoint.state.clients[index].payload = payload;
        }
        for (client, object, id, data) in pending_objects {
            let payload = encoder.object(id, &data, &mut checkpoint.descriptors)?;
            total_bytes = total_bytes
                .checked_add(payload.len())
                .ok_or_else(|| invalid("Object payload size overflow"))?;
            if total_bytes > MAX_BYTES || checkpoint.descriptors.len() > MAX_DESCRIPTORS {
                return Err(invalid("Object payload exceeds checkpoint limits"));
            }
            checkpoint.state.clients[client].objects[object].payload = payload;
        }
        checkpoint.state.descriptor_count = checkpoint.descriptors.len();
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    pub(crate) fn restore_connections(
        &self,
        checkpoint: &Checkpoint,
        decoder: &mut impl ConnectionDecoder<D>,
    ) -> io::Result<()> {
        checkpoint.validate()?;
        let metadata = &checkpoint.state;
        {
            let state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("Poisoned backend restore lock"))?;
            if state.clients.clients.iter().any(Option::is_some)
                || !state.pending_destructors.is_empty()
                || !state.registry.known_registries.is_empty()
            {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Restore requires a backend without clients",
                ));
            }
            if globals(&state) != metadata.globals {
                return Err(invalid(
                    "Recreate the checkpoint's globals before restoring clients",
                ));
            }
        }
        let mut restored = ClientStore::new(metadata.debug);
        restored.last_serial = metadata.client_serial;
        restored.clients = Vec::with_capacity(metadata.client_capacity);
        restored.clients.resize_with(metadata.client_slots, || None);
        let mut endpoint_inodes = BTreeSet::new();
        for client in &metadata.clients {
            let socket = BufferedSocket::restore(&client.socket, &checkpoint.descriptors)?;
            let socket_stat = rustix::fs::fstat(&socket)?;
            if !endpoint_inodes.insert((socket_stat.st_dev, socket_stat.st_ino)) {
                return Err(invalid("Duplicate client socket endpoints"));
            }
            let id = InnerClientId {
                id: client.key.slot,
                serial: client.key.serial,
            };
            let data = decoder.client(client.key, &client.payload, &checkpoint.descriptors)?;
            let mut map = ObjectMap::with_checkpoint_layout(
                client.client_slots,
                client.server_slots,
                client.client_capacity,
                client.server_capacity,
            );
            for object in &client.objects {
                let interface = object_interface(decoder, &object.interface)?;
                if protocol_identity(interface) != object.protocol {
                    return Err(invalid(
                        "Checkpoint wire protocol differs from the new executable",
                    ));
                }
                let info = ObjectInfo {
                    id: object.id,
                    interface,
                    version: object.version,
                };
                let user_data = if object.id == 1 || object.interface == "wl_registry" {
                    Arc::new(DumbObjectData) as Arc<_>
                } else {
                    decoder.object(
                        ObjectKey {
                            client: client.key,
                            id: object.id,
                            serial: object.serial,
                        },
                        info,
                        &object.payload,
                        &checkpoint.descriptors,
                    )?
                };
                map.insert_at(
                    object.id,
                    Object {
                        interface,
                        version: object.version,
                        data: Data {
                            user_data,
                            serial: object.serial,
                        },
                    },
                )
                .map_err(|_| invalid("Cannot restore protocol object slot"))?;
            }
            restored.clients[client.key.slot as usize] = Some(Client {
                socket,
                map,
                debug: client.debug,
                last_serial: client.last_serial,
                id,
                killed: false,
                data,
            });
        }
        let mut registries = Vec::with_capacity(metadata.registry_capacity);
        for registry in &metadata.registries {
            registries.push(InnerObjectId {
                id: registry.id,
                serial: registry.serial,
                client_id: InnerClientId {
                    id: registry.client.slot,
                    serial: registry.client.serial,
                },
                interface: &WL_REGISTRY_INTERFACE,
            });
        }
        let mut real_sockets = BTreeMap::new();
        let mut objects = BTreeMap::new();
        for client in restored.clients.iter_mut().flatten() {
            let (socket, peer) = UnixStream::pair()?;
            socket.set_nonblocking(true)?;
            peer.set_nonblocking(true)?;
            let placeholder = BufferedSocket::new(Socket::from(socket), None);
            real_sockets.insert(
                client_key(&client.id),
                (std::mem::replace(&mut client.socket, placeholder), peer),
            );
            for (id, object) in client.map.all_objects() {
                let id = InnerObjectId {
                    id,
                    serial: object.data.serial,
                    client_id: client.id.clone(),
                    interface: object.interface,
                };
                objects.insert(object_key(&id), ObjectId { id });
            }
        }
        let context = RestoreContext {
            handle: self.handle(),
            objects,
        };
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("Poisoned backend restore lock"))?;
        if state.clients.clients.iter().any(Option::is_some)
            || !state.pending_destructors.is_empty()
            || !state.registry.known_registries.is_empty()
            || globals(&state) != metadata.globals
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Backend changed during restore preparation",
            ));
        }
        let saved_registry = Registry {
            globals: state
                .registry
                .globals
                .iter()
                .map(|global| {
                    global.as_ref().map(|global| Global {
                        id: global.id.clone(),
                        interface: global.interface,
                        version: global.version,
                        handler: global.handler.clone(),
                        disabled: global.disabled,
                    })
                })
                .collect(),
            known_registries: Vec::new(),
            last_serial: state.registry.last_serial,
        };
        let saved_clients = std::mem::replace(&mut state.clients, restored);
        let saved_maximum = state.default_max_buffer_size;
        state.registry.known_registries = registries;
        state.registry.last_serial = metadata.global_serial;
        state.default_max_buffer_size = metadata.default_maximum_buffer;
        drop(state);
        let finalization = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decoder.finish(&context, &checkpoint.descriptors)
        }));
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let outcome = match finalization {
            Ok(Ok(())) => (|| {
                if globals(&state) != metadata.globals
                    || state.registry.last_serial != metadata.global_serial
                    || state
                        .registry
                        .known_registries
                        .iter()
                        .map(object_key)
                        .collect::<Vec<_>>()
                        != metadata.registries
                    || state.clients.clients.len() != metadata.client_slots
                    || state.clients.last_serial != metadata.client_serial
                    || state.clients.clients.iter().flatten().count() != metadata.clients.len()
                    || state.default_max_buffer_size != metadata.default_maximum_buffer
                    || !state.pending_destructors.is_empty()
                {
                    return Err(invalid("Finalizer changed backend topology"));
                }
                for expected in &metadata.clients {
                    let client = state.clients.clients[expected.key.slot as usize]
                        .as_ref()
                        .ok_or_else(|| invalid("Finalizer removed a client"))?;
                    let mut scratch = Vec::new();
                    let socket = client.socket.checkpoint(&mut scratch)?;
                    let (_, peer) = real_sockets.get_mut(&expected.key).unwrap();
                    let mut byte = [0_u8];
                    let peer_untouched = matches!(peer.read(&mut byte), Err(error) if error.kind() == io::ErrorKind::WouldBlock);
                    if client_key(&client.id) != expected.key
                        || client.killed
                        || client.last_serial != expected.last_serial
                        || client.map.checkpoint_layout()
                            != (
                                expected.client_slots,
                                expected.server_slots,
                                expected.client_capacity,
                                expected.server_capacity,
                            )
                        || client.map.all_objects().count() != expected.objects.len()
                        || !peer_untouched
                        || !socket.incoming.is_empty()
                        || !socket.outgoing.is_empty()
                        || !socket.incoming_fds.is_empty()
                        || !socket.outgoing_fds.is_empty()
                    {
                        return Err(invalid(
                            "Finalizer dispatched, sent events or changed object topology",
                        ));
                    }
                    for object in &expected.objects {
                        let restored = client
                            .map
                            .find(object.id)
                            .ok_or_else(|| invalid("Finalizer removed a resource"))?;
                        if restored.data.serial != object.serial
                            || restored.version != object.version
                            || protocol_identity(restored.interface) != object.protocol
                            || restored
                                .data
                                .user_data
                                .downcast_ref::<crate::handoff::DeferredObjectData>()
                                .is_some()
                        {
                            return Err(invalid(
                                "Finalizer left an incomplete resource or changed its identity",
                            ));
                        }
                    }
                }
                for client in state.clients.clients.iter_mut().flatten() {
                    let (socket, _) = real_sockets.remove(&client_key(&client.id)).unwrap();
                    client.socket = socket;
                }
                let mut registered: Vec<&Client<D>> = Vec::new();
                for client in state.clients.clients.iter().flatten() {
                    if let Err(error) = epoll::add(
                        &state.poll_fd,
                        client,
                        epoll::EventData::new_u64(client.id.as_u64()),
                        epoll::EventFlags::IN,
                    ) {
                        for registered_client in registered {
                            let _ = epoll::delete(&state.poll_fd, registered_client);
                        }
                        return Err(error.into());
                    }
                    registered.push(client);
                }
                Ok(())
            })(),
            Ok(Err(error)) => Err(error),
            Err(panic) => {
                let clients = std::mem::replace(&mut state.clients, saved_clients);
                let registry = std::mem::replace(&mut state.registry, saved_registry);
                state.default_max_buffer_size = saved_maximum;
                let destructors = std::mem::take(&mut state.pending_destructors);
                drop(state);
                drop((clients, registry, destructors));
                std::panic::resume_unwind(panic);
            }
        };
        if let Err(error) = outcome {
            let clients = std::mem::replace(&mut state.clients, saved_clients);
            let registry = std::mem::replace(&mut state.registry, saved_registry);
            state.default_max_buffer_size = saved_maximum;
            let destructors = std::mem::take(&mut state.pending_destructors);
            drop(state);
            drop((clients, registry, destructors));
            return Err(error);
        }
        let mut globals = Vec::with_capacity(metadata.global_capacity);
        globals.extend(std::mem::take(&mut state.registry.globals));
        state.registry.globals = globals;
        state.pending_destructors = Vec::with_capacity(metadata.destructor_capacity);
        Ok(())
    }
}

impl<D: 'static> crate::rs::server::Backend<D> {
    #[doc(hidden)]
    pub fn checkpoint_connections(
        &mut self,
        encoder: &mut impl ConnectionEncoder<D>,
    ) -> io::Result<Checkpoint> {
        self.backend.checkpoint_connections(encoder)
    }

    #[doc(hidden)]
    pub fn restore_connections(
        &mut self,
        checkpoint: &Checkpoint,
        decoder: &mut impl ConnectionDecoder<D>,
    ) -> io::Result<()> {
        self.backend.restore_connections(checkpoint, decoder)
    }
}
