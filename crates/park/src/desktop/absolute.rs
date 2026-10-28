use super::*;
use bevy::input::touch::TouchPhase;
use lince_interface::host::{input::AbsoluteDevice, mapping::Mapping};
use crate::native_input::absolute::{Focus, FocusReceipt, FocusTarget};
use smithay::reexports::wayland_server::Resource;
use std::{collections::{BTreeMap, BTreeSet, VecDeque}, sync::{mpsc::TrySendError, atomic::{AtomicU64, Ordering}}};

#[derive(Clone)]
pub(super) enum Recipient {
    Native(crate::native_input::PointerTarget),
    Client(input::Target),
    Lock(WlSurface, [i32; 2]),
}

#[cfg(test)]
mod tests {
    use super::*;
    use lince_interface::host::{OutputViewport, mapping::QuarterTurn, raster::RasterMapping};

    #[test]
    fn calibrated_display_points_use_the_selected_output_inverse_once_at_every_rotation_and_scale() {
        for rotation in [QuarterTurn::Normal, QuarterTurn::Clockwise90, QuarterTurn::UpsideDown, QuarterTurn::Clockwise270] {
            for scale in [1.0, 1.25, 2.0] {
                for magnification in [1.0, 2.0, 4.0] {
                    let physical = [800, 1200];
                    let size = rotation.source_size(physical).map(|size| f64::from(size) / scale);
                    let bounds = Rectangle::new([1700.0, 80.0], size).unwrap();
                    let viewport = OutputViewport { id: OutputId(2), bounds, usable: Some(bounds), workspace: 4, physical_size: physical, raster: RasterMapping { origin: bounds.origin(), scale: [scale; 2] }, rotation };
                    let mapping = Mapping::new(&viewport, rotation, magnification, bounds.origin(), 9).unwrap();
                    for (normalized, corners) in [
                        ([0.25, 0.5], [[200.0, 600.0], [600.0, 600.0], [600.0, 600.0], [600.0, 200.0]]),
                        ([0.1, 0.2], [[80.0, 240.0], [240.0, 720.0], [720.0, 960.0], [960.0, 80.0]]),
                    ] {
                        let point = Desktop::touch_point(&mapping, normalized).unwrap();
                        let index = match rotation { QuarterTurn::Normal => 0, QuarterTurn::Clockwise90 => 1, QuarterTurn::UpsideDown => 2, QuarterTurn::Clockwise270 => 3 };
                        let expected: [f64; 2] = std::array::from_fn(|axis| bounds.origin()[axis] + corners[index][axis] / (scale * magnification));
                        for axis in 0..2 { assert!((point[axis] - expected[axis]).abs() < 1e-6); }
                        assert!(point.iter().all(|value| value.is_finite()));
                    }
                }
            }
        }
    }
}

struct Contact {
    device: String,
    generation: u64,
    id: u64,
    slot: u32,
    mapping: Mapping,
    recipient: Recipient,
    point: [f64; 2],
    local: Option<[f64; 2]>,
    locked: bool,
}

#[derive(Default)]
pub(super) struct State {
    contacts: BTreeMap<(String, i32), Contact>,
    devices: BTreeSet<String>,
    next_id: u64,
    queue: VecDeque<Queued>,
    cancel: bool,
    cancel_tablet: bool,
    cancel_keys: bool,
    feedback: Option<String>,
    focus: Option<Pending>,
    latest_focus: Option<(Source, Arc<FocusReceipt>)>,
    replaying: bool,
    protected: bool,
    lifetime: Arc<AtomicU64>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Source { Touch, Tablet(u64), Deferred }

struct Pending {
    source: Source,
    serial: u64,
    receipt: Arc<FocusReceipt>,
    native: bool,
    layer: Option<WlSurface>,
}

enum Delivery {
    Worker(Input),
    Deferred(input::Deferred, Option<Arc<FocusReceipt>>),
}

struct Queued { source: Source, delivery: Delivery, layer: Option<WlSurface> }

impl Desktop {
    pub(super) fn absolute_recipient(&self, mapping: &Mapping, point: [f64; 2]) -> Option<Recipient> {
        if self.server.locked() {
            return self.server.lock_surfaces().into_iter().find_map(|(output, origin, nodes)| {
                if output != mapping.output() { return None; }
                let local = std::array::from_fn(|axis| point[axis] - f64::from(origin[axis]));
                nodes.into_iter().find_map(|node| node.hit(local).map(|_| Recipient::Lock(node.surface, origin)))
            });
        }
        match self.target_at(point) {
            input::Target::Native => self.frame_at(point)?.frame.input.native_pointer.target(point).map(Recipient::Native),
            input::Target::Empty => None,
            target => Some(Recipient::Client(target)),
        }
    }

