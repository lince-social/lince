use super::*;
use lince_interface::host::input::{AbsoluteDevice, TabletAxes, TabletKind, TabletPhase};
use smithay::{backend::{input::{Event, TabletToolCapabilities as Caps, TabletToolDescriptor, TabletToolEvent, TabletToolType}, libinput::LibinputInputBackend},
    reexports::input::event::tablet_tool::{TabletTool, TabletToolEventTrait as RawEvent}};

pub(super) struct Tool {
    id: u64,
    backend: TabletTool,
    pub(super) buttons: Vec<u32>,
    pub(super) proximity: bool,
    tip: bool,
    held: BTreeSet<u32>,
}

pub(crate) struct Report {
    pub id: u64,
    pub device: AbsoluteDevice,
    pub descriptor: TabletToolDescriptor,
    pub axes: TabletAxes,
    pub kind: TabletKind,
    pub phase: TabletPhase,
    pub normalized: [f64; 2],
    pub time: u32,
    pub proximity: bool,
    pub held: bool,
}

impl Devices {
    pub fn tablet_problem(&mut self, device: &Device, reason: &str) {
        if !self.registered(device) { return; }
        if let Some(entry) = self.entries.get_mut(device.sysname()) { entry.absolute_error = Some(reason.into()); }
        self.refresh_absolute();
    }
    pub fn tablet_event<E>(&mut self, event: &E, mut phase: TabletPhase) -> Option<Report>
    where E: TabletToolEvent<LibinputInputBackend> + RawEvent {
        let device = Event::<LibinputInputBackend>::device(event);
        if !self.registered(&device) || !device.has_capability(DeviceCapability::TabletTool) { return None; }
        let backend = RawEvent::tool(event);
        let count = self.entries.values().map(|entry| entry.tools.len()).sum::<usize>();
        let shared = backend.is_unique().then(|| self.entries.values().flat_map(|entry| &entry.tools).find(|tool| tool.backend == backend).map(|tool| tool.id)).flatten();
        let entry = self.entries.get_mut(device.sysname())?;
        let index = if let Some(index) = entry.tools.iter().position(|tool| tool.backend == backend) { index }
            else {
                if phase != TabletPhase::Enter { return None; }
                if entry.tools.len() >= 32 || count >= 128 {
                    entry.absolute_error = Some("Tablet tool inventory is full; reconnect the device to retire old tools.".into());
                    self.refresh_absolute();
                    return None;
                }
                let id = if let Some(id) = shared { id } else {
                    let id = self.absolute.next_tool.checked_add(1)?;
                    self.absolute.next_tool = id;
                    id
                };
                let buttons = (0x100..=0x15f).filter(|button| backend.has_button(*button)).collect();
                entry.tools.push(Tool { id, backend, buttons, proximity: false, tip: false, held: BTreeSet::new() });
                entry.tools.len() - 1
            };
        let tool = &mut entry.tools[index];
        if let TabletPhase::Button { code, pressed } = phase {
            if !(0x100..=0x15f).contains(&code) || pressed && tool.held.len() >= 16 {
                phase = TabletPhase::Cancel;
                entry.absolute_error = Some("The tablet reported an invalid or excessive button sequence.".into());
            } else if !tool.buttons.contains(&code) { tool.buttons.push(code); tool.buttons.sort_unstable(); }
        }
        match phase {
            TabletPhase::Enter => { tool.proximity = true; tool.tip = false; tool.held.clear(); }
            TabletPhase::Leave | TabletPhase::Cancel => { tool.proximity = false; tool.tip = false; tool.held.clear(); }
            TabletPhase::Tip(down) => tool.tip = down,
            TabletPhase::Button { code, pressed } => { if pressed { tool.held.insert(code); } else { tool.held.remove(&code); } }
            TabletPhase::Motion => {}
        }
        let (id, proximity, held) = (tool.id, tool.proximity, tool.tip || !tool.held.is_empty());
        let descriptor = TabletToolEvent::<LibinputInputBackend>::tool(event);
        let caps = descriptor.capabilities;
        let axes = TabletAxes {
            pressure: caps.contains(Caps::PRESSURE).then(|| TabletToolEvent::<LibinputInputBackend>::pressure(event)),
            distance: caps.contains(Caps::DISTANCE).then(|| TabletToolEvent::<LibinputInputBackend>::distance(event)),
            tilt: caps.contains(Caps::TILT).then(|| { let (x, y) = TabletToolEvent::<LibinputInputBackend>::tilt(event); [x, y] }),
            rotation: caps.contains(Caps::ROTATION).then(|| TabletToolEvent::<LibinputInputBackend>::rotation(event)),
            slider: caps.contains(Caps::SLIDER).then(|| TabletToolEvent::<LibinputInputBackend>::slider_position(event)),
            wheel: (caps.contains(Caps::WHEEL) && TabletToolEvent::<LibinputInputBackend>::wheel_has_changed(event)).then(|| (
                TabletToolEvent::<LibinputInputBackend>::wheel_delta(event), TabletToolEvent::<LibinputInputBackend>::wheel_delta_discrete(event))),
        };
        let kind = match descriptor.tool_type {
            TabletToolType::Pen => TabletKind::Pen, TabletToolType::Eraser => TabletKind::Eraser, TabletToolType::Brush => TabletKind::Brush,
            TabletToolType::Pencil => TabletKind::Pencil, TabletToolType::Airbrush => TabletKind::Airbrush, TabletToolType::Mouse => TabletKind::Mouse,
            TabletToolType::Lens => TabletKind::Lens, TabletToolType::Totem => TabletKind::Totem, TabletToolType::Unknown => TabletKind::Unknown,
        };
        self.restore_absolute_admission();
        self.refresh_absolute();
        let device = self.absolute_device(&device)?;
        let point = smithay::backend::input::AbsolutePositionEvent::<LibinputInputBackend>::position_transformed(event, (1, 1).into());
        Some(Report { id, device, descriptor, axes, kind, phase, normalized: [point.x, point.y], time: Event::<LibinputInputBackend>::time_msec(event), proximity, held })
    }
}
