use super::*;
use smithay::reexports::wayland_server::{DataInit, Dispatch, GlobalDispatch, New};
use wayland_protocols_wlr::foreign_toplevel::v1::server::{
    zwlr_foreign_toplevel_handle_v1::{self as handle, ZwlrForeignToplevelHandleV1 as Handle},
    zwlr_foreign_toplevel_manager_v1::{self as manager, ZwlrForeignToplevelManagerV1 as Manager},
};

#[derive(Clone, PartialEq)]
struct Snapshot {
    id: SurfaceId,
    title: String,
    app_id: String,
    states: Vec<u32>,
    parent: Option<SurfaceId>,
    outputs: Vec<OutputId>,
}

struct Tracked {
    handle: Handle,
    snapshot: Option<Snapshot>,
    parent: Option<Handle>,
    outputs: Vec<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
}

struct Binding {
    manager: Manager,
    stopped: bool,
    handles: BTreeMap<SurfaceId, Tracked>,
    suppressed: BTreeSet<SurfaceId>,
}

#[derive(Default)]
pub(super) struct Management { bindings: Vec<Binding> }

fn permitted(client: &Client, control: bool) -> bool {
    client.get_data::<ClientState>().is_some_and(|data| {
        data.capabilities.contains(&crate::launch::Capability::WindowControl)
            || !control && data.capabilities.contains(&crate::launch::Capability::WindowList)
    })
}

impl Management {
    pub(super) fn new(display: &DisplayHandle) -> Self {
        display.create_global::<State, Manager, _>(3, ());
        Self::default()
    }
}

impl State {
    pub(super) fn sync_management(&mut self) {
        use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as XdgState;
        let focused = self.seat.get_keyboard().and_then(|keyboard| keyboard.current_focus());
        let snapshots: Vec<_> = self.windows.iter().filter(|(_, window)| window.mapped && window.role.wl_surface().is_alive())
            .map(|(id, window)| {
                let (title, app_id, parent) = with_states(window.role.wl_surface(), |states| {
                    let metadata = states.data_map.get::<XdgToplevelSurfaceData>().unwrap().lock().unwrap_or_else(|error| error.into_inner());
                    (bounded(metadata.title.as_deref().unwrap_or_default()), bounded(metadata.app_id.as_deref().unwrap_or_default()),
                        metadata.parent.as_ref().and_then(|parent| self.id(parent)))
                });
                let current = window.role.current_state();
                let mut states = Vec::new();
                if window.minimized { states.push(handle::State::Minimized as u32); }
                if current.states.contains(XdgState::Maximized) { states.push(handle::State::Maximized as u32); }
                if focused.as_ref() == Some(window.role.wl_surface()) { states.push(handle::State::Activated as u32); }
                if current.states.contains(XdgState::Fullscreen) { states.push(handle::State::Fullscreen as u32); }
                let outputs = self.presented_geometry.as_ref().map(|geometry| geometry.items.iter()
                    .filter(|item| item.id == lince_interface::host::ItemId::Application(*id)).map(|item| item.output).collect()).unwrap_or_default();
                Snapshot { id: *id, title, app_id, parent, states, outputs }
            }).collect();
        self.management.bindings.retain(|binding| binding.manager.is_alive() || binding.handles.values().any(|tracked| tracked.handle.is_alive()));
        for binding in &mut self.management.bindings {
            binding.suppressed.retain(|id| snapshots.iter().any(|snapshot| snapshot.id == *id));
            binding.handles.retain(|id, tracked| {
                if !tracked.handle.is_alive() { return false; }
                if snapshots.iter().any(|snapshot| snapshot.id == *id) { return true; }
                tracked.handle.closed();
                false
            });
            let Some(client) = binding.manager.client().or_else(|| binding.handles.values().find_map(|tracked| tracked.handle.client())) else { continue };
            if !binding.stopped && binding.manager.is_alive() {
                for snapshot in &snapshots {
                    if binding.handles.contains_key(&snapshot.id) || binding.suppressed.contains(&snapshot.id) { continue; }
                    let Ok(resource) = client.create_resource::<Handle, _, State>(&self.handle, binding.manager.version(), snapshot.id) else { continue };
                    binding.manager.toplevel(&resource);
                    binding.handles.insert(snapshot.id, Tracked { handle: resource, snapshot: None, parent: None, outputs: Vec::new() });
                }
            }
            for snapshot in &snapshots {
                let parent = snapshot.parent.and_then(|id| binding.handles.get(&id)).map(|tracked| tracked.handle.clone());
                let Some(tracked) = binding.handles.get_mut(&snapshot.id) else { continue };
                let outputs: Vec<_> = snapshot.outputs.iter().filter_map(|id| self.outputs.get(id))
                    .flat_map(|output| output.output.client_outputs(&client)).collect();
                if tracked.snapshot.as_ref() == Some(snapshot) && tracked.outputs == outputs && tracked.parent == parent { continue; }
                if tracked.snapshot.as_ref().is_none_or(|old| old.title != snapshot.title) { tracked.handle.title(snapshot.title.clone()); }
                if tracked.snapshot.as_ref().is_none_or(|old| old.app_id != snapshot.app_id) { tracked.handle.app_id(snapshot.app_id.clone()); }
                for output in &tracked.outputs {
                    if output.is_alive() && !outputs.contains(output) { tracked.handle.output_leave(output); }
                }
                for output in &outputs {
                    if !tracked.outputs.contains(output) { tracked.handle.output_enter(output); }
                }
                let states: Vec<u8> = snapshot.states.iter().filter(|state| tracked.handle.version() >= 2 || **state != handle::State::Fullscreen as u32)
                    .flat_map(|state| state.to_ne_bytes()).collect();
                tracked.handle.state(states);
                if tracked.handle.version() >= 3 { tracked.handle.parent(parent.as_ref()); }
                tracked.handle.done();
                tracked.snapshot = Some(snapshot.clone());
                tracked.parent = parent;
                tracked.outputs = outputs;
            }
        }
    }
}

