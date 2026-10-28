use super::*;
use crate::control_protocol::server::lince_park_control_v1::Request;
use smithay::reexports::wayland_server::{DataInit, Dispatch, GlobalDispatch, New};

fn permitted(client: &Client) -> bool {
    client.get_data::<ClientState>().is_some_and(|data| data.capabilities.contains(&crate::launch::Capability::WindowList) || data.capabilities.contains(&crate::launch::Capability::DesktopControl))
}

fn integer(value: &str) -> Option<u64> {
    (value.len() <= 20 && !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())).then(|| value.parse().ok()).flatten()
}

impl GlobalDispatch<Manager, ()> for State {
    fn can_view(client: Client, _: &()) -> bool { permitted(&client) }

    fn bind(state: &mut Self, _: &DisplayHandle, client: &Client, resource: New<Manager>, _: &(), data: &mut DataInit<'_, Self>) {
        let manager = data.init(resource, ());
        let Some(client_data) = client.get_data::<ClientState>().filter(|_| permitted(client)) else { manager.post_error(0_u32, "The management connection is not authorized"); return; };
        if state.control.bindings.len() >= MAX_CLIENTS || state.control.bindings.iter().any(|binding| binding.manager.is_alive() && binding.manager.client().is_some_and(|owner| owner.id() == client.id())) {
            manager.post_error(0_u32, "Only one management binding per authorized connection is supported"); return;
        }
        state.control.bindings.push(Binding { manager, connected: client_data.connected.clone(), alive: Arc::new(AtomicBool::new(true)), grants: client_data.capabilities.clone(), pending: BTreeMap::new(), rate: Rate::default(), flow: Flow::default(), subscribed: false, dirty: 0, changes: 0, last_change: None });
        state.refresh_control();
    }
}

impl Dispatch<Manager, ()> for State {
    fn request(state: &mut Self, _: &Client, resource: &Manager, request: Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        let Some(index) = state.control.bindings.iter().position(|binding| &binding.manager == resource && binding.alive.load(Ordering::Acquire)) else { return; };
        match request {
            Request::Destroy => { state.control.bindings[index].alive.store(false, Ordering::Release); }
            Request::Acknowledge { serial, token } => {
                let binding = &mut state.control.bindings[index];
                if !integer(&serial).is_some_and(|serial| binding.flow.acknowledge(serial, &token)) { binding.fatal("The acknowledgement does not match a delivered management event"); }
            }
            Request::Subscribe { enabled } => {
                let binding = &mut state.control.bindings[index];
                if !binding.rate.take(Instant::now()) { binding.failed(0, Error::RateLimited, "Management requests exceeded the connection rate limit."); return; }
                if enabled > 1 || !binding.grants.contains(&crate::launch::Capability::WindowList) { binding.failed(0, Error::Denied, "Observing desktop state requires WindowList permission."); return; }
                binding.subscribed = enabled == 1;
                if binding.subscribed { binding.dirty |= contract::CHANGE_RESYNC; binding.changes = 1; }
                else { binding.dirty = 0; binding.changes = 0; }
            }
            Request::Snapshot { id, revision, cursor } => {
                let binding = &mut state.control.bindings[index];
                if !binding.admission(id) { return; }
                if !binding.grants.contains(&crate::launch::Capability::WindowList) { binding.failed(id, Error::Denied, "Reading desktop state requires WindowList permission."); return; }
                let Some(revision) = integer(&revision) else { binding.failed(id, Error::InvalidArguments, "Use a decimal snapshot revision."); return; };
                let response = snapshot::page(&state.control.entries, state.control.revision, revision, cursor);
                binding.reply(id, response);
            }
            Request::Invoke { id, body } => {
                let total: usize = state.control.bindings.iter().map(|binding| binding.pending.len()).sum();
                let binding = &mut state.control.bindings[index];
                if !binding.admission(id) { return; }
                if !binding.grants.contains(&crate::launch::Capability::DesktopControl) { binding.failed(id, Error::Denied, "Invoking desktop commands requires DesktopControl permission."); return; }
                if state.control.locked.load(Ordering::Acquire) { binding.failed(id, Error::Locked, "Desktop commands are unavailable while the session is locked."); return; }
                if !state.control.active.load(Ordering::Acquire) { binding.failed(id, Error::Unavailable, "The Park seat or nested view is inactive."); return; }
                if state.control.input_owned.load(Ordering::Acquire) { binding.failed(id, Error::Unavailable, "Finish the current application popup or input-method interaction before using desktop automation."); return; }
                if body.len() > contract::ARGUMENT_LIMIT { binding.failed(id, Error::InvalidArguments, "The management invocation exceeds 16 KiB."); return; }
                let invocation = match serde_json::from_str::<Invocation>(&body) {
                    Ok(invocation) => invocation,
                    Err(_) => { binding.failed(id, Error::InvalidArguments, "Use the typed invocation schema; extra fields and invalid values are refused."); return; }
                };
                if let Err(failure) = invocation.validate() { binding.reply(id, Response::Failed { failure }); return; }
                if binding.pending.len() >= contract::IN_FLIGHT_LIMIT || total >= 128 { binding.failed(id, Error::Busy, "The bounded management queue is busy. Retry after a terminal reply."); return; }
                let Some(ticket) = state.control.next_ticket.checked_add(1) else { binding.failed(id, Error::Unavailable, "Management request identifiers are exhausted."); return; };
                state.control.next_ticket = ticket;
                let guard = Guard { connected: binding.connected.clone(), alive: binding.alive.clone(), cancelled: Arc::new(AtomicBool::new(false)), locked: state.control.locked.clone(), active: state.control.active.clone(), input_owned: state.control.input_owned.clone(), target_live: Arc::new(AtomicBool::new(true)), epoch: state.control.epoch.clone(), captured: state.control.epoch.load(Ordering::Acquire) };
                let (reply, received) = sync_channel(1);
                binding.pending.insert(id, Pending { ticket, invocation: invocation.clone(),
                    #[cfg(feature = "native-presentation")]
                    grants: binding.grants.clone(), guard: guard.clone(), reply: received, started: Instant::now(), host: false });
                state.control.queued.push_back(Work { invocation, grants: binding.grants.clone(), guard, reply });
            }
        }
        state.refresh_control();
    }

    fn destroyed(state: &mut Self, _: ClientId, resource: &Manager, _: &()) {
        if let Some(binding) = state.control.bindings.iter().find(|binding| &binding.manager == resource) { binding.alive.store(false, Ordering::Release); }
    }
}
