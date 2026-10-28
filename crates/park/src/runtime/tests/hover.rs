use super::super::*;
use bevy::prelude::*;
use lince_interface::host::FileDragContext;
use lince_interface::native;
use std::time::{Duration, Instant};

mod scroll;

fn frame(worker: &CanvasWorker, serial: u64, modal: bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(worker.failure().is_none());
        if let Some(frame) = worker.frame()
            && frame.input_serial >= serial
            && frame.frame.input.keyboard_modal == modal
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "The expected hover frame was not published"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn a_stationary_file_hover_wakes_a_quiet_worker_once_and_activates_only_after_acknowledgement() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [1000, 720],
        directory: directory.path().into(),
        runtime: runtime.handle().clone(),
        cell: None,
    })
    .unwrap();
    frame(&worker, 0, false);
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    let serial = worker
        .input(Input::Test(Box::new(move |canvas| {
            let world = canvas.world_mut();
            let root = world
                .query_filtered::<Entity, With<native::workspace::Workspaces>>()
                .single(world)
                .unwrap();
            canvas.create_workspace(root.to_bits()).unwrap();
            let destination = canvas
                .world_mut()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active;
            for workspace in [1, destination] {
                native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
            }
            canvas.activate_workspace(root.to_bits(), 1).unwrap();
            sent.send((root.to_bits(), destination)).unwrap();
        })))
        .unwrap();
    frame(&worker, serial, false);
    let (owner, destination) = received.recv_timeout(Duration::from_secs(1)).unwrap();
    let serial = worker
        .input(Input::Action(DesktopAction::ToggleOverview))
        .unwrap();
    frame(&worker, serial, true);
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    let serial = worker
        .input(Input::Test(Box::new(move |canvas| {
            let world = canvas.world_mut();
            let entity = world
                .query::<(Entity, &Name)>()
                .iter(world)
                .find(|(_, name)| name.as_str() == format!("overview.workspace.{destination}"))
                .unwrap()
                .0;
            let point = world
                .get::<UiGlobalTransform>(entity)
                .unwrap()
                .transform_point2(Vec2::ZERO)
                * world
                    .get::<ComputedNode>(entity)
                    .unwrap()
                    .inverse_scale_factor();
            sent.send(point.to_array()).unwrap();
        })))
        .unwrap();
    frame(&worker, serial, true);
    let point = received.recv_timeout(Duration::from_secs(1)).unwrap();
    let started = Instant::now();
    worker.input(Input::Pointer(point)).unwrap();
    let serial = worker
        .input(Input::FileDrag(Some(FileDragContext {
            owner,
            output: None,
            token: 1,
        })))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut quiet = false;
    let mut applied = false;
    let request = loop {
        assert!(worker.failure().is_none());
        if let Some(frame) = worker.frame() {
            applied |= frame.input_serial >= serial;
            assert!(frame.frame.input.keyboard_modal);
        }
        quiet |= applied && worker.idle.load(Ordering::Acquire);
        if let Some(request) = worker.request() {
            let HostRequest::OverviewHover(request) = request else {
                panic!("Unexpected hover request");
            };
            break request;
        }
        assert!(
            Instant::now() < deadline,
            "A stationary hover did not wake the worker"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert!(
        quiet,
        "The worker never returned to idle before its hover deadline"
    );
    assert!(started.elapsed() >= Duration::from_millis(390));
    assert_eq!(request.workspace, destination);
    assert!(worker.request().is_none());
    assert!(applied);
    let serial = worker
        .input(Input::OverviewHover {
            request,
            accepted: true,
        })
        .unwrap();
    frame(&worker, serial, false);
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    worker
        .input(Input::Test(Box::new(move |canvas| {
            sent.send(
                canvas
                    .world_mut()
                    .get::<native::workspace::Workspaces>(Entity::from_bits(owner))
                    .unwrap()
                    .active,
            )
            .unwrap();
        })))
        .unwrap();
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        destination
    );
    worker.stop().unwrap();
}
