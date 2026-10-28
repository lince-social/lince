use super::*;
use crate::{
    runtime::{CanvasWorker, Configuration},
    server::Server,
};
use bevy::{math::DVec2, prelude::*};
use lince_interface::{
    host::{OutputId, Rectangle, WorkspaceLayout},
    native,
};
use std::{
    io::Write,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

fn wait(desktop: &mut Desktop, serial: u64) {
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut latest = None;
    loop {
        assert!(desktop.worker.failure().is_none());
        assert!(desktop.worker.is_running());
        if let Some(frame) = desktop.worker.frame()
            && frame.input_serial >= serial
        {
            latest = Some(frame);
        }
        if latest
            .as_ref()
            .is_some_and(|frame| frame.frame.ready().unwrap())
        {
            desktop.adopt(Arc::new(latest.take().unwrap()));
            desktop.focus().unwrap();
            desktop.replay().unwrap();
            return;
        }
        assert!(
            Instant::now() < deadline,
            "The gesture worker did not acknowledge {serial}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn settle(desktop: &mut Desktop) {
    let serial = desktop.worker.input(Input::Test(Box::new(|_| {}))).unwrap();
    wait(desktop, serial);
}

fn view(desktop: &mut Desktop) -> native::canvas::CanvasView {
    inspect(desktop, |world| {
        *world
            .query::<&native::canvas::CanvasView>()
            .single(world)
            .unwrap()
    })
}

fn inspect<T: Send + 'static>(
    desktop: &mut Desktop,
    read: impl FnOnce(&mut World) -> T + Send + 'static,
) -> T {
    let (sender, receiver) = mpsc::sync_channel(1);
    let serial = desktop
        .worker
        .input(Input::Test(Box::new(move |canvas| {
            assert!(sender.send(read(canvas.world_mut())).is_ok());
        })))
        .unwrap();
    wait(desktop, serial);
    receiver.recv_timeout(Duration::from_secs(1)).unwrap()
}

fn pixels(desktop: &Desktop, phase: &str) -> Vec<u8> {
    let frame = desktop.current.as_ref().unwrap();
    let pixels = crate::presentation::probe_pixels(frame.frame.dmabuf().unwrap()).unwrap();
    assert_eq!(pixels.len(), 800 * 600 * 4);
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mut file =
            std::fs::File::create(directory.join(format!("gesture-{phase}.ppm"))).unwrap();
        file.write_all(b"P6\n800 600\n255\n").unwrap();
        let mut rgb = Vec::with_capacity(800 * 600 * 3);
        for pixel in pixels.chunks_exact(4) {
            rgb.extend_from_slice(&pixel[..3]);
        }
        file.write_all(&rgb).unwrap();
    }
    pixels
}

fn click(desktop: &mut Desktop, caption: &'static str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let point = loop {
        let point = inspect(desktop, move |world| {
            world
                .query::<(Entity, &Text)>()
                .iter(world)
                .filter(|(entity, text)| {
                    text.0 == caption
                        && world
                            .get::<InheritedVisibility>(*entity)
                            .is_none_or(|visibility| visibility.get())
                })
                .find_map(|(entity, _)| {
                    let node = world.get::<ComputedNode>(entity)?;
                    let transform = world.get::<UiGlobalTransform>(entity)?;
                    let half = node.size() * 0.5;
                    let corners = [
                        Vec2::new(-half.x, -half.y),
                        Vec2::new(half.x, -half.y),
                        half,
                        Vec2::new(-half.x, half.y),
                    ]
                    .map(|point| transform.transform_point2(point));
                    let min = corners
                        .into_iter()
                        .fold(Vec2::splat(f32::INFINITY), Vec2::min);
                    let max = corners
                        .into_iter()
                        .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
                    let mut bounds = Rect::from_corners(min, max);
                    if let Some(clip) = world.get::<bevy::ui::CalculatedClip>(entity) {
                        bounds = bounds.intersect(clip.clip);
                    }
                    (bounds.size().min_element() > 0.0).then(|| bounds.center().to_array())
                })
        });
        if let Some(point) = point {
            break point;
        }
        if Instant::now() >= deadline {
            pixels(desktop, "missing-control");
            let details = inspect(desktop, move |world| {
                let windows: Vec<_> = world
                    .query::<&Window>()
                    .iter(world)
                    .map(|window| window.focused)
                    .collect();
                let controls: Vec<_> = world
                    .query::<(Entity, &Text)>()
                    .iter(world)
                    .filter(|(_, text)| text.0 == caption)
                    .take(8)
                    .map(|(entity, _)| {
                        (
                            entity,
                            world
                                .get::<InheritedVisibility>(entity)
                                .map(|visibility| visibility.get()),
                            native::topology::presentation::bounds(world, entity),
                        )
                    })
                    .collect();
                format!("windows={windows:?}; controls={controls:?}")
            });
            panic!("The ordinary {caption} control is not visible: {details}");
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    desktop.pointer = point.map(f64::from);
    for input in [
        Input::Pointer(point),
        Input::Button(bevy::input::mouse::MouseButton::Left, true, point, 0),
        Input::Button(bevy::input::mouse::MouseButton::Left, false, point, 0),
    ] {
        let serial = desktop.worker.input(input).unwrap();
        wait(desktop, serial);
    }
}

struct Resume(Option<mpsc::SyncSender<()>>);

impl Drop for Resume {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.try_send(());
        }
    }
}

