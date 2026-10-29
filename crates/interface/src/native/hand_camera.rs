use bevy::{math::DVec2, prelude::*};
use lince_hand_input::{HandObservation, HandTracker, HeadRelativePan, Point, PointerGesture, PointerTracker};
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
        app.init_resource::<Runtime>()
            .add_systems(PreUpdate, cancel_for_manual_input.after(bevy::input::InputSystems)
                .before(crate::native::layout::desktop::manipulation::pointer::input)
                .before(crate::native::canvas_selection::SelectInput))
            .add_systems(Update, tick);
    }
}

#[derive(Component)]
struct Controls {
    root: Entity,
    sand: Entity,
    status: Entity,
    settings: Entity,
    preview: Entity,
}

type Sample = Result<(Instant, Option<Point>, Vec<HandObservation>), String>;

fn discontinuous(previous: Option<Instant>, current: Instant) -> bool {
    previous.is_some_and(|previous| {
        current.checked_duration_since(previous).is_none_or(|gap| gap > Duration::from_millis(350))
    })
}

struct Session {
    owner: Entity,
    root: Entity,
    workspace: u64,
    target: Option<u64>,
    stop: Arc<AtomicBool>,
    analysis: Arc<AtomicBool>,
    analysis_error: Arc<Mutex<Option<(Instant, String)>>>,
    latest: Arc<Mutex<Option<Sample>>>,
    frame: Arc<Mutex<Option<image::RgbImage>>>,
    image: Option<Handle<Image>>,
    task: tokio::task::JoinHandle<()>,
    last_frame: Option<Instant>,
}

struct PointerOverlay {
    camera: Entity,
    root: Entity,
    marker: Entity,
    output: crate::host::OutputId,
    image: Handle<Image>,
}

type CompanionSnapshot = (Entity, DVec2, Vec2, Option<([f64; 2], f64)>, Option<([f64; 2], [f64; 2])>, Option<crate::native::canvas_selection::SandGroup>, Option<crate::native::layout::LayoutBox>, Option<crate::native::topology::groups::GroupPose>, Option<crate::native::topology::Attachment>);

fn companion_snapshots(world: &World, companions: &[Entity], target: Entity) -> Option<Vec<CompanionSnapshot>> {
    companions.iter().map(|entity| {
        if *entity != target && (world.get::<crate::native::canvas::ItemPreview>(*entity).is_some()
            || world.get::<crate::native::sand_placement::PinnedPreview>(*entity).is_some()) {
            return None;
        }
        let item = world.get::<crate::native::canvas::CanvasItem>(*entity)?;
        Some((*entity, item.position, item.size,
            world.get::<crate::native::sand_placement::Pinned>(*entity).map(|pin| (pin.anchor, pin.scale)),
            world.get::<crate::native::area::InfluenceArea>(*entity).map(|area| (area.center, area.size)),
            world.get::<crate::native::canvas_selection::SandGroup>(*entity).copied(),
            world.get::<crate::native::layout::LayoutBox>(*entity).copied(),
            world.get::<crate::native::topology::groups::GroupPose>(*entity).copied(),
            world.get::<crate::native::topology::Attachment>(*entity).copied()))
    }).collect()
}

#[derive(Resource)]
struct UnmanagedDrag {
    root: Entity,
    entity: Entity,
    original: crate::native::canvas::CanvasItem,
    preview: crate::native::canvas::CanvasItem,
    start: Vec2,
    zoom: f64,
    view_zoom: f64,
    center: DVec2,
    viewport: Vec2,
    area_center: Option<[f64; 2]>,
    pin: Option<crate::native::sand_placement::Pinned>,
    preview_pin: Option<crate::native::sand_placement::Pinned>,
    companions: Vec<Entity>,
    companion_snapshots: Vec<CompanionSnapshot>,
    selection: Vec<Entity>,
}

fn cancel_unmanaged_drag(world: &mut World) {
    if let Some(drag) = world.remove_resource::<UnmanagedDrag>() {
        if world.get::<crate::native::canvas::ItemPreview>(drag.entity)
            .is_some_and(|preview| preview.0.position == drag.preview.position && preview.0.size == drag.preview.size)
        {
            world.entity_mut(drag.entity).remove::<crate::native::canvas::ItemPreview>();
        }
        if world.get::<crate::native::sand_placement::PinnedPreview>(drag.entity).is_some_and(|preview| {
            drag.preview_pin.is_some_and(|pin| preview.0.anchor == pin.anchor && preview.0.scale == pin.scale)
        }) {
            world.entity_mut(drag.entity).remove::<crate::native::sand_placement::PinnedPreview>();
        }
    }
}

fn unmanaged_hit(world: &World, root: Entity, candidate: Entity, hit: Entity) -> Option<bool> {
    let mut current = Some(hit);
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(entity) = current else { break; };
        if entity == candidate { return Some(false); }
        if entity == root {
            return if hit == root { Some(false) } else {
                world.get::<Pickable>(hit).is_none_or(|pickable| pickable.should_block_lower).then_some(true)
            };
        }
        if world.get::<crate::native::inspection::InspectionExcluded>(entity).is_some()
            || world.get::<crate::native::canvas::CanvasItem>(entity).is_some()
        { return Some(true); }
        current = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    world.get::<Pickable>(hit).is_none_or(|pickable| pickable.should_block_lower).then_some(true)
}

fn unmanaged_blocked(world: &World, root: Entity, window: Entity, candidate: Entity, position: Vec2) -> bool {
    crate::native::pointer_hit::nodes(world, window, position).into_iter()
        .find_map(|hit| unmanaged_hit(world, root, candidate, hit)).unwrap_or(false)
}

fn begin_unmanaged_drag(world: &mut World, root: Entity, window: Entity, position: Vec2) -> bool {
    if !world.get::<crate::native::edit_mode::EditMode>(root).is_some_and(|mode| mode.enabled) { return false; }
    let Some(view) = world.get::<crate::native::canvas::CanvasView>(root).copied() else { return false; };
    if !view.zoom.is_finite() || view.zoom <= 0.0 { return false; }
    let candidate = world.get::<Children>(root).and_then(|children| children.iter().rev().find(|entity| {
        crate::native::canvas_item::eligible(world, root, *entity)
            && world.get::<crate::native::layout::desktop::Managed>(*entity).is_none()
            && world.get::<crate::native::canvas::ItemPreview>(*entity).is_none()
            && world.get::<crate::native::sand_placement::PinnedPreview>(*entity).is_none()
            && crate::native::canvas_selection::screen_bounds(world, root, *entity).is_some_and(|bounds| bounds.contains(position))
    }));
    let Some(entity) = candidate else { return false; };
    if unmanaged_blocked(world, root, window, entity, position) { return false; }
    let original = *world.get::<crate::native::canvas::CanvasItem>(entity).unwrap();
    let Some(viewport) = crate::native::inspection::bounds(world, root).map(|bounds| bounds.size()) else { return false; };
    let pin = world.get::<crate::native::sand_placement::Pinned>(entity).copied();
    let zoom = pin.map_or(view.zoom, |pin| pin.scale);
    if !viewport.is_finite() || viewport.min_element() <= 0.0 || !zoom.is_finite() || zoom <= 0.0 { return false; }
    let area_center = world.get::<crate::native::area::InfluenceArea>(entity).map(|area| area.center);
    crate::native::canvas_item::select(world, root, entity);
    let companions = crate::native::canvas_selection::companions(world, root, entity);
    let Some(companion_snapshots) = companion_snapshots(world, &companions, entity) else { return false; };
    let selection = crate::native::canvas_selection::selected(world, root);
    world.entity_mut(entity).insert(crate::native::canvas::ItemPreview(original));
    if let Some(pin) = pin { world.entity_mut(entity).insert(crate::native::sand_placement::PinnedPreview(pin)); }
    world.insert_resource(UnmanagedDrag { root, entity, original, preview: original, start: position, zoom, view_zoom: view.zoom, center: view.center, viewport, area_center, pin, preview_pin: pin, companions, companion_snapshots, selection });
    true
}

