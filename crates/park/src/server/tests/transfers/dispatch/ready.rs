use super::*;
use crate::desktop::Ready;

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn session_readiness_waits_for_a_presented_frame_and_dispatches_its_helper_once() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("ready-count");
    let commands = directory.path().join("park.json");
    std::fs::write(&commands, serde_json::to_vec(&serde_json::json!({
        "ready": {"program":"dd", "arguments":["if=/dev/zero",format!("of={}", marker.display()),"oflag=append","conv=notrunc","bs=1","count=1","status=none"]}
    })).unwrap()).unwrap();
    let mut harness = Harness::new();
    harness.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Ready fixture".into(), origin: [0, 0], size: [1000, 720] }]).unwrap();
    harness.sync();
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    let socket = directory.path().join("wayland-ready");
    fixture.listen(&socket);
    fixture.desktop.commands(commands, socket);
    assert_eq!(fixture.desktop.ready(), Ready::Waiting);
    assert!(!marker.exists());
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|current| current.frame.input.opening_ready));
    assert_eq!(fixture.desktop.ready(), Ready::Dispatched);
    fixture.until(|_| std::fs::metadata(&marker).is_ok_and(|metadata| metadata.len() == 1));
    for _ in 0..20 { assert_eq!(fixture.desktop.ready(), Ready::Dispatched); fixture.tick(); }
    assert_eq!(std::fs::metadata(marker).unwrap().len(), 1);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn a_failed_ready_helper_reports_failure_without_repeated_spawn_attempts() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let commands = directory.path().join("park.json");
    std::fs::write(&commands, serde_json::to_vec(&serde_json::json!({"ready":{"program":directory.path().join("missing-ready-helper")}})).unwrap()).unwrap();
    let mut harness = Harness::new();
    harness.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Failed ready fixture".into(), origin: [0, 0], size: [1000, 720] }]).unwrap();
    harness.sync();
    let mut fixture = Fixture::new(harness, &runtime, directory.path());
    let socket = directory.path().join("wayland-failed-ready");
    fixture.listen(&socket);
    fixture.desktop.commands(commands, socket);
    fixture.until(|fixture| fixture.desktop.current.as_ref().is_some_and(|current| current.frame.input.opening_ready));
    assert_eq!(fixture.desktop.ready(), Ready::Failed);
    for _ in 0..20 { assert_eq!(fixture.desktop.ready(), Ready::Failed); fixture.tick(); }
}
