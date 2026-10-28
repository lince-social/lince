use super::*;
use lince_interface::host::input::TabletAxes;
use smithay::{backend::input::{ButtonState, TabletToolCapabilities as Caps, TabletToolDescriptor}, delegate_tablet_manager, utils::SERIAL_COUNTER,
    wayland::tablet_manager::{TabletDescriptor, TabletHandle, TabletManagerState, TabletSeatHandler, TabletSeatTrait, TabletToolHandle}};

struct Device {
    descriptor: TabletDescriptor,
    handle: TabletHandle,
}

struct Tool {
    device: u64,
    descriptor: TabletToolDescriptor,
    handle: TabletToolHandle,
    focus: Option<WlSurface>,
    active: bool,
    native: bool,
    tip: bool,
    buttons: BTreeSet<u32>,
    output: OutputId,
    point: [f64; 2],
    cursor: CursorImageStatus,
}

pub(super) struct Tablets {
    _manager: TabletManagerState,
    devices: BTreeMap<u64, Device>,
    tools: BTreeMap<u64, Tool>,
}

impl Tablets {
    pub(super) fn new(handle: &DisplayHandle) -> Self {
        Self { _manager: TabletManagerState::new::<State>(handle), devices: BTreeMap::new(), tools: BTreeMap::new() }
    }

    pub(super) fn active_surfaces(&self) -> impl Iterator<Item = &WlSurface> {
        self.tools.values().filter(|tool| tool.active && (tool.tip || !tool.buttons.is_empty())).filter_map(|tool| tool.focus.as_ref())
    }
}

#[derive(Clone)]
pub struct Cursor {
    pub tool: u64,
    pub output: OutputId,
    pub point: [f64; 2],
    pub image: CursorImageStatus,
    pub owner: capture::ContentOwner,
}

impl State {
    fn tablet_eligible(&self, surface: &WlSurface) -> bool {
        if !surface.is_alive() || !self.buffers.contains_key(surface) { return false; }
        if self.lock.owner.is_some() { return self.lock_allows(surface); }
        let Some(root) = self.input_root(surface) else { return false; };
        self.id(&root).and_then(|id| self.windows.get(&id)).is_some_and(|window| window.mapped) || self.layers.surfaces.contains_key(&root)
    }

    fn tablet_out(&mut self, id: u64, time: u32) {
        if let Some(tool) = self.tablets.tools.get_mut(&id) {
            for button in std::mem::take(&mut tool.buttons) { tool.handle.button(button, ButtonState::Released, SERIAL_COUNTER.next_serial(), time); }
            tool.handle.proximity_out(time);
            tool.active = false;
            tool.tip = false;
            tool.native = false;
            tool.focus = None;
            tool.cursor = CursorImageStatus::default_named();
        }
        self.authority.retain(|entry| !matches!(entry.origin, input::Trigger::Tablet(tool, _) if tool == id));
        self.presentation_changed();
    }

    pub(super) fn refresh_tablets(&mut self) {
        let invalid = self.tablets.tools.iter().filter_map(|(id, tool)|
            (tool.active && (!self.outputs.contains_key(&tool.output) || !self.output_powered(tool.output)
                || tool.focus.as_ref().is_some_and(|surface| !self.tablet_eligible(surface))
                || self.lock.owner.is_some() && tool.native)).then_some(*id)).collect::<Vec<_>>();
        for id in invalid { self.tablet_out(id, monotonic()); }
    }
}

fn monotonic() -> u32 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    ((now.tv_sec as u64).wrapping_mul(1000).wrapping_add(now.tv_nsec as u64 / 1_000_000)) as u32
}

impl Server {
    pub fn tablet_device(&mut self, id: u64, descriptor: TabletDescriptor) -> bool {
        if id == 0 { return false; }
        if let Some(device) = self.state.tablets.devices.get(&id) { return device.descriptor == descriptor; }
        if self.state.tablets.devices.len() >= 32 { return false; }
        let handle = self.state.seat.tablet_seat().add_tablet_with_id::<State>(&self.state.handle, id, &descriptor);
        self.state.tablets.devices.insert(id, Device { descriptor, handle });
        true
    }