    pub(super) fn absolute_position(&self, recipient: &Recipient, point: [f64; 2]) -> Option<(WlSurface, [f64; 2])> {
        match recipient {
            Recipient::Native(_) => None,
            Recipient::Client(target) => self.target_position(target.clone(), point),
            Recipient::Lock(surface, origin) => Some((surface.clone(), std::array::from_fn(|axis| point[axis] - f64::from(origin[axis])))),
        }
    }
    pub(super) fn touch_mapping(&self, output: OutputId) -> Option<&Mapping> {
        self.output_frames.get(&output).and_then(|(frame, _)| frame.frame.input.mappings.get(output))
            .or_else(|| self.current.as_ref()?.frame.input.mappings.get(output))
    }

    pub(super) fn touch_output(&self, device: &AbsoluteDevice) -> Option<Mapping> {
        let preference = device.key.as_ref().and_then(|key| self.absolute_snapshot.as_ref()?.settings.get(key));
        let display = self.display.as_ref()?;
        let named = |name: &str| display.heads.iter().find(|head| head.name == name && head.current.is_some()).map(|head| head.id);
        let eligible = |output: OutputId| display.heads.iter().any(|head| head.id == output && head.current.is_some())
            && self.server.output_powered(output) && self.touch_mapping(output).is_some();
        let output = if let Some(name) = preference.and_then(|preference| preference.output.as_deref()) { named(name)? }
            else { device.associated_output.as_deref().and_then(named).filter(|output| eligible(*output))
                .or_else(|| self.current.as_ref()?.frame.output.filter(|output| eligible(*output)))
                .or_else(|| self.viewports.iter().find(|output| eligible(output.id)).map(|output| output.id))? };
        eligible(output).then(|| self.touch_mapping(output).cloned()).flatten()
    }

    pub(super) fn touch_point(mapping: &Mapping, normalized: [f64; 2]) -> Option<[f64; 2]> {
        if normalized.iter().any(|value| !value.is_finite()) { return None; }
        let physical = std::array::from_fn(|axis| (normalized[axis].clamp(0.0, 1.0) * f64::from(mapping.physical_size()[axis]))
            .min(f64::from(mapping.physical_size()[axis].saturating_sub(1))));
        mapping.physical_to_logical(physical)
    }

    fn queue_touch(&mut self, event: Input) -> bool {
        self.queue_absolute(Source::Touch, event)
    }

    fn queue_touch_focus(&mut self, mapping: &Mapping, target: FocusTarget, layer: Option<WlSurface>) -> bool {
        self.queue_absolute_focus(Source::Touch, Focus { output: mapping.output(), mapping: mapping.revision(), target,
            lifetime: self.absolute.lifetime.clone(), epoch: self.absolute.lifetime.load(Ordering::Acquire), scope: None,
            receipt: Arc::new(FocusReceipt::default()) }, layer)
    }

    pub(super) fn queue_tablet(&mut self, id: u64, event: Input) -> bool { self.queue_absolute(Source::Tablet(id), event) }

    pub(super) fn discard_tablet_queue(&mut self, id: u64) {
        self.cancel_absolute_focus(Source::Tablet(id));
        self.absolute.queue.retain(|entry| entry.source != Source::Tablet(id));
    }

    pub(super) fn retire_tablet_queue(&mut self) {
        let sources = self.absolute.queue.iter().filter_map(|entry| matches!(entry.source, Source::Tablet(_)).then_some(entry.source))
            .chain(self.absolute.focus.as_ref().filter(|pending| matches!(pending.source, Source::Tablet(_))).map(|pending| pending.source))
            .collect::<Vec<_>>();
        for source in sources { self.cancel_absolute_focus(source); }
        self.absolute.queue.retain(|entry| !matches!(entry.source, Source::Tablet(_)));
        self.absolute.cancel_tablet = true;
    }

