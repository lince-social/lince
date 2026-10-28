use super::*;

fn activate(harness: &mut Harness, token: &str, target: &Window) {
    harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .activate(token.to_owned(), &target.surface);
    harness.sync();
}

fn input_token(harness: &mut Harness, source: &Window, serial: u32) -> String {
    let request = harness
        .client
        .activation
        .as_ref()
        .unwrap()
        .get_activation_token(&harness.queue.handle(), ());
    request.set_serial(serial, harness.client.seat.as_ref().unwrap());
    request.set_surface(&source.surface);
    request.commit();
    harness.sync();
    harness.client.tokens.last().unwrap().clone()
}

fn input(harness: &mut Harness, keyboard: bool, pressed: bool) -> u32 {
    if keyboard {
        harness.server.keyboard(30, pressed, 10, |_| false).unwrap();
        harness.sync();
        harness.client.key_serial
    } else {
        let serial = harness
            .server
            .pointer_button_serial(272, pressed, 10)
            .unwrap();
        harness.sync();
        serial.into()
    }
}

#[test]
fn paired_pointer_and_keyboard_release_serials_activate_without_revoking_press_time_launches() {
    for keyboard in [false, true] {
        let mut harness = Harness::new();
        let source = harness.window();
        let _source_buffer = harness.attach(&source, [320, 240]);
        let origin = harness.server.windows()[0].clone();
        let target = harness.window();
        let _target_buffer = harness.attach(&target, [200, 160]);
        let target_id = harness
            .server
            .windows()
            .iter()
            .find(|window| window.id != origin.id)
            .unwrap()
            .id;
        harness.server.focus(
            Some(origin.id),
            smithay::utils::SERIAL_COUNTER.next_serial(),
        );
        harness
            .server
            .pointer_motion(Some((origin.surface, [40.0, 50.0])), 1)
            .unwrap();
        input(&mut harness, keyboard, true);
        let launch = harness.server.launch_activation_token().unwrap();
        let serial = input(&mut harness, keyboard, false);
        activate(&mut harness, launch.as_str(), &target);
        assert_eq!(harness.server.activations(), [target_id]);
        let token = input_token(&mut harness, &source, serial);
        activate(&mut harness, &token, &target);
        assert_eq!(
            harness.server.activations(),
            [target_id],
            "keyboard={keyboard}"
        );
    }
}

#[test]
fn compositor_launch_token_survives_trigger_release_waits_for_map_and_is_single_use() {
    let mut harness = Harness::new();
    harness.server.keyboard(125, true, 1, |_| true).unwrap();
    harness.server.keyboard(20, true, 2, |_| true).unwrap();
    let token = harness.server.launch_activation_token().unwrap();
    harness.server.keyboard(20, false, 3, |_| true).unwrap();
    harness.server.keyboard(125, false, 4, |_| true).unwrap();
    let target = harness.window();
    let id = harness.server.windows()[0].id;
    activate(&mut harness, token.as_str(), &target);
    assert!(harness.server.activations().is_empty());
    assert_eq!(harness.server.state.activations.len(), 1);
    let _buffer = harness.attach(&target, [320, 240]);
    assert_eq!(harness.server.activations(), [id]);
    activate(&mut harness, token.as_str(), &target);
    assert!(harness.server.activations().is_empty());
    activate(&mut harness, "unknown-token", &target);
    assert!(harness.server.activations().is_empty());
}

