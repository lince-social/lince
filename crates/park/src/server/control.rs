use super::*;
use crate::control_protocol::server::lince_park_control_v1::LinceParkControlV1 as Manager;
use smithay::wayland::input_method::InputMethodSeat;
use lince_interface::host::control::{self as contract, Catalog, Disposition, Entry, Error, Failure, Invocation, Response};
#[cfg(feature = "native-presentation")]
use lince_interface::host::DesktopAction;
use std::{sync::{atomic::AtomicU64, mpsc::{Receiver, SyncSender, TryRecvError, sync_channel}}, time::Duration};

mod flow;
mod snapshot;
mod transport;
use flow::{Flow, Rate};

#[derive(Clone)]
struct Guard {
    connected: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    locked: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
    input_owned: Arc<AtomicBool>,
    target_live: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
    captured: u64,
}

impl Guard {
    fn check(&self) -> Result<(), Failure> {
        if self.cancelled.load(Ordering::Acquire) || !self.connected.load(Ordering::Acquire) || !self.alive.load(Ordering::Acquire) {
            Err(Failure::new(Error::Cancelled, "The original management connection or request was retired."))
        } else if self.locked.load(Ordering::Acquire) {
            Err(Failure::new(Error::Locked, "A lock transition interrupted this request. Refresh state before retrying."))
        } else if !self.active.load(Ordering::Acquire) {
            Err(Failure::new(Error::Unavailable, "The Park seat or nested view is inactive."))
        } else if !self.target_live.load(Ordering::Acquire) {
            Err(Failure::new(Error::StaleTarget, "The captured native window or output is no longer available."))
        } else if self.input_owned.load(Ordering::Acquire) {
            Err(Failure::new(Error::Unavailable, "Finish the current application popup or input-method interaction before using desktop automation."))
        } else if self.epoch.load(Ordering::Acquire) != self.captured {
            Err(Failure::new(Error::Cancelled, "The session input context changed. Refresh state before retrying."))
        } else { Ok(()) }
    }
}

#[derive(Clone)]
#[cfg(feature = "native-presentation")]
pub(crate) enum Effect { Launch(DesktopAction), Close(SurfaceId), Magnification(lince_interface::host::magnification::Request) }
enum Step {
    Complete(Result<Disposition, Failure>),
    #[cfg(feature = "native-presentation")]
    Host(Effect),
}

pub struct Work {
    invocation: Invocation,
    grants: Vec<crate::launch::Capability>,
    guard: Guard,
    reply: SyncSender<Step>,
}

impl Work {
    pub fn invocation(&self) -> &Invocation { &self.invocation }
    pub fn grants(&self) -> &[crate::launch::Capability] { &self.grants }
    pub fn valid(&self) -> Result<(), Failure> { self.guard.check() }
    pub fn complete(self, result: Result<Disposition, Failure>) {
        let result = self.guard.check().and(result);
        let _ = self.reply.try_send(Step::Complete(result));
    }
    #[cfg(feature = "native-presentation")]
    pub(crate) fn host(self, effect: Effect) {
        let step = match self.guard.check() { Ok(()) => Step::Host(effect), Err(error) => Step::Complete(Err(error)) };
        let _ = self.reply.try_send(step);
    }
}

#[cfg(feature = "native-presentation")]
pub struct HostCommand {
    pub(crate) ticket: u64,
    pub(crate) effect: Effect,
    pub(crate) invocation: Invocation,
    pub(crate) grants: Vec<crate::launch::Capability>,
    guard: Guard,
}

#[cfg(feature = "native-presentation")]
impl HostCommand {
    pub(crate) fn valid(&self) -> Result<(), Failure> { self.guard.check() }
}

struct Pending {
    ticket: u64,
    invocation: Invocation,
    #[cfg(feature = "native-presentation")]
    grants: Vec<crate::launch::Capability>,
    guard: Guard,
    reply: Receiver<Step>,
    started: Instant,
    host: bool,
}

impl Drop for Pending {
    fn drop(&mut self) { self.guard.cancelled.store(true, Ordering::Release); }
}

struct Binding {
    manager: Manager,
    connected: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    grants: Vec<crate::launch::Capability>,
    pending: BTreeMap<u32, Pending>,
    rate: Rate,
    flow: Flow,
    subscribed: bool,
    dirty: u32,
    changes: usize,
    last_change: Option<u64>,
}

impl Drop for Binding {
    fn drop(&mut self) { self.alive.store(false, Ordering::Release); }
}

impl Binding {
    fn fatal(&self, reason: &str) {
        self.alive.store(false, Ordering::Release);
        self.manager.post_error(0_u32, reason);
    }

