use super::*;
use smithay::reexports::wayland_server::{DataInit, Dispatch, GlobalDispatch, New, WEnum};
use std::{sync::atomic::AtomicU64, time::Duration};
use wayland_protocols_wlr::output_power_management::v1::server::{
    zwlr_output_power_manager_v1::{self as manager, ZwlrOutputPowerManagerV1 as Manager},
    zwlr_output_power_v1::{self as output, ZwlrOutputPowerV1 as Power},
};

pub(super) struct Control {
    backend: bool,
    modes: BTreeMap<OutputId, bool>,
    managers: Vec<smithay::reexports::wayland_server::Weak<Manager>>,
    objects: Vec<Power>,
    requests: VecDeque<Request>,
}

impl Control {
    pub(super) fn new(handle: &DisplayHandle) -> Self {
        handle.create_global::<State, Manager, _>(1, ());
        Self { backend: false, modes: Default::default(), managers: Vec::new(), objects: Vec::new(), requests: VecDeque::new() }
    }
}

struct Target { id: OutputId, output: smithay::output::WeakOutput }
struct Data { target: Option<Target>, failed: AtomicBool, serial: AtomicU64, observed: Mutex<Option<bool>> }

pub struct Request {
    pub output: OutputId,
    pub on: bool,
    pub deadline: Instant,
    serial: u64,
    owner: Power,
}

fn permitted(client: &Client) -> bool {
    client.get_data::<ClientState>().is_some_and(|data| data.connected.load(Ordering::Acquire)
        && data.capabilities.contains(&crate::launch::Capability::OutputControl))
}

fn fail(resource: &Power) {
    if let Some(data) = resource.data::<Data>() && !data.failed.swap(true, Ordering::AcqRel) { resource.failed(); }
}

impl State {
    pub(super) fn output_powered(&self, output: OutputId) -> bool { self.power.modes.get(&output) != Some(&false) }

    fn power_target(&self, target: &Target) -> bool {
        self.power.backend && target.output.upgrade().is_some_and(|output|
            self.outputs.get(&target.id).is_some_and(|registered| registered.output == output))
            && self.power.modes.contains_key(&target.id)
    }

    fn power_current(&self, request: &Request) -> bool {
        request.owner.is_alive() && request.owner.client().is_some_and(|client| permitted(&client))
            && request.owner.data::<Data>().is_some_and(|data| !data.failed.load(Ordering::Acquire)
                && data.serial.load(Ordering::Acquire) == request.serial
                && data.target.as_ref().is_some_and(|target| target.id == request.output && self.power_target(target)))
    }

    pub(super) fn refresh_power(&mut self) {
        let outputs = &self.outputs;
        self.power.modes.retain(|id, _| outputs.contains_key(id));
        self.power.managers.retain(|manager| manager.upgrade().is_ok());
        let objects = std::mem::take(&mut self.power.objects);
        for object in objects {
            if !object.is_alive() || object.client().is_none_or(|client| !permitted(&client)) { continue; }
            if let Some(data) = object.data::<Data>() && !data.failed.load(Ordering::Acquire) {
                if let Some(target) = data.target.as_ref().filter(|target| self.power_target(target)) {
                    let current = self.power.modes.get(&target.id).copied();
                    let mut previous = data.observed.lock().unwrap_or_else(|error| error.into_inner());
                    if *previous != current && let Some(on) = current {
                        object.mode(if on { output::Mode::On } else { output::Mode::Off });
                        *previous = current;
                    }
                } else { fail(&object); }
            }
            self.power.objects.push(object);
        }
        let requests = std::mem::take(&mut self.power.requests);
        for request in requests {
            if self.power_current(&request) { self.power.requests.push_back(request); }
        }
    }
}

impl Server {
    #[cfg(all(test, feature = "session", feature = "server-tests"))]
    pub(crate) fn power_test_resources(&self) -> [usize; 3] {
        [self.state.power.managers.len(), self.state.power.objects.len(), self.state.power.requests.len()]
    }

    #[cfg(feature = "session")]
    pub(crate) fn power_output_instance(&self, output: OutputId) -> Option<(smithay::output::WeakOutput, u64)> {
        self.state.outputs.get(&output).map(|registered| (registered.output.downgrade(), self.state.output_control.revision()))
    }

    pub fn output_power_backend(&mut self, available: bool) {
        self.state.power.backend = available;
        self.state.refresh_power();
    }

