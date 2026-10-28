use super::*;
use super::super::{configuration, magnification::clients, opening};
use lince_interface::host::{color::OutputColor, display::{AdaptiveSyncStatus, AdaptiveSyncSupport, ColorStatus, DisplayOutcome}};

fn failed(fixture: &mut Fixture, caption: &str) {
    let before = fixture.desktop.server.display_snapshot().last_request;
    opening::activate(fixture, caption);
    fixture.until(|fixture| {
        let snapshot = fixture.desktop.server.display_snapshot();
        snapshot.last_request > before && snapshot.response.as_ref().is_some_and(|response| matches!(response.outcome, DisplayOutcome::Failed(_)))
    });
}

#[test]
#[ignore = "requires hardware Vulkan and EGL; physical HDR and VRR are not claimed by this nested refusal fixture"]
fn ordinary_color_and_vrr_controls_preserve_rejected_drafts_without_saving_or_resizing_then_keep_and_reload_a_usable_sdr_desktop() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park.json");
    let original = br#"{"keyboard":{"layout":"br"}}"#;
    std::fs::write(&path, original).unwrap();
    let mut fixture = rotation::create(&runtime, directory.path());
    rotation::output(&mut fixture, OutputId(2));
    let (window, initial, id) = opening::map(&mut fixture);
    window.role.set_max_size(240, 180); window.surface.commit(); fixture.exchange();
    fixture.desktop.send(Input::Application(id)).unwrap(); fixture.inspect(|_| ());
    let mut buffers = vec![initial];
    let size = clients::repaint(&mut fixture, &window, id, &mut buffers);
    let item = clients::geometry(&fixture, id);
    let configurations = fixture.client.window_configure_sizes[&window.role.id()].len();
    fixture.desktop.server.output_color(OutputId(2), ColorStatus { hdr10: true, prepared: Some(OutputColor::default()),
        effective: Some(OutputColor::default()), revision: 2, reason: Some("Candidate capability fixture; nested output remains SDR.".into()), ..Default::default() });
    fixture.desktop.server.output_adaptive_sync(OutputId(2), AdaptiveSyncStatus { support: AdaptiveSyncSupport::Supported,
        prepared: Some(false), effective: Some(false), pending: false, reason: Some("Candidate capability fixture; parent cadence is external.".into()) });
    rotation::open(&mut fixture);
    let before = fixture.desktop.server.display_snapshot().configuration();
    opening::activate(&mut fixture, "Color: SDR sRGB · change");
    opening::activate(&mut fixture, "Adaptive sync: Off · change");
    failed(&mut fixture, "Test");
    assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(fixture.desktop.server.display_snapshot().preview.is_none());
    failed(&mut fixture, "Apply");
    assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(fixture.desktop.server.display_snapshot().preview.is_none());
    let messages = fixture.inspect(|canvas| {
        let world = canvas.world_mut();
        world.query::<&Text>().iter(world).map(|text| text.0.clone()).collect::<Vec<_>>()
    });
    assert!(messages.iter().any(|message| message.contains("last confirmed: SDR sRGB")));
    assert!(messages.iter().any(|message| message.contains("active: off")));
    opening::activate(&mut fixture, "Color: HDR10 · change");
    opening::activate(&mut fixture, "Adaptive sync: On · change");
    rotation::request(&mut fixture, "Test", DisplayOutcome::Tested);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    rotation::request(&mut fixture, "Apply", DisplayOutcome::Previewed);
    let preview = fixture.desktop.server.display_snapshot().preview.unwrap();
    assert!(preview.deadline <= Instant::now() + Duration::from_secs(15));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    rotation::request(&mut fixture, "Revert", DisplayOutcome::Reverted);
    assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    rotation::request(&mut fixture, "Apply", DisplayOutcome::Previewed);
    rotation::request(&mut fixture, "Keep changes", DisplayOutcome::Kept);
    let saved = std::fs::read(&path).unwrap();
    let loaded = crate::commands::Commands::load(&path).unwrap();
    assert_eq!(loaded.keyboard.layout, "br");
    assert_eq!(loaded.outputs["Rotation-B"].color, lince_interface::host::color::ColorMode::Srgb);
    assert!(!loaded.outputs["Rotation-B"].adaptive_sync);
    rotation::close(&mut fixture);
    assert_eq!(clients::geometry(&fixture, id), item);
    assert!(fixture.client.window_configure_sizes[&window.role.id()][configurations..].iter().all(|configured| *configured == size.map(|value| value as i32)),
        "Color/cadence-only control changes resized the ordinary application");
    configuration::application_input(&mut fixture, id, &window);
    fixture.desktop.worker.stop().unwrap();
    let mut restarted = rotation::create(&runtime, directory.path());
    assert_eq!(restarted.desktop.server.display_snapshot().configuration(), before);
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert_eq!(restarted.desktop.server.keyboard_snapshot().settings.layout, "br");
    assert!(restarted.desktop.running);
    restarted.desktop.worker.stop().unwrap();
}