impl GlobalDispatch<Manager, ()> for State {
    fn bind(state: &mut Self, _: &DisplayHandle, _: &Client, resource: New<Manager>, _: &(), data: &mut DataInit<'_, Self>) {
        let manager = data.init(resource, ());
        if state.management.bindings.len() >= 128 {
            manager.finished();
            return;
        }
        state.management.bindings.push(Binding { manager, stopped: false, handles: BTreeMap::new(), suppressed: BTreeSet::new() });
        state.sync_management();
    }

    fn can_view(client: Client, _: &()) -> bool { permitted(&client, false) }
}

impl Dispatch<Manager, ()> for State {
    fn request(state: &mut Self, _: &Client, resource: &Manager, request: manager::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if let manager::Request::Stop = request {
            if let Some(binding) = state.management.bindings.iter_mut().find(|binding| &binding.manager == resource) { binding.stopped = true; }
            resource.finished();
        }
    }
}

impl Dispatch<Handle, SurfaceId> for State {
    fn request(state: &mut Self, client: &Client, resource: &Handle, request: handle::Request, id: &SurfaceId, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        if !state.management.bindings.iter().any(|binding| binding.handles.get(id).is_some_and(|tracked| &tracked.handle == resource)) { return; }
        if let handle::Request::SetRectangle { width, height, .. } = &request {
            if *width < 0 || *height < 0 { resource.post_error(handle::Error::InvalidRectangle, "Rectangle dimensions must be nonnegative"); }
            return;
        }
        if !permitted(client, true) || state.lock.owner.is_some() || !state.windows.get(id).is_some_and(|window| window.mapped && window.role.wl_surface().is_alive()) { return; }
        match request {
            handle::Request::SetMaximized => state.requests.entry(*id).or_default().maximized = Some(true),
            handle::Request::SetMinimized => state.requests.entry(*id).or_default().minimized = Some(true),
            handle::Request::UnsetMinimized => state.requests.entry(*id).or_default().minimized = Some(false),
            handle::Request::UnsetMaximized => state.requests.entry(*id).or_default().maximized = Some(false),
            handle::Request::SetFullscreen { output } => {
                if output.as_ref().is_none_or(|output| Output::from_resource(output).is_some_and(|output| state.outputs.values().any(|registered| registered.output == output))) {
                    state.requests.entry(*id).or_default().fullscreen = Some(true);
                }
            }
            handle::Request::UnsetFullscreen => state.requests.entry(*id).or_default().fullscreen = Some(false),
            handle::Request::Activate { seat } if Seat::<State>::from_resource(&seat).as_ref() == Some(&state.seat) => {
                if state.activations.len() < 64 {
                    state.activations.retain(|(pending, _, _)| pending != id);
                    state.activations.push_back((*id, Instant::now(), state.input_epoch));
                }
            }
            handle::Request::Close => { if let Some(window) = state.windows.get(id) { window.role.send_close(); } }
            _ => {}
        }
    }

    fn destroyed(state: &mut Self, _: ClientId, resource: &Handle, id: &SurfaceId) {
        for binding in &mut state.management.bindings {
            if binding.handles.get(id).is_some_and(|tracked| &tracked.handle == resource) {
                binding.handles.remove(id);
                binding.suppressed.insert(*id);
            }
        }
    }
}
