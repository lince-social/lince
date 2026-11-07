#![cfg(target_os = "linux")]

use lince_refresh::coordinator;
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Service(Child);

impl Drop for Service {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn start(runtime: &Path) -> Service {
    let mut service = Service(
        Command::new(env!("CARGO_BIN_EXE_lince-refresh"))
            .args(["serve", "--runtime"])
            .arg(runtime)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if coordinator::request(runtime, false).is_ok() {
            return service;
        }
        assert!(
            service.0.try_wait().unwrap().is_none(),
            "Private coordinator exited before readiness"
        );
        assert!(
            Instant::now() < deadline,
            "Private coordinator did not become queryable"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn coordinator_serializes_authority_reports_its_scope_and_restarts_without_park() {
    let parent = tempfile::tempdir().unwrap();
    let runtime = parent.path().join("refresh");
    let mut service = start(&runtime);
    let status = coordinator::request(&runtime, false).unwrap();
    assert_eq!(status.coordinator_pid, service.0.id());
    assert_eq!(status.scope, "refresh_coordinator");
    assert!(status.checkpoint_transport_available);
    assert!(!status.full_session_supported);
    assert!(!status.swap_complete);
    let duplicate = Command::new(env!("CARGO_BIN_EXE_lince-refresh"))
        .args(["serve", "--runtime"])
        .arg(&runtime)
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    assert_eq!(
        coordinator::request(&runtime, false)
            .unwrap()
            .coordinator_pid,
        service.0.id()
    );
    assert_eq!(
        coordinator::request(&runtime, true).unwrap().stage,
        "stopped"
    );
    assert!(service.0.wait().unwrap().success());
    assert!(!runtime.join("control.sock").exists());
    let mut restarted = start(&runtime);
    coordinator::request(&runtime, true).unwrap();
    assert!(restarted.0.wait().unwrap().success());
}
