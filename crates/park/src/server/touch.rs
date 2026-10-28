use super::*;
use smithay::{backend::input::TouchSlot, input::touch::{DownEvent, MotionEvent, UpEvent}, utils::SERIAL_COUNTER};

impl State {
    fn touch_eligible(&self, surface: &WlSurface) -> bool {
        if !surface.is_alive() || !self.buffers.contains_key(surface) { return false; }
        if self.lock.owner.is_some() { return self.lock_allows(surface); }
        let Some(root) = self.input_root(surface) else { return false; };
        self.id(&root).and_then(|id| self.windows.get(&id)).is_some_and(|window| window.mapped)
            || self.layers.surfaces.contains_key(&root)
    }

    pub(super) fn refresh_touch(&mut self) {
        if self.touches.values().any(|surface| !self.touch_eligible(surface)) {
            if let Some(touch) = self.seat.get_touch() { touch.cancel(self); }
            self.touches.clear();
            self.authority.retain(|entry| !matches!(entry.origin, input::Trigger::Touch(_)));
        }
    }
}

impl Server {
    pub fn touch_down(&mut self, slot: u32, surface: WlSurface, local: [f64; 2], time: u32) -> Option<Serial> {
        if slot >= 64 { return None; }
        let slot = TouchSlot::from(Some(slot));
        if self.state.touches.len() >= 64 || self.state.touches.contains_key(&slot) || !self.state.touch_eligible(&surface) || local.iter().any(|value| !value.is_finite()) { return None; }
        let touch = self.state.seat.get_touch()?;
        let serial = SERIAL_COUNTER.next_serial();
        touch.down(&mut self.state, Some((surface.clone(), (0.0, 0.0).into())), &DownEvent { slot, location: (local[0], local[1]).into(), serial, time });
        self.state.touches.insert(slot, surface.clone());
        self.state.remember_input(serial, surface, input::Trigger::Touch(slot));
        Some(serial)
    }

    pub fn touch_motion(&mut self, slot: u32, local: [f64; 2], time: u32) -> bool {
        self.state.refresh_touch();
        let slot = TouchSlot::from(Some(slot));
        let Some(surface) = self.state.touches.get(&slot).cloned() else { return false; };
        if local.iter().any(|value| !value.is_finite()) { self.touch_cancel(); return false; }
        let Some(touch) = self.state.seat.get_touch() else { return false; };
        touch.motion(&mut self.state, Some((surface, (0.0, 0.0).into())), &MotionEvent { slot, location: (local[0], local[1]).into(), time });
        true
    }

    pub fn touch_up(&mut self, slot: u32, time: u32) {
        self.state.refresh_touch();
        let slot = TouchSlot::from(Some(slot));
        let Some(surface) = self.state.touches.remove(&slot) else { return; };
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(touch) = self.state.seat.get_touch() { touch.up(&mut self.state, &UpEvent { slot, serial, time }); }
        self.state.remember_release(serial, surface, input::Trigger::Touch(slot));
        self.state.forget_trigger(input::Trigger::Touch(slot));
    }

    pub fn touch_frame(&mut self) {
        if let Some(touch) = self.state.seat.get_touch() { touch.frame(&mut self.state); }
    }

    pub fn touch_cancel(&mut self) {
        if let Some(touch) = self.state.seat.get_touch() { touch.cancel(&mut self.state); }
        self.state.touches.clear();
        self.state.authority.retain(|entry| !matches!(entry.origin, input::Trigger::Touch(_)));
    }

    pub fn touch_active(&self, slot: u32) -> bool { self.state.touches.contains_key(&TouchSlot::from(Some(slot))) }
}
