use super::*;

pub(super) fn focus(
    mut event: On<Pointer<Press>>,
    screens: Query<&Screen>,
    mut focus: ResMut<InputFocus>,
) {
    if screens.contains(event.entity) && event.button == PointerButton::Primary {
        focus.set(event.entity, bevy::input_focus::FocusCause::Pressed);
        event.propagate(false);
    }
}

pub(super) fn keyboard(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    screens: Query<&Screen>,
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<InputFocus>,
    mut commands: Commands,
) {
    let Ok(screen) = screens.get(event.focused_entity) else {
        return;
    };
    let owner = screen.0;
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let control = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let super_key = keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight]);
    event.propagate(false);
    if control && event.input.key_code == KeyCode::Tab {
        focus.clear();
        return;
    }
    if control && shift && matches!(event.input.key_code, KeyCode::KeyC | KeyCode::KeyV) {
        if event.input.state.is_pressed() && !event.input.repeat {
            let command = if event.input.key_code == KeyCode::KeyC {
                Command::Copy
            } else {
                Command::Paste
            };
            commands.queue(move |world: &mut World| command.apply(world, owner));
        }
        return;
    }
    let code = key(event.input.key_code);
    let action = if !event.input.state.is_pressed() {
        0
    } else if event.input.repeat {
        2
    } else {
        1
    };
    let text = match &event.input.logical_key {
        bevy::input::keyboard::Key::Character(text) => text.to_string(),
        bevy::input::keyboard::Key::Space => " ".into(),
        _ => String::new(),
    };
    let mods = u32::from(shift)
        | (u32::from(control) << 1)
        | (u32::from(alt) << 2)
        | (u32::from(super_key) << 3);
    commands.queue(move |world: &mut World| {
        if let Err(error) = command(
            world,
            owner,
            worker::Command::Key {
                key: code,
                mods,
                text,
                action,
            },
        ) && let Some(terminal) = world.get::<TerminalSand>(owner)
        {
            let status = terminal.status;
            panel::status(world, status, error);
        }
    });
}

