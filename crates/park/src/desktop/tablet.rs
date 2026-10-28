use super::*;
use super::absolute::Recipient;
use crate::native_input::absolute::{Focus, FocusReceipt, FocusTarget};
use lince_interface::host::{input::{TabletInput, TabletPhase}, mapping::Mapping};
use smithay::reexports::wayland_server::Resource;
use std::{collections::BTreeMap, sync::{Weak, atomic::{AtomicU64, Ordering}}};

struct Tool {
    id: u64,
    device: String,
    generation: u64,
    mapping: Mapping,
    point: [f64; 2],
    recipient: Option<Recipient>,
    route: Option<(WlSurface, [f64; 2])>,
    buttons: BTreeMap<u32, u32>,
    remap: BTreeMap<u32, u32>,
    tip: bool,
    locked: bool,
    native: bool,
    native_seen: bool,
    lifetime: Arc<AtomicU64>,
    scope_epoch: u64,
    last: TabletInput,
}

#[derive(Default)]
pub(super) struct State { tools: BTreeMap<u64, Tool>, scope: Arc<AtomicU64>, retired: Vec<Retired> }

struct Retired { device: String, generation: u64, lifetime: Weak<AtomicU64> }

impl Desktop {
    #[cfg(all(test, feature = "server-tests"))]
    pub(crate) fn tablet_test_counts(&self) -> [usize; 2] {
        [self.tablets.tools.len(), self.tablets.retired.iter().filter(|retired| retired.lifetime.strong_count() > 0).count()]
    }

    fn native_tablet(&mut self, tool: &Tool, mut event: TabletInput, target: Option<crate::native_input::PointerTarget>, fresh_press: bool) -> bool {
        event.tool = tool.id;
        let event = crate::native_input::TabletEvent { event, mapping: tool.mapping.revision(),
            lifetime: tool.lifetime.clone(), epoch: 0, scope: self.tablets.scope.clone(), scope_epoch: tool.scope_epoch,
            held: tool.tip || !tool.buttons.is_empty(), fresh_press, target };
        if matches!(event.event.phase, TabletPhase::Tip(true) | TabletPhase::Button { pressed: true, .. }) {
            self.queue_tablet_focus(tool, FocusTarget::Tablet(event), None)
        } else { self.queue_tablet(tool.id, Input::Tablet(event)) }
    }

    fn queue_tablet_focus(&mut self, tool: &Tool, target: FocusTarget, layer: Option<WlSurface>) -> bool {
        self.queue_absolute_focus(super::absolute::Source::Tablet(tool.id), Focus { output: tool.mapping.output(), mapping: tool.mapping.revision(), target,
            lifetime: tool.lifetime.clone(), epoch: 0, scope: Some((self.tablets.scope.clone(), tool.scope_epoch)),
            receipt: Arc::new(FocusReceipt::default()) }, layer)
    }

    fn retire_tablet(&mut self, tool: Tool, time: u32, cancelled: bool) {
        if cancelled {
            tool.lifetime.store(1, Ordering::Release);
            self.discard_tablet_queue(tool.id);
        }
        self.server.tablet_out(tool.id, time);
        if tool.native_seen {
            let mut event = tool.last.clone();
            event.phase = if cancelled { TabletPhase::Cancel } else { TabletPhase::Leave };
            event.axes.wheel = None;
            self.native_tablet(&tool, event, None, false);
        }
        if !cancelled {
            self.tablets.retired.retain(|retired| retired.lifetime.strong_count() > 0);
            if self.tablets.retired.len() >= 384 { self.cancel_tablets(); }
            else { self.tablets.retired.push(Retired { device: tool.device, generation: tool.generation, lifetime: Arc::downgrade(&tool.lifetime) }); }
        }
    }

