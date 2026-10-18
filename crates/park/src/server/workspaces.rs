use super::*;
use smithay::reexports::{
    wayland_protocols::ext::workspace::v1::server::{
        ext_workspace_manager_v1::{self as manager, ExtWorkspaceManagerV1 as Manager},
        ext_workspace_group_handle_v1::{self as group, ExtWorkspaceGroupHandleV1 as Group},
        ext_workspace_handle_v1::{self as workspace, ExtWorkspaceHandleV1 as Handle},
    },
    wayland_server::{DataInit, Dispatch, GlobalDispatch, New, protocol::wl_output::WlOutput},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub owner: u64,
    pub entries: Vec<WorkspaceEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceEntry { pub id: u64, pub name: String, pub active: bool }

struct Tracked { handle: Handle, entry: WorkspaceEntry, index: u32 }

struct Binding {
    manager: Manager,
    group: Option<Group>,
    outputs: Vec<WlOutput>,
    owner: u64,
    handles: BTreeMap<u64, Tracked>,
    suppressed: BTreeSet<u64>,
    pending: Option<(u64, u64)>,
    stopped: bool,
}

pub(super) struct HandleData { manager: Manager, owner: u64, id: u64 }

#[derive(Default)]
pub(super) struct Workspaces {
    snapshot: WorkspaceSnapshot,
    bindings: Vec<Binding>,
    activations: VecDeque<(u64, u64)>,
}

fn permitted(client: &Client, control: bool) -> bool {
    client.get_data::<ClientState>().is_some_and(|data| data.capabilities.contains(&crate::launch::Capability::WindowControl)
        || !control && data.capabilities.contains(&crate::launch::Capability::WindowList))
}

impl Workspaces {
    pub(super) fn new(display: &DisplayHandle) -> Self {
        display.create_global::<State, Manager, _>(1, ());
        Self::default()
    }
}

impl Server {
    pub fn workspaces(&mut self, snapshot: WorkspaceSnapshot) {
        let mut ids = BTreeSet::new();
        if snapshot.entries.len() > 4096 || snapshot.entries.iter().any(|entry| entry.id == 0 || entry.name.len() > 4096
            || entry.name.contains('\0') || !ids.insert(entry.id)) { return; }
        if self.state.workspaces.snapshot != snapshot {
            self.state.workspaces.activations.retain(|(owner, id)| *owner == snapshot.owner && snapshot.entries.iter().any(|entry| entry.id == *id));
            self.state.workspaces.snapshot = snapshot;
            self.state.sync_workspaces();
        }
    }

    pub fn workspace_activations(&mut self) -> Vec<(u64, u64)> {
        if self.locked() { self.state.workspaces.activations.clear(); return Vec::new(); }
        let snapshot = &self.state.workspaces.snapshot;
        self.state.workspaces.activations.drain(..)
            .filter(|(owner, id)| *owner == snapshot.owner && snapshot.entries.iter().any(|entry| entry.id == *id)).collect()
    }
}

impl State {
    pub(super) fn invalidate_workspace_requests(&mut self) {
        self.workspaces.activations.clear();
        for binding in &mut self.workspaces.bindings { binding.pending = None; }
    }

    pub(super) fn sync_workspaces(&mut self) {
        let snapshot = &self.workspaces.snapshot;
        self.workspaces.bindings.retain(|binding| binding.manager.is_alive()
            || binding.group.as_ref().is_some_and(Resource::is_alive) || binding.handles.values().any(|tracked| tracked.handle.is_alive()));
        for binding in &mut self.workspaces.bindings {
            if binding.stopped || !binding.manager.is_alive() { continue; }
            let Some(client) = binding.manager.client() else { continue };
            let mut changed = false;
            let outputs: Vec<_> = self.outputs.values().flat_map(|registered| registered.output.client_outputs(&client)).collect();
            if let Some(group) = binding.group.as_ref().filter(|group| group.is_alive()) {
                for output in &binding.outputs {
                    if output.is_alive() && !outputs.contains(output) { group.output_leave(output); changed = true; }
                }
                for output in &outputs {
                    if !binding.outputs.contains(output) { group.output_enter(output); changed = true; }
                }
            }
            binding.outputs = outputs;
            let owner_changed = binding.owner != snapshot.owner;
            binding.handles.retain(|id, tracked| {
                if !tracked.handle.is_alive() { return false; }
                if !owner_changed && snapshot.entries.iter().any(|entry| entry.id == *id) { return true; }
                if let Some(group) = binding.group.as_ref().filter(|group| group.is_alive()) { group.workspace_leave(&tracked.handle); }
                tracked.handle.removed();
                changed = true;
                false
            });
            if owner_changed { binding.suppressed.clear(); binding.pending = None; }
            if binding.pending.is_some_and(|(owner, id)| owner != snapshot.owner || !snapshot.entries.iter().any(|entry| entry.id == id)) { binding.pending = None; }
            binding.owner = snapshot.owner;
            binding.suppressed.retain(|id| snapshot.entries.iter().any(|entry| entry.id == *id));
            for (index, entry) in snapshot.entries.iter().enumerate() {
                if binding.suppressed.contains(&entry.id) { continue; }
                let index = index as u32;
                if let Some(tracked) = binding.handles.get_mut(&entry.id) {
                    if tracked.entry.name != entry.name { tracked.handle.name(entry.name.clone()); changed = true; }
                    if tracked.entry.active != entry.active {
                        tracked.handle.state(if entry.active { workspace::State::Active } else { workspace::State::empty() }); changed = true;
                    }
                    if tracked.index != index { tracked.handle.coordinates(index.to_ne_bytes().to_vec()); changed = true; }
                    tracked.entry = entry.clone();
                    tracked.index = index;
                    continue;
                }
                let Ok(handle) = client.create_resource::<Handle, _, State>(&self.handle, 1,
                    HandleData { manager: binding.manager.clone(), owner: snapshot.owner, id: entry.id }) else { continue };
                binding.manager.workspace(&handle);
                handle.name(entry.name.clone());
                handle.coordinates(index.to_ne_bytes().to_vec());
                handle.state(if entry.active { workspace::State::Active } else { workspace::State::empty() });
                handle.capabilities(if permitted(&client, true) { workspace::WorkspaceCapabilities::Activate } else { workspace::WorkspaceCapabilities::empty() });
                if let Some(group) = binding.group.as_ref().filter(|group| group.is_alive()) { group.workspace_enter(&handle); }
                binding.handles.insert(entry.id, Tracked { handle, entry: entry.clone(), index });
                changed = true;
            }
            if changed { binding.manager.done(); }
        }
    }
}

impl GlobalDispatch<Manager, ()> for State {
    fn bind(state: &mut Self, display: &DisplayHandle, client: &Client, resource: New<Manager>, _: &(), data: &mut DataInit<'_, Self>) {
        let manager = data.init(resource, ());
        if state.workspaces.bindings.len() >= 128 { manager.finished(); return; }
        let Ok(group) = client.create_resource::<Group, _, State>(display, 1, manager.clone()) else { manager.finished(); return };
        manager.workspace_group(&group);
        group.capabilities(group::GroupCapabilities::empty());
        state.workspaces.bindings.push(Binding { manager: manager.clone(), group: Some(group), outputs: Vec::new(), owner: 0,
            handles: BTreeMap::new(), suppressed: BTreeSet::new(), pending: None, stopped: false });
        state.sync_workspaces();
        manager.done();
    }

    fn can_view(client: Client, _: &()) -> bool { permitted(&client, false) }
}

impl Dispatch<Manager, ()> for State {
    fn request(state: &mut Self, client: &Client, resource: &Manager, request: manager::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        let Some(binding) = state.workspaces.bindings.iter_mut().find(|binding| &binding.manager == resource && !binding.stopped) else { return };
        match request {
            manager::Request::Stop => { binding.stopped = true; binding.pending = None; resource.finished(); }
            manager::Request::Commit => {
                let pending = binding.pending.take();
                if permitted(client, true) && state.lock.owner.is_none() && let Some((owner, id)) = pending
                    && owner == state.workspaces.snapshot.owner && state.workspaces.snapshot.entries.iter().any(|entry| entry.id == id)
                    && state.workspaces.activations.len() < 64 {
                    state.workspaces.activations.push_back((owner, id));
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<Handle, HandleData> for State {
    fn request(state: &mut Self, client: &Client, resource: &Handle, request: workspace::Request, data: &HandleData, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if !matches!(request, workspace::Request::Activate) || !permitted(client, true) || state.lock.owner.is_some() { return; }
        let Some(binding) = state.workspaces.bindings.iter_mut().find(|binding| binding.manager == data.manager && !binding.stopped
            && binding.owner == data.owner && binding.handles.get(&data.id).is_some_and(|tracked| &tracked.handle == resource)) else { return };
        binding.pending = Some((data.owner, data.id));
    }

    fn destroyed(state: &mut Self, _: ClientId, resource: &Handle, data: &HandleData) {
        if let Some(binding) = state.workspaces.bindings.iter_mut().find(|binding| binding.manager == data.manager
            && binding.handles.get(&data.id).is_some_and(|tracked| &tracked.handle == resource)) {
            binding.handles.remove(&data.id);
            binding.suppressed.insert(data.id);
            if binding.pending == Some((data.owner, data.id)) { binding.pending = None; }
        }
    }
}

impl Dispatch<Group, Manager> for State {
    fn request(_: &mut Self, _: &Client, _: &Group, _: group::Request, _: &Manager, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}

    fn destroyed(state: &mut Self, _: ClientId, resource: &Group, manager: &Manager) {
        if let Some(binding) = state.workspaces.bindings.iter_mut().find(|binding| &binding.manager == manager && binding.group.as_ref() == Some(resource)) {
            binding.group = None;
            binding.outputs.clear();
        }
    }
}
