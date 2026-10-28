use super::*;
use lince_interface::host::mapping::QuarterTurn;

mod rotation;
mod color;
mod controls;
mod lifetimes;

fn create(runtime: &tokio::runtime::Runtime, directory: &std::path::Path) -> Fixture {
    let mut harness = Harness::new();
    harness.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Park-1".into(), origin: [0, 0], size: [1000, 720] }]).unwrap();
    harness.server.registered_output_catalog().unwrap();
    harness.sync();
    let mut fixture = Fixture::new(harness, runtime, directory);
    fixture.desktop.commands(directory.join("park.json"), directory.join("wayland-rotation-fixture"));
    fixture.until(|fixture| fixture.desktop.current.is_some());
    fixture
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn saved_rotation_loads_at_actual_worker_startup_survives_fitting_parent_resize_and_recovers_without_overwriting_saved_intent() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park.json");
    let saved = serde_json::to_vec(&serde_json::json!({"outputs":{"Park-1": {
        "mode": [600, 800], "rotation": "clockwise90", "position": [100, 40], "scale": 1.25 }}, "keyboard":{"layout":"br"}})).unwrap();
    std::fs::write(&path, &saved).unwrap();
    let mut fixture = create(&runtime, directory.path());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.outputs.iter().any(|output| output.id == OutputId(1) && output.rotation == QuarterTurn::Clockwise90))));
    let before = fixture.desktop.server.display_snapshot();
    assert_eq!(before.heads[0].rotation, QuarterTurn::Clockwise90);
    assert_eq!(before.heads[0].settings().unwrap().logical_size(), [640, 480]);
    fixture.desktop.resize_virtual_outputs([1100, 800]).unwrap();
    assert_eq!(fixture.desktop.server.display_snapshot().configuration(), before.configuration());
    assert_eq!(fixture.desktop.server.display_snapshot().revision, before.revision);
    fixture.desktop.resize_virtual_outputs([320, 240]).unwrap();
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|frame| frame.frame.geometry.as_ref()
        .is_some_and(|geometry| geometry.outputs.iter().any(|output| output.id == OutputId(1) && output.physical_size == [320, 240]))));
    assert_eq!(fixture.desktop.server.display_snapshot().heads[0].rotation, QuarterTurn::Normal);
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    fixture.desktop.worker.stop().unwrap();
    let mut restarted = create(&runtime, directory.path());
    assert_eq!(restarted.desktop.server.display_snapshot().configuration(), before.configuration());
    restarted.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn invalid_saved_virtual_geometry_preserves_the_usable_default_and_other_startup_settings() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park.json");
    let saved = serde_json::to_vec(&serde_json::json!({"outputs":{"Park-1": {
        "rotation":"clockwise90", "position":[10000,10000]}},"keyboard":{"layout":"br"}})).unwrap();
    std::fs::write(&path, &saved).unwrap();
    let mut fixture = create(&runtime, directory.path());
    let snapshot = fixture.desktop.server.display_snapshot();
    assert_eq!(snapshot.heads[0].rotation, QuarterTurn::Normal);
    assert_eq!(snapshot.heads[0].position, [0, 0]);
    assert_eq!(snapshot.heads[0].settings().unwrap().mode.size, [1000, 720]);
    assert_eq!(fixture.desktop.server.keyboard_snapshot().settings.layout, "br");
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    fixture.desktop.worker.stop().unwrap();
}