    pub(super) fn queue_absolute_focus(&mut self, source: Source, request: Focus, layer: Option<WlSurface>) -> bool {
        let receipt = request.receipt.clone();
        if !self.queue_absolute(source, Input::AbsoluteFocus(Box::new(request))) { receipt.cancel(); return false; }
        if let Some(entry) = self.absolute.queue.back_mut() { entry.layer = layer; }
        self.absolute.latest_focus = Some((source, receipt));
        true
    }

    pub(crate) fn absolute_waiting(&self) -> bool {
        !self.absolute.replaying && (self.absolute.focus.is_some() || !self.absolute.queue.is_empty()
            || self.absolute.cancel || self.absolute.cancel_tablet || self.absolute.cancel_keys)
    }

    pub(crate) fn absolute_replaying(&mut self, replaying: bool) { self.absolute.replaying = replaying; }

    pub(crate) fn replaying_absolute(&self) -> bool { self.absolute.replaying }

    pub(crate) fn defer_absolute(&mut self, event: input::Deferred) -> io::Result<()> {
        if self.absolute.queue.len() >= 128 { self.recover_input_pressure(); return Ok(()); }
        let receipt = self.absolute.latest_focus.as_ref().map(|(_, receipt)| receipt.clone());
        self.absolute.queue.push_back(Queued { source: Source::Deferred, delivery: Delivery::Deferred(event, receipt), layer: None });
        Ok(())
    }

    fn cancel_absolute_focus(&mut self, source: Source) {
        let mut affected = false;
        for entry in &self.absolute.queue {
            if entry.source == source && let Delivery::Worker(Input::AbsoluteFocus(request)) = &entry.delivery {
                request.receipt.cancel();
                affected = true;
            }
        }
        if let Some(pending) = self.absolute.focus.as_ref().filter(|pending| pending.source == source) {
            pending.receipt.cancel();
            affected = true;
        }
        if let Some((_, receipt)) = self.absolute.latest_focus.as_ref().filter(|(owner, _)| *owner == source) {
            receipt.cancel();
        }
        if affected { self.reset_absolute_keyboard(); }
    }

    fn reset_absolute_keyboard(&mut self) {
        self.server.cancel_keyboard(0);
        self.keys.clear();
        self.consumed.clear();
        self.consumed_actions.clear();
        self.modal_keys.clear();
        self.compose.reset();
        self.repeat = None;
        self.deferred.clear();
        self.absolute.cancel_keys = true;
    }

    pub(crate) fn recover_input_pressure(&mut self) {
        self.cancel_absolute();
        self.absolute.queue.clear();
        self.reset_absolute_keyboard();
        self.absolute.feedback = Some("Input was cancelled because its pending queue filled. Try the action again.".into());
    }

    fn queue_absolute(&mut self, source: Source, event: Input) -> bool {
        if let Input::Touch(next) = &event && matches!(next.phase, TouchPhase::Moved)
            && let Some(Queued { source: Source::Touch, delivery: Delivery::Worker(Input::Touch(previous)), .. }) = self.absolute.queue.back_mut()
            && previous.id == next.id && matches!(previous.phase, TouchPhase::Moved) {
            *previous = next.clone();
            return true;
        }
        if let Input::Tablet(next) = &event && next.event.phase == lince_interface::host::input::TabletPhase::Motion && next.event.axes.wheel.is_none()
            && let Some(Queued { source: previous_source, delivery: Delivery::Worker(Input::Tablet(previous)), .. }) = self.absolute.queue.back_mut()
            && *previous_source == source && previous.event.phase == lince_interface::host::input::TabletPhase::Motion && previous.event.axes.wheel.is_none()
            && Arc::ptr_eq(&previous.lifetime, &next.lifetime) && previous.epoch == next.epoch {
            *previous = next.clone();
            return true;
        }
        if self.absolute.queue.len() >= 128 { self.recover_input_pressure(); return false; }
        self.absolute.queue.push_back(Queued { source, delivery: Delivery::Worker(event), layer: None });
        true
    }