fn fixture() -> (tokio::runtime::Runtime, tempfile::TempDir, Desktop) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let worker = CanvasWorker::spawn(Configuration {
        showoff: false,
        size: [800, 600],
        directory: directory.path().into(),
        runtime: runtime.handle().clone(),
        cell: None,
    })
    .unwrap();
    let mut desktop = Desktop::new(
        Server::new([800, 600]).unwrap(),
        worker,
        [800, 600],
        1,
        true,
    );
    let serial = desktop
        .worker
        .input(Input::Test(Box::new(|canvas| {
            let bounds = Rectangle::new([0.0; 2], [800.0, 600.0]).unwrap();
            canvas
                .outputs(vec![crate::presentation::Output {
                    name: "Gesture fixture".into(),
                    viewport: OutputViewport { rotation: Default::default(),
                        id: OutputId(1),
                        bounds,
                        usable: Some(bounds),
                        workspace: 1,
                        physical_size: [800, 600],
                        raster: lince_interface::host::raster::RasterMapping {
                            origin: [0.0; 2],
                            scale: [1.0; 2],
                        },
                    },
                }])
                .unwrap();
            canvas
                .desktop_action(lince_interface::host::DesktopAction::SetLayout(
                    WorkspaceLayout::Free,
                ))
                .unwrap();
            let world = canvas.world_mut();
            let root = world
                .query_filtered::<Entity, With<native::workspace::Workspaces>>()
                .single(world)
                .unwrap();
            world
                .get_mut::<native::canvas::CanvasView>(root)
                .unwrap()
                .center = DVec2::new(12.0, 20.0);
            world
                .get_mut::<native::canvas::CanvasView>(root)
                .unwrap()
                .zoom = 2.0;
            world.spawn((
                native::canvas::CanvasItem {
                    position: DVec2::new(12.0, 20.0),
                    size: Vec2::new(180.0, 120.0),
                },
                native::layout::LayoutBox::new(Vec2::new(180.0, 120.0)),
                native::workspace::WorkspaceMember(1),
                ChildOf(root),
            ));
        })))
        .unwrap();
    wait(&mut desktop, serial);
    desktop.pointer = [400.0, 300.0];
    desktop.gesture_support(true).unwrap();
    settle(&mut desktop);
    (runtime, directory, desktop)
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn live_desktop_pan_survives_a_full_worker_queue_and_device_cancel_restores_the_view() {
    let (_runtime, _directory, mut desktop) = fixture();
    let before = view(&mut desktop);
    desktop.gesture_swipe_begin("fixture-touchpad", 3, 1);
    assert!(desktop.gestures.claimed);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    desktop.gesture_swipe_update("fixture-touchpad", [-20.0, 0.0], 2);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    let live = view(&mut desktop);
    assert!((live.center.x - before.center.x - 20.0 / before.zoom).abs() < 1e-6);

    let (entered, entering) = mpsc::sync_channel(1);
    let (resume, resumed) = mpsc::sync_channel(1);
    let mut release = Resume(Some(resume));
    desktop
        .worker
        .input(Input::Test(Box::new(move |_| {
            entered.send(()).unwrap();
            resumed.recv_timeout(Duration::from_secs(40)).unwrap();
        })))
        .unwrap();
    entering.recv_timeout(Duration::from_secs(40)).unwrap();
    let mut last = 0;
    let mut accepted = 0;
    loop {
        match desktop.worker.input(Input::Test(Box::new(|_| {}))) {
            Ok(serial) => {
                last = serial;
                accepted += 1;
            }
            Err(TrySendError::Full(_)) => break,
            Err(TrySendError::Disconnected(_)) => panic!("The paused gesture worker disconnected"),
        }
        assert!(accepted <= 128);
    }
    assert_eq!(accepted, 128);
    for tick in 0..10_000 {
        desktop.gesture_swipe_update("fixture-touchpad", [-0.01, 0.0], tick + 3);
    }
    desktop.gesture_swipe_end("fixture-touchpad", false, 10_004);
    let pending = desktop.gestures.buffer.next_delivery();
    assert!(matches!(pending, Some(GestureDelivery::Finish(_))));
    desktop.flush_gesture().unwrap();
    assert_eq!(desktop.gestures.buffer.next_delivery(), pending);
    release.0.take().unwrap().send(()).unwrap();
    wait(&mut desktop, last);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    assert!(!desktop.gestures.buffer.busy());
    let committed = view(&mut desktop);
    assert!((committed.center.x - before.center.x - 120.0 / before.zoom).abs() < 1e-6);
    desktop.gesture_swipe_end("fixture-touchpad", false, 10_005);
    assert!(!desktop.gestures.buffer.busy());

    desktop.gesture_swipe_begin("fixture-touchpad", 3, 10_006);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    desktop.gesture_swipe_update("fixture-touchpad", [60.0, 0.0], 10_007);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    assert!(
        (view(&mut desktop).center.x - committed.center.x + 60.0 / committed.zoom).abs() < 1e-6
    );
    desktop.cancel_device_gestures("fixture-touchpad", 10_008);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    assert!(view(&mut desktop) == committed);
    assert!(!desktop.gestures.buffer.busy());
    let serial = desktop
        .worker
        .input(Input::Action(
            lince_interface::host::DesktopAction::ToggleTiling,
        ))
        .unwrap();
    wait(&mut desktop, serial);
    assert!(!desktop.current.as_ref().unwrap().frame.input.keyboard_modal);
    desktop.worker.stop().unwrap();
}

