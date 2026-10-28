use super::*;
use lince_interface::host::input::{InputOutcome, KeyboardRequest, KeyboardSettings};

#[test]
fn consumed_modifier_release_updates_the_newly_focused_client_without_replaying_keys() {
    for code in [29, 42, 56, 125] {
        let mut harness = Harness::new();
        let window = harness.window();
        let _buffer = harness.attach(&window, [400, 300]);
        let id = harness.server.windows()[0].id;
        harness.server.keyboard(code, true, 1, |_| true).unwrap();
        harness
            .server
            .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
        harness.sync();
        assert_ne!(
            harness.client.modifiers.last().unwrap()[0],
            0,
            "modifier={code}"
        );
        harness.client.modifiers.clear();
        harness.server.keyboard(code, false, 2, |_| true).unwrap();
        harness.sync();
        assert!(harness.client.keys.is_empty());
        assert_eq!(
            harness.client.modifiers.last(),
            Some(&[0; 4]),
            "modifier={code}"
        );
        harness.server.keyboard(34, true, 3, |_| false).unwrap();
        harness.server.keyboard(34, false, 4, |_| false).unwrap();
        harness.sync();
        assert_eq!(harness.client.keys, [(34, true), (34, false)]);
        assert_eq!(harness.client.modifiers.last(), Some(&[0; 4]));
    }
}

#[test]
fn intercepted_modifier_changes_respect_the_input_method_grab_and_update_its_next_event() {
    let mut harness = Harness::connect_authorized(
        Server::new([640, 480]).unwrap(),
        Some(vec![crate::launch::Capability::InputMethod]),
    );
    let window = harness.window();
    let _buffer = harness.attach(&window, [400, 300]);
    let id = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(id), smithay::utils::SERIAL_COUNTER.next_serial());
    let method = harness
        .client
        .input_method
        .as_ref()
        .unwrap()
        .get_input_method(
            harness.client.seat.as_ref().unwrap(),
            &harness.queue.handle(),
            (),
        );
    harness.sync();
    let grab = method.grab_keyboard(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.input_method_grabbed());
    harness.client.modifiers.clear();
    harness.client.input_modifiers.clear();
    harness.server.keyboard(29, true, 1, |_| true).unwrap();
    harness.server.keyboard(29, false, 2, |_| true).unwrap();
    harness.sync();
    assert!(harness.client.modifiers.is_empty());
    assert!(harness.client.keys.is_empty());
    assert!(harness.client.input_modifiers.is_empty());
    assert!(harness.client.input_keys.is_empty());
    harness.server.keyboard(34, true, 3, |_| false).unwrap();
    harness.server.keyboard(34, false, 4, |_| false).unwrap();
    harness.sync();
    assert!(harness.client.modifiers.is_empty());
    assert!(harness.client.keys.is_empty());
    assert_eq!(harness.client.input_modifiers.last(), Some(&[0; 4]));
    assert_eq!(harness.client.input_keys, [(34, true), (34, false)]);
    grab.release();
    method.destroy();
    harness.sync();
}

fn settings(layout: &str) -> KeyboardSettings {
    KeyboardSettings {
        layout: layout.into(),
        repeat_rate: 35,
        repeat_delay_ms: 300,
        ..Default::default()
    }
}

fn request(server: &mut Server, id: u64, settings: KeyboardSettings) {
    let revision = server.keyboard_snapshot().revision;
    server.keyboard_request(KeyboardRequest {
        id,
        revision,
        settings,
    });
}

#[test]
fn keyboard_map_and_repeat_changes_reach_a_real_client_and_keep_physical_key_identity() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [400, 300]);
    let surface = harness.server.windows()[0].id;
    harness
        .server
        .focus(Some(surface), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.configure_keyboard(settings("us")).unwrap();
    harness.sync();
    let previous = harness.client.keymaps.last().unwrap().clone();
    request(&mut harness.server, 1, settings("br"));
    assert!(harness.server.advance_keyboard(|_, _| Ok(None)).is_some());
    harness.sync();
    assert_ne!(harness.client.keymaps.last().unwrap(), &previous);
    assert_eq!(harness.client.repeat_info.last(), Some(&(35, 300)));
    let snapshot = harness.server.keyboard_snapshot();
    assert_eq!(snapshot.settings.layout, "br");
    assert_eq!(snapshot.labels["Semicolon"].plain, "ç");
    assert_eq!(snapshot.labels["Semicolon"].shifted, "Ç");
    assert_eq!(
        harness.server.key_symbol(39).unwrap().text.as_deref(),
        Some("ç")
    );
    harness.server.keyboard(42, true, 10, |_| false).unwrap();
    assert_eq!(
        harness.server.key_symbol(39).unwrap().text.as_deref(),
        Some("Ç")
    );
    harness.server.keyboard(39, true, 11, |_| false).unwrap();
    harness.server.keyboard(39, false, 12, |_| false).unwrap();
    harness.server.keyboard(42, false, 13, |_| false).unwrap();
    harness.sync();
    assert!(harness.client.keys.contains(&(39, true)));
    assert!(harness.client.keys.contains(&(39, false)));
}

