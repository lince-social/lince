use super::*;
use crate::server::PowerRequest;
use std::time::Duration;

fn trace_time() -> f64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn trace_request(output: OutputId, epoch: u64, request: &PowerRequest) {
    if crate::runtime::tracing_frames() {
        let remaining = request.deadline.saturating_duration_since(Instant::now()).as_secs_f64();
        let started = trace_time() - (2.0 - remaining);
        eprintln!("Park power request: {} {} epoch {epoch} at {started:.6}", output.0, if request.on { "on" } else { "off" });
    }
}

pub(crate) struct Frame {
    id: OutputId,
    output: smithay::output::WeakOutput,
    revision: u64,
    lock: u64,
    pub(super) epoch: u64,
}

impl Frame {
    pub(crate) fn capture(server: &Server, output: OutputId, epoch: u64) -> Option<Self> {
        let (instance, revision) = server.power_output_instance(output)?;
        Some(Self { id: output, output: instance, revision, lock: server.lock_stamp(), epoch })
    }

    pub(crate) fn current(&self, server: &Server, output: OutputId, epoch: u64) -> bool {
        self.id == output && self.epoch == epoch && self.lock == server.lock_stamp()
            && self.output.upgrade().is_some_and(|captured| server.power_output_instance(output)
                .is_some_and(|(current, revision)| revision == self.revision && current.upgrade().is_some_and(|current| current == captured)))
    }
}

pub(crate) fn request_current(server: &Server, request: &PowerRequest, now: Instant) -> bool {
    request.deadline > now && server.output_power_current(request)
}

pub(crate) struct State {
    pub on: bool,
    observed: Option<bool>,
    confirm: bool,
    pub epoch: u64,
    waiting: Option<PowerRequest>,
}

impl Default for State {
    fn default() -> Self { Self { on: true, observed: None, confirm: true, epoch: 0, waiting: None } }
}

impl State {
    pub(crate) fn confirmed_on(&self) -> bool {
        self.on && self.observed == Some(true) && !self.confirm
    }

    #[cfg(all(test, feature = "server-tests"))]
    pub(crate) fn with_waiting(request: PowerRequest) -> Self {
        Self { waiting: Some(request), ..Self::default() }
    }

    pub fn pause(&mut self, server: &mut Server, queued: bool) {
        if let Some(request) = self.waiting.take() {
            if !queued { self.on = false; }
            server.output_power_failed(request);
        }
        self.epoch = self.epoch.saturating_add(1);
        self.observed = None;
        self.confirm = true;
    }

    pub fn reconfigure(&mut self) { self.confirm = true; }
}

fn observed(device: &DrmDevice, crtc: crtc::Handle, connector: connector::Handle) -> io::Result<Option<bool>> {
    if device.is_atomic() {
        for (handle, value) in device.get_properties(crtc)? {
            if device.get_property(handle)?.name().to_bytes() == b"ACTIVE" {
                return match value { 0 => Ok(Some(false)), 1 => Ok(Some(true)), _ => Err(io::Error::other("The kernel reported an invalid output ACTIVE state.")) };
            }
        }
    } else {
        for (handle, raw) in device.get_properties(connector)? {
            let property = device.get_property(handle)?;
            if property.name().to_bytes() == b"DPMS" {
                let kind = property.value_type();
                let value = kind.convert_value(raw);
                return Ok(value.as_enum().and_then(|value| match value.name().to_bytes() {
                    b"On" => Some(true), b"Off" | b"Suspend" | b"Standby" => Some(false), _ => None,
                }));
            }
        }
    }
    Ok(None)
}

fn sleeping(screen: &mut Screen, server: &mut Server) {
    screen.color.sleep();
    screen.adaptive_sync.effective = None;
    screen.adaptive_sync.pending = false;
    screen.adaptive_sync.reason = Some("The output is sleeping; adaptive sync will be confirmed after wake.".into());
    server.output_color(screen.id, screen.color.status.clone());
    server.output_adaptive_sync(screen.id, screen.adaptive_sync.clone());
}

pub(super) fn may_submit(screen: &Screen, server: &Server) -> bool {
    screen.power.waiting.as_ref().is_none_or(|request| request_current(server, request, Instant::now()))
}

pub(super) fn cancel_unsent(screen: &mut Screen, server: &mut Server) {
    if let Some(request) = screen.power.waiting.take() { server.output_power_failed(request); }
    screen.power.on = false;
    screen.power.confirm = false;
    screen.pending = false;
    sleeping(screen, server);
}

fn sleep(device: &DrmDevice, screen: &mut Screen, crtc: crtc::Handle, server: &mut Server) -> io::Result<()> {
    let epoch = screen.power.epoch.checked_add(1).ok_or_else(|| io::Error::other("Output power generation exhausted."))?;
    screen.output.with_compositor(|compositor| compositor.clear()).map_err(error)?;
    let observed = observed(device, crtc, screen.connector);
    screen.power.observed = observed.as_ref().ok().copied().flatten();
    server.output_power_state(screen.id, screen.power.observed);
    let actual = observed?;
    if actual != Some(false) {
        return Err(io::Error::other("The backend did not confirm that the output entered its power-saving state."));
    }
    screen.power.on = false;
    screen.power.confirm = false;
    screen.power.epoch = epoch;
    screen.protected = false;
    if crate::runtime::tracing_frames() {
        eprintln!("Park power observed: {} off epoch {epoch} at {:.6}", screen.id.0, trace_time());
    }
    sleeping(screen, server);
    Ok(())
}