    pub fn tablet_tool(&mut self, device: u64, id: u64, descriptor: TabletToolDescriptor) -> bool {
        if id == 0 || !self.state.tablets.devices.contains_key(&device) { return false; }
        if let Some(tool) = self.state.tablets.tools.get_mut(&id) {
            if tool.descriptor != descriptor || tool.device != device && tool.active { return false; }
            tool.device = device;
            return true;
        }
        if self.state.tablets.tools.len() >= 128 { return false; }
        let seat = self.state.seat.tablet_seat();
        let handle = self.state.handle.clone();
        let tool = seat.add_tool_with_id(&mut self.state, &handle, id, &descriptor);
        self.state.tablets.tools.insert(id, Tool { device, descriptor, handle: tool, focus: None, active: false, native: false, tip: false,
            buttons: BTreeSet::new(), output: OutputId(0), point: [0.0; 2], cursor: CursorImageStatus::default_named() });
        true
    }

    pub fn tablet_motion(&mut self, id: u64, output: OutputId, point: [f64; 2], target: Option<(WlSurface, [f64; 2])>, native: bool, axes: TabletAxes, time: u32) -> bool {
        if point.iter().any(|value| !value.is_finite()) || !axes.valid() || !self.state.outputs.contains_key(&output) || !self.state.output_powered(output)
            || native && self.locked() || target.as_ref().is_some_and(|(surface, local)| !self.state.tablet_eligible(surface) || local.iter().any(|value| !value.is_finite())) {
            self.state.tablet_out(id, time);
            return false;
        }
        let Some(tool) = self.state.tablets.tools.get(&id) else { return false; };
        let focus = target.as_ref().map(|(surface, _)| surface.clone());
        if (tool.tip || !tool.buttons.is_empty()) && (tool.focus != focus || tool.native != native) { self.state.tablet_out(id, time); return false; }
        let Some(tablet) = self.state.tablets.devices.get(&tool.device).map(|device| device.handle.clone()) else { return false; };
        let caps = tool.descriptor.capabilities;
        let changed = tool.focus != focus || tool.native != native;
        let tool = self.state.tablets.tools.get_mut(&id).unwrap();
        let position = target.as_ref().map_or([0.0; 2], |(_, local)| *local);
        let target = target.map(|(surface, _)| (surface, (0.0, 0.0).into()));
        if changed {
            tool.handle.motion((position[0], position[1]).into(), target.clone(), &tablet, SERIAL_COUNTER.next_serial(), time);
            tool.cursor = CursorImageStatus::default_named();
        }
        if focus.is_some() {
            if let Some(value) = axes.pressure.filter(|_| caps.contains(Caps::PRESSURE)) { tool.handle.pressure(value); }
            if let Some(value) = axes.distance.filter(|_| caps.contains(Caps::DISTANCE)) { tool.handle.distance(value); }
            if let Some([x, y]) = axes.tilt.filter(|_| caps.contains(Caps::TILT)) { tool.handle.tilt((x, y)); }
            if let Some(value) = axes.rotation.filter(|_| caps.contains(Caps::ROTATION)) { tool.handle.rotation(value); }
            if let Some(value) = axes.slider.filter(|_| caps.contains(Caps::SLIDER)) { tool.handle.slider_position(value); }
            if let Some((degrees, clicks)) = axes.wheel.filter(|_| caps.contains(Caps::WHEEL)) { tool.handle.wheel(degrees, clicks); }
            tool.handle.motion((position[0], position[1]).into(), target, &tablet, SERIAL_COUNTER.next_serial(), time);
        }
        tool.active = native || focus.is_some();
        tool.native = native;
        tool.focus = focus;
        tool.output = output;
        tool.point = point;
        self.state.presentation_changed();
        true
    }