    fn reply(&mut self, id: u32, response: Response) {
        let body = match serde_json::to_string(&response) {
            Ok(body) if body.len() <= contract::RESPONSE_LIMIT => body,
            _ => { self.fatal("The bounded management response could not be encoded"); return; }
        };
        let file = match crate::control_protocol::response_file(body.as_bytes()) {
            Ok(file) => file,
            Err(_) => { self.fatal("The bounded management response could not be stored"); return; }
        };
        match self.flow.issue(self.pending.len()) {
            Ok((serial, token)) => self.manager.reply(id, std::os::fd::AsFd::as_fd(&file), serial.to_string(), token),
            Err(reason) => self.fatal(&reason),
        }
    }

    fn failed(&mut self, id: u32, code: Error, message: &str) {
        self.reply(id, Response::Failed { failure: Failure::new(code, message) });
    }

    fn admission(&mut self, id: u32) -> bool {
        if id == 0 { self.failed(id, Error::InvalidArguments, "Request identifiers must be nonzero."); return false; }
        if self.pending.contains_key(&id) { self.fatal("A request identifier is already in flight"); return false; }
        if !self.rate.take(Instant::now()) { self.failed(id, Error::RateLimited, "Management requests exceeded the connection rate limit."); return false; }
        if !self.flow.room(self.pending.len()) { self.fatal("Acknowledge delivered management events before sending more requests"); return false; }
        true
    }

    fn changed(&mut self, revision: u64) {
        if !self.subscribed || self.dirty == 0 || !self.flow.room(self.pending.len())
            || self.last_change.is_some_and(|serial| serial > self.flow.acknowledged()) { return; }
        let scope = if self.changes > contract::EVENT_LIMIT { contract::CHANGE_RESYNC } else { self.dirty };
        match self.flow.issue(self.pending.len()) {
            Ok((serial, token)) => {
                self.manager.changed(revision.to_string(), scope, serial.to_string(), token);
                self.last_change = Some(serial);
                self.dirty = 0;
                self.changes = 0;
            }
            Err(reason) => self.fatal(&reason),
        }
    }
}

pub(super) struct Control {
    bindings: Vec<Binding>,
    queued: VecDeque<Work>,
    #[cfg(feature = "native-presentation")]
    hosts: Vec<HostCommand>,
    catalog: Catalog,
    entries: Vec<Entry>,
    revision: u64,
    next_ticket: u64,
    locked: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
    input_owned: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
}

impl Control {
    pub(super) fn new(display: &DisplayHandle) -> Self {
        display.create_global::<State, Manager, _>(1, ());
        Self { bindings: Vec::new(), queued: VecDeque::new(),
            #[cfg(feature = "native-presentation")]
            hosts: Vec::new(), catalog: Catalog::default(), entries: Vec::new(), revision: 1, next_ticket: 1, locked: Arc::new(AtomicBool::new(false)), active: Arc::new(AtomicBool::new(true)), input_owned: Arc::new(AtomicBool::new(false)), epoch: Arc::new(AtomicU64::new(1)) }
    }

    pub(super) fn locked(&mut self, locked: bool) {
        self.locked.store(locked, Ordering::Release);
        self.advance();
    }

    fn advance(&mut self) {
        if self.epoch.fetch_update(Ordering::AcqRel, Ordering::Acquire, |epoch| epoch.checked_add(1)).is_err() {
            for binding in &self.bindings { binding.fatal("Management context identifiers are exhausted"); }
        }
    }
}

impl Server {
    pub fn control_active(&mut self, active: bool) {
        if self.state.control.active.swap(active, Ordering::AcqRel) != active {
            self.state.control.advance();
            self.state.refresh_control();
        }
    }

    pub fn control_status(&self) -> contract::Status {
        let mut status = contract::Status { available: true, active: self.state.control.active.load(Ordering::Acquire), locked: self.locked(), ..Default::default() };
        for binding in &self.state.control.bindings {
            if !binding.manager.is_alive() || !binding.alive.load(Ordering::Acquire) || !binding.connected.load(Ordering::Acquire) { continue; }
            status.readers += u16::from(binding.grants.contains(&crate::launch::Capability::WindowList));
            status.controllers += u16::from(binding.grants.contains(&crate::launch::Capability::DesktopControl));
            status.pending += binding.pending.len() as u16;
        }
        status
    }

    pub fn revoke_control(&mut self) {
        for binding in &self.state.control.bindings { binding.fatal("The session owner revoked desktop automation access"); }
        self.state.refresh_control();
    }

    pub fn control_catalog(&mut self, catalog: Catalog) {
        if catalog.context.as_ref().is_some_and(|context| !context.valid()) || catalog.commands.len() > 256
            || catalog.commands.iter().any(|command| command.id.len() > 128 || command.label.len() > 4096 || command.unavailable.as_ref().is_some_and(|reason| reason.len() > 4096)) { return; }
        self.state.control.catalog = catalog;
        self.state.refresh_control();
    }

    pub fn control_work(&mut self) -> Vec<Work> {
        let count = self.state.control.queued.len().min(8);
        self.state.control.queued.drain(..count).collect()
    }