#[test]
fn keyboard_apply_waits_for_held_keys_and_persists_once_after_release() {
    let mut server = Server::new([800, 600]).unwrap();
    server.configure_keyboard(settings("us")).unwrap();
    server.keyboard(42, true, 1, |_| true).unwrap();
    request(&mut server, 1, settings("br"));
    assert!(
        server
            .advance_keyboard(|_, _| panic!("A held key must prevent persistence"))
            .is_none()
    );
    assert_eq!(server.keyboard_snapshot().settings.layout, "us");
    assert!(matches!(
        server.keyboard_snapshot().response.unwrap().outcome,
        InputOutcome::Pending
    ));
    server.keyboard(42, false, 2, |_| true).unwrap();
    let calls = std::cell::Cell::new(0);
    assert!(
        server
            .advance_keyboard(|candidate, previous| {
                calls.set(calls.get() + 1);
                assert_eq!(candidate.layout, "br");
                assert_eq!(previous.layout, "us");
                Ok(None)
            })
            .is_some()
    );
    assert!(
        server
            .advance_keyboard(|_, _| panic!("A completed request must not save twice"))
            .is_none()
    );
    assert_eq!(calls.get(), 1);
    assert!(matches!(
        server.keyboard_snapshot().response.unwrap().outcome,
        InputOutcome::Saved { .. }
    ));
}

#[test]
fn keyboard_failures_preserve_the_active_map_and_stale_requests_never_save() {
    let mut server = Server::new([800, 600]).unwrap();
    server.configure_keyboard(settings("us")).unwrap();
    let original = server.keyboard_snapshot();
    request(&mut server, 1, settings("br"));
    assert!(
        server
            .advance_keyboard(|_, _| Err(io::Error::other("fixture write failure")))
            .is_none()
    );
    let failed = server.keyboard_snapshot();
    assert_eq!(failed.settings, original.settings);
    assert_eq!(failed.labels, original.labels);
    assert_eq!(failed.revision, original.revision);
    assert_eq!(server.key_symbol(39).unwrap().text.as_deref(), Some(";"));
    request(&mut server, 2, settings("not-a-real-layout"));
    assert!(
        server
            .advance_keyboard(|_, _| panic!("An invalid keymap must not be saved"))
            .is_none()
    );
    assert_eq!(server.keyboard_snapshot().settings, original.settings);
    server.keyboard_request(KeyboardRequest {
        id: 3,
        revision: original.revision + 1,
        settings: settings("br"),
    });
    assert!(
        server
            .advance_keyboard(|_, _| panic!("A stale request must not be saved"))
            .is_none()
    );
    assert!(matches!(
        server.keyboard_snapshot().response.unwrap().outcome,
        InputOutcome::Failed(_)
    ));
}

#[test]
fn locking_cancels_a_pending_keyboard_change_without_saving() {
    let mut harness = Harness::new();
    harness.server.configure_keyboard(settings("us")).unwrap();
    request(&mut harness.server, 1, settings("br"));
    let _locker = harness
        .client
        .lock_manager
        .as_ref()
        .unwrap()
        .lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    assert!(
        harness
            .server
            .advance_keyboard(|_, _| panic!("Lock must prevent keyboard configuration saves"))
            .is_none()
    );
    assert_eq!(harness.server.keyboard_snapshot().settings.layout, "us");
    request(&mut harness.server, 2, settings("br"));
    assert!(matches!(
        harness.server.keyboard_snapshot().response.unwrap().outcome,
        InputOutcome::Failed(_)
    ));
}
