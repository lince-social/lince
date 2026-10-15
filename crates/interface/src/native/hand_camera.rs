use bevy::{math::DVec2, prelude::*};
use lince_hand_input::{HandTracker, PanTracker, Point};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub struct HandCameraPlugin;

impl Plugin for HandCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Runtime>().add_systems(Update, tick);
    }
}

#[derive(Component)]
struct Controls {
    root: Entity,
    sand: Entity,
    status: Entity,
}

type Sample = Result<(Instant, Option<Point>), String>;

struct Session {
    owner: Entity,
    root: Entity,
    workspace: u64,
    stop: Arc<AtomicBool>,
    latest: Arc<Mutex<Option<Sample>>>,
    task: tokio::task::JoinHandle<()>,
    last_frame: Option<Instant>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Resource)]
struct Runtime {
    session: Option<Session>,
    motion: PanTracker,
    epoch: Instant,
    status: String,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            session: None,
            motion: PanTracker::default(),
            epoch: Instant::now(),
            status: "Off".into(),
        }
    }
}

pub(crate) fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    if let Some(mut item) = world.get_mut::<crate::native::canvas::CanvasItem>(sand) {
        item.size = Vec2::new(460.0, 220.0);
    }
    let body = crate::native::sand_panel::frame(world, sand, "Hand Camera Castle");
    crate::native::edit_mode::label(
        world,
        body,
        "Move an open hand up, down or sideways to move the 2D camera. Close your hand to pause.",
        14.0,
    );
    let row = crate::native::sand_panel::row(world, body);
    crate::native::sand_panel::button(world, row, body, "Enable camera", Control(true));
    crate::native::sand_panel::button(world, row, body, "Disable camera", Control(false));
    let status = crate::native::edit_mode::label(world, body, "Off", 13.0);
    world
        .entity_mut(body)
        .insert(Controls { root, sand, status });
    world.entity_mut(sand).insert((
        crate::native::castle::Castle,
        crate::native::sand_store::SandCredits(CREDITS),
    ));
    body
}

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "MediaPipe hand models, ONNX conversion by OpenCV Zoo",
        author: "Google and OpenCV Zoo contributors",
        license: include_str!("../../licenses/native/hand-models-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "tract",
        author: "Sonos and tract contributors",
        license: include_str!("../../licenses/native/tract-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "nokhwa",
        author: "l1npengtul and nokhwa contributors",
        license: include_str!("../../licenses/native/nokhwa-Apache-2.0.txt"),
    },
];

#[derive(Clone, Copy)]
struct Control(bool);

impl crate::native::actions::Action for Control {
    fn apply(&self, world: &mut World, owner: Entity) {
        world.init_resource::<Runtime>();
        if !self.0 {
            let mut runtime = world.resource_mut::<Runtime>();
            if let Some(session) = &runtime.session {
                session.stop.store(true, Ordering::Relaxed);
            }
            runtime.motion.reset();
            runtime.status = "Off".into();
            return;
        }
        let Some(controls) = world.get::<Controls>(owner) else {
            return;
        };
        let (root, sand) = (controls.root, controls.sand);
        if crate::native::laboratory::active(world)
            || crate::native::laboratory::suspended(world, sand)
        {
            world.resource_mut::<Runtime>().status = "Camera unavailable in the Laboratory".into();
            return;
        }
        if world
            .get::<crate::native::topology::view::View>(root)
            .is_some_and(|view| view.spatial)
        {
            world.resource_mut::<Runtime>().status = "Switch to the 2D view first".into();
            return;
        }
        if world.resource::<Runtime>().session.is_some() {
            return;
        }
        let Some(workspace) = world
            .get::<crate::native::workspace::WorkspaceMember>(sand)
            .map(|member| member.0)
        else {
            return;
        };
        if !world
            .get::<crate::native::workspace::Workspaces>(root)
            .is_some_and(|spaces| spaces.active == workspace)
        {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            world.resource_mut::<Runtime>().status = "Camera runtime is unavailable".into();
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        let latest = Arc::new(Mutex::new(None));
        let wake = world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned();
        let worker_stop = stop.clone();
        let worker_latest = latest.clone();
        let task = handle.spawn_blocking(move || {
            let publish = |sample| {
                if let Ok(mut latest) = worker_latest.lock() {
                    *latest = Some(sample);
                }
                if let Some(wake) = &wake {
                    wake.ring();
                }
            };
            if let Err(error) = capture(&worker_stop, &publish) {
                publish(Err(error));
            }
        });
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.motion.reset();
        runtime.status = "Starting camera…".into();
        runtime.session = Some(Session {
            owner,
            root,
            workspace,
            stop,
            latest,
            task,
            last_frame: None,
        });
    }
}

fn model_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache-lince-hand")
}