    #[cfg(feature = "native-presentation")]
    pub fn control_hosts(&mut self) -> Vec<HostCommand> { std::mem::take(&mut self.state.control.hosts) }

    pub fn complete_control(&mut self, ticket: u64, result: Result<Disposition, Failure>) {
        for binding in &mut self.state.control.bindings {
            let id = binding.pending.iter().find(|(_, pending)| pending.ticket == ticket).map(|(id, _)| *id);
            let Some(id) = id else { continue; };
            let pending = binding.pending.remove(&id).unwrap();
            let result = pending.guard.check().and(result);
            binding.reply(id, match result { Ok(outcome) => Response::Complete { outcome }, Err(failure) => Response::Failed { failure } });
            return;
        }
    }

    pub fn control_timeout(&self) -> Option<Duration> {
        if !self.state.control.queued.is_empty() { return Some(Duration::ZERO); }
        #[cfg(feature = "native-presentation")]
        if !self.state.control.hosts.is_empty() { return Some(Duration::ZERO); }
        self.state.control.bindings.iter().flat_map(|binding| binding.pending.values())
            .map(|pending| (pending.started + Duration::from_secs(10)).saturating_duration_since(Instant::now())).min()
    }
}

impl State {
    pub(super) fn retire_control_surface(&self, id: SurfaceId) {
        for pending in self.control.bindings.iter().flat_map(|binding| binding.pending.values()) {
            if matches!(pending.invocation.context.item, Some(contract::Item::Application { surface, .. }) if surface == id.0) {
                pending.guard.target_live.store(false, Ordering::Release);
            }
        }
    }

    pub(super) fn refresh_control_input(&mut self) {
        let owned = self.popups.grab.as_ref().is_some_and(|grab| !grab.has_ended()) || self.seat.input_method().keyboard_grabbed();
        if self.control.input_owned.swap(owned, Ordering::AcqRel) != owned { self.control.advance(); }
    }

    pub(super) fn refresh_control(&mut self) {
        self.refresh_control_input();
        for pending in self.control.bindings.iter().flat_map(|binding| binding.pending.values()) {
            let window_live = match pending.invocation.context.item {
                Some(contract::Item::Application { surface, .. }) => self.windows.get(&SurfaceId(surface)).is_some_and(|window| window.mapped && window.role.alive()),
                _ => true,
            };
            let output_live = pending.invocation.context.output.is_none_or(|output| self.outputs.contains_key(&OutputId(output)));
            if !window_live || !output_live { pending.guard.target_live.store(false, Ordering::Release); }
        }
        self.control.bindings.retain(|binding| binding.manager.is_alive() && binding.alive.load(Ordering::Acquire) && binding.connected.load(Ordering::Acquire));
        self.control.queued.retain(|work| work.guard.check().is_ok());
        if self.control.bindings.is_empty() {
            #[cfg(feature = "native-presentation")]
            self.control.hosts.clear();
            return;
        }
        let entries = self.control_entries();
        if self.control.entries != entries {
            let scope = snapshot::changed(&self.control.entries, &entries);
            let Some(revision) = self.control.revision.checked_add(1) else {
                for binding in &self.control.bindings { binding.fatal("Management snapshot identifiers are exhausted"); }
                return;
            };
            self.control.entries = entries;
            self.control.revision = revision;
            for binding in &mut self.control.bindings {
                binding.dirty |= scope;
                binding.changes = binding.changes.saturating_add(1);
            }
        }
        for binding in &mut self.control.bindings {
            let ids: Vec<_> = binding.pending.keys().copied().collect();
            for id in ids {
                let pending = binding.pending.remove(&id).unwrap();
                #[cfg(feature = "native-presentation")]
                let mut pending = pending;
                let result = if let Err(error) = pending.guard.check() { Some(Err(error)) }
                    else if pending.started.elapsed() >= Duration::from_secs(10) { Some(Err(Failure::new(Error::Cancelled, "The command deadline elapsed. Refresh state before retrying; do not assume that no change occurred."))) }
                    else if pending.host { None }
                    else {
                        match pending.reply.try_recv() {
                            Ok(Step::Complete(result)) => Some(result),
                            #[cfg(feature = "native-presentation")]
                            Ok(Step::Host(effect)) => {
                                pending.host = true;
                                self.control.hosts.push(HostCommand { ticket: pending.ticket, effect, invocation: pending.invocation.clone(), grants: pending.grants.clone(), guard: pending.guard.clone() });
                                None
                            }
                            Err(TryRecvError::Disconnected) => Some(Err(Failure::new(Error::Cancelled, "The command worker stopped before returning an outcome."))),
                            Err(TryRecvError::Empty) => None,
                        }
                    };
                if let Some(result) = result {
                    binding.reply(id, match result { Ok(outcome) => Response::Complete { outcome }, Err(failure) => Response::Failed { failure } });
                } else { binding.pending.insert(id, pending); }
            }
            binding.changed(self.control.revision);
        }
    }
}