    pub(crate) fn tablet_event(&mut self, report: crate::session::TabletReport) -> Option<&'static str> {
        self.reconcile_tablet();
        if matches!(report.phase, TabletPhase::Leave | TabletPhase::Cancel) || !report.proximity {
            if let Some(tool) = self.tablets.tools.remove(&report.id) { self.retire_tablet(tool, report.time, report.phase == TabletPhase::Cancel); }
            return None;
        }
        if !self.focused || !report.axes.valid() {
            if let Some(tool) = self.tablets.tools.remove(&report.id) { self.retire_tablet(tool, report.time, true); }
            return (!report.axes.valid()).then_some("The tablet reported invalid axes; the affected tool was cancelled.");
        }
        if report.phase == TabletPhase::Enter && let Some(tool) = self.tablets.tools.remove(&report.id) { self.retire_tablet(tool, report.time, true); }
        if !self.server.tablet_tool(report.device.generation, report.id, report.descriptor.clone()) { return Some("The tablet tool could not be admitted; its device or tool inventory changed or reached the limit."); }
        let mut tool = if let Some(tool) = self.tablets.tools.remove(&report.id) { tool }
            else {
                if report.held || self.tablets.tools.len() >= 128 || self.tablets.scope.load(Ordering::Acquire) == u64::MAX { return None; }
                let Some(mapping) = self.touch_output(&report.device) else { return None; };
                let Some(point) = Self::touch_point(&mapping, report.normalized) else { return None; };
                let remap = report.device.key.as_ref().and_then(|key| self.absolute_snapshot.as_ref()?.settings.get(key)).map(|settings| settings.buttons.clone()).unwrap_or_default();
                Tool { id: report.id, device: report.device.id.clone(), generation: report.device.generation, point, recipient: None, route: None, buttons: BTreeMap::new(), remap, tip: false,
                    locked: self.server.locked(), native: false, native_seen: false, lifetime: Arc::new(AtomicU64::new(0)), scope_epoch: self.tablets.scope.load(Ordering::Acquire),
                    last: TabletInput { tool: report.id, output: mapping.output(), point, axes: report.axes, kind: report.kind, phase: TabletPhase::Enter }, mapping }
            };
        let Some(point) = Self::touch_point(&tool.mapping, report.normalized) else { self.retire_tablet(tool, report.time, true); return None; };
        let event = TabletInput { tool: report.id, output: tool.mapping.output(), point, axes: report.axes, kind: report.kind, phase: report.phase };
        if tool.generation != report.device.generation || !self.update_tablet(&mut tool, event, report.time) {
            self.retire_tablet(tool, report.time, true);
            return None;
        }
        self.tablets.tools.insert(report.id, tool);
        None
    }

    fn update_tablet(&mut self, tool: &mut Tool, mut event: TabletInput, time: u32) -> bool {
        let point = event.point;
        let phase = event.phase;
        if !tool.tip && tool.buttons.is_empty() { tool.recipient = self.absolute_recipient(&tool.mapping, point); }
        let native = matches!(tool.recipient, Some(Recipient::Native(_)));
        let was_native = tool.native;
        let target = tool.recipient.as_ref().and_then(|recipient| self.absolute_position(recipient, point));
        if tool.recipient.as_ref().is_some_and(|recipient| !matches!(recipient, Recipient::Native(_))) && target.is_none() { return false; }
        if was_native && !native {
            let mut event = tool.last.clone(); event.phase = TabletPhase::Leave; event.axes.wheel = None;
            if !self.native_tablet(tool, event, None, false) { return false; }
        }
        if !self.server.tablet_motion(tool.id, tool.mapping.output(), point, target.clone(), native, event.axes, time) { return false; }
        tool.route = target;
        tool.point = point;
        if native && !was_native {
            tool.native_seen = true;
            let mut enter = event.clone(); enter.phase = TabletPhase::Enter;
            if phase != TabletPhase::Enter { enter.axes.wheel = None; }
            let captured = match &tool.recipient { Some(Recipient::Native(target)) => Some(target.clone()), _ => None };
            if !self.native_tablet(tool, enter, captured, false) { return false; }
        }
        let first_press = !tool.tip && tool.buttons.is_empty() && matches!(phase, TabletPhase::Tip(true) | TabletPhase::Button { pressed: true, .. });
        if first_press {
            if tool.locked { self.server.focus_lock(Some(tool.mapping.output())); }
            else if !native && self.current.as_ref().is_none_or(|current| current.frame.input.focus_policy.click)
                && let Some(Recipient::Client(input::Target::Application(id, _, _))) = tool.recipient {
                if !self.queue_tablet_focus(tool, FocusTarget::Application(id), None) { return false; }
            } else if let Some(Recipient::Client(input::Target::Layer(root, _))) = &tool.recipient
                && self.layers.iter().any(|layer| &layer.surface == root && layer.keyboard != KeyboardInteractivity::None) {
                if !self.queue_tablet_focus(tool, FocusTarget::Layer, Some(root.clone())) { return false; }
            }
        }
        let mut effective = true;
        match phase {
            TabletPhase::Tip(down) => {
                effective = tool.tip != down;
                if effective { let _ = self.server.tablet_tip(tool.id, down, time); }
                tool.tip = down;
            }
            TabletPhase::Button { code, pressed } => {
                if pressed {
                    if tool.buttons.contains_key(&code) { effective = false; }
                    else {
                        let mapped = tool.remap.get(&code).copied().unwrap_or(code);
                        effective = !tool.buttons.values().any(|value| *value == mapped);
                        tool.buttons.insert(code, mapped);
                        event.phase = TabletPhase::Button { code: mapped, pressed: true };
                        if effective { let _ = self.server.tablet_button(tool.id, mapped, true, time); }
                    }
                } else if let Some(mapped) = tool.buttons.remove(&code) {
                    effective = !tool.buttons.values().any(|value| *value == mapped);
                    event.phase = TabletPhase::Button { code: mapped, pressed: false };
                    if effective { let _ = self.server.tablet_button(tool.id, mapped, false, time); }
                } else { effective = false; }
            }
            _ => {}
        }
        if native && phase != TabletPhase::Enter {
            if !effective { event.phase = TabletPhase::Motion; }
            let target = match &tool.recipient { Some(Recipient::Native(target)) => Some(target.clone()), _ => None };
            if !self.native_tablet(tool, event.clone(), target, first_press && effective) { return false; }
        }
        tool.last = event;
        tool.native = native;
        true
    }

    pub(crate) fn tablet_device_removed(&mut self, device: &str, generation: u64, time: u32) {
        self.tablets.retired.retain(|retired| {
            if retired.device == device && retired.generation == generation {
                if let Some(lifetime) = retired.lifetime.upgrade() { lifetime.store(1, Ordering::Release); }
                false
            } else { retired.lifetime.strong_count() > 0 }
        });
        let ids = self.tablets.tools.iter().filter_map(|(id, tool)| (tool.device == device && tool.generation == generation).then_some(*id)).collect::<Vec<_>>();
        for id in ids { if let Some(tool) = self.tablets.tools.remove(&id) { self.retire_tablet(tool, time, true); } }
        self.server.tablet_remove_device(generation, time);
    }

    pub(crate) fn reconcile_tablet(&mut self) {
        self.tablets.retired.retain(|retired| retired.lifetime.strong_count() > 0);
        let ids = self.tablets.tools.iter().filter_map(|(id, tool)| {
            (!self.focused || tool.locked != self.server.locked() || !self.server.output_powered(tool.mapping.output())
                || self.touch_mapping(tool.mapping.output()).is_none_or(|mapping| mapping.revision() != tool.mapping.revision())
                || match &tool.recipient {
                    Some(Recipient::Native(_)) | None => false,
                    Some(Recipient::Client(target)) => !self.server.tablet_active(*id) || self.target_position(target.clone(), tool.point).is_none(),
                    Some(Recipient::Lock(surface, _)) => !surface.is_alive() || !self.server.tablet_active(*id),
                }).then_some(*id)
        }).collect::<Vec<_>>();
        let time = renderer::capture::monotonic().as_millis() as u32;
        for id in ids { if let Some(tool) = self.tablets.tools.remove(&id) { self.retire_tablet(tool, time, true); } }
    }

    pub(crate) fn refresh_tablet_projection(&mut self, time: u32) {
        self.reconcile_tablet();
        let ids = self.tablets.tools.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let Some(mut tool) = self.tablets.tools.remove(&id) else { continue; };
            let recipient = if tool.tip || !tool.buttons.is_empty() { tool.recipient.clone() } else { self.absolute_recipient(&tool.mapping, tool.point) };
            let route = recipient.as_ref().and_then(|recipient| self.absolute_position(recipient, tool.point));
            let native = matches!(recipient, Some(Recipient::Native(_)));
            let changed = tool.route != route || tool.native != native || match (&tool.recipient, &recipient) {
                (Some(Recipient::Native(old)), Some(Recipient::Native(new))) => old != new,
                _ => false,
            };
            if changed {
                let mut event = tool.last.clone(); event.phase = TabletPhase::Motion; event.axes.wheel = None;
                if !self.update_tablet(&mut tool, event, time) { self.retire_tablet(tool, time, true); continue; }
            } else { tool.recipient = recipient; }
            self.tablets.tools.insert(id, tool);
        }
    }

    fn cancel_tablets(&mut self) {
        let _ = self.tablets.scope.fetch_update(Ordering::AcqRel, Ordering::Acquire, |epoch| epoch.checked_add(1));
        for tool in std::mem::take(&mut self.tablets.tools).into_values() { tool.lifetime.store(1, Ordering::Release); }
        for retired in self.tablets.retired.drain(..) {
            if let Some(lifetime) = retired.lifetime.upgrade() { lifetime.store(1, Ordering::Release); }
        }
        self.server.tablet_cancel(renderer::capture::monotonic().as_millis() as u32);
        self.retire_tablet_queue();
    }

    pub(crate) fn cancel_absolute(&mut self) { self.cancel_touch(); self.cancel_tablets(); }
}
