use super::*;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug)]
struct Observed {
    position: f32,
    limit: f32,
    top: [f32; 2],
    bottom: [f32; 2],
}

fn watch(world: &mut World, observed: Arc<Mutex<Option<Observed>>>) {
    world
        .resource_mut::<Schedules>()
        .get_mut(PostUpdate)
        .unwrap()
        .add_systems(
            (move |world: &mut World| {
                let value = world
                    .query::<(&ScrollPosition, &ComputedNode, &UiGlobalTransform, &ChildOf)>()
                    .iter(world)
                    .find_map(|(position, node, transform, parent)| {
                        if !world
                            .get::<Name>(parent.parent())
                            .is_some_and(|name| name.as_str() == "desktop.overview")
                        {
                            return None;
                        }
                        let scale = node.inverse_scale_factor();
                        let bounds = Rect::from_center_size(
                            transform.transform_point2(Vec2::ZERO) * scale,
                            node.size() * scale,
                        );
                        Some(Observed {
                            position: position.0.y,
                            limit: (node.content_size().y - node.size().y).max(0.0) * scale,
                            top: [bounds.min.x + 1.0, bounds.min.y + 1.0],
                            bottom: [bounds.min.x + 1.0, bounds.max.y - 1.0],
                        })
                    });
                *observed.lock().unwrap() = value;
            })
            .after(bevy::ui::UiSystems::PostLayout),
        );
}

fn quiet(worker: &CanvasWorker, serial: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut idle_since = None;
    let mut publications = worker.publications.load(Ordering::Acquire);
    loop {
        assert!(worker.failure().is_none());
        assert!(worker.is_running());
        if let Some(frame) = worker.frame() {
            assert!(frame.input_serial >= serial);
            assert!(frame.frame.input.keyboard_modal);
        }
        assert!(
            worker.request().is_none(),
            "An edge gap must not target a workspace"
        );
        let current = worker.publications.load(Ordering::Acquire);
        if current != publications || !worker.idle.load(Ordering::Acquire) {
            publications = current;
            idle_since = None;
        } else if idle_since.get_or_insert_with(Instant::now).elapsed()
            >= Duration::from_millis(600)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Edge scrolling did not return to quiet idle"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn stationary_file_edge_scroll_reaches_its_real_limit_and_cancels_without_late_wakes() {
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
    let observed = Arc::new(Mutex::new(None));
    let watching = observed.clone();
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    let serial = worker
        .input(Input::Test(Box::new(move |canvas| {
            let world = canvas.world_mut();
            let root = world
                .query_filtered::<Entity, With<native::workspace::Workspaces>>()
                .single(world)
                .unwrap();
            for _ in 0..5 {
                canvas.create_workspace(root.to_bits()).unwrap();
            }
            for workspace in 1..=6 {
                native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
            }
            canvas.activate_workspace(root.to_bits(), 1).unwrap();
            watch(canvas.world_mut(), watching);
            sent.send(root.to_bits()).unwrap();
        })))
        .unwrap();
    frame(&worker, serial, false);
    let owner = received.recv_timeout(Duration::from_secs(1)).unwrap();
    let serial = worker
        .input(Input::Action(DesktopAction::ToggleOverview))
        .unwrap();
    frame(&worker, serial, true);
    let initial = observed.lock().unwrap().unwrap();
    assert!(
        initial.limit > 100.0,
        "The actual overview list must overflow"
    );
    assert_eq!(initial.position, 0.0);
    worker.input(Input::Pointer(initial.bottom)).unwrap();
    let serial = worker
        .input(Input::FileDrag(Some(FileDragContext {
            owner,
            output: None,
            token: 1,
        })))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut applied = false;
    loop {
        assert!(worker.failure().is_none());
        if let Some(frame) = worker.frame() {
            applied |= frame.input_serial >= serial;
            assert!(frame.frame.input.keyboard_modal);
        }
        let current = observed.lock().unwrap().unwrap();
        assert!(current.position >= 0.0 && current.position <= current.limit + 1.0);
        assert!(
            worker.request().is_none(),
            "The stationary edge gap must not activate a card"
        );
        if applied && current.position >= current.limit - 1.0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "A real stationary hover did not scroll to the end: {current:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    quiet(&worker, serial);
    let clamped = observed.lock().unwrap().unwrap();
    assert!((clamped.position - clamped.limit).abs() <= 1.0);
    let serial = worker.input(Input::FileDrag(None)).unwrap();
    frame(&worker, serial, true);
    worker.input(Input::Pointer(initial.top)).unwrap();
    let serial = worker
        .input(Input::FileDrag(Some(FileDragContext {
            owner,
            output: None,
            token: 2,
        })))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(worker.failure().is_none());
        let current = observed.lock().unwrap().unwrap();
        if current.position < clamped.position - 5.0 {
            break;
        }
        if let Some(frame) = worker.frame() {
            assert!(frame.frame.input.keyboard_modal);
        }
        assert!(
            Instant::now() < deadline,
            "The replacement drag did not scroll in the opposite direction after {serial}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let serial = worker.input(Input::FileDrag(None)).unwrap();
    frame(&worker, serial, true);
    let cancelled = observed.lock().unwrap().unwrap();
    quiet(&worker, serial);
    let stopped = observed.lock().unwrap().unwrap();
    assert!((cancelled.position - stopped.position).abs() <= 1.0);
    assert!(stopped.position > 0.0 && stopped.position < stopped.limit - 1.0);
    worker.stop().unwrap();
}
