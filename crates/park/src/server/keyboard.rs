use super::*;
use lince_interface::host::input::{
    InputOutcome, InputResponse, KeyLabel, KeyModifiers, KeyboardRequest, KeyboardSettings,
    KeyboardSnapshot,
};
use smithay::input::keyboard::xkb;

#[derive(Default)]
pub(super) struct Settings {
    snapshot: KeyboardSnapshot,
    saved: KeyboardSettings,
    pending: Option<KeyboardRequest>,
}

fn compile(settings: &KeyboardSettings) -> io::Result<String> {
    settings.validate().map_err(io::Error::other)?;
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    xkb::Keymap::new_from_names(
        &context,
        "",
        "",
        settings.layout.as_str(),
        settings.variant.as_str(),
        Some(settings.options.clone()),
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .map(|map| map.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1))
    .ok_or_else(|| io::Error::other("The XKB layout, variant or options could not be compiled; the previous keyboard remains active."))
}

fn symbol_label(symbol: Option<&xkb::Keysym>) -> String {
    symbol.map_or_else(String::new, |symbol| {
        symbol
            .key_char()
            .filter(|character| !character.is_control())
            .map_or_else(
                || xkb::keysym_get_name(*symbol),
                |character| character.to_string(),
            )
    })
}

impl Server {
    fn refresh_keyboard_labels(&mut self, force: bool) {
        let Some(keyboard) = self.state.seat.get_keyboard() else {
            return;
        };
        let previous_group = self.keyboard_settings.snapshot.active_layout;
        let refresh = force || self.keyboard_settings.snapshot.revision == 0;
        let labels = keyboard.with_xkb_state(&mut self.state, |context| {
            let state = context
                .xkb()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let state = unsafe { state.state() };
            let group = state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE);
            if !refresh && group == previous_group {
                return None;
            }
            let keymap = state.get_keymap();
            let layouts = (0..keymap.num_layouts())
                .map(|index| keymap.layout_get_name(index).to_owned())
                .collect();
            let mut labels = BTreeMap::new();
            use smithay::reexports::winit::{
                keyboard::PhysicalKey, platform::scancode::PhysicalKeyExtScancode,
            };
            for code in 0..768 {
                let physical = bevy::winit::converters::convert_physical_key_code(
                    PhysicalKey::from_scancode(code),
                );
                if matches!(physical, bevy::prelude::KeyCode::Unidentified(_)) {
                    continue;
                }
                let key = (code + 8).into();
                let layout = state.key_get_layout(key);
                let plain = symbol_label(keymap.key_get_syms_by_level(key, layout, 0).first());
                let shifted = symbol_label(keymap.key_get_syms_by_level(key, layout, 1).first());
                let mut single = xkb::State::new(&keymap);
                single.update_mask(0, 0, 0, 0, 0, group);
                single.update_key(key, xkb::KeyDirection::Down);
                let modifiers = KeyModifiers {
                    control: single
                        .mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_EFFECTIVE),
                    alt: single.mod_name_is_active(xkb::MOD_NAME_ALT, xkb::STATE_MODS_EFFECTIVE),
                    shift: single
                        .mod_name_is_active(xkb::MOD_NAME_SHIFT, xkb::STATE_MODS_EFFECTIVE),
                    super_key: single
                        .mod_name_is_active(xkb::MOD_NAME_LOGO, xkb::STATE_MODS_EFFECTIVE),
                };
                let is_modifier = single.serialize_mods(xkb::STATE_MODS_DEPRESSED) != 0;
                if !plain.is_empty() {
                    labels.insert(
                        format!("{physical:?}"),
                        KeyLabel {
                            plain,
                            shifted,
                            modifiers,
                            is_modifier,
                        },
                    );
                }
            }
            Some((group, layouts, labels))
        });
        if let Some((group, layouts, labels)) = labels {
            let snapshot = &mut self.keyboard_settings.snapshot;
            snapshot.active_layout = group;
            snapshot.layouts = layouts;
            snapshot.labels = Arc::new(labels);
            snapshot.revision = snapshot.revision.saturating_add(1);
        }
    }

    pub(crate) fn keyboard_snapshot(&mut self) -> KeyboardSnapshot {
        self.refresh_keyboard_labels(false);
        self.keyboard_settings.snapshot.clone()
    }

    pub(crate) fn configure_keyboard(&mut self, settings: KeyboardSettings) -> io::Result<()> {
        let keymap = compile(&settings)?;
        let keyboard = self
            .state
            .seat
            .get_keyboard()
            .ok_or_else(|| io::Error::other("The Park keyboard is unavailable"))?;
        keyboard
            .set_keymap_from_string(&mut self.state, keymap)
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.keyboard_repeat(settings.repeat_rate, settings.repeat_delay_ms);
        self.keyboard_settings.saved = settings.clone();
        self.keyboard_settings.snapshot.settings = settings;
        self.refresh_keyboard_labels(true);
        Ok(())
    }

    fn keyboard_response(&mut self, request: u64, outcome: InputOutcome) {
        self.keyboard_settings.snapshot.response = Some(InputResponse { request, outcome });
    }

    pub(crate) fn keyboard_request(&mut self, request: KeyboardRequest) {
        self.refresh_keyboard_labels(false);
        let failure = if request.id == 0
            || request.id <= self.keyboard_settings.snapshot.last_request
        {
            Some("This keyboard request has already been used; try again.".to_owned())
        } else if self.locked() {
            Some("Keyboard settings cannot change while the session is locked.".to_owned())
        } else if self.keyboard_settings.pending.is_some() {
            Some("Another keyboard change is waiting for held keys to be released.".to_owned())
        } else if request.revision != self.keyboard_settings.snapshot.revision {
            Some("The keyboard changed while editing; cancel the draft to load its current settings.".to_owned())
        } else {
            request.settings.validate().err()
        };
        self.keyboard_settings.snapshot.last_request =
            self.keyboard_settings.snapshot.last_request.max(request.id);
        if let Some(reason) = failure {
            self.keyboard_response(request.id, InputOutcome::Failed(reason));
        } else {
            self.keyboard_response(request.id, InputOutcome::Pending);
            self.keyboard_settings.pending = Some(request);
        }
    }

    pub(crate) fn advance_keyboard(
        &mut self,
        save: impl FnOnce(&KeyboardSettings, &KeyboardSettings) -> io::Result<Option<String>>,
    ) -> Option<KeyboardSettings> {
        let request = self.keyboard_settings.pending.as_ref()?;
        let id = request.id;
        if self.locked() {
            self.keyboard_settings.pending = None;
            self.keyboard_response(
                id,
                InputOutcome::Failed(
                    "The session locked before keyboard settings could be applied.".into(),
                ),
            );
            return None;
        }
        let keyboard = self.state.seat.get_keyboard()?;
        if !keyboard.pressed_keys().is_empty() {
            return None;
        }
        let request = self.keyboard_settings.pending.take().unwrap();
        if request.revision != self.keyboard_settings.snapshot.revision {
            self.keyboard_response(
                id,
                InputOutcome::Failed(
                    "The keyboard changed before settings could be applied; reload the draft."
                        .into(),
                ),
            );
            return None;
        }
        let result = (|| {
            let candidate = compile(&request.settings)?;
            let previous = keyboard.with_xkb_state(&mut self.state, |context| {
                let state = context
                    .xkb()
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                unsafe { state.state() }
                    .get_keymap()
                    .get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1)
            });
            keyboard
                .set_keymap_from_string(&mut self.state, candidate)
                .map_err(|error| io::Error::other(error.to_string()))?;
            match save(&request.settings, &self.keyboard_settings.saved) {
                Ok(warning) => Ok(warning),
                Err(error) => {
                    if let Err(restore) = keyboard.set_keymap_from_string(&mut self.state, previous)
                    {
                        let mut active = request.settings.clone();
                        active.repeat_rate = self.keyboard_settings.snapshot.settings.repeat_rate;
                        active.repeat_delay_ms =
                            self.keyboard_settings.snapshot.settings.repeat_delay_ms;
                        self.keyboard_settings.snapshot.settings = active;
                        self.refresh_keyboard_labels(true);
                        return Err(io::Error::other(format!(
                            "Could not save keyboard settings: {error}. The previous keymap could not be restored: {restore}. The current layout remains usable but is not saved; retry Save."
                        )));
                    }
                    Err(error)
                }
            }
        })();
        match result {
            Ok(warning) => {
                self.keyboard_repeat(
                    request.settings.repeat_rate,
                    request.settings.repeat_delay_ms,
                );
                self.keyboard_settings.saved = request.settings.clone();
                self.keyboard_settings.snapshot.settings = request.settings.clone();
                self.refresh_keyboard_labels(true);
                self.keyboard_response(id, InputOutcome::Saved { warning });
                Some(request.settings)
            }
            Err(error) => {
                self.keyboard_response(id, InputOutcome::Failed(error.to_string()));
                None
            }
        }
    }
}
