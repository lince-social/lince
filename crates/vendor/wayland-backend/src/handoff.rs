use std::{io, os::fd::OwnedFd, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    protocol::{Interface, ObjectInfo},
    rs::server::{ClientData, ClientId, ObjectData, ObjectId},
};

pub const MAX_CLIENTS: usize = 128;
pub const MAX_OBJECTS: usize = 65536;
pub const MAX_SLOTS: usize = 1048576;
pub const MAX_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_DESCRIPTORS: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientKey {
    pub slot: u32,
    pub serial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectKey {
    pub client: ClientKey,
    pub id: u32,
    pub serial: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketState {
    pub descriptor: usize,
    pub incoming: Vec<u8>,
    pub incoming_capacity: usize,
    pub incoming_fds: Vec<usize>,
    pub outgoing: Vec<u8>,
    pub outgoing_capacity: usize,
    pub outgoing_fds: Vec<usize>,
    pub maximum_buffer: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectState {
    pub id: u32,
    pub serial: u32,
    pub interface: String,
    pub protocol: String,
    pub version: u32,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientState {
    pub key: ClientKey,
    pub last_serial: u32,
    pub debug: bool,
    pub client_slots: usize,
    pub server_slots: usize,
    pub client_capacity: usize,
    pub server_capacity: usize,
    pub socket: SocketState,
    pub objects: Vec<ObjectState>,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalState {
    pub serial: u32,
    pub interface: String,
    pub protocol: String,
    pub version: u32,
    pub disabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionState {
    pub schema: u32,
    pub little_endian: bool,
    pub client_slots: usize,
    pub client_capacity: usize,
    pub client_serial: u32,
    pub debug: bool,
    pub default_maximum_buffer: usize,
    pub globals: Vec<Option<GlobalState>>,
    pub global_serial: u32,
    pub global_capacity: usize,
    pub registry_capacity: usize,
    pub destructor_capacity: usize,
    pub registries: Vec<ObjectKey>,
    pub clients: Vec<ClientState>,
    pub descriptor_count: usize,
}

pub struct Checkpoint {
    pub state: ConnectionState,
    pub descriptors: Vec<OwnedFd>,
}

impl std::fmt::Debug for Checkpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checkpoint")
            .field("clients", &self.state.clients.len())
            .field("descriptors", &self.descriptors.len())
            .finish_non_exhaustive()
    }
}

pub trait ConnectionEncoder<D: 'static> {
    fn client(
        &mut self,
        id: ClientId,
        data: &Arc<dyn ClientData>,
        descriptors: &mut Vec<OwnedFd>,
    ) -> io::Result<Vec<u8>>;
    fn object(
        &mut self,
        id: ObjectId,
        data: &Arc<dyn ObjectData<D>>,
        descriptors: &mut Vec<OwnedFd>,
    ) -> io::Result<Vec<u8>>;
}

pub trait ConnectionDecoder<D: 'static> {
    fn interface(&self, name: &str) -> Option<&'static Interface>;
    fn client(
        &mut self,
        key: ClientKey,
        payload: &[u8],
        descriptors: &[OwnedFd],
    ) -> io::Result<Arc<dyn ClientData>>;
    fn object(
        &mut self,
        key: ObjectKey,
        info: ObjectInfo,
        payload: &[u8],
        descriptors: &[OwnedFd],
    ) -> io::Result<Arc<dyn ObjectData<D>>>;

    fn finish(&mut self, _: &RestoreContext, _: &[OwnedFd]) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct RestoreContext {
    pub(crate) handle: crate::rs::server::Handle,
    pub(crate) objects: std::collections::BTreeMap<ObjectKey, ObjectId>,
}

#[derive(Debug)]
pub(crate) struct DeferredObjectData;

impl<D: 'static> ObjectData<D> for DeferredObjectData {
    fn request(
        self: Arc<Self>,
        _: &crate::rs::server::Handle,
        _: &mut D,
        _: ClientId,
        _: crate::protocol::Message<ObjectId, OwnedFd>,
    ) -> Option<Arc<dyn ObjectData<D>>> {
        None
    }

    fn destroyed(
        self: Arc<Self>,
        _: &crate::rs::server::Handle,
        _: &mut D,
        _: ClientId,
        _: ObjectId,
    ) {
    }
}

pub fn deferred_object_data<D: 'static>() -> Arc<dyn ObjectData<D>> {
    Arc::new(DeferredObjectData)
}

impl RestoreContext {
    pub fn object(&self, key: ObjectKey) -> io::Result<ObjectId> {
        self.objects
            .get(&key)
            .cloned()
            .ok_or_else(|| invalid("Checkpoint resource reference is missing or stale"))
    }

    pub fn backend_handle(&self) -> &crate::rs::server::Handle {
        &self.handle
    }
}

pub(crate) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(crate) fn duplicate(fd: impl std::os::fd::AsFd) -> io::Result<OwnedFd> {
    rustix::io::fcntl_dupfd_cloexec(fd, 3).map_err(Into::into)
}

pub(crate) fn protocol_identity(interface: &Interface) -> String {
    let messages = |descriptions: &[crate::protocol::MessageDesc]| {
        descriptions
            .iter()
            .map(|description| {
                format!(
                    "{:?}",
                    (
                        description.name,
                        description.since,
                        description.is_destructor,
                        description.signature,
                        description.child_interface.map(|child| child.name),
                        description
                            .arg_interfaces
                            .iter()
                            .map(|argument| argument.name)
                            .collect::<Vec<_>>(),
                    )
                )
            })
            .collect::<Vec<_>>()
    };
    format!(
        "{:?}",
        (
            interface.name,
            interface.version,
            messages(interface.requests),
            messages(interface.events)
        )
    )
}

impl Checkpoint {
    pub fn validate(&self) -> io::Result<()> {
        self.state.validate(self.descriptors.len())
    }
}

impl ConnectionState {
    pub fn validate(&self, descriptor_count: usize) -> io::Result<()> {
        use std::collections::BTreeSet;
        let state = self;
        if state.schema != 1 || state.little_endian != cfg!(target_endian = "little") {
            return Err(invalid("Unsupported checkpoint schema or byte order"));
        }
        if state.clients.len() > MAX_CLIENTS
            || state.client_slots > MAX_CLIENTS
            || state.client_capacity < state.client_slots
            || state.client_capacity > MAX_CLIENTS
            || state.descriptor_count != descriptor_count
            || descriptor_count > MAX_DESCRIPTORS
            || state.globals.len() > MAX_OBJECTS
            || state.registries.len() > MAX_OBJECTS
            || state.global_capacity < state.globals.len()
            || state.global_capacity > MAX_OBJECTS
            || state.registry_capacity < state.registries.len()
            || state.registry_capacity > MAX_OBJECTS
            || state.destructor_capacity > MAX_OBJECTS
            || state.default_maximum_buffer == 0
            || state.default_maximum_buffer > MAX_BYTES
        {
            return Err(invalid("Checkpoint exceeds backend limits"));
        }
        let mut clients = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut registries = BTreeSet::new();
        let mut descriptor_uses = BTreeSet::new();
        let mut bytes = 0_usize;
        let mut count = 0_usize;
        let mut slots = 0_usize;
        for global in state.globals.iter().flatten() {
            if global.interface.is_empty()
                || global.interface.len() > 256
                || global.protocol.is_empty()
                || global.protocol.len() > 65536
                || global.version == 0
            {
                return Err(invalid("Invalid global metadata"));
            }
            bytes = bytes
                .checked_add(global.protocol.len())
                .ok_or_else(|| invalid("Global metadata size overflow"))?;
        }
        for client in &state.clients {
            if client.key.slot as usize >= state.client_slots
                || !clients.insert(client.key.slot)
                || client.client_slots == 0
                || client.client_slots > MAX_SLOTS
                || client.server_slots > MAX_SLOTS
                || client.client_capacity < client.client_slots
                || client.server_capacity < client.server_slots
                || client.client_capacity > MAX_SLOTS
                || client.server_capacity > MAX_SLOTS
            {
                return Err(invalid("Duplicate client or invalid object-map layout"));
            }
            slots = slots
                .checked_add(client.client_capacity)
                .and_then(|value| value.checked_add(client.server_capacity))
                .ok_or_else(|| invalid("Object-map size overflow"))?;
            if slots > MAX_SLOTS {
                return Err(invalid("Object-map slots exceed checkpoint limit"));
            }
            bytes = bytes
                .checked_add(client.payload.len())
                .and_then(|value| value.checked_add(client.socket.incoming.len()))
                .and_then(|value| value.checked_add(client.socket.outgoing.len()))
                .ok_or_else(|| invalid("Checkpoint byte size overflow"))?;
            if client
                .socket
                .maximum_buffer
                .is_some_and(|maximum| maximum == 0 || maximum > MAX_BYTES)
            {
                return Err(invalid("Invalid socket buffer limit"));
            }
            for (capacity, occupied) in [
                (
                    client.socket.incoming_capacity,
                    client.socket.incoming.len(),
                ),
                (
                    client.socket.outgoing_capacity,
                    client.socket.outgoing.len(),
                ),
            ] {
                if capacity < crate::rs::DEFAULT_MAX_BUFFER_SIZE
                    || !capacity.is_power_of_two()
                    || capacity < occupied
                    || capacity > MAX_BYTES
                {
                    return Err(invalid("Invalid socket buffer allocation"));
                }
                bytes = bytes
                    .checked_add(capacity)
                    .ok_or_else(|| invalid("Socket allocation size overflow"))?;
            }
            for index in std::iter::once(&client.socket.descriptor)
                .chain(&client.socket.incoming_fds)
                .chain(&client.socket.outgoing_fds)
            {
                if *index >= descriptor_count || !descriptor_uses.insert(*index) {
                    return Err(invalid("Invalid or reused connection descriptor index"));
                }
            }
            let mut display = false;
            let mut object_ids = BTreeSet::new();
            for object in &client.objects {
                let valid_slot = if object.id >= crate::rs::map::SERVER_ID_LIMIT {
                    ((object.id - crate::rs::map::SERVER_ID_LIMIT) as usize) < client.server_slots
                } else {
                    object.id != 0 && object.id as usize <= client.client_slots
                };
                let key = ObjectKey {
                    client: client.key,
                    id: object.id,
                    serial: object.serial,
                };
                if !valid_slot
                    || !object_ids.insert(object.id)
                    || !keys.insert(key)
                    || object.interface.is_empty()
                    || object.interface.len() > 256
                    || object.protocol.is_empty()
                    || object.protocol.len() > 65536
                    || object.version == 0
                {
                    return Err(invalid("Invalid or duplicate object metadata"));
                }
                if object.id == 1 {
                    if object.interface != "wl_display"
                        || object.version != 1
                        || object.serial != 0
                        || !object.payload.is_empty()
                    {
                        return Err(invalid("Invalid display object"));
                    }
                    display = true;
                } else if object.interface == "wl_display" {
                    return Err(invalid("Unexpected display object"));
                }
                if object.interface == "wl_registry" {
                    if object.version != 1 || !object.payload.is_empty() {
                        return Err(invalid("Invalid registry object"));
                    }
                    registries.insert(key);
                }
                bytes = bytes
                    .checked_add(object.payload.len())
                    .and_then(|value| value.checked_add(object.protocol.len()))
                    .ok_or_else(|| invalid("Checkpoint payload size overflow"))?;
                count += 1;
            }
            if !display {
                return Err(invalid("Checkpoint is missing a client display"));
            }
        }
        let mut seen_registries = BTreeSet::new();
        for registry in &state.registries {
            if !registries.contains(registry) || !seen_registries.insert(*registry) {
                return Err(invalid("Invalid registry binding"));
            }
        }
        if registries != seen_registries || bytes > MAX_BYTES || count > MAX_OBJECTS {
            return Err(invalid(
                "Incomplete registries or excessive checkpoint contents",
            ));
        }
        Ok(())
    }
}
