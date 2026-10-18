use crate::server::KeyEvent;
use smithay::input::keyboard::xkb::{self, compose};

pub(crate) struct Compose(Option<compose::State>);

impl Default for Compose {
    fn default() -> Self {
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .into_iter()
            .filter_map(std::env::var_os)
            .find(|locale| !locale.is_empty())
            .unwrap_or_else(|| "C.UTF-8".into());
        let table = compose::Table::new_from_locale(
            &xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
            &locale,
            compose::COMPILE_NO_FLAGS,
        )
        .ok();
        Self(table.map(|table| compose::State::new(&table, compose::STATE_NO_FLAGS)))
    }
}

impl Compose {
    pub(crate) fn reset(&mut self) {
        if let Some(state) = &mut self.0 {
            state.reset();
        }
    }

    pub(crate) fn text(&mut self, event: &KeyEvent, pressed: bool, repeat: bool) -> Option<String> {
        if !pressed || repeat {
            return event.text.clone();
        }
        if event.modifiers.ctrl || event.modifiers.alt || event.modifiers.logo {
            self.reset();
            return event.text.clone();
        }
        let Some(state) = &mut self.0 else {
            return event.text.clone();
        };
        state.feed(event.symbol.into());
        match state.status() {
            compose::Status::Nothing => event.text.clone(),
            compose::Status::Composing => None,
            compose::Status::Composed => {
                let text = state.utf8();
                state.reset();
                text
            }
            compose::Status::Cancelled => {
                state.reset();
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::input::keyboard::{ModifiersState, keysyms};

    #[test]
    fn native_dead_keys_compose_and_focus_reset_discards_pending_accents() {
        let table = compose::Table::new_from_buffer(
            &xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
            "<dead_acute> <e> : \"é\" eacute\n",
            "C.UTF-8",
            compose::FORMAT_TEXT_V1,
            compose::COMPILE_NO_FLAGS,
        )
        .unwrap();
        let mut compose = Compose(Some(compose::State::new(&table, compose::STATE_NO_FLAGS)));
        let accent = KeyEvent {
            symbol: keysyms::KEY_dead_acute,
            text: None,
            modifiers: ModifiersState::default(),
        };
        let letter = KeyEvent {
            symbol: keysyms::KEY_e,
            text: Some("e".into()),
            modifiers: ModifiersState::default(),
        };
        assert_eq!(compose.text(&accent, true, false), None);
        assert_eq!(compose.text(&letter, true, false).as_deref(), Some("é"));
        compose.text(&accent, true, false);
        compose.reset();
        assert_eq!(compose.text(&letter, true, false).as_deref(), Some("e"));
    }
}
