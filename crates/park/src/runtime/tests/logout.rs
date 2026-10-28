use super::super::*;
use std::time::{Duration, Instant};

fn frame(worker: &CanvasWorker, serial: u64, modal: bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(worker.failure().is_none());
        assert!(worker.is_running());
        if let Some(completed) = worker.frame()
            && completed.input_serial >= serial
            && completed.frame.input.keyboard_modal == modal
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "The worker did not publish the expected modal/input state"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn key(worker: &CanvasWorker, physical: KeyCode, logical: Key, pressed: bool) -> u64 {
    worker
        .input(Input::Key {
            physical,
            logical,
            text: None,
            pressed,
            repeat: false,
        })
        .unwrap()
}

fn request(worker: &CanvasWorker) -> HostRequest {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(worker.failure().is_none());
        if let Some(request) = worker.request() {
            return request;
        }
        assert!(
            Instant::now() < deadline,
            "The worker did not publish the expected host request"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn confirmed_logout_save_failure_is_recoverable_and_trusted_shutdown_saves_through_an_open_dialog()
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [640, 360],
        directory: directory.path().into(),
        runtime: runtime.handle().clone(),
        cell: None,
    })
    .unwrap();
    frame(&worker, 0, false);
    let serial = worker.input(Input::Action(DesktopAction::Logout)).unwrap();
    frame(&worker, serial, true);
    assert!(worker.request().is_none());
    key(&worker, KeyCode::Escape, Key::Escape, true);
    let serial = key(&worker, KeyCode::Escape, Key::Escape, false);
    frame(&worker, serial, false);
    assert!(worker.request().is_none());

    let snapshots = directory.path().join("interface.snapshots");
    let retained = directory.path().join("retained.snapshots");
    if snapshots.exists() {
        std::fs::rename(&snapshots, &retained).unwrap();
    }
    std::fs::write(&snapshots, b"blocked snapshot directory").unwrap();
    worker
        .input(Input::Action(DesktopAction::ToggleTiling))
        .unwrap();
    let serial = worker.input(Input::Action(DesktopAction::Logout)).unwrap();
    frame(&worker, serial, true);
    key(&worker, KeyCode::Tab, Key::Tab, true);
    key(&worker, KeyCode::Tab, Key::Tab, false);
    key(&worker, KeyCode::Enter, Key::Enter, true);
    key(&worker, KeyCode::Enter, Key::Enter, false);
    assert_eq!(request(&worker), HostRequest::Action(DesktopAction::Logout));
    let serial = worker.input(Input::Logout).unwrap();
    frame(&worker, serial, false);
    assert!(worker.request().is_none());
    assert!(worker.is_running());
    std::fs::remove_file(&snapshots).unwrap();
    if retained.exists() {
        std::fs::rename(&retained, &snapshots).unwrap();
    }
    let serial = worker.input(Input::Action(DesktopAction::Logout)).unwrap();
    frame(&worker, serial, true);
    worker.input(Input::Logout).unwrap();
    assert_eq!(request(&worker), HostRequest::Exit);
    assert!(snapshots.is_dir());
    worker.stop().unwrap();
}