    pub(crate) fn touch_down(&mut self, device: &AbsoluteDevice, backend_slot: i32, normalized: [f64; 2], time: u32) {
        self.reconcile_touch();
        let key = (device.id.clone(), backend_slot);
        if self.absolute.contacts.contains_key(&key) { self.cancel_touch(); return; }
        if !self.focused || self.absolute.contacts.len() >= 64 || self.absolute.lifetime.load(Ordering::Acquire) == u64::MAX { return; }
        let Some(mapping) = self.touch_output(device) else { return; };
        let Some(point) = Self::touch_point(&mapping, normalized) else { return; };
        let locked = self.server.locked();
        let target = self.target_at(point);
        let Some(recipient) = self.absolute_recipient(&mapping, point) else { return; };
        let Some(id) = self.absolute.next_id.checked_add(1) else { return; };
        let Some(slot) = (0..64).find(|slot| self.absolute.contacts.values().all(|contact| contact.slot != *slot)) else { return; };
        let local = match &recipient {
            Recipient::Native(_) => None,
            Recipient::Client(target) => self.target_position(target.clone(), point),
            Recipient::Lock(surface, origin) => Some((surface.clone(), std::array::from_fn(|axis| point[axis] - f64::from(origin[axis])))),
        };
        let delivered_local = local.as_ref().map(|(_, local)| *local);
        if let Some((surface, local)) = local {
            if self.server.touch_down(slot, surface, local, time).is_none() { return; }
        } else if !matches!(recipient, Recipient::Native(_)) { return; }
        self.absolute.next_id = id;
        self.absolute.devices.insert(device.id.clone());
        if locked { self.server.focus_lock(Some(mapping.output())); }
        if let Recipient::Native(captured) = &recipient {
            let event = crate::native_input::absolute::Touch { id, phase: TouchPhase::Started, point, mapping: mapping.revision(), lifetime: self.absolute.lifetime.clone(), epoch: self.absolute.lifetime.load(Ordering::Acquire), target: Some(captured.clone()) };
            if !self.queue_touch_focus(&mapping, FocusTarget::Touch(event), None) { return; }
        } else if let input::Target::Application(application, _, _) = target && !locked {
            if self.current.as_ref().is_none_or(|current| current.frame.input.focus_policy.click)
                && !self.queue_touch_focus(&mapping, FocusTarget::Application(application), None) { return; }
        } else if let input::Target::Layer(root, _) = target && !locked
            && self.layers.iter().any(|layer| layer.surface == root && layer.keyboard != KeyboardInteractivity::None) {
            if !self.queue_touch_focus(&mapping, FocusTarget::Layer, Some(root)) { return; }
        }
        self.absolute.contacts.insert(key, Contact { device: device.id.clone(), generation: device.generation, id, slot, mapping, recipient, point, local: delivered_local, locked });
    }

    pub(crate) fn touch_motion(&mut self, device: &AbsoluteDevice, backend_slot: i32, normalized: [f64; 2], time: u32) {
        self.reconcile_touch();
        let key = (device.id.clone(), backend_slot);
        let Some(contact) = self.absolute.contacts.get(&key) else { return; };
        if contact.generation != device.generation { self.cancel_touch(); return; }
        let Some(point) = Self::touch_point(&contact.mapping, normalized) else { self.cancel_touch(); return; };
        let (id, slot) = (contact.id, contact.slot);
        let local = match &contact.recipient {
            Recipient::Native(_) => {
                let mapping = contact.mapping.revision();
                self.queue_touch(Input::Touch(crate::native_input::absolute::Touch { id, phase: TouchPhase::Moved, point, mapping, lifetime: self.absolute.lifetime.clone(), epoch: self.absolute.lifetime.load(Ordering::Acquire), target: None }));
                None
            }
            Recipient::Client(target) => self.target_position(target.clone(), point),
            Recipient::Lock(surface, origin) => Some((surface.clone(), std::array::from_fn(|axis| point[axis] - f64::from(origin[axis])))),
        };
        let delivered_local = local.as_ref().map(|(_, local)| *local);
        if let Some((_, local)) = local && !self.server.touch_motion(slot, local, time) { self.cancel_touch(); return; }
        if let Some(contact) = self.absolute.contacts.get_mut(&key) { contact.point = point; contact.local = delivered_local; }
    }