fn update_unmanaged_drag(world: &mut World, position: Vec2, commit: bool) -> bool {
    let Some(mut drag) = world.remove_resource::<UnmanagedDrag>() else { return false; };
    let valid = position.is_finite()
        && !crate::native::layout::desktop::manipulation::active(world)
        && crate::native::canvas_item::eligible(world, drag.root, drag.entity)
        && crate::native::canvas_selection::companions(world, drag.root, drag.entity) == drag.companions
        && companion_snapshots(world, &drag.companions, drag.entity).as_ref() == Some(&drag.companion_snapshots)
        && crate::native::canvas_selection::selected(world, drag.root) == drag.selection
        && world.get::<crate::native::edit_mode::EditMode>(drag.root).is_some_and(|mode| mode.enabled)
        && world.get::<crate::native::canvas::CanvasView>(drag.root).is_some_and(|view| view.center == drag.center && view.zoom == drag.view_zoom)
        && crate::native::inspection::bounds(world, drag.root).is_some_and(|bounds| bounds.size() == drag.viewport)
        && world.get::<crate::native::sand_placement::Pinned>(drag.entity).map(|pin| (pin.anchor, pin.scale)) == drag.pin.map(|pin| (pin.anchor, pin.scale))
        && world.get::<crate::native::sand_placement::PinnedPreview>(drag.entity).map(|pin| (pin.0.anchor, pin.0.scale)) == drag.preview_pin.map(|pin| (pin.anchor, pin.scale))
        && world.get::<crate::native::area::InfluenceArea>(drag.entity).map(|area| area.center) == drag.area_center
        && world.get::<crate::native::canvas::CanvasItem>(drag.entity).is_some_and(|item| item.position == drag.original.position && item.size == drag.original.size)
        && world.get::<crate::native::canvas::ItemPreview>(drag.entity).is_some_and(|preview| preview.0.position == drag.preview.position && preview.0.size == drag.preview.size);
    if !valid {
        world.insert_resource(drag);
        cancel_unmanaged_drag(world);
        return false;
    }
    drag.preview.position = drag.original.position + (position - drag.start).as_dvec2() / drag.zoom;
    drag.preview_pin = drag.pin.map(|mut pin| {
        pin.moved((position - drag.start).as_dvec2(), drag.viewport);
        pin
    });
    if !drag.preview.position.is_finite() {
        world.insert_resource(drag);
        cancel_unmanaged_drag(world);
        return false;
    }
    if commit {
        world.entity_mut(drag.entity).remove::<crate::native::canvas::ItemPreview>();
        world.entity_mut(drag.entity).remove::<crate::native::sand_placement::PinnedPreview>();
        *world.get_mut::<crate::native::canvas::CanvasItem>(drag.entity).unwrap() = drag.preview;
        if let Some(pin) = drag.preview_pin {
            *world.get_mut::<crate::native::sand_placement::Pinned>(drag.entity).unwrap() = pin;
        }
        if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(drag.entity) {
            area.center = drag.preview.position.to_array();
        }
        crate::native::layout::edited(world, drag.entity, drag.original, drag.preview);
        crate::native::canvas_selection::transform_members(world, drag.entity, drag.original, drag.preview);
    } else {
        world.entity_mut(drag.entity).insert(crate::native::canvas::ItemPreview(drag.preview));
        if let Some(pin) = drag.preview_pin { world.entity_mut(drag.entity).insert(crate::native::sand_placement::PinnedPreview(pin)); }
        world.insert_resource(drag);
    }
    true
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Resource)]
struct Runtime {
    session: Option<Session>,
    motion: HeadRelativePan,
    pointer: PointerTracker,
    pointer_mode: bool,
    pointer_position: Option<Point>,
    overlay: Option<PointerOverlay>,
    dragging: bool,
    pointer_message: Option<&'static str>,
    epoch: Instant,
    status: String,
    camera_index: u32,
    analysis: bool,
    analysis_failure: Option<String>,
    analysis_since: Instant,
    output: Option<crate::host::OutputId>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            session: None,
            motion: HeadRelativePan::default(),
            pointer: PointerTracker::default(),
            pointer_mode: false,
            pointer_position: None,
            overlay: None,
            dragging: false,
            pointer_message: None,
            epoch: Instant::now(),
            status: "Off".into(),
            camera_index: 0,
            analysis: false,
            analysis_failure: None,
            analysis_since: Instant::now(),
            output: None,
        }
    }
}