fn swipe(desktop: &mut Desktop, fingers: u32, delta: [f64; 2], time: u32) {
    desktop.gesture_swipe_begin("fixture-touchpad", fingers, time);
    assert!(desktop.gestures.claimed);
    desktop.flush_gesture().unwrap();
    settle(desktop);
    desktop.gesture_swipe_update("fixture-touchpad", delta, time + 1);
    desktop.gesture_swipe_end("fixture-touchpad", false, time + 2);
    desktop.flush_gesture().unwrap();
    settle(desktop);
    assert!(!desktop.gestures.buffer.busy());
}

fn workspace(desktop: &Desktop) -> u64 {
    desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .geometry
        .as_ref()
        .unwrap()
        .outputs[0]
        .workspace
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn workspace_and_overview_swipes_use_worker_actions_and_cannot_commit_over_new_input() {
    let (_runtime, _directory, mut desktop) = fixture();
    let owner = desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .input
        .workspaces
        .owner;
    let initial = workspace(&desktop);
    let serial = desktop
        .worker
        .input(Input::CreateWorkspace { owner })
        .unwrap();
    wait(&mut desktop, serial);
    let second = workspace(&desktop);
    assert_ne!(second, initial);
    assert_eq!(
        desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .workspaces
            .entries
            .len(),
        2
    );
    swipe(&mut desktop, 3, [0.0, -100.0], 10);
    assert_eq!(workspace(&desktop), initial);
    desktop.gesture_swipe_end("fixture-touchpad", false, 13);
    settle(&mut desktop);
    assert_eq!(workspace(&desktop), initial);
    swipe(&mut desktop, 3, [0.0, 100.0], 20);
    assert_eq!(workspace(&desktop), second);
    swipe(&mut desktop, 4, [0.0, 100.0], 30);
    assert!(desktop.current.as_ref().unwrap().frame.input.overview);
    assert!(desktop.current.as_ref().unwrap().frame.input.keyboard_modal);
    swipe(&mut desktop, 4, [0.0, -100.0], 40);
    assert!(!desktop.current.as_ref().unwrap().frame.input.overview);
    assert!(!desktop.current.as_ref().unwrap().frame.input.keyboard_modal);
    desktop.gesture_swipe_begin("fixture-touchpad", 3, 50);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    desktop.gesture_swipe_update("fixture-touchpad", [0.0, 100.0], 51);
    let serial = desktop
        .worker
        .input(Input::Workspace { owner, id: initial })
        .unwrap();
    desktop.gesture_swipe_end("fixture-touchpad", false, 52);
    desktop.flush_gesture().unwrap();
    wait(&mut desktop, serial);
    settle(&mut desktop);
    assert_eq!(workspace(&desktop), initial);
    assert!(!desktop.gestures.buffer.busy());

    let before = view(&mut desktop);
    desktop.gesture_swipe_begin("fixture-touchpad", 3, 60);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    desktop.gesture_swipe_update("fixture-touchpad", [40.0, 0.0], 61);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    assert!(view(&mut desktop).center != before.center);
    desktop.gesture_support(false).unwrap();
    settle(&mut desktop);
    desktop.gesture_swipe_end("fixture-touchpad", true, 62);
    assert!(view(&mut desktop) == before);
    desktop.gesture_swipe_begin("fixture-touchpad", 3, 70);
    assert!(!desktop.gestures.claimed);
    desktop.gesture_swipe_end("fixture-touchpad", false, 71);
    assert!(!desktop.gestures.buffer.busy());
    desktop.gesture_support(true).unwrap();
    settle(&mut desktop);
    assert!(
        desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .gestures
            .enabled
    );
    desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn spatial_pan_changes_real_pixels_after_the_normal_view_control_and_cancel_restores_the_camera() {
    let (_runtime, _directory, mut desktop) = fixture();
    let serial = desktop
        .worker
        .input(Input::Pointer([799.0, 599.0]))
        .unwrap();
    wait(&mut desktop, serial);
    pixels(&desktop, "spatial-controls-hover");
    click(&mut desktop, "2D / 3D");
    let state = |world: &mut World| {
        *world
            .query::<&native::topology::view::View>()
            .single(world)
            .unwrap()
    };
    let before = inspect(&mut desktop, state);
    assert!(before.spatial);
    let initial_pixels = pixels(&desktop, "spatial-before");
    desktop.gesture_swipe_begin("fixture-touchpad", 3, 1);
    assert!(desktop.gestures.claimed);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    desktop.gesture_swipe_update("fixture-touchpad", [37.0, 0.0], 2);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    let moved = inspect(&mut desktop, state);
    assert!(moved.valid());
    assert!(moved.position[0] < before.position[0]);
    assert_eq!((moved.yaw, moved.pitch), (before.yaw, before.pitch));
    let moved_pixels = pixels(&desktop, "spatial-pan");
    assert!(
        (80..350).any(|y| {
            let start = (y * 800 + 120) * 4;
            let end = (y * 800 + 680) * 4;
            moved_pixels[start..end] != initial_pixels[start..end]
        }),
        "The spatial pan did not change the rendered canvas away from the controls."
    );
    desktop.gesture_swipe_end("fixture-touchpad", true, 3);
    desktop.flush_gesture().unwrap();
    settle(&mut desktop);
    assert_eq!(inspect(&mut desktop, state), before);
    pixels(&desktop, "spatial-cancel");
    desktop.worker.stop().unwrap();
}