    pub(crate) fn touch_up(&mut self, device: &AbsoluteDevice, backend_slot: i32, time: u32) {
        self.reconcile_touch();
        let Some(contact) = self.absolute.contacts.remove(&(device.id.clone(), backend_slot)) else { return; };
        if contact.generation != device.generation { self.cancel_touch(); return; }
        match contact.recipient {
            Recipient::Native(_) => { self.queue_touch(Input::Touch(crate::native_input::absolute::Touch { id: contact.id, phase: TouchPhase::Ended, point: contact.point, mapping: contact.mapping.revision(), lifetime: self.absolute.lifetime.clone(), epoch: self.absolute.lifetime.load(Ordering::Acquire), target: None })); }
            _ => self.server.touch_up(contact.slot, time),
        }
    }

    pub(crate) fn touch_device_removed(&mut self, device: &str) {
        if self.absolute.devices.contains(device) { self.cancel_touch(); }
    }

    pub(crate) fn touch_devices(&self) -> BTreeSet<String> { self.absolute.contacts.values().map(|contact| contact.device.clone()).collect() }

    pub(crate) fn reconcile_touch(&mut self) {
        let invalid = self.absolute.contacts.values().any(|contact| {
            !self.focused || contact.locked != self.server.locked() || !self.server.output_powered(contact.mapping.output())
                || self.touch_mapping(contact.mapping.output()).is_none_or(|mapping| mapping.revision() != contact.mapping.revision())
                || match &contact.recipient {
                    Recipient::Native(_) => false,
                    Recipient::Client(target) => !self.server.touch_active(contact.slot) || self.target_position(target.clone(), contact.point).is_none(),
                    Recipient::Lock(surface, _) => !surface.is_alive() || !self.server.touch_active(contact.slot),
                }
        });
        if invalid { self.cancel_touch(); }
    }

    pub(crate) fn cancel_touch(&mut self) {
        self.cancel_absolute_focus(Source::Touch);
        let _ = self.absolute.lifetime.fetch_update(Ordering::AcqRel, Ordering::Acquire, |epoch| epoch.checked_add(1));
        let active = !self.absolute.contacts.is_empty() || self.absolute.queue.iter().any(|entry| entry.source == Source::Touch);
        self.absolute.contacts.clear();
        self.absolute.devices.clear();
        self.absolute.queue.retain(|entry| entry.source != Source::Touch);
        self.absolute.cancel |= active;
        self.server.touch_cancel();
    }

    pub(crate) fn refresh_touch_projection(&mut self, time: u32) {
        self.reconcile_touch();
        let updates = self.absolute.contacts.iter().filter_map(|(key, contact)| {
            if matches!(contact.recipient, Recipient::Native(_)) { return None; }
            let (_, local) = self.absolute_position(&contact.recipient, contact.point)?;
            (contact.local != Some(local)).then_some((key.clone(), contact.slot, local))
        }).collect::<Vec<_>>();
        let mut changed = false;
        for (key, slot, local) in updates {
            if !self.server.touch_motion(slot, local, time) { self.cancel_touch(); return; }
            if let Some(contact) = self.absolute.contacts.get_mut(&key) { contact.local = Some(local); }
            changed = true;
        }
        if changed { self.server.touch_frame(); }
    }

