#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|argument| argument == "--help") {
        println!("input_probe text|menu|escape: send native keys to the focused Wayland window");
        return Ok(());
    }
    use smithay::input::keyboard::xkb;
    use std::{io::Write, os::fd::AsFd, time::Instant};
    use wayland_client::{
        Connection, Dispatch, QueueHandle, delegate_noop,
        protocol::{wl_registry, wl_seat},
    };
    use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
        zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1,
    };

    #[derive(Default)]
    struct State {
        seat: Option<wl_seat::WlSeat>,
        manager: Option<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1>,
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for State {
        fn event(
            state: &mut Self,
            registry: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            {
                match interface.as_str() {
                    "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
                    "zwp_virtual_keyboard_manager_v1" => {
                        state.manager = Some(registry.bind(name, 1, qh, ()))
                    }
                    _ => {}
                }
            }
        }
    }

    delegate_noop!(State: ignore wl_seat::WlSeat);
    delegate_noop!(State: ignore zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
    delegate_noop!(State: ignore zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);

    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state)?;
    let seat = state.seat.ok_or("No Wayland seat")?;
    let manager = state.manager.ok_or("Virtual keyboard is unavailable")?;
    let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());
    let keymap = xkb::Keymap::new_from_names(
        &xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
        "",
        "",
        "us",
        "",
        None,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .ok_or("Could not create keyboard map")?;
    let mut data = keymap
        .get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1)
        .into_bytes();
    data.push(0);
    let mut file = tempfile::tempfile()?;
    file.write_all(&data)?;
    keyboard.keymap(1, file.as_fd(), data.len().try_into()?);
    let started = Instant::now();
    let stroke = |code, shift: bool| {
        if shift {
            keyboard.key(started.elapsed().as_millis() as u32, 42, 1);
        }
        keyboard.modifiers(u32::from(shift), 0, 0, 0);
        keyboard.key(started.elapsed().as_millis() as u32, code, 1);
        keyboard.key(started.elapsed().as_millis() as u32, code, 0);
        keyboard.modifiers(0, 0, 0, 0);
        if shift {
            keyboard.key(started.elapsed().as_millis() as u32, 42, 0);
        }
    };
    match std::env::args().nth(1).as_deref() {
        Some("text") => {
            for (code, shift) in [
                (25, true),
                (30, false),
                (19, false),
                (37, false),
                (57, false),
                (20, false),
                (21, false),
                (25, false),
                (18, false),
                (32, false),
                (57, false),
                (7, false),
                (8, false),
            ] {
                stroke(code, shift);
            }
        }
        Some("menu") => stroke(68, true),
        Some("escape") => stroke(1, false),
        _ => return Err("Expected text, menu or escape".into()),
    }
    queue.roundtrip(&mut State::default())?;
    keyboard.destroy();
    connection.flush()?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {}
