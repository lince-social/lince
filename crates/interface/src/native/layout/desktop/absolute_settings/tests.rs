use super::*;
use crate::host::{OutputId, display::OutputHead, input::InputResponse};

fn fixture() -> (App, Entity, std::sync::mpsc::Receiver<HostRequest>) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>().init_resource::<crate::native::theme::Typography>().init_resource::<InputFocus>();
    super::super::input_settings::install(&mut app);
    install(&mut app);
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    app.insert_resource(HostRequests(sender));
    app.insert_resource(AbsoluteSnapshot {
        available: true, revision: 1,
        devices: vec![AbsoluteDevice { id: "event-test".into(), key: Some("stable-touch".into()), generation: 7, name: "Test touch screen".into(), touch: true, tablet: false, calibration: true, default_calibration: Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]), effective_calibration: Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]), associated_output: None, buttons: Vec::new(), error: None }],
        ..Default::default()
    });
    app.insert_resource(DisplaySnapshot {
        heads: vec![OutputHead { id: OutputId(2), name: "DP-1".into(), modes: Vec::new(), current: None, position: [0, 0], scale: 1.0, rotation: Default::default(), adaptive_sync: false, color: Default::default() }],
        ..Default::default()
    });
    let parent = app.world_mut().spawn(Node::default()).id();
    panel(app.world_mut(), parent);
    app.update();
    let page = app.world_mut().query_filtered::<Entity, With<Page>>().single(app.world()).unwrap();
    (app, page, receiver)
}

#[test]
fn controls_save_captured_preferences_preserve_newer_edits_and_cancel_without_another_request() {
    let (mut app, page, receiver) = fixture();
    Change::Toggle.apply(app.world_mut(), page);
    let state = app.world().get::<Page>(page).unwrap();
    assert_eq!(app.world().get::<Node>(state.body).unwrap().display, Display::Flex);
    Change::Output.apply(app.world_mut(), page);
    Change::Save.apply(app.world_mut(), page);
    let HostRequest::Absolute(request) = receiver.try_recv().unwrap() else { panic!("Expected a device settings request"); };
    assert_eq!(request.revision, 1);
    assert_eq!(request.settings["stable-touch"].output.as_deref(), Some("DP-1"));
    let editor = app.world().get::<Page>(page).unwrap().calibration;
    set_value(app.world_mut(), editor, "1 0 0.1 0 1 0".into());
    let mut snapshot = app.world().resource::<AbsoluteSnapshot>().clone();
    snapshot.settings = request.settings;
    snapshot.revision += 1;
    snapshot.last_request = request.id;
    snapshot.response = Some(InputResponse { request: request.id, outcome: InputOutcome::Saved { warning: None } });
    app.insert_resource(snapshot.clone());
    app.update();
    assert_eq!(current(app.world(), page, &snapshot).unwrap()["stable-touch"].calibration, Some([1.0, 0.0, 0.1, 0.0, 1.0, 0.0]));
    assert!(app.world().get::<Page>(page).unwrap().pending.is_none());
    Change::Cancel.apply(app.world_mut(), page);
    assert_eq!(current(app.world(), page, &snapshot).unwrap(), snapshot.settings);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn replaced_device_invalid_matrix_and_unsupported_host_preserve_the_draft_without_sending() {
    let (mut app, page, receiver) = fixture();
    let editor = app.world().get::<Page>(page).unwrap().calibration;
    set_value(app.world_mut(), editor, "0 0 0 0 0 0".into());
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    set_value(app.world_mut(), editor, "1 0 0.1 0 1 0".into());
    let mut snapshot = app.world().resource::<AbsoluteSnapshot>().clone();
    snapshot.devices[0].generation += 1;
    snapshot.revision += 1;
    app.insert_resource(snapshot);
    app.update();
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    assert_eq!(app.world().get::<EditableText>(editor).unwrap().value(), "1 0 0.1 0 1 0");
    Change::Cancel.apply(app.world_mut(), page);
    app.world_mut().resource_mut::<AbsoluteSnapshot>().available = false;
    app.update();
    Change::Save.apply(app.world_mut(), page);
    assert!(receiver.try_recv().is_err());
    assert!(app.world().get::<bevy::ui::InteractionDisabled>(app.world().get::<Page>(page).unwrap().save).is_some());
}

#[test]
fn reported_tool_buttons_have_a_simple_cycle_and_saved_file_reload_has_its_own_request() {
    let (mut app, page, receiver) = fixture();
    {
        let mut snapshot = app.world_mut().resource_mut::<AbsoluteSnapshot>();
        snapshot.devices[0].tablet = true;
        snapshot.devices[0].buttons = vec![331, 332];
        snapshot.revision += 1;
    }
    app.update();
    assert_eq!(app.world().get::<Page>(page).unwrap().button_rows.len(), 2);
    let advanced = app.world().get::<Page>(page).unwrap().advanced;
    assert_eq!(app.world().get::<Node>(advanced).unwrap().display, Display::None);
    Change::MapButton(331).apply(app.world_mut(), page);
    Change::Save.apply(app.world_mut(), page);
    let HostRequest::Absolute(request) = receiver.try_recv().unwrap() else { panic!("Expected saving a button mapping"); };
    assert_eq!(request.settings["stable-touch"].buttons, BTreeMap::from([(331, 272)]));
    let mut snapshot = app.world().resource::<AbsoluteSnapshot>().clone();
    snapshot.settings = request.settings;
    snapshot.revision += 1;
    snapshot.last_request = request.id;
    snapshot.response = Some(InputResponse { request: request.id, outcome: InputOutcome::Saved { warning: None } });
    app.insert_resource(snapshot);
    app.update();
    Change::Reload.apply(app.world_mut(), page);
    let HostRequest::AbsoluteReload(request) = receiver.try_recv().unwrap() else { panic!("Expected an explicit file reload"); };
    app.update();
    let editor = app.world().get::<Page>(page).unwrap().calibration;
    assert!(app.world().get::<bevy::ui::InteractionDisabled>(editor).is_some());
    let mut snapshot = app.world().resource::<AbsoluteSnapshot>().clone();
    snapshot.settings.insert("stable-touch".into(), AbsolutePreference { output: Some("HDMI-A-1".into()), ..Default::default() });
    snapshot.revision += 1;
    snapshot.last_request = request.id;
    snapshot.response = Some(InputResponse { request: request.id, outcome: InputOutcome::Saved { warning: None } });
    app.insert_resource(snapshot.clone());
    app.update();
    assert_eq!(current(app.world(), page, &snapshot).unwrap(), snapshot.settings);
    assert!(!app.world().get::<Page>(page).unwrap().pending_reload);
}