    pub fn tablet_tip(&mut self, id: u64, down: bool, time: u32) -> Option<Serial> {
        self.state.refresh_tablets();
        let tool = self.state.tablets.tools.get_mut(&id)?;
        if !tool.active || tool.tip == down { return None; }
        let serial = SERIAL_COUNTER.next_serial();
        if down { tool.handle.tip_down(serial, time); } else { tool.handle.tip_up(time); }
        tool.tip = down;
        let focus = tool.focus.clone();
        if let Some(focus) = focus {
            if down { self.state.remember_input(serial, focus, input::Trigger::Tablet(id, 0)); }
            else { self.state.remember_release(serial, focus, input::Trigger::Tablet(id, 0)); self.state.forget_trigger(input::Trigger::Tablet(id, 0)); }
        }
        Some(serial)
    }

    pub fn tablet_button(&mut self, id: u64, button: u32, pressed: bool, time: u32) -> Option<Serial> {
        self.state.refresh_tablets();
        let tool = self.state.tablets.tools.get_mut(&id)?;
        if !tool.active || !(0x100..=0x15f).contains(&button) || pressed && tool.buttons.len() >= 16 { return None; }
        let changed = if pressed { tool.buttons.insert(button) } else { tool.buttons.remove(&button) };
        if !changed { return None; }
        let serial = SERIAL_COUNTER.next_serial();
        tool.handle.button(button, if pressed { ButtonState::Pressed } else { ButtonState::Released }, serial, time);
        let focus = tool.focus.clone();
        if let Some(focus) = focus {
            if pressed { self.state.remember_input(serial, focus, input::Trigger::Tablet(id, button)); }
            else { self.state.remember_release(serial, focus, input::Trigger::Tablet(id, button)); self.state.forget_trigger(input::Trigger::Tablet(id, button)); }
        }
        Some(serial)
    }

    pub fn tablet_out(&mut self, id: u64, time: u32) { self.state.tablet_out(id, time); }

    pub fn tablet_active(&self, id: u64) -> bool { self.state.tablets.tools.get(&id).is_some_and(|tool| tool.active) }

    pub fn tablet_remove_device(&mut self, id: u64, time: u32) {
        let ids = self.state.tablets.tools.iter().filter_map(|(tool, state)| (state.device == id).then_some(*tool)).collect::<Vec<_>>();
        let seat = self.state.seat.tablet_seat();
        for tool in ids {
            self.state.tablet_out(tool, time);
            if let Some(tool_state) = self.state.tablets.tools.remove(&tool) { seat.remove_tool_with_id(tool, &tool_state.descriptor); }
        }
        if let Some(device) = self.state.tablets.devices.remove(&id) { seat.remove_tablet_with_id(id, &device.descriptor); }
    }

    pub fn tablet_cancel(&mut self, time: u32) {
        let ids = self.state.tablets.tools.keys().copied().collect::<Vec<_>>();
        for id in ids { self.state.tablet_out(id, time); }
    }

    pub fn tablet_reset(&mut self, time: u32) {
        let ids = self.state.tablets.devices.keys().copied().collect::<Vec<_>>();
        for id in ids { self.tablet_remove_device(id, time); }
    }

    pub fn tablet_cursors(&self) -> Vec<Cursor> {
        if self.locked() { return Vec::new(); }
        self.state.tablets.tools.iter().filter(|(_, tool)| tool.active && self.state.output_powered(tool.output)).map(|(id, tool)| Cursor {
            tool: *id, output: tool.output, point: tool.point, image: tool.cursor.clone(),
            owner: if tool.native { capture::ContentOwner::Native } else { self.state.capture_content_owner(tool.focus.clone()) },
        }).collect()
    }
}

impl TabletSeatHandler for State {
    fn tablet_tool_image_for(&mut self, handle: &TabletToolHandle, _: &TabletToolDescriptor, image: CursorImageStatus) {
        if let Some(tool) = self.tablets.tools.values_mut().find(|tool| tool.handle.same_instance(handle) && tool.active) {
            tool.cursor = image;
            self.presentation_changed();
        }
    }
}

delegate_tablet_manager!(State);
