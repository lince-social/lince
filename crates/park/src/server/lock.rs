use super::*;
use smithay::{
    reexports::{
        wayland_protocols::ext::session_lock::v1::server::{
            ext_session_lock_manager_v1::ExtSessionLockManagerV1,
            ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
            ext_session_lock_v1::{self, ExtSessionLockV1},
        },
        wayland_server::{DataInit, Dispatch, protocol::wl_output::WlOutput},
    },
    wayland::session_lock::{
        ExtLockSurfaceUserData, LockSurface, SessionLockHandler, SessionLockManagerGlobalData,
        SessionLockManagerState, SessionLockState, SessionLocker,
    },
};

pub(super) struct Lock {
    revision: u64,
    state: SessionLockManagerState,
    pub(super) owner: Option<ExtSessionLockV1>,
    confirmation: Option<SessionLocker>,
    pub(super) surfaces: BTreeMap<OutputId, LockSurface>,
}

impl Lock {
    pub(super) fn new(handle: &DisplayHandle) -> Self {
        Self {
            revision: 0,
            state: SessionLockManagerState::new::<State, _>(handle, |_| true),
            owner: None,
            confirmation: None,
            surfaces: BTreeMap::new(),
        }
    }
}

impl Server {
    #[cfg(feature = "session")]
    pub(crate) fn lock_stamp(&self) -> u64 {
        self.state.lock.revision
    }

    pub fn locked(&self) -> bool {
        self.state.lock.owner.is_some()
    }

    pub fn lock_presented(&mut self) {
        if let Some(confirmation) = self.state.lock.confirmation.take() {
            confirmation.lock();
        }
    }

    pub fn lock_surfaces(&self) -> Vec<(OutputId, [i32; 2], Vec<SurfaceNode>)> {
        self.state
            .lock
            .surfaces
            .iter()
            .filter_map(|(id, surface)| {
                let output = &self.state.outputs.get(id)?.output;
                let origin = output.current_location();
                surface.wl_surface().is_alive().then(|| {
                    (
                        *id,
                        [origin.x, origin.y],
                        self.state.nodes(surface.wl_surface()),
                    )
                })
            })
            .collect()
    }

    pub fn focus_lock(&mut self, output: Option<OutputId>) {
        let target = output
            .and_then(|id| self.state.lock.surfaces.get(&id))
            .or_else(|| self.state.lock.surfaces.values().next())
            .filter(|surface| surface.wl_surface().is_alive())
            .map(|surface| surface.wl_surface().clone());
        self.focus_surface(target, smithay::utils::SERIAL_COUNTER.next_serial());
    }

    pub fn lock_motion(&mut self, point: [f64; 2], time: u32) -> io::Result<()> {
        let target = self
            .lock_surfaces()
            .into_iter()
            .find_map(|(_, origin, nodes)| {
                let local = [
                    point[0] - f64::from(origin[0]),
                    point[1] - f64::from(origin[1]),
                ];
                nodes
                    .into_iter()
                    .find_map(|node| node.hit(local).map(|local| (node.surface, local)))
            });
        self.pointer_motion(target, time)
    }
}

impl State {
    pub(super) fn lock_allows(&self, surface: &WlSurface) -> bool {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        self.lock
            .surfaces
            .values()
            .any(|surface| surface.wl_surface() == &root)
    }

    pub(super) fn configure_locks(&mut self) {
        self.lock
            .surfaces
            .retain(|id, _| self.outputs.contains_key(id));
        for (id, surface) in &self.lock.surfaces {
            let output = &self.outputs[id].output;
            if let Some(size) = super::outputs::logical_size(output) {
                surface.with_pending_state(|state| {
                    state.size = Some((size.w as u32, size.h as u32).into())
                });
                surface.send_configure();
            }
        }
    }
}

impl SessionLockHandler for State {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.lock.state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        if self.lock.owner.is_some() {
            return;
        }
        self.lock.revision = self.lock.revision.checked_add(1).expect("session lock generation exhausted");
        self.lock.owner = Some(confirmation.ext_session_lock().clone());
        self.control.locked(true);
        self.lock.confirmation = Some(confirmation);
        self.stop_capture();
        self.authority.clear();
        self.activations.clear();
        self.activation.retain_tokens(|_, _| false);
        self.input_epoch = self.input_epoch.saturating_add(1);
        self.invalidate_workspace_requests();
        self.drag_icon = None;
        self.drag_origin = None;
        self.dragging = false;
        self.popups.grab = None;
        self.popups.root = None;
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.unset_grab(self);
            keyboard.set_focus(self, None, smithay::utils::SERIAL_COUNTER.next_serial());
        }
        if let Some(pointer) = self.seat.get_pointer() {
            pointer.unset_grab(self, smithay::utils::SERIAL_COUNTER.next_serial(), 0);
            pointer.motion(
                self,
                None,
                &smithay::input::pointer::MotionEvent {
                    location: (0.0, 0.0).into(),
                    serial: smithay::utils::SERIAL_COUNTER.next_serial(),
                    time: 0,
                },
            );
            pointer.frame(self);
        }
        self.changed();
    }

    fn unlock(&mut self) {
        self.lock.revision = self.lock.revision.checked_add(1).expect("session lock generation exhausted");
        self.control.locked(false);
        self.lock.owner = None;
        self.lock.confirmation = None;
        self.lock.surfaces.clear();
        self.changed();
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        if self
            .lock
            .owner
            .as_ref()
            .and_then(Resource::client)
            .map(|client| client.id())
            != surface.wl_surface().client().map(|client| client.id())
        {
            return;
        }
        let output = Output::from_resource(&output);
        if let Some(id) = self
            .outputs
            .iter()
            .find(|(_, registered)| Some(&registered.output) == output.as_ref())
            .map(|(id, _)| *id)
        {
            self.lock.surfaces.insert(id, surface);
            self.configure_locks();
            self.presentation_changed();
        }
    }
}

impl Dispatch<ExtSessionLockV1, SessionLockState> for State {
    fn request(
        state: &mut Self,
        client: &Client,
        resource: &ExtSessionLockV1,
        request: ext_session_lock_v1::Request,
        data: &SessionLockState,
        handle: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if matches!(request, ext_session_lock_v1::Request::UnlockAndDestroy)
            && (state.lock.owner.as_ref() != Some(resource) || state.lock.confirmation.is_some())
        {
            resource.post_error(
                ext_session_lock_v1::Error::InvalidUnlock,
                "Only the confirmed lock owner may unlock Park",
            );
            return;
        }
        <SessionLockManagerState as Dispatch<ExtSessionLockV1, SessionLockState, Self>>::request(
            state, client, resource, request, data, handle, init,
        );
    }
}

smithay::reexports::wayland_server::delegate_global_dispatch!(State: [ExtSessionLockManagerV1: SessionLockManagerGlobalData] => SessionLockManagerState);
smithay::reexports::wayland_server::delegate_dispatch!(State: [ExtSessionLockManagerV1: ()] => SessionLockManagerState);
smithay::reexports::wayland_server::delegate_dispatch!(State: [ExtSessionLockSurfaceV1: ExtLockSurfaceUserData] => SessionLockManagerState);