fn key(code: KeyCode) -> u32 {
    match code {
        KeyCode::Backquote => 1,
        KeyCode::Backslash => 2,
        KeyCode::BracketLeft => 3,
        KeyCode::BracketRight => 4,
        KeyCode::Comma => 5,
        KeyCode::Digit0 => 6,
        KeyCode::Digit1 => 7,
        KeyCode::Digit2 => 8,
        KeyCode::Digit3 => 9,
        KeyCode::Digit4 => 10,
        KeyCode::Digit5 => 11,
        KeyCode::Digit6 => 12,
        KeyCode::Digit7 => 13,
        KeyCode::Digit8 => 14,
        KeyCode::Digit9 => 15,
        KeyCode::Equal => 16,
        KeyCode::IntlBackslash => 17,
        KeyCode::IntlRo => 18,
        KeyCode::IntlYen => 19,
        KeyCode::KeyA => 20,
        KeyCode::KeyB => 21,
        KeyCode::KeyC => 22,
        KeyCode::KeyD => 23,
        KeyCode::KeyE => 24,
        KeyCode::KeyF => 25,
        KeyCode::KeyG => 26,
        KeyCode::KeyH => 27,
        KeyCode::KeyI => 28,
        KeyCode::KeyJ => 29,
        KeyCode::KeyK => 30,
        KeyCode::KeyL => 31,
        KeyCode::KeyM => 32,
        KeyCode::KeyN => 33,
        KeyCode::KeyO => 34,
        KeyCode::KeyP => 35,
        KeyCode::KeyQ => 36,
        KeyCode::KeyR => 37,
        KeyCode::KeyS => 38,
        KeyCode::KeyT => 39,
        KeyCode::KeyU => 40,
        KeyCode::KeyV => 41,
        KeyCode::KeyW => 42,
        KeyCode::KeyX => 43,
        KeyCode::KeyY => 44,
        KeyCode::KeyZ => 45,
        KeyCode::Minus => 46,
        KeyCode::Period => 47,
        KeyCode::Quote => 48,
        KeyCode::Semicolon => 49,
        KeyCode::Slash => 50,
        KeyCode::AltLeft => 51,
        KeyCode::AltRight => 52,
        KeyCode::Backspace => 53,
        KeyCode::CapsLock => 54,
        KeyCode::ContextMenu => 55,
        KeyCode::ControlLeft => 56,
        KeyCode::ControlRight => 57,
        KeyCode::Enter => 58,
        KeyCode::SuperLeft => 59,
        KeyCode::SuperRight => 60,
        KeyCode::ShiftLeft => 61,
        KeyCode::ShiftRight => 62,
        KeyCode::Space => 63,
        KeyCode::Tab => 64,
        KeyCode::Convert => 65,
        KeyCode::KanaMode => 66,
        KeyCode::NonConvert => 67,
        KeyCode::Delete => 68,
        KeyCode::End => 69,
        KeyCode::Help => 70,
        KeyCode::Home => 71,
        KeyCode::Insert => 72,
        KeyCode::PageDown => 73,
        KeyCode::PageUp => 74,
        KeyCode::ArrowDown => 75,
        KeyCode::ArrowLeft => 76,
        KeyCode::ArrowRight => 77,
        KeyCode::ArrowUp => 78,
        KeyCode::NumLock => 79,
        KeyCode::Numpad0 => 80,
        KeyCode::Numpad1 => 81,
        KeyCode::Numpad2 => 82,
        KeyCode::Numpad3 => 83,
        KeyCode::Numpad4 => 84,
        KeyCode::Numpad5 => 85,
        KeyCode::Numpad6 => 86,
        KeyCode::Numpad7 => 87,
        KeyCode::Numpad8 => 88,
        KeyCode::Numpad9 => 89,
        KeyCode::NumpadAdd => 90,
        KeyCode::NumpadBackspace => 91,
        KeyCode::NumpadClear => 92,
        KeyCode::NumpadClearEntry => 93,
        KeyCode::NumpadComma => 94,
        KeyCode::NumpadDecimal => 95,
        KeyCode::NumpadDivide => 96,
        KeyCode::NumpadEnter => 97,
        KeyCode::NumpadEqual => 98,
        KeyCode::NumpadMemoryAdd => 99,
        KeyCode::NumpadMemoryClear => 100,
        KeyCode::NumpadMemoryRecall => 101,
        KeyCode::NumpadMemoryStore => 102,
        KeyCode::NumpadMemorySubtract => 103,
        KeyCode::NumpadMultiply => 104,
        KeyCode::NumpadParenLeft => 105,
        KeyCode::NumpadParenRight => 106,
        KeyCode::NumpadSubtract => 107,
        KeyCode::Escape => 120,
        KeyCode::F1 => 121,
        KeyCode::F2 => 122,
        KeyCode::F3 => 123,
        KeyCode::F4 => 124,
        KeyCode::F5 => 125,
        KeyCode::F6 => 126,
        KeyCode::F7 => 127,
        KeyCode::F8 => 128,
        KeyCode::F9 => 129,
        KeyCode::F10 => 130,
        KeyCode::F11 => 131,
        KeyCode::F12 => 132,
        KeyCode::F13 => 133,
        KeyCode::F14 => 134,
        KeyCode::F15 => 135,
        KeyCode::F16 => 136,
        KeyCode::F17 => 137,
        KeyCode::F18 => 138,
        KeyCode::F19 => 139,
        KeyCode::F20 => 140,
        KeyCode::F21 => 141,
        KeyCode::F22 => 142,
        KeyCode::F23 => 143,
        KeyCode::F24 => 144,
        KeyCode::PrintScreen => 148,
        KeyCode::ScrollLock => 149,
        KeyCode::Pause => 150,
        KeyCode::BrowserBack => 151,
        KeyCode::BrowserFavorites => 152,
        KeyCode::BrowserForward => 153,
        KeyCode::BrowserHome => 154,
        KeyCode::BrowserRefresh => 155,
        KeyCode::BrowserSearch => 156,
        KeyCode::BrowserStop => 157,
        KeyCode::Eject => 158,
        KeyCode::LaunchApp1 => 159,
        KeyCode::LaunchApp2 => 160,
        KeyCode::LaunchMail => 161,
        KeyCode::MediaPlayPause => 162,
        KeyCode::MediaSelect => 163,
        KeyCode::MediaStop => 164,
        KeyCode::MediaTrackNext => 165,
        KeyCode::MediaTrackPrevious => 166,
        KeyCode::Power => 167,
        KeyCode::Sleep => 168,
        KeyCode::AudioVolumeDown => 169,
        KeyCode::AudioVolumeMute => 170,
        KeyCode::AudioVolumeUp => 171,
        KeyCode::WakeUp => 172,
        KeyCode::Copy => 173,
        KeyCode::Cut => 174,
        KeyCode::Paste => 175,
        _ => 0,
    }
}
