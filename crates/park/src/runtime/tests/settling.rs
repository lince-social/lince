use super::super::*;
use bevy::prelude::*;
use lince_interface::native;
use std::time::{Duration, Instant};

fn quiet(worker: &CanvasWorker, serial: u64) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut applied = false;
    let mut since = None;
    let mut count = worker.publications.load(Ordering::Acquire);
    loop {
        assert!(worker.failure().is_none());
        if let Some(frame) = worker.frame() {
            applied |= frame.input_serial >= serial;
        }
        let current = worker.publications.load(Ordering::Acquire);
        if current != count || !worker.idle.load(Ordering::Acquire) || !applied {
            count = current;
            since = None;
        } else if since.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(500) {
            return current;
        }
        assert!(
            Instant::now() < deadline,
            "The worker did not publish the input and return to idle"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn quiet_pointer_input_publishes_once_without_unowned_followup_frames() {
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
    quiet(&worker, 0);
    let serial = worker
        .input(Input::Test(Box::new(|canvas| {
            canvas
                .desktop_action(DesktopAction::SetLayout(
                    lince_interface::host::WorkspaceLayout::Free,
                ))
                .unwrap();
            let world = canvas.world_mut();
            let root = world
                .query_filtered::<Entity, With<native::workspace::Workspaces>>()
                .single(world)
                .unwrap();
            let ids: Vec<_> = world
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries
                .iter()
                .map(|workspace| workspace.id)
                .collect();
            for id in ids {
                native::workspace_config::set_physics(world, root, id, false);
            }
            let active = world
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active;
            for index in 0..16 {
                let item = world
                    .spawn((
                        native::canvas::CanvasItem {
                            position: bevy::math::DVec2::new(
                                (index % 8) as f64 * 90.0 - 315.0,
                                (index / 8) as f64 * 80.0 - 40.0,
                            ),
                            size: Vec2::new(68.0, 48.0),
                        },
                        native::layout::LayoutBox::new(Vec2::new(68.0, 48.0)),
                        Node::default(),
                        BackgroundColor(Color::srgb(0.0, 1.0, 0.0)),
                        native::workspace::WorkspaceMember(active),
                        ChildOf(root),
                    ))
                    .id();
                world.spawn((
                    Text::new(format!("Item {index}")),
                    TextFont::from_font_size(12.0),
                    ChildOf(item),
                ));
            }
        })))
        .unwrap();
    let mut publications = quiet(&worker, serial);
    let mut counts = Vec::new();
    let mut samples = Vec::new();
    for index in 0..6 {
        let started = Instant::now();
        let cpu_started = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        let serial = worker
            .input(Input::Pointer([-10000.0 - index as f32, -10000.0]))
            .unwrap();
        let current = quiet(&worker, serial);
        let cpu_ended = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        counts.push(current - publications);
        samples.push(serde_json::json!({"elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
            "cpu_ms": (cpu_ended.tv_sec - cpu_started.tv_sec) as f64 * 1000.0 + (cpu_ended.tv_nsec - cpu_started.tv_nsec) as f64 / 1_000_000.0,
            "publications": current - publications}));
        publications = current;
    }
    eprintln!(
        "quiet-pointer-work: {}",
        serde_json::json!({"scope":"Private warmed 16-Sand worker; each pointer round includes 500 ms quiet verification; no physical input-visible latency", "samples":samples})
    );
    worker.stop().unwrap();
    assert_eq!(
        counts, [1; 6],
        "Quiet pointer input produced redundant native frames"
    );
}