pub(crate) fn populate(world: &mut World, root: Entity, sand: Entity) -> Entity {
    if let Some(mut item) = world.get_mut::<crate::native::canvas::CanvasItem>(sand) {
        item.size = Vec2::new(640.0, 470.0);
    }
    let body = crate::native::sand_panel::frame(world, sand, "Hand Camera Castle");
    crate::native::edit_mode::label(
        world,
        body,
        "Pan: hold an open palm above, below or beside your head. Pointer: aim with your index finger, pinch to select and move, and open your hand to release. Press Escape to cancel.",
        14.0,
    );
    let row = crate::native::sand_panel::row(world, body);
    crate::native::sand_panel::button(world, row, body, "Previous camera", CameraIndex(false));
    crate::native::sand_panel::button(world, row, body, "Next camera", CameraIndex(true));
    crate::native::sand_panel::button(world, row, body, "Enable camera", Control(true));
    crate::native::sand_panel::button(world, row, body, "Disable camera", Control(false));
    let row = crate::native::sand_panel::row(world, body);
    crate::native::sand_panel::button(world, row, body, "Enable hand analysis", Analysis(true));
    crate::native::sand_panel::button(world, row, body, "Disable hand analysis", Analysis(false));
    crate::native::sand_panel::button(world, row, body, "Next control display", NextOutput);
    let row = crate::native::sand_panel::row(world, body);
    crate::native::sand_panel::button(world, row, body, "Pan mode", PointerMode(false));
    crate::native::sand_panel::button(world, row, body, "Pointer mode", PointerMode(true));
    let settings = crate::native::edit_mode::label(world, body, "Camera 0 · display automatic", 13.0);
    let status = crate::native::edit_mode::label(world, body, "Off", 13.0);
    let preview = world
        .spawn((
            ChildOf(body),
            ImageNode::default(),
            Node {
                width: px(320),
                height: px(240),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    world.entity_mut(body).insert(Controls {
        root,
        sand,
        status,
        settings,
        preview,
    });
    world.entity_mut(sand).insert((
        crate::native::castle::Castle,
        crate::native::sand_store::SandCredits(CREDITS),
    ));
    body
}

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "YuNet face detection model",
        author: "Shiqi Yu and OpenCV Zoo contributors",
        license: include_str!("../../licenses/native/yunet-MIT.txt"),
    },
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

#[derive(Clone, Copy)]
struct CameraIndex(bool);

impl crate::native::actions::Action for CameraIndex {
    fn apply(&self, world: &mut World, _: Entity) {
        world.init_resource::<Runtime>();
        let mut runtime = world.resource_mut::<Runtime>();
        if runtime.session.is_some() {
            runtime.status = "Stop the camera before changing its index".into();
            return;
        }
        runtime.camera_index = if self.0 {
            runtime.camera_index.saturating_add(1).min(255)
        } else {
            runtime.camera_index.saturating_sub(1)
        };
        runtime.status = format!("Camera index {}", runtime.camera_index);
    }
}

#[derive(Clone, Copy)]
struct Analysis(bool);

#[derive(Clone, Copy)]
struct PointerMode(bool);

impl crate::native::actions::Action for PointerMode {
    fn apply(&self, world: &mut World, _: Entity) {
        world.init_resource::<Runtime>();
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.pointer_mode = self.0;
        runtime.pointer_message = None;
        runtime.pointer.disarm();
        runtime.pointer_position = None;
        runtime.motion.reset();
        runtime.analysis_since = Instant::now();
        runtime.status = if self.0 { "Pointer mode" } else { "Pan mode" }.into();
        let dragging = std::mem::take(&mut runtime.dragging);
        drop(runtime);
        let unmanaged = world.contains_resource::<UnmanagedDrag>();
        cancel_unmanaged_drag(world);
        world.remove_resource::<crate::native::output_views::HandFocusRequest>();
        if dragging && !unmanaged { crate::native::layout::desktop::manipulation::pointer::cancel(world); }
    }
}

#[derive(Clone, Copy)]
struct NextOutput;

impl crate::native::actions::Action for NextOutput {
    fn apply(&self, world: &mut World, owner: Entity) {
        world.init_resource::<Runtime>();
        let Some(root) = world.get::<Controls>(owner).map(|controls| controls.root) else { return; };
        let outputs: Vec<_> = world.get_resource::<crate::native::output_views::OutputViews>()
            .filter(|views| views.applies_to(root))
            .map(|views| views.outputs.iter().map(|output| output.id).collect()).unwrap_or_default();
        if outputs.is_empty() {
            world.resource_mut::<Runtime>().status = "No Park displays available".into();
            return;
        }
        let mut runtime = world.resource_mut::<Runtime>();
        let current = runtime.output.and_then(|id| outputs.iter().position(|output| *output == id)).unwrap_or(0);
        let next = (current + 1) % outputs.len();
        runtime.output = Some(outputs[next]);
        runtime.motion.reset();
        runtime.pointer.disarm();
        runtime.pointer_position = None;
        runtime.status = format!("Hand control display {} of {}", next + 1, outputs.len());
        let dragging = std::mem::take(&mut runtime.dragging);
        drop(runtime);
        let unmanaged = world.contains_resource::<UnmanagedDrag>();
        cancel_unmanaged_drag(world);
        world.remove_resource::<crate::native::output_views::HandFocusRequest>();
        if dragging && !unmanaged { crate::native::layout::desktop::manipulation::pointer::cancel(world); }
    }
}

impl crate::native::actions::Action for Analysis {
    fn apply(&self, world: &mut World, _: Entity) {
        world.init_resource::<Runtime>();
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.analysis = self.0;
        runtime.analysis_failure = None;
        if let Some(session) = &runtime.session
            && let Ok(mut error) = session.analysis_error.lock()
        {
            *error = None;
        }
        runtime.analysis_since = Instant::now();
        if let Some(session) = &runtime.session {
            session.analysis.store(self.0, Ordering::Relaxed);
        }
        runtime.motion.reset();
        runtime.status = if self.0 {
            "Hand analysis enabled"
        } else {
            "Hand analysis disabled"
        }
        .into();
        let dragging = std::mem::take(&mut runtime.dragging);
        runtime.pointer.disarm();
        runtime.pointer_position = None;
        drop(runtime);
        let unmanaged = world.contains_resource::<UnmanagedDrag>();
        cancel_unmanaged_drag(world);
        world.remove_resource::<crate::native::output_views::HandFocusRequest>();
        if dragging && !unmanaged { crate::native::layout::desktop::manipulation::pointer::cancel(world); }
    }
}

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
            let dragging = std::mem::take(&mut runtime.dragging);
            let _ = runtime.pointer.cancel();
            runtime.pointer_position = None;
            drop(runtime);
            let unmanaged = world.contains_resource::<UnmanagedDrag>();
            cancel_unmanaged_drag(world);
            world.remove_resource::<crate::native::output_views::HandFocusRequest>();
            if dragging && !unmanaged { crate::native::layout::desktop::manipulation::pointer::cancel(world); }
            return;
        }
        let Some(controls) = world.get::<Controls>(owner) else {
            return;
        };
        let (root, sand) = (controls.root, controls.sand);
        if crate::native::laboratory::blocked(world, sand)
            || crate::native::laboratory::suspended(world, sand)
        {
            world.resource_mut::<Runtime>().status = "Camera unavailable in the Laboratory".into();
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
        let analysis = Arc::new(AtomicBool::new(world.resource::<Runtime>().analysis));
        let analysis_error = Arc::new(Mutex::new(None));
        let index = world.resource::<Runtime>().camera_index;
        let latest = Arc::new(Mutex::new(None));
        let frame = Arc::new(Mutex::new(None));
        let wake = world
            .get_resource::<crate::native::wake::WakeSignal>()
            .cloned();
        let worker_stop = stop.clone();
        let worker_analysis = analysis.clone();
        let worker_analysis_error = analysis_error.clone();
        let worker_latest = latest.clone();
        let worker_frame = frame.clone();
        let task = handle.spawn_blocking(move || {
            let publish = |sample| {
                if let Ok(mut latest) = worker_latest.lock() {
                    *latest = Some(sample);
                }
                if let Some(wake) = &wake {
                    wake.ring();
                }
            };
            if let Err(error) = capture(
                index,
                &worker_stop,
                &worker_analysis,
                &worker_analysis_error,
                &worker_frame,
                &publish,
            ) {
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
            target: None,
            stop,
            analysis,
            analysis_error,
            latest,
            frame,
            image: None,
            task,
            last_frame: None,
        });
    }
}

fn model_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cache-lince-hand")
}

fn capture(
    index: u32,
    stop: &AtomicBool,
    analysis: &AtomicBool,
    analysis_error: &Mutex<Option<(Instant, String)>>,
    preview: &Mutex<Option<image::RgbImage>>,
    publish: &impl Fn(Sample),
) -> Result<(), String> {
    use nokhwa::{
        Camera,
        pixel_format::RgbFormat,
        utils::{
            CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
            Resolution,
        },
    };
    let mut tracker = None;
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
        CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 30),
    ));
    let mut camera = Camera::new(CameraIndex::Index(index), format).map_err(|e| e.to_string())?;
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
        let (head, hands) = if analysis.load(Ordering::Relaxed) {
            let result = (|| {
                if tracker.is_none() {
                    tracker = Some(HandTracker::load(&model_directory())?);
                }
                let tracker = tracker.as_ref().unwrap();
                Ok::<_, String>((tracker.head(&image)?, tracker.observe_hands(&image)?))
            })();
            match result {
                Ok(observation) => observation,
                Err(error) => {
                    tracker = None;
                    if let Ok(mut status) = analysis_error.lock() { *status = Some((started, error)); }
                    (None, Vec::new())
                }
            }
        } else {
            (None, Vec::new())
        };
        if let Ok(mut frame) = preview.lock() {
            *frame = Some(image);
        }
        if !stop.load(Ordering::Relaxed) {
            publish(Ok((started, head, hands)));
        }
        if let Some(delay) = Duration::from_millis(33).checked_sub(started.elapsed()) {
            std::thread::sleep(delay);
        }
    }
    camera.stop_stream().map_err(|e| e.to_string())
}

