use super::*;
use smithay::{
    input::pointer::RelativeMotionEvent,
    wayland::keyboard_shortcuts_inhibit::{
        KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState,
        KeyboardShortcutsInhibitor, KeyboardShortcutsInhibitorSeat,
    },
};

impl KeyboardShortcutsInhibitHandler for State {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.shortcuts
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        if self.shortcut_inhibitors.len() >= MAX_SURFACES {
            self.reject(inhibitor.wl_surface(), "Park shortcut inhibitor limit reached");
            return;
        }
        self.shortcut_inhibitors.push(inhibitor);
        self.refresh_shortcut_inhibitors();
    }

    fn inhibitor_destroyed(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        self.shortcut_inhibitors.retain(|existing| existing != &inhibitor);
    }
}

impl State {
    pub(super) fn update_surface_scale(&self, surface: &WlSurface) {
        let root = self.input_root(surface);
        let output = root.as_ref().and_then(|root| {
            self.layers.outputs.get(root).copied().or_else(|| {
                let id = self.id(root)?;
                self.presented_geometry.as_ref()?.items.iter()
                    .find(|item| item.id == lince_interface::host::ItemId::Application(id))
                    .map(|item| item.output)
            })
        }).and_then(|id| self.outputs.get(&id)).map(|output| &output.output)
            .or_else(|| self.lock.surfaces.iter().find(|(_, lock)| lock.wl_surface() == surface).and_then(|(id, _)| self.outputs.get(id)).map(|output| &output.output))
            .unwrap_or(&self.output);
        with_states(surface, |states| {
            smithay::wayland::fractional_scale::with_fractional_scale(states, |fractional| {
                fractional.set_preferred_scale(output.current_scale().fractional_scale());
            });
        });
    }

    pub(super) fn update_surface_scales(&self) {
        for surface in &self.surfaces {
            if surface.is_alive() { self.update_surface_scale(surface); }
        }
    }

    pub(super) fn refresh_shortcut_inhibitors(&mut self) {
        let focus = self.seat.get_keyboard().and_then(|keyboard| keyboard.current_focus());
        self.update_shortcut_focus(focus.as_ref());
    }

    pub(super) fn update_shortcut_focus(&mut self, focus: Option<&WlSurface>) {
        if self.controls_suppressed.as_ref() != focus {
            self.controls_suppressed = None;
        }
        self.shortcut_inhibitors.retain(|inhibitor| inhibitor.wl_surface().is_alive());
        for inhibitor in &self.shortcut_inhibitors {
            let active = self.lock.owner.is_none()
                && focus == Some(inhibitor.wl_surface())
                && self.controls_suppressed.as_ref() != focus
                && self.input_root(inhibitor.wl_surface()).is_some();
            if active && !inhibitor.is_active() {
                inhibitor.activate();
            } else if !active && inhibitor.is_active() {
                inhibitor.inactivate();
            }
        }
    }
}

impl smithay::wayland::fractional_scale::FractionalScaleHandler for State {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        self.update_surface_scale(&surface);
    }
}

impl Server {
    pub fn shortcuts_inhibited(&self) -> bool {
        !self.locked() && self.state.seat.keyboard_shortcuts_inhibited()
    }

    pub fn relative_motion(&mut self, delta: [f64; 2], unaccelerated: [f64; 2], time: u64) {
        if self.locked() || !delta.iter().chain(&unaccelerated).all(|value| value.is_finite()) {
            return;
        }
        let Some(pointer) = self.state.seat.get_pointer() else { return };
        let Some(surface) = pointer.current_focus() else { return };
        let root = self.state.input_root(&surface);
        let keyboard = self.state.seat.get_keyboard().and_then(|keyboard| keyboard.current_focus());
        if root.is_none() || root != keyboard.as_ref().and_then(|surface| self.state.input_root(surface)) {
            return;
        }
        pointer.relative_motion(&mut self.state, Some((surface, (0.0, 0.0).into())), &RelativeMotionEvent {
            delta: (delta[0], delta[1]).into(),
            delta_unaccel: (unaccelerated[0], unaccelerated[1]).into(),
            utime: time,
        });
        pointer.frame(&mut self.state);
    }
}

smithay::delegate_relative_pointer!(State);
smithay::delegate_keyboard_shortcuts_inhibit!(State);
smithay::delegate_fractional_scale!(State);