fn capture(stop: &AtomicBool, publish: &impl Fn(Sample)) -> Result<(), String> {
    use nokhwa::{
        Camera,
        pixel_format::RgbFormat,
        utils::{
            CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
            Resolution,
        },
    };
    let tracker = HandTracker::load(&model_directory())?;
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
        CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 30),
    ));
    let mut camera = Camera::new(CameraIndex::Index(0), format).map_err(|e| e.to_string())?;
    if u64::from(camera.resolution().width()) * u64::from(camera.resolution().height()) > 4_000_000
    {
        return Err("Camera resolution is too large".into());
    }
    camera.open_stream().map_err(|e| e.to_string())?;
    while !stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        let image = camera
            .frame()
            .map_err(|e| e.to_string())?
            .decode_image::<RgbFormat>()
            .map_err(|e| e.to_string())?;
        let point = tracker.open_hand(&image)?.map(|p| [1.0 - p[0], p[1]]);
        if !stop.load(Ordering::Relaxed) {
            publish(Ok((started, point)));
        }
        if let Some(delay) = Duration::from_millis(33).checked_sub(started.elapsed()) {
            std::thread::sleep(delay);
        }
    }
    camera.stop_stream().map_err(|e| e.to_string())
}

fn tick(world: &mut World) {
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    if let Some(mut session) = runtime.session.take() {
        let valid = world.get::<Controls>(session.owner).is_some()
            && world
                .get::<crate::native::workspace::Workspaces>(session.root)
                .is_some_and(|spaces| spaces.active == session.workspace)
            && !crate::native::laboratory::suspended(world, session.owner);
        if !valid {
            session.stop.store(true, Ordering::Relaxed);
            runtime.status = "Off".into();
        }
        let spatial = world
            .get::<crate::native::topology::view::View>(session.root)
            .is_some_and(|view| view.spatial);
        let focused = world
            .query::<&Window>()
            .iter(world)
            .any(|window| window.focused);
        if spatial || !focused {
            runtime.motion.reset();
        }
        let sample = session
            .latest
            .lock()
            .ok()
            .and_then(|mut latest| latest.take());
        if session.stop.load(Ordering::Relaxed) {
            runtime.motion.reset();
        } else if let Some(sample) = sample {
            match sample {
                Err(error) => {
                    session.stop.store(true, Ordering::Relaxed);
                    runtime.motion.reset();
                    runtime.status = error;
                }
                Ok((timestamp, point)) => {
                    session.last_frame = Some(timestamp);
                    if spatial || !focused || timestamp.elapsed() > Duration::from_millis(350) {
                        runtime.motion.reset();
                        runtime.status = "Paused".into();
                    } else {
                        let delta = runtime
                            .motion
                            .update(point, timestamp.duration_since(runtime.epoch).as_secs_f64());
                        let viewport = world
                            .get::<ComputedNode>(session.root)
                            .map(|node| node.size() * node.inverse_scale_factor());
                        if let Some(size) = viewport
                            && let Some(mut view) =
                                world.get_mut::<crate::native::canvas::CanvasView>(session.root)
                            && size.is_finite()
                            && size.min_element() > 0.0
                            && view.center.is_finite()
                            && view.zoom.is_finite()
                            && view.zoom > 0.0
                        {
                            let center = view.center
                                + DVec2::new(
                                    f64::from(delta[0] * size.x),
                                    f64::from(delta[1] * size.y),
                                ) / view.zoom;
                            if center.is_finite() {
                                view.center = center;
                            }
                        }
                        runtime.status = if point.is_some() {
                            "Open hand · moving camera"
                        } else {
                            "Show an open hand"
                        }
                        .into();
                    }
                }
            }
        }
        if session
            .last_frame
            .is_some_and(|timestamp| timestamp.elapsed() > Duration::from_millis(350))
        {
            runtime.motion.reset();
        }
        if session.task.is_finished() {
            runtime.motion.reset();
            if !session.stop.load(Ordering::Relaxed) {
                runtime.status = "Camera stopped".into();
            }
        } else {
            runtime.session = Some(session);
        }
    }
    let statuses: Vec<_> = world
        .query::<&Controls>()
        .iter(world)
        .map(|controls| controls.status)
        .collect();
    for status in statuses {
        if let Some(mut text) = world.get_mut::<Text>(status)
            && text.0 != runtime.status
        {
            text.0.clone_from(&runtime.status);
        }
    }
    world.insert_resource(runtime);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::actions::Action;

    #[test]
    fn castle_is_disabled_on_creation_and_uses_the_existing_store_kind() {
        let mut app = crate::native::sand_panel::tests::app();
        app.add_plugins(HandCameraPlugin);
        let root = app
            .world_mut()
            .spawn((
                crate::native::workspace::Workspaces::default(),
                crate::native::canvas::CanvasView::default(),
            ))
            .id();
        let sand = crate::native::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            crate::native::sand_store::SandKind::HandCamera,
            "",
            DVec2::ZERO,
        );
        let body = app
            .world()
            .get::<crate::native::sand_store::StoredSand>(sand)
            .unwrap()
            .content
            .unwrap();
        assert!(app.world().get::<Controls>(body).is_some());
        assert!(
            app.world()
                .get::<crate::native::castle::Castle>(sand)
                .is_some()
        );
        assert_eq!(
            app.world()
                .get::<crate::native::sand_store::SandCredits>(sand)
                .unwrap()
                .0
                .len(),
            3
        );
        assert!(app.world().resource::<Runtime>().session.is_none());
        app.update();
        assert_eq!(app.world().resource::<Runtime>().status, "Off");
        Control(false).apply(app.world_mut(), body);
        assert!(app.world().resource::<Runtime>().session.is_none());
    }

    #[test]
    fn samples_pan_only_the_camera_and_disable_or_workspace_change_stops_them() {
        let executor = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut app = crate::native::sand_panel::tests::app();
        app.add_plugins(HandCameraPlugin);
        app.world_mut().spawn(Window {
            focused: true,
            ..default()
        });
        let root = app
            .world_mut()
            .spawn((
                crate::native::workspace::Workspaces::default(),
                crate::native::canvas::CanvasView {
                    center: DVec2::ZERO,
                    zoom: 2.0,
                },
                ComputedNode {
                    size: Vec2::new(2000.0, 1000.0),
                    inverse_scale_factor: 0.5,
                    ..default()
                },
            ))
            .id();
        let sand = crate::native::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            crate::native::sand_store::SandKind::HandCamera,
            "",
            DVec2::new(50.0, 60.0),
        );
        let owner = app
            .world()
            .get::<crate::native::sand_store::StoredSand>(sand)
            .unwrap()
            .content
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let latest = Arc::new(Mutex::new(None));
        let epoch = Instant::now() - Duration::from_millis(100);
        {
            let mut runtime = app.world_mut().resource_mut::<Runtime>();
            runtime.epoch = epoch;
            runtime.session = Some(Session {
                owner,
                root,
                workspace: 1,
                stop: stop.clone(),
                latest: latest.clone(),
                task: executor.spawn(std::future::pending()),
                last_frame: None,
            });
        }
        *latest.lock().unwrap() = Some(Ok((epoch, Some([0.5, 0.5]))));
        app.update();
        assert_eq!(
            app.world()
                .get::<crate::native::canvas::CanvasView>(root)
                .unwrap()
                .center,
            DVec2::ZERO
        );
        *latest.lock().unwrap() = Some(Ok((epoch + Duration::from_millis(30), Some([0.55, 0.45]))));
        app.update();
        let view = *app
            .world()
            .get::<crate::native::canvas::CanvasView>(root)
            .unwrap();
        assert!(view.center.x > 0.0 && view.center.y < 0.0);
        assert!((view.center.x + view.center.y * 2.0).abs() < 0.001);
        assert_eq!(view.zoom, 2.0);
        assert_eq!(
            app.world()
                .get::<crate::native::canvas::CanvasItem>(sand)
                .unwrap()
                .position,
            DVec2::new(50.0, 60.0)
        );
        Control(false).apply(app.world_mut(), owner);
        assert!(stop.load(Ordering::Relaxed));
        *latest.lock().unwrap() = Some(Ok((epoch + Duration::from_millis(60), Some([0.6, 0.4]))));
        app.update();
        assert_eq!(
            app.world()
                .get::<crate::native::canvas::CanvasView>(root)
                .unwrap()
                .center,
            view.center
        );
        stop.store(false, Ordering::Relaxed);
        app.world_mut()
            .get_mut::<crate::native::workspace::Workspaces>(root)
            .unwrap()
            .active = 2;
        app.update();
        assert!(stop.load(Ordering::Relaxed));
    }
}