pub(super) fn presented(device: &DrmDevice, screen: &mut Screen, crtc: crtc::Handle, epoch: u64, protected: bool, server: &mut Server) -> Option<String> {
    if !screen.power.on || !screen.power.confirm || epoch != screen.power.epoch || server.locked() && !protected { return None; }
    screen.power.confirm = false;
    let result = observed(device, crtc, screen.connector);
    screen.power.observed = result.as_ref().ok().copied().flatten();
    if let Some(request) = screen.power.waiting.take()
        && (!request_current(server, &request, Instant::now()) || screen.power.observed != Some(true)) { server.output_power_failed(request); }
    server.output_power_state(screen.id, screen.power.observed);
    match result {
        Ok(Some(true)) => {
            if crate::runtime::tracing_frames() {
                eprintln!("Park power observed: {} on epoch {epoch} protected {protected} at {:.6}", screen.id.0, trace_time());
            }
            None
        }
        Ok(Some(false)) => {
            screen.power.on = false;
            sleeping(screen, server);
            Some("The submitted output frame did not wake the display; its workspace and clients are retained.".into())
        }
        Ok(None) => Some("The backend cannot report the output's effective power state.".into()),
        Err(failure) => Some(format!("The output power state could not be confirmed: {failure}")),
    }
}

impl Hardware {
    pub(super) fn publish_power(&mut self, server: &mut Server) {
        for (crtc, screen) in &mut self.screens {
            if screen.power.observed.is_none() {
                match observed(self.outputs.device(), *crtc, screen.connector) {
                    Ok(mode) => screen.power.observed = mode,
                    Err(failure) => eprintln!("Park output power: {failure}"),
                }
            }
            server.output_power_state(screen.id, screen.power.observed);
        }
    }

    pub(super) fn power_timeout(&self) -> Option<Duration> {
        self.power_requests.iter().map(|request| request.deadline)
            .chain(self.screens.values().filter_map(|screen| screen.power.waiting.as_ref().map(|request| request.deadline)))
            .min().map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    pub(super) fn configure_power(&mut self, desktop: &mut Desktop) -> io::Result<()> {
        for request in desktop.server.output_power_requests() {
            if self.power_requests.len() >= 16 { desktop.server.output_power_failed(request); }
            else { self.power_requests.push_back(request); }
        }
        for screen in self.screens.values_mut() {
            if screen.power.waiting.as_ref().is_some_and(|request| !request_current(&desktop.server, request, Instant::now())) {
                let request = screen.power.waiting.take().unwrap();
                let queued = screen.pending && screen.prepared.is_none();
                if !queued {
                    if let Some(prepared) = screen.prepared.take() { self.retired.push(prepared); }
                    screen.pending = false;
                    screen.power.on = false;
                    screen.power.confirm = false;
                    sleeping(screen, &mut desktop.server);
                }
                desktop.server.output_power_failed(request);
            }
        }
        if self.active {
            for (crtc, screen) in &mut self.screens {
                if !screen.power.on && screen.power.confirm && !screen.pending && screen.prepared.is_none() {
                    if let Err(failure) = sleep(self.outputs.device(), screen, *crtc, &mut desktop.server) {
                        screen.power.on = true;
                        screen.power.confirm = true;
                        desktop.redraw = true;
                        desktop.send(Input::Feedback(format!("Could not restore the sleeping output: {failure}")))?;
                    }
                }
            }
        }
        loop {
            let Some(request) = self.power_requests.front() else { break };
            if !self.active || !request_current(&desktop.server, request, Instant::now()) {
                let request = self.power_requests.pop_front().unwrap();
                desktop.server.output_power_failed(request);
                continue;
            }
            if !self.configurations.is_empty() { break; }
            let Some(crtc) = self.screens.iter().find(|(_, screen)| screen.id == request.output).map(|(crtc, _)| *crtc) else {
                desktop.server.output_power_failed(self.power_requests.pop_front().unwrap());
                continue;
            };
            let screen = self.screens.get_mut(&crtc).unwrap();
            if screen.pending || screen.prepared.is_some() || screen.power.waiting.is_some() { break; }
            let request = self.power_requests.pop_front().unwrap();
            if request.on {
                if screen.power.confirmed_on() { continue; }
                let Some(epoch) = screen.power.epoch.checked_add(1) else { desktop.server.output_power_failed(request); continue; };
                trace_request(screen.id, epoch, &request);
                if let Err(failure) = screen.color.wake() {
                    desktop.server.output_power_failed(request);
                    desktop.send(Input::Feedback(failure.to_string()))?;
                    continue;
                }
                screen.power.on = true;
                screen.power.epoch = epoch;
                screen.power.confirm = true;
                screen.power.waiting = Some(request);
                screen.protected = false;
                screen.adaptive_sync.effective = None;
                screen.adaptive_sync.pending = true;
                screen.output.reset_buffers();
                desktop.server.output_color(screen.id, screen.color.status.clone());
                desktop.server.output_adaptive_sync(screen.id, screen.adaptive_sync.clone());
                desktop.redraw = true;
            } else {
                if !screen.power.on && screen.power.observed == Some(false) { continue; }
                trace_request(screen.id, screen.power.epoch.saturating_add(1), &request);
                if let Err(failure) = sleep(self.outputs.device(), screen, crtc, &mut desktop.server) {
                    desktop.server.output_power_failed(request);
                    screen.output.reset_buffers();
                    screen.power.confirm = true;
                    desktop.redraw = true;
                    desktop.send(Input::Feedback(format!("Could not change output power: {failure}")))?;
                }
            }
        }
        Ok(())
    }
}
