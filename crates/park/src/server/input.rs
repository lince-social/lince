use super::*;
use smithay::{
    backend::input::KeyState,
    input::{
        keyboard::{FilterResult, ModifiersState},
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

pub(super) struct Authority {
    serial: Serial,
    surface: WlSurface,
    time: Instant,
}

impl State {
    pub(super) fn remember_input(&mut self, serial: Serial, surface: WlSurface) {
        self.input_epoch = self.input_epoch.saturating_add(1);
        self.authority
            .retain(|entry| entry.time.elapsed() < std::time::Duration::from_secs(5));
        while self.authority.len() >= 64 {
            self.authority.pop_front();
        }
        self.authority.push_back(Authority {
            serial,
            surface,
            time: Instant::now(),
        });
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
                    == root)
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
        if pressed {
            self.state.input_epoch = self.state.input_epoch.saturating_add(1);
        }
        let serial = SERIAL_COUNTER.next_serial();
        let focused = keyboard.current_focus();
        let mut forwarded = false;
        keyboard.input::<(), _>(
            &mut self.state,
            (keycode + 8).into(),
            if pressed {
                KeyState::Pressed
            } else {
                KeyState::Released
            },
            serial,
            time,
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
                if intercepted {
                    FilterResult::Intercept(())
                } else {
                    forwarded = true;
                    FilterResult::Forward
                }
            },
        );
        if pressed
            && forwarded
            && let Some(surface) = focused
        {
            self.state.remember_input(serial, surface);
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
        if pressed {
            self.state.buttons.insert(button);
        } else {
            self.state.buttons.remove(&button);
        }
        if let Some(pointer) = self.state.seat.get_pointer() {
            let serial = SERIAL_COUNTER.next_serial();
            if pressed && let Some(surface) = pointer.current_focus() {
                self.state.remember_input(serial, surface);
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
        }
    }

    pub fn pointer_axis(&mut self, frame: AxisFrame) {
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.axis(&mut self.state, frame);
            pointer.frame(&mut self.state);
        }
    }

    pub fn clear_input(&mut self, time: u32) {
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
