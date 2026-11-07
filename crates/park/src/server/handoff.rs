use super::*;
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ProtocolInventory {
    pub interface: String,
    pub versions: BTreeSet<u32>,
    pub objects: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct HandoffInventory {
    pub schema: u32,
    pub clients: usize,
    pub objects: usize,
    pub protocols: Vec<ProtocolInventory>,
    pub surfaces: usize,
    pub mapped_windows: usize,
    pub pending_frame_callbacks: usize,
    pub retained_buffers: usize,
    pub locked: bool,
    pub supported: bool,
    pub blockers: Vec<String>,
}

impl Server {
    pub fn handoff_inventory(&self) -> io::Result<HandoffInventory> {
        let backend = self.display.handle().backend_handle();
        let mut clients = Vec::new();
        backend.with_all_clients(|client| clients.push(client));
        let mut protocols = BTreeMap::<String, ProtocolInventory>::new();
        let mut total = 0;
        for client in &clients {
            let mut objects = Vec::new();
            backend
                .with_all_objects_for(client.clone(), |object| objects.push(object))
                .map_err(|error| {
                    io::Error::other(format!("Client changed during handoff inventory: {error}"))
                })?;
            for object in objects {
                let info = backend.object_info(object).map_err(|error| {
                    io::Error::other(format!("Object changed during handoff inventory: {error}"))
                })?;
                let name = info.interface.name.to_owned();
                let protocol = protocols.entry(name.clone()).or_insert(ProtocolInventory {
                    interface: name,
                    versions: BTreeSet::new(),
                    objects: 0,
                });
                protocol.versions.insert(info.version);
                protocol.objects += 1;
                total += 1;
            }
        }
        Ok(HandoffInventory {
            schema: 1,
            clients: clients.len(),
            objects: total,
            protocols: protocols.into_values().collect(),
            surfaces: self.state.surfaces.len(),
            mapped_windows: self.state.windows.values().filter(|window| window.mapped).count(),
            pending_frame_callbacks: self.state.callbacks.values().flat_map(|queue| queue.iter()).map(|batch| batch.callbacks.len()).sum(),
            retained_buffers: self.state.buffers.len(),
            locked: self.locked(),
            supported: false,
            blockers: vec![
                "Park client and protocol checkpoint adapters have not been implemented or qualified against the maintained backend handoff API".into(),
                "Smithay surface caches, protocol user data and registered callbacks have no complete typed checkpoint adapter".into(),
                "Park display/seat/GPU, native interface and Cell state have no full restore path".into(),
            ],
        })
    }
}
