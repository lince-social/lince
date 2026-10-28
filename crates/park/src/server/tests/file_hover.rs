use super::*;

#[test]
fn only_a_real_uri_offer_exposes_hover_authority_without_an_accepted_payload_target() {
    for mime in ["text/uri-list", "text/plain;charset=utf-8"] {
        let mut harness = Harness::new();
        let window = harness.window();
        let _buffer = harness.attach(&window, [320, 240]);
        for _ in 0..8 {
            harness.sync();
        }
        let surface = harness.server.windows()[0].surface.clone();
        harness.server.focus_surface(
            Some(surface.clone()),
            smithay::utils::SERIAL_COUNTER.next_serial(),
        );
        harness
            .server
            .pointer_motion(Some((surface.clone(), [20.0, 20.0])), 1)
            .unwrap();
        harness.server.pointer_button(272, true, 2);
        harness.sync();
        let qh = harness.queue.handle();
        let manager = harness.client.data_manager.as_ref().unwrap();
        let device = manager.get_data_device(harness.client.seat.as_ref().unwrap(), &qh, ());
        let source = manager.create_data_source(&qh, ());
        source.offer(mime.into());
        source.set_actions(wl_data_device_manager::DndAction::Copy);
        assert!(harness.server.native_file_drag().is_none());
        device.start_drag(
            Some(&source),
            &window.surface,
            None,
            harness.client.pointer_serial,
        );
        for _ in 0..4 {
            harness.sync();
        }
        harness.server.native_drop_target(None);
        let native = harness.server.native_surface().unwrap();
        harness
            .server
            .pointer_motion(Some((native.clone(), [100.0, 100.0])), 3)
            .unwrap();
        for _ in 0..8 {
            harness.sync();
        }
        let token = harness.server.native_file_drag();
        assert_eq!(token.is_some(), mime == "text/uri-list");
        if let Some(token) = token {
            harness.server.native_focus();
            for _ in 0..4 {
                harness.sync();
            }
            assert_eq!(harness.server.native_file_drag(), Some(token));
            harness
                .server
                .native_editor(Some(crate::native_input::Editor {
                    owner: 9,
                    revision: 1,
                    projection: 0,
                    from_ime: false,
                    selection: [0, 0],
                    surrounding: String::new(),
                    cursor: 0,
                    anchor: 0,
                    sensitive: false,
                    multiline: false,
                    caret: [0, 0, 1, 20],
                }));
            for _ in 0..4 {
                harness.sync();
            }
            assert_eq!(harness.server.native_file_drag(), Some(token));
            harness.server.suspend_native_drop();
            assert!(harness.server.native_file_drag().is_none());
            harness
                .server
                .pointer_motion(Some((surface, [20.0, 20.0])), 4)
                .unwrap();
            for _ in 0..4 {
                harness.sync();
            }
            harness
                .server
                .pointer_motion(Some((native, [100.0, 100.0])), 5)
                .unwrap();
            for _ in 0..8 {
                harness.sync();
            }
            assert!(
                harness
                    .server
                    .native_file_drag()
                    .is_some_and(|next| next > token)
            );
        }
        harness.server.pointer_button(272, false, 6);
        for _ in 0..8 {
            harness.sync();
        }
        assert!(harness.server.native_file_drag().is_none());
        assert!(
            !harness
                .server
                .native_events()
                .into_iter()
                .any(|event| matches!(event, crate::native_input::client::Event::Drop(_)))
        );
        assert_eq!(harness.client.drop_finished, 0);
        source.destroy();
        device.release();
        harness.sync();
    }
}

#[test]
fn an_invalid_drag_start_serial_cannot_arm_overview_hover() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    for _ in 0..8 {
        harness.sync();
    }
    let surface = harness.server.windows()[0].surface.clone();
    harness.server.focus_surface(
        Some(surface.clone()),
        smithay::utils::SERIAL_COUNTER.next_serial(),
    );
    harness
        .server
        .pointer_motion(Some((surface, [20.0, 20.0])), 1)
        .unwrap();
    harness.server.pointer_button(272, true, 2);
    harness.sync();
    let qh = harness.queue.handle();
    let manager = harness.client.data_manager.as_ref().unwrap();
    let device = manager.get_data_device(harness.client.seat.as_ref().unwrap(), &qh, ());
    let source = manager.create_data_source(&qh, ());
    source.offer("text/uri-list".into());
    source.set_actions(wl_data_device_manager::DndAction::Copy);
    device.start_drag(
        Some(&source),
        &window.surface,
        None,
        harness.client.pointer_serial.wrapping_add(100),
    );
    for _ in 0..4 {
        harness.sync();
    }
    let native = harness.server.native_surface().unwrap();
    harness
        .server
        .pointer_motion(Some((native, [100.0, 100.0])), 3)
        .unwrap();
    for _ in 0..8 {
        harness.sync();
    }
    assert!(harness.server.native_file_drag().is_none());
    harness.server.pointer_button(272, false, 4);
    source.destroy();
    device.release();
    harness.sync();
}