    pub(crate) fn flush_touch(&mut self) -> io::Result<()> {
        let protected = !self.focused || self.server.locked();
        if self.absolute.protected != protected {
            self.absolute.protected = protected;
            if protected { self.cancel_absolute(); }
        }
        loop {
            if self.absolute.queue.is_empty() && self.absolute.focus.is_none() { self.absolute.latest_focus = None; }
            if self.absolute.cancel {
                match self.worker.input(Input::CancelTouch) {
                    Ok(_) => self.absolute.cancel = false,
                    Err(TrySendError::Full(_)) => return Ok(()),
                    Err(TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas disconnected during touch cancellation.")),
                }
            }
            if self.absolute.cancel_tablet {
                match self.worker.input(Input::CancelTablet) {
                    Ok(_) => self.absolute.cancel_tablet = false,
                    Err(TrySendError::Full(_)) => return Ok(()),
                    Err(TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas disconnected during tablet cancellation.")),
                }
            }
            if self.absolute.cancel_keys {
                match self.worker.input(Input::CancelKeys) {
                    Ok(serial) => {
                        self.absolute.cancel_keys = false;
                        self.pending_output = Some((serial, None));
                    }
                    Err(TrySendError::Full(_)) => return Ok(()),
                    Err(TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas disconnected during keyboard recovery.")),
                }
                return Ok(());
            }
            if self.pending_output.is_some() { return Ok(()); }
            if let Some(pending) = &self.absolute.focus {
                if self.current.as_ref().is_none_or(|frame| frame.input_serial < pending.serial) { return Ok(()); }
                let pending = self.absolute.focus.take().unwrap();
                let layer_current = pending.layer.as_ref().is_none_or(|surface| surface.is_alive()
                    && self.layers.iter().any(|layer| &layer.surface == surface && layer.keyboard != KeyboardInteractivity::None));
                if !pending.receipt.accepted() || !layer_current || !self.focused || self.server.locked() {
                    pending.receipt.cancel();
                    self.reset_absolute_keyboard();
                    continue;
                }
                self.native_keyboard = pending.native;
                self.worker.focus_native(pending.native);
                self.layer_focus = pending.layer;
                self.absolute.replaying = true;
                let result = self.focus();
                self.absolute.replaying = false;
                result?;
                #[cfg(test)]
                eprintln!("Absolute focus handoff: requested_native={}, native={}, pending_focus={:?}, frame={:?}", pending.native, self.native_keyboard,
                    self.pending_focus, self.current.as_ref().map(|frame| (frame.input_serial, frame.frame.output, frame.frame.input.keyboard,
                        frame.frame.input.editor.is_some(), frame.frame.geometry.as_ref().and_then(|geometry| geometry.focused))));
                if pending.native && self.native_keyboard { self.restore_modifiers()?; }
            }
            if let Some(message) = self.absolute.feedback.take() {
                match self.worker.input(Input::Feedback(message)) {
                    Ok(_) => {}
                    Err(TrySendError::Full(Input::Feedback(message))) => { self.absolute.feedback = Some(message); return Ok(()); }
                    Err(TrySendError::Full(_)) => unreachable!(),
                    Err(TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas disconnected during input recovery.")),
                }
            }
            let Some(Queued { source, delivery, layer }) = self.absolute.queue.pop_front() else { return Ok(()); };
            match delivery {
                Delivery::Deferred(event, receipt) => {
                    if receipt.as_ref().is_some_and(|receipt| !receipt.accepted()) {
                        if matches!(&event, input::Deferred::Key(code, false, _) if self.keys.contains_key(code)) { self.reset_absolute_keyboard(); }
                        if !matches!(&event, input::Deferred::Button(button, false, _, _) if self.buttons.contains(button)) { continue; }
                    }
                    self.absolute.replaying = true;
                    let result = self.replay_event(event);
                    self.absolute.replaying = false;
                    result?;
                }
                Delivery::Worker(event) => {
                    let focus = if let Input::AbsoluteFocus(request) = &event {
                        if !request.current() || layer.as_ref().is_some_and(|surface| !surface.is_alive()) {
                            request.receipt.complete(false);
                            continue;
                        }
                        Some((request.receipt.clone(), request.native()))
                    } else { None };
                    match self.worker.input(event) {
                        Ok(serial) => {
                            if let Some((receipt, native)) = focus {
                                self.absolute.focus = Some(Pending { source, serial, receipt, native, layer });
                                return Ok(());
                            }
                        }
                        Err(TrySendError::Full(event)) => {
                            self.absolute.queue.push_front(Queued { source, delivery: Delivery::Worker(event), layer });
                            return Ok(());
                        }
                        Err(TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas disconnected during absolute input delivery.")),
                    }
                }
            }
        }
    }
}