    pub fn output_power_state(&mut self, output: OutputId, on: Option<bool>) {
        let previous = self.state.power.modes.get(&output).copied();
        if let Some(on) = on { self.state.power.modes.insert(output, on); }
        else { self.state.power.modes.remove(&output); }
        self.state.refresh_power();
        if previous != on { self.state.presentation_changed(); }
    }

    pub fn output_power_requests(&mut self) -> Vec<Request> { self.state.power.requests.drain(..).collect() }

    pub fn output_power_current(&self, request: &Request) -> bool { self.state.power_current(request) }

    pub fn output_power_failed(&mut self, request: Request) {
        if self.state.power_current(&request) { fail(&request.owner); }
        self.state.refresh_power();
    }

    pub fn output_power_timeout(&self) -> Option<Duration> {
        (!self.state.power.requests.is_empty()).then_some(Duration::ZERO)
    }

    pub fn output_powered(&self, output: OutputId) -> bool { self.state.output_powered(output) }
}

impl GlobalDispatch<Manager, ()> for State {
    fn bind(state: &mut Self, _: &DisplayHandle, client: &Client, resource: New<Manager>, _: &(), data: &mut DataInit<'_, Self>) {
        state.refresh_power();
        let object = data.init(resource, ());
        if !permitted(client) || state.power.managers.len() >= 128 {
            object.post_error(0u32, "Park output-power authority or manager limit exceeded.");
        } else { state.power.managers.push(object.downgrade()); }
    }

    fn can_view(client: Client, _: &()) -> bool { permitted(&client) }
}

impl Dispatch<Manager, ()> for State {
    fn request(state: &mut Self, client: &Client, resource: &Manager, request: manager::Request, _: &(), _: &DisplayHandle, data: &mut DataInit<'_, Self>) {
        if let manager::Request::GetOutputPower { id, output } = request {
            state.refresh_power();
            if !permitted(client) || state.power.objects.len() >= 256
                || state.power.objects.iter().filter(|object| object.client().is_some_and(|owner| owner.id() == client.id())).count() >= 32 {
                let object = data.init(id, Data { target: None, failed: AtomicBool::new(true), serial: AtomicU64::new(0), observed: Mutex::new(None) });
                object.failed();
                resource.post_error(0u32, "Park output-power authority or object limit exceeded.");
                return;
            }
            let output = Output::from_resource(&output);
            let target = output.and_then(|output| state.outputs.iter().find(|(_, registered)| registered.output == output)
                .map(|(id, _)| Target { id: *id, output: output.downgrade() }));
            let current = target.as_ref().filter(|target| state.power_target(target)).and_then(|target| state.power.modes.get(&target.id).copied());
            let object = data.init(id, Data { target, failed: AtomicBool::new(false), serial: AtomicU64::new(0), observed: Mutex::new(current) });
            if let Some(on) = current { object.mode(if on { output::Mode::On } else { output::Mode::Off }); }
            else { fail(&object); }
            state.power.objects.push(object);
        }
    }
}

impl Dispatch<Power, Data> for State {
    fn request(state: &mut Self, client: &Client, resource: &Power, request: output::Request, data: &Data, _: &DisplayHandle, _: &mut DataInit<'_, Self>) {
        let output::Request::SetMode { mode } = request else { return };
        if data.failed.load(Ordering::Acquire) { return; }
        let on = match mode {
            WEnum::Value(output::Mode::On) => true,
            WEnum::Value(output::Mode::Off) => false,
            _ => { resource.post_error(output::Error::InvalidMode, "Invalid output power mode."); return; }
        };
        let Some(target) = data.target.as_ref().filter(|target| permitted(client) && state.power_target(target)) else { fail(resource); return; };
        let Some(serial) = data.serial.load(Ordering::Acquire).checked_add(1) else { fail(resource); return; };
        state.power.requests.retain(|request| request.owner != *resource);
        if state.power.requests.len() >= 16 { fail(resource); return; }
        data.serial.store(serial, Ordering::Release);
        state.power.requests.push_back(Request { output: target.id, on, deadline: Instant::now() + Duration::from_secs(2), serial, owner: resource.clone() });
        state.presentation_changed();
    }

    fn destroyed(state: &mut Self, _: ClientId, resource: &Power, data: &Data) {
        data.failed.store(true, Ordering::Release);
        state.power.objects.retain(|object| object != resource);
        state.power.requests.retain(|request| request.owner != *resource);
    }
}
