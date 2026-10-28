use super::*;
use smithay::{
    backend::input::KeyState,
    input::{
        keyboard::{FilterResult, KeyboardTarget, ModifiersState},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    utils::SERIAL_COUNTER,
};

#[derive(Clone, Debug)]
pub struct KeyEvent {
    pub symbol: u32,
    pub text: Option<String>,
    pub modifiers: ModifiersState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Trigger {
    Key(u32),
    Button(u32),
    Touch(smithay::backend::input::TouchSlot),
    Tablet(u64, u32),
}

pub(super) struct Authority {
    serial: Serial,
    surface: WlSurface,
    time: Instant,
    pub(super) trigger: Option<Trigger>,
    pub(super) origin: Trigger,
    epoch: u64,
}

impl State {
    pub(super) fn forget_trigger(&mut self, trigger: Trigger) {
        for entry in &mut self.authority {
            if entry.trigger == Some(trigger) {
                entry.trigger = None;
            }
        }
    }

    pub(super) fn remember_input(&mut self, serial: Serial, surface: WlSurface, trigger: Trigger) {
        self.input_epoch = self.input_epoch.saturating_add(1);
        self.authority
            .retain(|entry| entry.time.elapsed() < std::time::Duration::from_secs(5));
        while self.authority.len() >= 64 {
            self.authority.pop_front();
        }
        self.authority.push_back(Authority {
            serial,
            surface: self.input_root(&surface).unwrap_or(surface),
            time: Instant::now(),
            trigger: Some(trigger),
            origin: trigger,
            epoch: self.input_epoch,
        });
    }

    pub(super) fn remember_release(&mut self, serial: Serial, surface: WlSurface, trigger: Trigger) {
        let Some(root) = self.input_root(&surface) else { return; };
        let Some((surface, time, epoch)) = self.authority.iter().rev().find(|entry| {
            entry.trigger == Some(trigger) && entry.surface == root
                && entry.time.elapsed() < std::time::Duration::from_secs(5)
        }).map(|entry| (entry.surface.clone(), entry.time, entry.epoch)) else { return; };
        while self.authority.len() >= 64 {
            self.authority.pop_front();
        }
        self.authority.push_back(Authority { serial, surface, time, trigger: None, origin: trigger, epoch });
    }

    pub(super) fn authorized_client_input(
        &self,
        serial: Serial,
        client: &ClientId,
        surface: Option<&WlSurface>,
    ) -> bool {
        self.authority.iter().any(|entry| {
            entry.serial == serial
                && entry
                    .surface
                    .client()
                    .is_some_and(|owner| &owner.id() == client)
                && surface.is_none_or(|surface| {
                    self.input_root(surface) == self.input_root(&entry.surface)
                })
                && self.authorized_input(serial, &entry.surface)
        })
    }

    pub(super) fn authorized_input(&self, serial: Serial, surface: &WlSurface) -> bool {
        let root = self.input_root(surface);
        let focused = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus());
        let pointer = self
            .seat
            .get_pointer()
            .and_then(|pointer| pointer.current_focus());
        root.is_some()
            && (focused
                .as_ref()
                .and_then(|surface| self.input_root(surface))
                == root
                || pointer
                    .as_ref()
                    .and_then(|surface| self.input_root(surface))
                    == root
                || self.touches.values().any(|surface| self.input_root(surface) == root)
                || self.tablets.active_surfaces().any(|surface| self.input_root(surface) == root)
                || self.lock.owner.is_none() && self.authority.iter().any(|entry| entry.serial == serial
                    && entry.epoch == self.input_epoch && matches!(entry.origin, Trigger::Touch(_) | Trigger::Tablet(..))
                    && self.input_root(&entry.surface) == root))
            && self.authority.iter().any(|entry| {
                entry.serial == serial
                    && entry.time.elapsed() < std::time::Duration::from_secs(5)
                    && self.input_root(&entry.surface) == root
            })
    }
}

impl Server {
    pub fn keyboard_repeat(&self, rate: u32, delay_ms: u32) {
        if let Some(keyboard) = self.state.seat.get_keyboard() {
            keyboard.change_repeat_info(rate.min(100) as i32, delay_ms.min(5000) as i32);
        }
    }

    #[cfg(feature = "session")]
    pub(crate) fn key_repeats(&mut self, code: u32) -> bool {
        let Some(keyboard) = self.state.seat.get_keyboard() else {
            return false;
        };
        keyboard.with_xkb_state(&mut self.state, |context| {
            let state = context
                .xkb()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            unsafe { state.state() }
                .get_keymap()
                .key_repeats((code + 8).into())
        })
    }

    pub fn dragging(&self) -> bool {
        self.state.dragging && !self.locked()
    }

    #[cfg(feature = "nested")]
    pub(crate) fn pointer_dragging(&self) -> bool {
        self.dragging() && self.state.seat.get_pointer().and_then(|pointer|
            pointer.with_grab(|_, grab| grab.is::<smithay::wayland::selection::data_device::DnDGrab<State>>())).unwrap_or(false)
    }

    #[cfg(feature = "nested")]
    pub(crate) fn cancel_pointer_drag(&mut self, time: u32) -> Vec<u32> {
        if !self.pointer_dragging() { return Vec::new(); }
        self.suspend_native_drop();
        let pointer = self.state.seat.get_pointer().unwrap();
        let buttons = self.state.buttons.iter().copied().collect::<Vec<_>>();
        pointer.motion(&mut self.state, None, &MotionEvent {
            location: pointer.current_location(), serial: SERIAL_COUNTER.next_serial(), time,
        });
        for &button in &buttons { self.pointer_button(button, false, time); }
        if self.pointer_dragging() { pointer.unset_grab(&mut self.state, SERIAL_COUNTER.next_serial(), time); }
        pointer.frame(&mut self.state);
        buttons
    }

    #[cfg(feature = "session")]
    pub(crate) fn pointer_grabbed(&self) -> bool {
        self.state.seat.get_pointer().is_some_and(|pointer| pointer.is_grabbed())
    }

    pub fn drag_nodes(&self) -> Vec<SurfaceNode> {
        if self.locked() {
            return Vec::new();
        }
        self.state
            .drag_icon
            .as_ref()
            .map(|surface| self.state.nodes(surface))
            .unwrap_or_default()
    }

    pub fn key_symbol(&mut self, keycode: u32) -> io::Result<KeyEvent> {
        if keycode > 767 {
            return Err(io::Error::other("invalid evdev key code"));
        }
        let keyboard = self
            .state
            .seat
            .get_keyboard()
            .ok_or_else(|| io::Error::other("the Park keyboard is unavailable"))?;
        let modifiers = keyboard.modifier_state();
        let symbol = keyboard.with_xkb_state(&mut self.state, |context| {
            let state = context
                .xkb()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            unsafe { state.state() }.key_get_one_sym((keycode + 8).into())
        });
        Ok(KeyEvent {
            symbol: symbol.raw(),
            text: symbol
                .key_char()
                .filter(|value| !value.is_control())
                .map(|value| value.to_string()),
            modifiers,
        })
    }

    pub fn keyboard(
        &mut self,
        keycode: u32,
        pressed: bool,
        time: u32,
        filter: impl FnOnce(&KeyEvent) -> bool,
    ) -> io::Result<KeyEvent> {
        if keycode > 767 {
            return Err(io::Error::other("invalid evdev key code"));
        }
        let keyboard = self
            .state
            .seat
            .get_keyboard()
            .ok_or_else(|| io::Error::other("the Park keyboard is unavailable"))?;
        let mut result = None;
        let trigger = Trigger::Key(keycode);
        let paired = keyboard.pressed_keys().contains(&(keycode + 8).into());
        if pressed {
            self.state.forget_trigger(trigger);
            self.state.input_epoch = self.state.input_epoch.saturating_add(1);
        }
        let serial = SERIAL_COUNTER.next_serial();
        let focused = keyboard.current_focus();
        let (forwarded, modifiers_changed) = keyboard.input_intercept(
            &mut self.state,
            (keycode + 8).into(),
            if pressed {
                KeyState::Pressed
            } else {
                KeyState::Released
            },
            |_, modifiers, key| {
                let symbol = key.modified_sym();
                let event = KeyEvent {
                    symbol: symbol.raw(),
                    text: symbol
                        .key_char()
                        .filter(|value| !value.is_control())
                        .map(|value| value.to_string()),
                    modifiers: *modifiers,
                };
                let intercepted = filter(&event);
                result = Some(event);
                !intercepted
            },
        );
        if forwarded {
            let pending = std::mem::take(&mut self.state.modifiers_pending);
            keyboard.input_forward(
                &mut self.state,
                (keycode + 8).into(),
                if pressed { KeyState::Pressed } else { KeyState::Released },
                serial,
                time,
                modifiers_changed || pending,
            );
        } else if modifiers_changed || self.state.modifiers_pending {
            self.state.modifiers_pending = keyboard.is_grabbed();
            if !self.state.modifiers_pending && let Some(surface) = keyboard.current_focus() {
                let seat = self.state.seat.clone();
                surface.modifiers(&seat, &mut self.state, keyboard.modifier_state(), serial);
            }
        }
        if forwarded && let Some(surface) = focused {
            if pressed {
                self.state.remember_input(serial, surface, trigger);
            } else if paired {
                self.state.remember_release(serial, surface, trigger);
            }
        }
        if !pressed {
            self.state.forget_trigger(trigger);
        }
        result.ok_or_else(|| io::Error::other("the Park key was not processed"))
    }

    pub fn pointer_motion(
        &mut self,
        target: Option<(WlSurface, [f64; 2])>,
        time: u32,
    ) -> io::Result<()> {
        let target =
            target.filter(|(surface, _)| !self.locked() || self.state.lock_allows(surface));
        if target.as_ref().is_some_and(|(surface, point)| {
            !surface.is_alive() || point.iter().any(|value| !value.is_finite())
        }) {
            return Err(io::Error::other("invalid Park pointer target"));
        }
        let pointer = self
            .state
            .seat
            .get_pointer()
            .ok_or_else(|| io::Error::other("the Park pointer is unavailable"))?;
        if self.state.popups.grab.as_ref().is_some_and(|grab| {
            grab.has_ended() && pointer.has_grab(grab.serial())
        }) {
            pointer.unset_grab(&mut self.state, SERIAL_COUNTER.next_serial(), time);
        }
        if self.pointer_locked() && target.as_ref().map(|(surface, _)| surface) == pointer.current_focus().as_ref() {
            return Ok(());
        }
        let position = target
            .as_ref()
            .map(|(_, position)| *position)
            .unwrap_or([0.0; 2]);
        pointer.motion(
            &mut self.state,
            target.map(|(surface, _)| (surface, (0.0, 0.0).into())),
            &MotionEvent {
                location: (position[0], position[1]).into(),
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(&mut self.state);
        Ok(())
    }

    pub fn pointer_button(&mut self, button: u32, pressed: bool, time: u32) {
        let _ = self.pointer_button_serial(button, pressed, time);
    }

    pub(crate) fn pointer_button_serial(&mut self, button: u32, pressed: bool, time: u32) -> Option<Serial> {
        let trigger = Trigger::Button(button);
        let paired = if pressed {
            self.state.forget_trigger(trigger);
            self.state.buttons.insert(button);
            false
        } else {
            self.state.buttons.remove(&button)
        };
        if let Some(pointer) = self.state.seat.get_pointer() {
            let serial = SERIAL_COUNTER.next_serial();
            if let Some(surface) = pointer.current_focus() {
                if pressed {
                    self.state.remember_input(serial, surface, trigger);
                } else if paired {
                    self.state.remember_release(serial, surface, trigger);
                }
            }
            if !pressed {
                self.state.forget_trigger(trigger);
            }
            pointer.button(
                &mut self.state,
                &ButtonEvent {
                    button,
                    state: if pressed {
                        smithay::backend::input::ButtonState::Pressed
                    } else {
                        smithay::backend::input::ButtonState::Released
                    },
                    serial,
                    time,
                },
            );
            pointer.frame(&mut self.state);
            return Some(serial);
        }
        if !pressed {
            self.state.forget_trigger(trigger);
        }
        None
    }

    pub fn pointer_axis(&mut self, frame: AxisFrame) {
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.axis(&mut self.state, frame);
            pointer.frame(&mut self.state);
        }
    }

    pub fn cancel_keyboard(&mut self, time: u32) {
        if let Some(keyboard) = self.state.seat.get_keyboard() {
            keyboard.unset_grab(&mut self.state);
            for key in keyboard.pressed_keys() {
                keyboard.input::<(), _>(
                    &mut self.state,
                    key,
                    KeyState::Released,
                    SERIAL_COUNTER.next_serial(),
                    time,
                    |_, _, _| FilterResult::Intercept(()),
                );
            }
        }
        self.focus(None, SERIAL_COUNTER.next_serial());
    }

    pub fn clear_input(&mut self, time: u32) {
        self.touch_cancel();
        self.tablet_cancel(time);
        self.cancel_keyboard(time);
        let buttons = std::mem::take(&mut self.state.buttons);
        for button in buttons {
            self.pointer_button(button, false, time);
        }
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.unset_grab(&mut self.state, SERIAL_COUNTER.next_serial(), time);
        }
        let _ = self.pointer_motion(None, time);
    }
}