fn update_pointer_overlay(world: &mut World, runtime: &mut Runtime) {
    use bevy::camera::{RenderTarget, ImageRenderTarget, visibility::RenderLayers};
    let desired = runtime.pointer_position.zip(runtime.output).and_then(|(point, output)| {
        let view = world.get_resource::<crate::native::output_views::OutputViews>()?
            .outputs.iter().find(|view| view.id == output)?;
        let image = world.get_resource::<crate::native::topology::output_views::Targets>()?.0.get(&output)?.clone();
        Some((point, output, image, view.raster.scale[0] as f32))
    });
    if runtime.overlay.as_ref().is_some_and(|overlay| {
        desired.as_ref().is_none_or(|(_, output, image, _)| *output != overlay.output || *image != overlay.image)
    }) {
        if let Some(overlay) = runtime.overlay.take() {
            world.despawn(overlay.root);
            world.despawn(overlay.camera);
        }
    }
    let Some((point, output, image, scale)) = desired else { return; };
    if runtime.overlay.is_none() {
        let camera = world.spawn((
            Camera2d,
            Camera {
                order: 50,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                output_mode: bevy::camera::CameraOutputMode::Write {
                    blend_state: Some(bevy::render::render_resource::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    clear_color: ClearColorConfig::None,
                },
                ..default()
            },
            RenderLayers::none(),
            RenderTarget::Image(ImageRenderTarget { handle: image.clone(), scale_factor: scale }),
        )).id();
        crate::native::app::configure_output_color(world, camera, &image);
        let root = world.spawn((
            UiTargetCamera(camera),
            Node { width: percent(100), height: percent(100), ..default() },
            Pickable::IGNORE,
        )).id();
        let marker = world.spawn((
            ChildOf(root),
            Node { position_type: PositionType::Absolute, width: px(20), height: px(20), ..default() },
            BackgroundColor(Color::srgb(0.1, 1.0, 0.3)),
            Pickable::IGNORE,
        )).id();
        runtime.overlay = Some(PointerOverlay { camera, root, marker, output, image: image.clone() });
    }
    if let Some(overlay) = &runtime.overlay {
        if let Some(mut camera_target) = world.get_mut::<RenderTarget>(overlay.camera) {
            *camera_target = RenderTarget::Image(ImageRenderTarget { handle: image, scale_factor: scale });
        }
        if let Some(mut marker) = world.get_mut::<Node>(overlay.marker) {
            marker.left = percent(point[0] * 100.0);
            marker.top = percent(point[1] * 100.0);
        }
    }
}

fn cancel_pointer_drag(world: &mut World, runtime: &mut Runtime) {
    let unmanaged = world.contains_resource::<UnmanagedDrag>();
    cancel_unmanaged_drag(world);
    world.remove_resource::<crate::native::output_views::HandFocusRequest>();
    if std::mem::take(&mut runtime.dragging) && !unmanaged {
        crate::native::layout::desktop::manipulation::pointer::cancel(world);
    }
    let _ = runtime.pointer.cancel();
    runtime.pointer_position = None;
}

fn cancel_for_manual_input(world: &mut World) {
    let mouse = world.get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|buttons| buttons.any_just_pressed([MouseButton::Left, MouseButton::Right, MouseButton::Middle]));
    let escape = world.get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Escape));
    if !(mouse || escape) { return; }
    let pending_focus = world.remove_resource::<crate::native::output_views::HandFocusRequest>().is_some();
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    if runtime.dragging || pending_focus {
        cancel_pointer_drag(world, &mut runtime);
        runtime.pointer.disarm();
        runtime.pointer_message = Some("Hand movement canceled by keyboard or mouse");
    }
    world.insert_resource(runtime);
}

fn apply_pointer_gesture(world: &mut World, runtime: &mut Runtime, root: Entity, target: u64, gesture: PointerGesture) {
    use crate::native::layout::desktop::manipulation::pointer;
    if gesture == PointerGesture::Cancel {
        cancel_pointer_drag(world, runtime);
        return;
    }
    let focus_target = world.get_resource::<crate::native::output_views::OutputViews>()
        .filter(|views| views.applies_to(root))
        .and_then(|views| runtime.output.filter(|id| *id != views.focused && views.outputs.iter().any(|view| view.id == *id)));
    if let Some(output) = focus_target {
        if runtime.dragging { cancel_pointer_drag(world, runtime); }
        if matches!(gesture, PointerGesture::Begin(_)) {
            world.insert_resource(crate::native::output_views::HandFocusRequest { output, workspace: target, root, created: Instant::now() });
            runtime.pointer.disarm();
        }
        return;
    }
    let view = world.get_resource::<crate::native::output_views::OutputViews>()
        .filter(|views| views.applies_to(root) && Some(views.focused) == runtime.output)
        .map(|views| views.focused().clone());
    let current = world.get::<crate::native::workspace::Workspaces>(root).map(|spaces| spaces.active);
    let window = world.query_filtered::<(Entity, &Window), With<bevy::window::PrimaryWindow>>()
        .iter(world).find_map(|(entity, window)| window.focused.then_some(entity));
    let Some((view, window)) = view.zip(window).filter(|_| current == Some(target)) else {
        if runtime.dragging { cancel_pointer_drag(world, runtime); }
        return;
    };
    let point = match gesture {
        PointerGesture::Hover(point) | PointerGesture::Begin(point) | PointerGesture::Update(point) | PointerGesture::End(point) => point,
        PointerGesture::Cancel => return,
    };
    let size = view.bounds.size();
    let position = Vec2::new(point[0] * size[0] as f32, point[1] * size[1] as f32);
    match gesture {
        PointerGesture::Begin(_) => {
            runtime.dragging = pointer::press(world, root, window, position, crate::host::Manipulation::Move, None);
            if !runtime.dragging && !crate::native::layout::desktop::manipulation::active(world) {
                runtime.dragging = begin_unmanaged_drag(world, root, window, position);
            }
            if !runtime.dragging {
                runtime.pointer_message = Some("No movable item at the hand pointer");
                let _ = runtime.pointer.cancel();
            } else {
                runtime.pointer_message = None;
            }
        }
        PointerGesture::Update(_) if runtime.dragging => {
            let unmanaged = world.contains_resource::<UnmanagedDrag>();
            let moved = if unmanaged {
                update_unmanaged_drag(world, position, false)
            } else { pointer::motion(world, position) };
            if !moved {
                if unmanaged { runtime.dragging = false; }
                cancel_pointer_drag(world, runtime);
            }
        }
        PointerGesture::End(_) if runtime.dragging => {
            runtime.dragging = false;
            let moved = if world.contains_resource::<UnmanagedDrag>() {
                update_unmanaged_drag(world, position, true)
            } else { pointer::release(world, position) };
            if !moved {
                runtime.pointer_message = Some("Movement canceled; select the item again");
            }
        }
        _ => {}
    }
}