fn source() -> (Harness, Window, WindowSnapshot) {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let snapshot = harness.server.windows()[0].clone();
    harness.server.focus(
        Some(snapshot.id),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    harness
        .server
        .pointer_motion(Some((snapshot.surface.clone(), [40.0, 50.0])), 1)
        .unwrap();
    (harness, window, snapshot)
}

#[test]
fn unmatched_and_repeated_releases_do_not_mint_activation_authority() {
    for keyboard in [false, true] {
        let (mut harness, window, _) = source();
        let unmatched = if keyboard {
            harness
                .server
                .keyboard(30, false, 1, |_| false)
                .ok()
                .map(|_| {
                    harness.sync();
                    harness.client.key_serial
                })
        } else {
            harness
                .server
                .pointer_button_serial(272, false, 1)
                .map(u32::from)
        };
        if let Some(serial) = unmatched {
            let token = input_token(&mut harness, &window, serial);
            activate(&mut harness, &token, &window);
            assert!(
                harness.server.activations().is_empty(),
                "keyboard={keyboard}"
            );
        }
        input(&mut harness, keyboard, true);
        input(&mut harness, keyboard, false);
        let repeated = if keyboard {
            harness
                .server
                .keyboard(30, false, 2, |_| false)
                .ok()
                .map(|_| {
                    harness.sync();
                    harness.client.key_serial
                })
        } else {
            harness
                .server
                .pointer_button_serial(272, false, 2)
                .map(u32::from)
        };
        if let Some(serial) = repeated {
            let token = input_token(&mut harness, &window, serial);
            activate(&mut harness, &token, &window);
            assert!(
                harness.server.activations().is_empty(),
                "keyboard={keyboard}"
            );
        }
    }
}

#[test]
fn release_serials_cannot_be_rebound_to_another_windows_source() {
    for keyboard in [false, true] {
        let (mut harness, window, _) = source();
        let other = harness.window();
        let _other_buffer = harness.attach(&other, [200, 160]);
        input(&mut harness, keyboard, true);
        let serial = input(&mut harness, keyboard, false);
        let token = input_token(&mut harness, &other, serial);
        activate(&mut harness, &token, &window);
        assert!(
            harness.server.activations().is_empty(),
            "keyboard={keyboard}"
        );
    }
}

#[test]
fn filtered_keyboard_presses_and_cancelled_gestures_do_not_authorize_later_releases() {
    let (mut harness, window, snapshot) = source();
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.server.keyboard(30, false, 2, |_| true).unwrap();
    harness.server.keyboard(30, true, 3, |_| true).unwrap();
    let serial = input(&mut harness, true, false);
    let token = input_token(&mut harness, &window, serial);
    activate(&mut harness, &token, &window);
    assert!(harness.server.activations().is_empty());
    for keyboard in [false, true] {
        input(&mut harness, keyboard, true);
        harness.server.clear_input(4);
        harness.server.focus(
            Some(snapshot.id),
            smithay::utils::SERIAL_COUNTER.next_serial(),
        );
        harness
            .server
            .pointer_motion(Some((snapshot.surface.clone(), [40.0, 50.0])), 5)
            .unwrap();
        let serial = input(&mut harness, keyboard, false);
        let token = input_token(&mut harness, &window, serial);
        activate(&mut harness, &token, &window);
        assert!(
            harness.server.activations().is_empty(),
            "keyboard={keyboard}"
        );
    }
}

#[test]
fn releases_do_not_refresh_expired_press_authority_and_history_remains_bounded() {
    let (mut harness, window, _) = source();
    for keyboard in [false, true] {
        input(&mut harness, keyboard, true);
    }
    std::thread::sleep(Duration::from_millis(5100));
    for keyboard in [false, true] {
        let serial = input(&mut harness, keyboard, false);
        let token = input_token(&mut harness, &window, serial);
        activate(&mut harness, &token, &window);
        assert!(
            harness.server.activations().is_empty(),
            "keyboard={keyboard}"
        );
    }
    for index in 0..80 {
        input(&mut harness, index % 2 == 0, true);
        input(&mut harness, index % 2 == 0, false);
    }
    assert!(harness.server.state.authority.len() <= 64);
}

#[test]
fn newer_input_and_expiry_revoke_launch_tokens_and_queued_activation() {
    for input in 0..4 {
        let mut harness = Harness::new();
        let target = harness.window();
        let token = harness.server.launch_activation_token().unwrap();
        let queued = harness.server.launch_activation_token().unwrap();
        activate(&mut harness, queued.as_str(), &target);
        assert_eq!(harness.server.state.activations.len(), 1);
        match input {
            0 => harness.server.native_input(),
            1 => {
                harness.server.keyboard(30, true, 1, |_| true).unwrap();
            }
            2 => {
                let source = harness.window();
                let _source = harness.attach(&source, [200, 100]);
                let id = harness.server.windows()[1].id;
                harness
                    .server
                    .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
                harness.server.keyboard(30, true, 1, |_| false).unwrap();
            }
            _ => {
                let mut data = harness
                    .server
                    .state
                    .activation
                    .tokens()
                    .find(|(known, _)| *known == &token)
                    .unwrap()
                    .1
                    .clone();
                harness.server.state.activation.remove_token(&token);
                data.timestamp = Instant::now() - Duration::from_secs(6);
                let expired = harness
                    .server
                    .state
                    .activation
                    .create_external_token(data)
                    .0
                    .clone();
                activate(&mut harness, expired.as_str(), &target);
                harness.server.state.activations.front_mut().unwrap().1 =
                    Instant::now() - Duration::from_secs(6);
            }
        }
        activate(&mut harness, token.as_str(), &target);
        let _buffer = harness.attach(&target, [320, 240]);
        assert!(harness.server.activations().is_empty());
        assert!(harness.server.state.activations.is_empty());
    }
}

#[test]
fn lock_revokes_tokens_and_pending_activation_across_unlock() {
    let mut harness = Harness::new();
    let target = harness.window();
    let token = harness.server.launch_activation_token().unwrap();
    let queued = harness.server.launch_activation_token().unwrap();
    activate(&mut harness, queued.as_str(), &target);
    let lock = harness
        .client
        .lock_manager
        .as_ref()
        .unwrap()
        .lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    assert!(harness.server.launch_activation_token().is_err());
    assert_eq!(harness.server.state.activation.tokens().count(), 0);
    assert!(harness.server.activations().is_empty());
    harness.server.lock_presented();
    harness.sync();
    lock.unlock_and_destroy();
    harness.sync();
    assert!(!harness.server.locked());
    let _buffer = harness.attach(&target, [320, 240]);
    activate(&mut harness, token.as_str(), &target);
    assert!(harness.server.activations().is_empty());
    let fresh = harness.server.launch_activation_token().unwrap();
    activate(&mut harness, fresh.as_str(), &target);
    assert_eq!(
        harness.server.activations(),
        [harness.server.windows()[0].id]
    );
}

#[test]
fn launch_tokens_are_bounded_and_stale_entries_do_not_exhaust_capacity() {
    let mut server = Server::new([640, 480]).unwrap();
    for _ in 0..128 {
        server.launch_activation_token().unwrap();
    }
    assert!(server.launch_activation_token().is_err());
    assert_eq!(server.state.activation.tokens().count(), 128);
    server.native_input();
    server.launch_activation_token().unwrap();
    assert_eq!(server.state.activation.tokens().count(), 1);
}

#[test]
fn foreground_child_gets_a_usable_token_and_failed_spawn_revokes_it() {
    use crate::launch::{Application, Capability, Description};
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("wayland-test");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let mut harness = Harness::new();
    let target = harness.window();
    let _buffer = harness.attach(&target, [320, 240]);
    let output = directory.path().join("token");
    for capabilities in [Vec::new(), vec![Capability::WindowList]] {
        let mut description = Description {
            program: "sh".into(),
            arguments: vec![
                "-c".into(),
                "printf '%s' \"${XDG_ACTIVATION_TOKEN-}\" > \"$1\"".into(),
                "launch-test".into(),
                output.to_str().unwrap().into(),
            ],
            directory: None,
            capabilities,
        };
        let mut child = harness
            .server
            .spawn_foreground_helper(&description, &socket)
            .unwrap();
        assert!(child.wait().unwrap().success());
        let token = std::fs::read_to_string(&output).unwrap();
        assert!(!token.is_empty());
        activate(&mut harness, &token, &target);
        assert_eq!(
            harness.server.activations(),
            [harness.server.windows()[0].id]
        );
        let mut child = harness.server.spawn_helper(&description, &socket).unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(std::fs::read_to_string(&output).unwrap(), "");
        description.program = directory
            .path()
            .join("missing-executable")
            .to_str()
            .unwrap()
            .into();
        assert!(
            harness
                .server
                .spawn_foreground_helper(&description, &socket)
                .is_err()
        );
        assert_eq!(harness.server.state.activation.tokens().count(), 0);
        harness.sync();
    }
    let absent = directory.path().join("absent-socket");
    assert!(
        harness
            .server
            .spawn_foreground(Application::new("sh").unwrap(), &absent)
            .is_err()
    );
    assert_eq!(harness.server.state.activation.tokens().count(), 0);
}
