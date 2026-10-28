use super::*;
use super::super::super::opening;
use lince_interface::host::input::{AbsoluteCommand, InputOutcome, InputResponse};

const CALIBRATION: &str = "Calibration: a b c d e f (empty uses device default)";
const DRAFT: &str = "1 0 0.1 0 1 0";

fn snapshot(fixture: &mut Fixture) -> AbsoluteSnapshot {
    fixture.inspect(|canvas| canvas.world_mut().resource::<AbsoluteSnapshot>().clone())
}

fn value(fixture: &mut Fixture) -> String {
    let editor = magnification::focus(fixture, CALIBRATION);
    fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(editor).unwrap().value().to_string())
}

fn request(fixture: &mut Fixture) -> AbsoluteCommand {
    fixture.inspect(|_| ());
    fixture.desktop.dispatch().unwrap();
    let queue = fixture.desktop.absolute_requests.as_mut().unwrap();
    assert_eq!(queue.len(), 1, "The actual settings control must issue exactly one request");
    queue.pop_front().unwrap()
}

fn observed(fixture: &mut Fixture, snapshot: AbsoluteSnapshot) {
    fixture.desktop.absolute_settings(snapshot).unwrap();
    fixture.inspect(|_| ());
}

fn reply(fixture: &mut Fixture, request: u64, settings: Option<lince_interface::host::input::AbsolutePreferences>, result: std::io::Result<Option<String>>) {
    let mut state = snapshot(fixture);
    let outcome = match result {
        Ok(warning) => { state.settings = settings.unwrap(); state.revision += 1; InputOutcome::Saved { warning } }
        Err(error) => InputOutcome::Failed(error.to_string()),
    };
    state.last_request = request;
    state.response = Some(InputResponse { request, outcome });
    observed(fixture, state);
}