fn tick(world: &mut World) {
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    if let Some(mut session) = runtime.session.take() {
        let views = world.get_resource::<crate::native::output_views::OutputViews>()
            .filter(|views| views.applies_to(session.root))
            .map(|views| views.outputs.clone());
        if runtime.output.is_none() {
            runtime.output = views.as_ref().and_then(|outputs| outputs.first()).map(|output| output.id);
        }
        let default_output = views.as_ref().and_then(|outputs| outputs.iter().find(|output| Some(output.id) == runtime.output))
            .map(|output| (output.workspace, output.bounds.size()));
        let target = default_output.map_or(session.workspace, |(workspace, _)| workspace);
        if session.target != Some(target) {
            runtime.motion.reset();
            cancel_pointer_drag(world, &mut runtime);
            session.target = Some(target);
        }
        let valid = world.get::<Controls>(session.owner).is_some()
            && world
                .get::<crate::native::workspace::Workspaces>(session.root)
                .is_some_and(|spaces| spaces.entries.iter().any(|entry| entry.id == target)
                    && (default_output.is_some() || views.is_none() && runtime.output.is_none() && spaces.active == session.workspace))
            && !crate::native::laboratory::suspended(world, session.owner);
        if !valid {
            session.stop.store(true, Ordering::Relaxed);
            runtime.status = "Off".into();
        }
        let spatial = world
            .get::<crate::native::workspace::Workspaces>(session.root)
            .and_then(|spaces| {
                if spaces.active == target {
                    world.get::<crate::native::topology::view::View>(session.root).map(|view| view.spatial)
                } else {
                    spaces.entries.iter().find(|space| space.id == target).map(|space| space.topology.spatial)
                }
            })
            .unwrap_or(false);
        let window_focused = world
            .query::<&Window>()
            .iter(world)
            .any(|window| window.focused);
        let focused = if views.is_some() {
            world.get_resource::<crate::host::input::InputActivity>()
                .map_or(window_focused, |activity| activity.active())
        } else {
            window_focused
        };
        if spatial || !focused {
            runtime.motion.reset();
            cancel_pointer_drag(world, &mut runtime);
        }
        let sample = session
            .latest
            .lock()
            .ok()
            .and_then(|mut latest| latest.take());
        let frame = session.frame.lock().ok().and_then(|mut frame| frame.take());
        let analysis_error = session.analysis_error.lock().ok().and_then(|mut error| error.take());
        if let Some((timestamp, error)) = analysis_error
            && runtime.analysis && timestamp >= runtime.analysis_since
        {
            runtime.analysis = false;
            session.analysis.store(false, Ordering::Relaxed);
            runtime.analysis_failure = Some(error);
        }
        if session.stop.load(Ordering::Relaxed) {
            if let Some(preview) = world
                .get::<Controls>(session.owner)
                .map(|controls| controls.preview)
                && let Some(mut node) = world.get_mut::<ImageNode>(preview)
            {
                node.image = Handle::default();
            }
            if let Some(handle) = session.image.take()
                && let Some(mut images) = world.get_resource_mut::<Assets<Image>>()
            {
                images.remove(handle.id());
            }
        }
        if !session.stop.load(Ordering::Relaxed)
            && let Some(frame) = frame
            && world.contains_resource::<Assets<Image>>()
            && let Some(preview) = world
                .get::<Controls>(session.owner)
                .map(|controls| controls.preview)
        {
            let (width, height) = frame.dimensions();
            let image = Image::new(
                bevy::render::render_resource::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                bevy::render::render_resource::TextureDimension::D2,
                image::DynamicImage::ImageRgb8(frame).to_rgba8().into_raw(),
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::MAIN_WORLD
                    | bevy::asset::RenderAssetUsages::RENDER_WORLD,
            );
            let handle = if let Some(handle) = session.image.clone() {
                if let Some(mut existing) = world.resource_mut::<Assets<Image>>().get_mut(&handle) {
                    *existing = image;
                }
                handle
            } else {
                let handle = world.resource_mut::<Assets<Image>>().add(image);
                session.image = Some(handle.clone());
                handle
            };
            if let Some(mut node) = world.get_mut::<ImageNode>(preview) {
                node.image = handle;
            }
        }
        if session.stop.load(Ordering::Relaxed) {
            runtime.motion.reset();
            cancel_pointer_drag(world, &mut runtime);
        } else if !runtime.analysis && !matches!(sample, Some(Err(_))) {
            runtime.motion.reset();
            cancel_pointer_drag(world, &mut runtime);
            runtime.status = runtime.analysis_failure.as_ref().map_or_else(
                || "Camera on · hand analysis disabled".into(),
                |error| format!("Camera on · hand analysis stopped: {error}"),
            );
        } else if let Some(sample) = sample {
            match sample {
                Err(error) => {
                    session.stop.store(true, Ordering::Relaxed);
                    runtime.motion.reset();
                    cancel_pointer_drag(world, &mut runtime);
                    runtime.status = error;
                }
                Ok((timestamp, head, hands)) => {
                    let gap = discontinuous(session.last_frame, timestamp);
                    session.last_frame = Some(timestamp);
                    if gap || spatial || !focused || timestamp < runtime.analysis_since || timestamp.elapsed() > Duration::from_millis(350) {
                        runtime.motion.reset();
                        cancel_pointer_drag(world, &mut runtime);
                        runtime.status = "Paused".into();
                    } else {
                        let open: Vec<_> = hands.iter().filter(|hand| !hand.pinched).map(|hand| hand.center).collect();
                        let delta = if runtime.pointer_mode {
                            runtime.motion.reset();
                            let gesture = runtime.pointer.update(&hands);
                            runtime.pointer_position = match gesture {
                                Some(PointerGesture::Hover(point) | PointerGesture::Begin(point) | PointerGesture::Update(point) | PointerGesture::End(point)) => Some(point),
                                Some(PointerGesture::Cancel) | None => None,
                            };
                            if let Some(gesture) = gesture {
                                apply_pointer_gesture(world, &mut runtime, session.root, target, gesture);
                            }
                            [0.0; 2]
                        } else {
                            runtime.motion.update(head, &open, timestamp.duration_since(runtime.epoch).as_secs_f64())
                        };
                        let viewport = default_output.map(|(_, size)| Vec2::new(size[0] as f32, size[1] as f32)).or_else(|| world
                            .get::<ComputedNode>(session.root)
                            .map(|node| node.size() * node.inverse_scale_factor()));
                        if let Some(size) = viewport
                            && size.is_finite()
                            && size.min_element() > 0.0
                        {
                            let displacement = DVec2::new(f64::from(delta[0] * size.x), f64::from(delta[1] * size.y));
                            let active = world.get::<crate::native::workspace::Workspaces>(session.root).map(|spaces| spaces.active);
                            if active == Some(target) {
                                if let Some(mut view) = world.get_mut::<crate::native::canvas::CanvasView>(session.root)
                                    && view.center.is_finite() && view.zoom.is_finite() && view.zoom > 0.0
                                {
                                    let center = view.center + displacement / view.zoom;
                                    if center.is_finite() { view.center = center; }
                                }
                            } else if let Some(mut spaces) = world.get_mut::<crate::native::workspace::Workspaces>(session.root)
                                && let Some(view) = spaces.entries.iter_mut().find(|entry| entry.id == target)
                                && view.zoom.is_finite() && view.zoom > 0.0
                            {
                                let center = DVec2::from_array(view.center) + displacement / view.zoom;
                                if center.is_finite() { view.center = center.to_array(); }
                            }
                        }
                        runtime.status = if runtime.pointer_mode && world.get_resource::<crate::native::output_views::OutputViews>()
                            .is_some_and(|views| Some(views.focused) != runtime.output) {
                            "Pinch to focus the display, then open and pinch again to select"
                        } else if runtime.pointer_mode && !window_focused {
                            "Focus the Lince canvas to select items"
                        } else if runtime.pointer_mode && runtime.pointer_message.is_some() {
                            runtime.pointer_message.unwrap()
                        } else if runtime.pointer_mode && runtime.pointer_position.is_some() {
                            "Hand pointer active"
                        } else if runtime.pointer_mode {
                            "Show an open palm to position the pointer"
                        } else if head.is_some() && !open.is_empty() {
                            "Open hand · moving camera"
                        } else {
                            "Show your face and an open hand"
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
            cancel_pointer_drag(world, &mut runtime);
        }
        if session.task.is_finished() {
            runtime.motion.reset();
            cancel_pointer_drag(world, &mut runtime);
            if !session.stop.load(Ordering::Relaxed) {
                runtime.status = "Camera stopped".into();
            }
            if let Some(preview) = world
                .get::<Controls>(session.owner)
                .map(|controls| controls.preview)
                && let Some(mut node) = world.get_mut::<ImageNode>(preview)
            {
                node.image = Handle::default();
            }
            if let Some(handle) = session.image.take()
                && let Some(mut images) = world.get_resource_mut::<Assets<Image>>()
            {
                images.remove(handle.id());
            }
        } else {
            runtime.session = Some(session);
        }
    }
    update_pointer_overlay(world, &mut runtime);
    let statuses: Vec<_> = world
        .query::<&Controls>()
        .iter(world)
        .map(|controls| (controls.status, controls.settings))
        .collect();
    let mode = if runtime.pointer_mode { "Pointer" } else { "Pan" };
    let settings = runtime.output.map_or_else(
        || format!("Camera {} · display automatic · {mode}", runtime.camera_index),
        |output| {
            let name = world.get_resource::<crate::native::output_views::OutputNames>()
                .and_then(|names| names.0.iter().find(|(id, _)| *id == output))
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| output.0.to_string());
            format!("Camera {} · display {} · {mode}", runtime.camera_index, name)
        },
    );
    for (status, settings_label) in statuses {
        if let Some(mut text) = world.get_mut::<Text>(status)
            && text.0 != runtime.status
        {
            text.0.clone_from(&runtime.status);
        }
        if let Some(mut text) = world.get_mut::<Text>(settings_label)
            && text.0 != settings
        {
            text.0.clone_from(&settings);
        }
    }
    world.insert_resource(runtime);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::actions::Action;

    fn hand(center: Point) -> HandObservation {
        HandObservation { center, pointer: center, pinched: false }
    }

    #[test]
    fn camera_error_stops_capture_even_when_analysis_is_disabled() {
        let executor = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let mut app = crate::native::sand_panel::tests::app();
        app.add_plugins(HandCameraPlugin);
        let root = app.world_mut().spawn(crate::native::workspace::Workspaces::default()).id();
        let placeholder = app.world_mut().spawn_empty().id();
        let owner = app.world_mut().spawn(Controls { root, sand: placeholder, status: placeholder, settings: placeholder, preview: placeholder }).id();
        let stop = Arc::new(AtomicBool::new(false));
        let latest = Arc::new(Mutex::new(None));
        {
            let mut runtime = app.world_mut().resource_mut::<Runtime>();
            runtime.analysis = true;
            runtime.session = Some(Session {
                owner, root, workspace: 1, target: None, stop: stop.clone(), analysis: Arc::new(AtomicBool::new(false)),
                analysis_error: Arc::new(Mutex::new(Some((Instant::now(), "Model failed".into())))),
                latest: latest.clone(), frame: Arc::new(Mutex::new(None)), image: None,
                task: executor.spawn(std::future::pending()), last_frame: None,
            });
        }
        app.update();
        assert!(!stop.load(Ordering::Relaxed));
        assert!(!app.world().resource::<Runtime>().analysis);
        assert!(app.world().resource::<Runtime>().status.contains("Model failed"));
        Analysis(true).apply(app.world_mut(), owner);
        let runtime = app.world().resource::<Runtime>();
        let old = runtime.analysis_since - Duration::from_millis(1);
        *runtime.session.as_ref().unwrap().analysis_error.lock().unwrap() = Some((old, "Old model failure".into()));
        app.update();
        assert!(app.world().resource::<Runtime>().analysis);
        assert!(!stop.load(Ordering::Relaxed));
        Analysis(false).apply(app.world_mut(), owner);
        *latest.lock().unwrap() = Some(Err("Camera failed".into()));
        app.update();
        assert!(stop.load(Ordering::Relaxed));
        assert_eq!(app.world().resource::<Runtime>().status, "Camera failed");
    }

    #[test]
    fn selected_output_receives_motion_and_stale_input_and_disconnect_stop_it() {
        use crate::host::{OutputId, OutputViewport, Rectangle, raster::RasterMapping};
        let executor = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let mut app = crate::native::sand_panel::tests::app();
        app.init_resource::<Assets<Image>>().add_plugins(HandCameraPlugin);
        app.world_mut().spawn(Window { focused: false, ..default() });
        app.insert_resource(crate::host::input::InputActivity::default());
        let mut spaces = crate::native::workspace::Workspaces::default();
        let mut other = spaces.entries[0].clone();
        other.id = 2;
        spaces.entries.push(other);
        let root = app.world_mut().spawn((spaces, crate::native::canvas::CanvasView::default(),
            crate::native::topology::view::View { spatial: true, ..default() })).id();
        let preview = app.world_mut().spawn(ImageNode::default()).id();
        let status = app.world_mut().spawn(Text::new("Off")).id();
        let settings = app.world_mut().spawn(Text::new("")).id();
        let owner = app.world_mut().spawn(Controls { root, sand: root, preview, status, settings }).id();
        let bounds = Rectangle::new([0.0; 2], [800.0, 600.0]).unwrap();
        let output = |id, workspace| OutputViewport {
            id: OutputId(id), bounds, usable: Some(bounds), physical_size: [800, 600],
            raster: RasterMapping { origin: [0.0; 2], scale: [1.0; 2] }, workspace, rotation: Default::default(),
        };
        let mut views = crate::native::output_views::OutputViews::new(vec![output(1, 2), output(2, 1)], OutputId(2)).unwrap();
        views.canvas = Some(root);
        app.world_mut().insert_resource(views);
        app.world_mut().insert_resource(crate::native::output_views::OutputNames(vec![(OutputId(1), "eDP-1".into()), (OutputId(2), "HDMI-A-1".into())]));
        let stop = Arc::new(AtomicBool::new(false));
        let analysis = Arc::new(AtomicBool::new(true));
        let latest = Arc::new(Mutex::new(None));
        let frame = Arc::new(Mutex::new(Some(image::RgbImage::from_pixel(2, 2, image::Rgb([3, 4, 5])))));
        let epoch = Instant::now() - Duration::from_millis(100);
        assert!(!discontinuous(None, epoch));
        assert!(!discontinuous(Some(epoch), epoch + Duration::from_millis(30)));
        assert!(discontinuous(Some(epoch), epoch + Duration::from_millis(351)));
        assert!(discontinuous(Some(epoch + Duration::from_millis(1)), epoch));
        {
            let mut runtime = app.world_mut().resource_mut::<Runtime>();
            runtime.epoch = epoch;
            runtime.analysis_since = epoch;
            runtime.analysis = true;
            runtime.session = Some(Session {
                owner, root, workspace: 1, target: None, stop: stop.clone(), analysis: analysis.clone(), analysis_error: Arc::new(Mutex::new(None)),
                latest: latest.clone(), frame, image: None, task: executor.spawn(std::future::pending()), last_frame: None,
            });
        }
        *latest.lock().unwrap() = Some(Ok((epoch, Some([0.5, 0.5]), vec![hand([0.8, 0.5])])));
        app.update();
        assert_ne!(app.world().get::<ImageNode>(preview).unwrap().image, Handle::default());
        assert!(app.world().get::<Text>(settings).unwrap().0.contains("eDP-1"));
        *latest.lock().unwrap() = Some(Ok((epoch + Duration::from_millis(30), Some([0.5, 0.5]), vec![hand([0.8, 0.5])])));
        app.update();
        assert_eq!(app.world().get::<crate::native::canvas::CanvasView>(root).unwrap().center, DVec2::ZERO);
        let moved = app.world().get::<crate::native::workspace::Workspaces>(root).unwrap().entries[1].center;
        assert!(moved[0] > 0.0);
        let image = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
        app.world_mut().insert_resource(crate::native::topology::output_views::Targets(
            [(OutputId(1), image)].into_iter().collect(),
        ));
        PointerMode(true).apply(app.world_mut(), owner);
        let mut runtime = app.world_mut().remove_resource::<Runtime>().unwrap();
        apply_pointer_gesture(app.world_mut(), &mut runtime, root, 2, PointerGesture::Begin([0.5, 0.5]));
        let request = app.world().get_resource::<crate::native::output_views::HandFocusRequest>().unwrap();
        assert_eq!((request.output, request.workspace, request.root), (OutputId(1), 2, root));
        assert!(request.matches(root, OutputId(1), 2));
        assert!(!request.matches(root, OutputId(1), 1));
        assert!(!request.matches(root, OutputId(2), 2));
        assert!(!request.matches(owner, OutputId(1), 2));
        let expired = crate::native::output_views::HandFocusRequest { created: Instant::now() - Duration::from_secs(1), ..*request };
        assert!(!expired.fresh());
        app.world_mut().insert_resource(runtime);
        app.world_mut().init_resource::<ButtonInput<MouseButton>>();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        cancel_for_manual_input(app.world_mut());
        assert!(app.world().get_resource::<crate::native::output_views::HandFocusRequest>().is_none());
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear_just_pressed(MouseButton::Left);
        app.world_mut().resource_mut::<Runtime>().analysis_since = epoch;
        *latest.lock().unwrap() = Some(Ok((epoch + Duration::from_millis(40), Some([0.5, 0.5]), vec![hand([0.8, 0.5])])));
        app.update();
        assert!(app.world().resource::<Runtime>().overlay.is_some());
        assert!(app.world().get::<Text>(settings).unwrap().0.contains("Pointer"));
        let closed = HandObservation { pinched: true, ..hand([0.8, 0.5]) };
        {
            let mut runtime = app.world_mut().resource_mut::<Runtime>();
            assert!(matches!(runtime.pointer.update(&[closed]), Some(PointerGesture::Hover(_))));
            assert!(matches!(runtime.pointer.update(&[closed]), Some(PointerGesture::Begin(_))));
            runtime.session.as_mut().unwrap().last_frame = Some(epoch - Duration::from_millis(400));
        }
        *latest.lock().unwrap() = Some(Ok((epoch + Duration::from_millis(50), Some([0.5, 0.5]), vec![closed])));
        app.update();
        assert!(app.world().resource::<Runtime>().overlay.is_none());
        assert_eq!(app.world_mut().resource_mut::<Runtime>().pointer.update(&[closed]), None);
        PointerMode(false).apply(app.world_mut(), owner);
        app.update();
        assert!(app.world().resource::<Runtime>().overlay.is_none());
        assert!(app.world().get::<Text>(settings).unwrap().0.contains("Pan"));
        app.world().resource::<crate::host::input::InputActivity>().set_active(false);
        *latest.lock().unwrap() = Some(Ok((epoch + Duration::from_millis(45), Some([0.5, 0.5]), vec![hand([0.8, 0.5])])));
        app.update();
        assert_eq!(app.world().get::<crate::native::workspace::Workspaces>(root).unwrap().entries[1].center, moved);
        app.world().resource::<crate::host::input::InputActivity>().set_active(true);
        Analysis(false).apply(app.world_mut(), owner);
        assert!(!analysis.load(Ordering::Relaxed));
        Analysis(true).apply(app.world_mut(), owner);
        *latest.lock().unwrap() = Some(Ok((epoch + Duration::from_millis(60), Some([0.5, 0.5]), vec![hand([0.8, 0.5])])));
        app.update();
        assert_eq!(app.world().get::<crate::native::workspace::Workspaces>(root).unwrap().entries[1].center, moved);
        app.world_mut().resource_mut::<crate::native::output_views::OutputViews>().outputs.remove(0);
        app.update();
        assert!(stop.load(Ordering::Relaxed));
        assert_eq!(app.world().get::<ImageNode>(preview).unwrap().image, Handle::default());
    }

    #[test]
    fn control_display_selection_cycles_without_changing_workspace_focus() {
        use crate::host::{OutputId, OutputViewport, Rectangle, raster::RasterMapping};
        let mut app = crate::native::sand_panel::tests::app();
        app.add_plugins(HandCameraPlugin);
        let root = app.world_mut().spawn(crate::native::workspace::Workspaces::default()).id();
        let placeholder = app.world_mut().spawn_empty().id();
        let owner = app.world_mut().spawn(Controls { root, sand: placeholder, status: placeholder, settings: placeholder, preview: placeholder }).id();
        let bounds = Rectangle::new([0.0; 2], [800.0, 600.0]).unwrap();
        let output = |id, workspace| OutputViewport {
            id: OutputId(id), bounds, usable: Some(bounds), physical_size: [800, 600],
            raster: RasterMapping { origin: [0.0; 2], scale: [1.0; 2] }, workspace,
            rotation: Default::default(),
        };
        let mut views = crate::native::output_views::OutputViews::new(vec![output(1, 1), output(2, 2)], OutputId(1)).unwrap();
        views.canvas = Some(root);
        app.world_mut().insert_resource(views);
        NextOutput.apply(app.world_mut(), owner);
        assert_eq!(app.world().resource::<Runtime>().output, Some(OutputId(2)));
        NextOutput.apply(app.world_mut(), owner);
        assert_eq!(app.world().resource::<Runtime>().output, Some(OutputId(1)));
        assert_eq!(app.world().get::<crate::native::workspace::Workspaces>(root).unwrap().active, 1);
        assert_eq!(app.world().resource::<crate::native::output_views::OutputViews>().focused, OutputId(1));

        let (mut world, root, native, application) = crate::native::layout::desktop::tests::fixture();
        world.get_mut::<crate::native::workspace::Workspaces>(root).unwrap().entries[0]
            .desktop.set_mode(crate::host::WorkspaceLayout::Free);
        world.get_mut::<crate::native::canvas::CanvasItem>(application).unwrap().position = DVec2::new(350.0, 200.0);
        world.init_resource::<bevy::input_focus::InputFocus>();
        crate::native::layout::desktop::resolve(&mut world);
        let window = world.spawn((Window { focused: true, ..default() }, bevy::window::PrimaryWindow)).id();
        let snapshot = world.resource::<crate::native::layout::desktop::PublishedGeometry>().0.as_ref().unwrap().clone();
        let id = world.get::<crate::native::layout::desktop::Managed>(native).unwrap().id;
        let item = snapshot.items.iter().find(|item| item.id == crate::host::ItemId::Native(id.0)).unwrap();
        let output = snapshot.outputs.iter().find(|output| output.id == item.output).unwrap().clone();
        let logical = item.mapping.canvas.origin();
        let size = item.mapping.canvas.size();
        let viewport = output.bounds.size();
        let point = [((logical[0] + size[0] * 0.5 - output.raster.origin[0]) / viewport[0]) as f32,
            ((logical[1] + size[1] * 0.5 - output.raster.origin[1]) / viewport[1]) as f32];
        let moved = [point[0] + 0.05, point[1]];
        let mut views = crate::native::output_views::OutputViews::new(vec![output.clone()], output.id).unwrap();
        views.canvas = Some(root);
        world.insert_resource(views);
        let original = *world.get::<crate::native::canvas::CanvasItem>(native).unwrap();
        let mut runtime = Runtime::default();
        runtime.output = Some(output.id);
        apply_pointer_gesture(&mut world, &mut runtime, root, 1, PointerGesture::Begin(point));
        assert!(runtime.dragging);
        apply_pointer_gesture(&mut world, &mut runtime, root, 1, PointerGesture::Update(moved));
        assert_ne!(crate::native::canvas::presented(&world, native).unwrap().position, original.position);
        apply_pointer_gesture(&mut world, &mut runtime, root, 1, PointerGesture::Cancel);
        assert_eq!(world.get::<crate::native::canvas::CanvasItem>(native).unwrap().position, original.position);
        apply_pointer_gesture(&mut world, &mut runtime, root, 1, PointerGesture::Begin(point));
        assert!(runtime.dragging);
        apply_pointer_gesture(&mut world, &mut runtime, root, 1, PointerGesture::End(moved));
        assert_ne!(world.get::<crate::native::canvas::CanvasItem>(native).unwrap().position, original.position);
        let committed = world.get::<crate::native::canvas::CanvasItem>(native).unwrap().position;
        apply_pointer_gesture(&mut world, &mut runtime, root, 1, PointerGesture::Begin(moved));
        assert!(runtime.dragging);
        apply_pointer_gesture(&mut world, &mut runtime, root, 1, PointerGesture::Update([moved[0] + 0.03, moved[1]]));
        world.get_mut::<Window>(window).unwrap().focused = false;
        apply_pointer_gesture(&mut world, &mut runtime, root, 1, PointerGesture::Update([moved[0] + 0.04, moved[1]]));
        assert!(!runtime.dragging);
        assert_eq!(world.get::<crate::native::canvas::CanvasItem>(native).unwrap().position, committed);

        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        let window = app.world_mut().spawn(Window::default()).id();
        app.world_mut().get_mut::<crate::native::edit_mode::EditMode>(root).unwrap().enabled = true;
        app.world_mut().entity_mut(root).insert((
            crate::native::canvas::CanvasView::default(),
            ComputedNode { size: Vec2::splat(1000.0), inverse_scale_factor: 1.0, ..default() },
            UiGlobalTransform::default(),
        ));
        let area = app.world_mut().spawn((
            crate::native::canvas::CanvasItem { position: DVec2::ZERO, size: Vec2::splat(100.0) },
            crate::native::workspace::WorkspaceMember(1),
            crate::native::area::InfluenceArea::new(crate::native::area::AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0)),
            ChildOf(root),
        )).id();
        let area_child = app.world_mut().spawn(ChildOf(area)).id();
        let modal = app.world_mut().spawn(crate::native::inspection::InspectionExcluded).id();
        let passthrough = app.world_mut().spawn(Pickable::IGNORE).id();
        let root_control = app.world_mut().spawn(ChildOf(root)).id();
        let root_passthrough = app.world_mut().spawn((Pickable::IGNORE, ChildOf(root))).id();
        assert_eq!(unmanaged_hit(app.world(), root, area, area_child), Some(false));
        assert_eq!(unmanaged_hit(app.world(), root, area, modal), Some(true));
        assert_eq!(unmanaged_hit(app.world(), root, area, passthrough), None);
        assert_eq!(unmanaged_hit(app.world(), root, area, root_control), Some(true));
        assert_eq!(unmanaged_hit(app.world(), root, area, root_passthrough), None);
        assert!(begin_unmanaged_drag(app.world_mut(), root, window, Vec2::ZERO));
        assert!(update_unmanaged_drag(app.world_mut(), Vec2::new(25.0, 10.0), false));
        assert_eq!(app.world().get::<crate::native::area::InfluenceArea>(area).unwrap().center, [0.0, 0.0]);
        app.world_mut().insert_resource(Runtime { dragging: true, ..default() });
        app.world_mut().init_resource::<ButtonInput<KeyCode>>();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        cancel_for_manual_input(app.world_mut());
        assert!(!app.world().resource::<Runtime>().dragging);
        assert!(app.world().get::<crate::native::canvas::ItemPreview>(area).is_none());
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear_just_pressed(KeyCode::Escape);
        assert!(begin_unmanaged_drag(app.world_mut(), root, window, Vec2::ZERO));
        assert!(update_unmanaged_drag(app.world_mut(), Vec2::new(12.0, 5.0), false));
        app.world_mut().resource_mut::<Runtime>().dragging = true;
        app.world_mut().init_resource::<ButtonInput<MouseButton>>();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        cancel_for_manual_input(app.world_mut());
        assert!(!app.world().resource::<Runtime>().dragging);
        assert!(app.world().get::<crate::native::canvas::ItemPreview>(area).is_none());
        assert_eq!(app.world().get::<crate::native::area::InfluenceArea>(area).unwrap().center, [0.0, 0.0]);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear_just_pressed(MouseButton::Left);
        assert!(begin_unmanaged_drag(app.world_mut(), root, window, Vec2::ZERO));
        assert!(update_unmanaged_drag(app.world_mut(), Vec2::new(25.0, 10.0), true));
        assert_eq!(app.world().get::<crate::native::area::InfluenceArea>(area).unwrap().center, [25.0, 10.0]);
        app.world_mut().despawn(area);
        let pinned = app.world_mut().spawn((
            crate::native::canvas::CanvasItem { position: DVec2::ZERO, size: Vec2::splat(100.0) },
            crate::native::workspace::WorkspaceMember(1),
            crate::native::sand_placement::Pinned { anchor: [0.5, 0.5], scale: 1.0 },
            ChildOf(root),
        )).id();
        let original_bounds = crate::native::canvas_selection::screen_bounds(app.world(), root, pinned).unwrap();
        assert!(begin_unmanaged_drag(app.world_mut(), root, window, Vec2::ZERO));
        assert!(update_unmanaged_drag(app.world_mut(), Vec2::new(25.0, 10.0), false));
        assert_eq!(app.world().get::<crate::native::sand_placement::Pinned>(pinned).unwrap().anchor, [0.5, 0.5]);
        assert_eq!(app.world().get::<crate::native::sand_placement::PinnedPreview>(pinned).unwrap().0.anchor, [0.525, 0.51]);
        let preview_bounds = crate::native::canvas_selection::screen_bounds(app.world(), root, pinned).unwrap();
        assert!((preview_bounds.min - original_bounds.min - Vec2::new(25.0, 10.0)).length() < 0.001);
        cancel_unmanaged_drag(app.world_mut());
        assert!(app.world().get::<crate::native::sand_placement::PinnedPreview>(pinned).is_none());
        assert!(begin_unmanaged_drag(app.world_mut(), root, window, Vec2::ZERO));
        assert!(update_unmanaged_drag(app.world_mut(), Vec2::new(25.0, 10.0), true));
        assert_eq!(app.world().get::<crate::native::sand_placement::Pinned>(pinned).unwrap().anchor, [0.525, 0.51]);
        assert!(begin_unmanaged_drag(app.world_mut(), root, window, Vec2::new(25.0, 10.0)));
        let other = app.world_mut().spawn((
            crate::native::canvas::CanvasItem { position: DVec2::new(300.0, 0.0), size: Vec2::splat(100.0) },
            crate::native::workspace::WorkspaceMember(1), ChildOf(root),
        )).id();
        crate::native::canvas_selection::set_selection(app.world_mut(), root, vec![other]);
        assert!(!update_unmanaged_drag(app.world_mut(), Vec2::new(30.0, 10.0), true));
        assert_eq!(app.world().get::<crate::native::sand_placement::Pinned>(pinned).unwrap().anchor, [0.525, 0.51]);
        assert!(app.world().get::<crate::native::sand_placement::PinnedPreview>(pinned).is_none());
        crate::native::canvas_selection::set_selection(app.world_mut(), root, vec![pinned, other]);
        assert!(begin_unmanaged_drag(app.world_mut(), root, window, Vec2::new(25.0, 10.0)));
        app.world_mut().get_mut::<crate::native::canvas::CanvasItem>(other).unwrap().position.x += 20.0;
        assert!(!update_unmanaged_drag(app.world_mut(), Vec2::new(35.0, 10.0), true));
        assert_eq!(app.world().get::<crate::native::canvas::CanvasItem>(other).unwrap().position.x, 320.0);
        assert_eq!(app.world().get::<crate::native::sand_placement::Pinned>(pinned).unwrap().anchor, [0.525, 0.51]);
    }

    #[test]
    fn camera_selection_and_analysis_are_independent() {
        let mut app = crate::native::sand_panel::tests::app();
        app.add_plugins(HandCameraPlugin);
        let owner = app.world_mut().spawn_empty().id();
        CameraIndex(true).apply(app.world_mut(), owner);
        assert_eq!(app.world().resource::<Runtime>().camera_index, 1);
        Analysis(true).apply(app.world_mut(), owner);
        assert!(app.world().resource::<Runtime>().analysis);
        assert!(app.world().resource::<Runtime>().session.is_none());
        Analysis(false).apply(app.world_mut(), owner);
        CameraIndex(false).apply(app.world_mut(), owner);
        let runtime = app.world().resource::<Runtime>();
        assert!(!runtime.analysis);
        assert_eq!(runtime.camera_index, 0);
    }

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
            4
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
                target: None,
                stop: stop.clone(),
                analysis: Arc::new(AtomicBool::new(true)),
                analysis_error: Arc::new(Mutex::new(None)),
                latest: latest.clone(),
                frame: Arc::new(Mutex::new(None)),
                image: None,
                task: executor.spawn(std::future::pending()),
                last_frame: None,
            });
            runtime.analysis = true;
            runtime.analysis_since = epoch;
        }
        *latest.lock().unwrap() = Some(Ok((epoch, Some([0.5, 0.5]), vec![hand([0.5, 0.5])])));
        app.update();
        assert_eq!(
            app.world()
                .get::<crate::native::canvas::CanvasView>(root)
                .unwrap()
                .center,
            DVec2::ZERO
        );
        *latest.lock().unwrap() = Some(Ok((epoch + Duration::from_millis(30), Some([0.5, 0.5]), vec![hand([0.8, 0.2])])));
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
        *latest.lock().unwrap() = Some(Ok((epoch + Duration::from_millis(60), Some([0.5, 0.5]), vec![hand([0.8, 0.2])])));
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