#[test]
#[ignore = "requires hardware Vulkan/EGL; supplied observations do not prove successful physical libinput calibration"]
fn ordinary_calibration_controls_preserve_failed_drafts_and_saved_state_then_save_reload_and_reconnect_using_real_file_io() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Fixture::new(Harness::new(), &runtime, directory.path());
    let path = directory.path().join("park.json");
    std::fs::write(&path, br#"{"keyboard":{"layout":"us"}}"#).unwrap();
    let original = std::fs::read(&path).unwrap();
    fixture.desktop.commands(path.clone(), directory.path().join("wayland-fixture"));
    fixture.desktop.absolute_requests = Some(Default::default());
    let mut device = device();
    device.calibration = true;
    device.default_calibration = Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    device.effective_calibration = device.default_calibration;
    let initial = AbsoluteSnapshot { available: true, revision: 1, devices: vec![device.clone()], ..Default::default() };
    observed(&mut fixture, initial.clone());
    fixture.desktop.key(125, true, 1).unwrap();
    fixture.desktop.key(18, true, 2).unwrap(); fixture.desktop.key(18, false, 3).unwrap();
    fixture.desktop.key(125, false, 4).unwrap(); fixture.inspect(|_| ());
    opening::activate(&mut fixture, "Park");
    opening::activate(&mut fixture, "Edit touch/tablet settings");
    opening::write(&mut fixture, CALIBRATION, "0 0 0 0 0 0");
    opening::activate(&mut fixture, "Save device settings");
    assert!(fixture.desktop.absolute_requests.as_ref().unwrap().is_empty());
    assert_eq!(value(&mut fixture), "0 0 0 0 0 0");
    assert_eq!(std::fs::read(&path).unwrap(), original);
    opening::write(&mut fixture, CALIBRATION, DRAFT);
    opening::activate(&mut fixture, "Save device settings");
    let AbsoluteCommand::Save(removed) = request(&mut fixture) else { panic!("Expected calibration save"); };
    let rejected = crate::session::reject_removed_calibration(removed, snapshot(&mut fixture));
    assert!(matches!(rejected.response.as_ref().unwrap().outcome, InputOutcome::Failed(_)));
    assert_eq!(rejected.settings, initial.settings);
    observed(&mut fixture, rejected);
    let field = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        world.query::<(Entity, &bevy::a11y::AccessibilityNode)>().iter(world).find(|(_, node)| node.label() == Some(CALIBRATION)).unwrap().0
    });
    assert_eq!(fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(field).unwrap().value().to_string()), DRAFT);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let mut reconnected = snapshot(&mut fixture);
    device.generation += 1;
    reconnected.devices = vec![device.clone()]; reconnected.revision += 1;
    observed(&mut fixture, reconnected);
    opening::activate(&mut fixture, "Cancel draft / restore active settings");
    opening::write(&mut fixture, CALIBRATION, DRAFT);
    opening::activate(&mut fixture, "Save device settings");
    let AbsoluteCommand::Save(save) = request(&mut fixture) else { panic!("Expected ordinary settings save"); };
    assert_eq!(save.settings[device.key.as_ref().unwrap()].calibration, Some([1.0, 0.0, 0.1, 0.0, 1.0, 0.0]));
    let retained = directory.path().join("retained-park.json");
    std::fs::rename(&path, &retained).unwrap();
    std::fs::create_dir(&path).unwrap();
    let previous = snapshot(&mut fixture).settings;
    let failed = fixture.desktop.save_devices(&save.settings, &previous);
    assert!(failed.is_err());
    reply(&mut fixture, save.id, None, failed);
    assert_eq!(value(&mut fixture), DRAFT);
    assert_eq!(snapshot(&mut fixture).settings, initial.settings);
    assert_eq!(snapshot(&mut fixture).devices[0].effective_calibration, device.default_calibration);
    assert_eq!(std::fs::read(&retained).unwrap(), original);
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&retained, &path).unwrap();
    opening::activate(&mut fixture, "Reset calibration to device default");
    assert_eq!(value(&mut fixture), "");
    opening::activate(&mut fixture, "Tool button 1: Device default · click to cycle");
    opening::activate(&mut fixture, "Save device settings");
    let AbsoluteCommand::Save(save) = request(&mut fixture) else { panic!("Expected ordinary button mapping save"); };
    assert_eq!(save.settings[device.key.as_ref().unwrap()].buttons.get(&331), Some(&272));
    let expected = snapshot(&mut fixture).settings;
    let saved = fixture.desktop.save_devices(&save.settings, &expected);
    assert!(saved.is_ok());
    reply(&mut fixture, save.id, Some(save.settings.clone()), saved);
    assert_eq!(fixture.desktop.load_devices().unwrap(), save.settings);
    assert_eq!(crate::commands::Commands::load(&path).unwrap().keyboard.layout, "us");
    let saved_bytes = std::fs::read(&path).unwrap();
    opening::write(&mut fixture, CALIBRATION, DRAFT);
    opening::activate(&mut fixture, "Reload saved file / discard draft");
    let AbsoluteCommand::Reload(reload) = request(&mut fixture) else { panic!("Expected ordinary file reload"); };
    let settings = fixture.desktop.load_devices().unwrap();
    reply(&mut fixture, reload.id, Some(settings), Ok(None));
    assert_eq!(value(&mut fixture), "");
    assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
    let mut unplugged = snapshot(&mut fixture);
    unplugged.devices.clear(); unplugged.revision += 1;
    observed(&mut fixture, unplugged);
    let mut connected = snapshot(&mut fixture);
    device.generation += 1; connected.devices = vec![device.clone()]; connected.revision += 1;
    observed(&mut fixture, connected);
    opening::activate(&mut fixture, "Cancel draft / restore active settings");
    assert_eq!(value(&mut fixture), "");
    assert_eq!(snapshot(&mut fixture).settings, save.settings);
    opening::activate(&mut fixture, "Tool button 1: Left button · click to cycle");
    opening::activate(&mut fixture, "Save device settings");
    let AbsoluteCommand::Save(next) = request(&mut fixture) else { panic!("Expected reconnected device mapping save"); };
    assert_eq!(next.settings[device.key.as_ref().unwrap()].buttons.get(&331), Some(&273));
    assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
    fixture.desktop.worker.stop().unwrap();
}
