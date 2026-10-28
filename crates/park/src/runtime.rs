use crate::{
    graphics::ProbeFailure,
    presentation::{Frame, Presentation},
    server::WindowSnapshot,
};
use bevy::{
    input::{
        keyboard::Key,
        mouse::{MouseButton, MouseScrollUnit},
        touch::TouchPhase,
    },
    prelude::KeyCode,
};
use lince_interface::{
    host::{
        DesktopAction, HostRequest, LatestReceiver, LatestSender, SurfaceId, WakeSignal,
        latest_channel,
    },
    native::app::CellHandle,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::{JoinHandle, Thread},
};

static NEXT_FRAME_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn tracing_frames() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("LINCE_PARK_TRACE_FRAMES").is_some())
}

pub(crate) fn trace_frame(stage: &str, started: std::time::Instant) {
    if tracing_frames() {
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_secs_f64();
        eprintln!("Park frame {stage}: {:.3} ms at {timestamp:.6}", started.elapsed().as_secs_f64() * 1000.0);
    }
}

#[cfg(feature = "worker-gate")]
use std::time::Duration;

#[cfg(feature = "worker-gate")]
mod probe;
#[cfg(feature = "worker-gate")]
pub use probe::{demo_client, probe};

pub struct Configuration {
    pub showoff: bool,
    pub size: [u32; 2],
    pub directory: PathBuf,
    pub runtime: tokio::runtime::Handle,
    pub cell: Option<CellHandle>,
}

#[derive(Clone)]
pub struct Scene {
    pub revision: u64,
    pub outputs: Vec<crate::presentation::Output>,
    pub windows: Arc<Vec<WindowSnapshot>>,
    pub display: Option<lince_interface::host::display::DisplaySnapshot>,
}

pub enum Input {
    #[cfg(test)]
    Test(Box<dyn FnOnce(&mut Presentation) + Send>),
    Manipulation(crate::presentation::manipulation::Event, u64),
    NativeDrop(crate::native_input::transfer::Request),
    FileDrag(Option<lince_interface::host::FileDragContext>),
    DragActive(bool),
    OverviewHover { request: lince_interface::host::OverviewHoverRequest, accepted: bool },
    NativeEdit(crate::native_input::Edit),
    Pointer([f32; 2]),
    Touch(crate::native_input::TouchEvent),
    CancelTouch,
    Tablet(crate::native_input::TabletEvent),
    CancelTablet,
    #[cfg(feature = "session")]
    AbsoluteFocus(Box<crate::native_input::AbsoluteFocus>),
    CancelKeys,
    Button(MouseButton, bool, [f32; 2], u64),
    CapturedButton(MouseButton, bool, [f32; 2], u64, crate::native_input::PointerTarget),
    Key {
        physical: KeyCode,
        logical: Key,
        text: Option<String>,
        pressed: bool,
        repeat: bool,
    },
    Scroll([f32; 2], MouseScrollUnit, TouchPhase, [f32; 2], u64),
    CapturedScroll([f32; 2], MouseScrollUnit, TouchPhase, [f32; 2], u64, crate::native_input::PointerTarget),
    Focus(bool),
    Modifiers(Vec<(KeyCode, Key)>),
    Action(DesktopAction),
    Control(crate::server::ControlWork),
    ControlStatus(lince_interface::host::control::Status),
    CapturePolicy(lince_interface::host::capture::PolicyGate),
    CaptureStatus(lince_interface::host::capture::Status),
    CaptureDemand(lince_interface::host::capture::Demand),
    Inspect(lince_interface::host::WindowInspection),
    Logout,
    Feedback(String),
    Keyboard(lince_interface::host::input::KeyboardSnapshot),
    PointerSettings(lince_interface::host::input::PointerSnapshot),
    AbsoluteSettings(lince_interface::host::input::AbsoluteSnapshot),
    Gesture(lince_interface::host::input::GestureDelivery),
    GestureSupport(bool),
    CursorWarpSupport(bool),
    PointerFocus(lince_interface::host::input::FocusTarget, u64),
    ExternalSettings(lince_interface::host::external::Snapshot),
    Magnification(lince_interface::host::magnification::Snapshot),
    MagnificationCaret(Option<lince_interface::host::magnification::Caret>),
    Application(SurfaceId),
    Activate(SurfaceId),
    InitialActivate(SurfaceId),
    ReapplyOpening(lince_interface::host::OpeningRequest),
    Workspace { owner: u64, id: u64 },
    CreateWorkspace { owner: u64 },
    Window(SurfaceId, lince_interface::host::WindowRequest),
    Output(lince_interface::host::OutputId),
}

pub struct Completed {
    pub sequence: u64,
    pub scene_revision: u64,
    pub frame: Frame,
    pub windows: Arc<Vec<WindowSnapshot>>,
    pub rejected: Vec<SurfaceId>,
    pub input_serial: u64,
}

pub struct Started {
    pub scene_revision: u64,
    pub windows: Vec<WindowSnapshot>,
}

#[derive(bevy::prelude::Resource, Clone, Default)]
pub(crate) struct InputProgress {
    submitted: Arc<AtomicU64>,
    applied: Arc<AtomicU64>,
}

impl InputProgress {
    pub(crate) fn queued(&self, serial: u64) { self.submitted.fetch_max(serial, Ordering::Release); }
    pub(crate) fn watermark(&self) -> u64 { self.submitted.load(Ordering::Acquire) }
    pub(crate) fn reached(&self, watermark: u64) -> bool { self.applied.load(Ordering::Acquire) >= watermark }
}

pub struct CanvasWorker {
    scene: LatestSender<Scene>,
    inputs: SyncSender<(u64, Input)>,
    next_input: AtomicU64,
    progress: InputProgress,
    pointer: LatestSender<(u64, [f32; 2], u64)>,
    next_pointer: AtomicU64,
    frames: LatestReceiver<Completed>,
    started: LatestReceiver<Started>,
    requests: Receiver<HostRequest>,
    failures: Receiver<ProbeFailure>,
    running: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
    input_activity: lince_interface::host::input::InputActivity,
    native_focus: lince_interface::host::input::InputActivity,
    retired_input: Arc<AtomicU64>,
    #[cfg(test)]
    idle: Arc<AtomicBool>,
    #[cfg(test)]
    publications: Arc<AtomicU64>,
    thread: Thread,
    join: Option<JoinHandle<()>>,
}

struct Active {
    running: Arc<AtomicBool>,
    notify: Option<WakeSignal>,
    input_activity: lince_interface::host::input::InputActivity,
    native_focus: lince_interface::host::input::InputActivity,
    retired_input: Arc<AtomicU64>,
    progress: InputProgress,
    #[cfg(test)]
    idle: Arc<AtomicBool>,
    #[cfg(test)]
    publications: Arc<AtomicU64>,
}

impl Drop for Active {
    fn drop(&mut self) {
        self.input_activity.set_active(false);
        self.native_focus.set_active(false);
        self.running.store(false, Ordering::Release);
        if let Some(notify) = &self.notify {
            notify.ring();
        }
    }
}

impl CanvasWorker {
    pub fn spawn(configuration: Configuration) -> std::io::Result<Self> {
        Self::spawn_notifying(configuration, None)
    }

    pub fn spawn_notifying(
        configuration: Configuration,
        notify: Option<WakeSignal>,
    ) -> std::io::Result<Self> {
        Self::spawn_configured(configuration, notify, false)
    }

    #[cfg(feature = "nested")]
    pub(crate) fn spawn_for_compositor(configuration: Configuration, notify: WakeSignal, linear_color: bool) -> std::io::Result<Self> {
        Self::spawn_configured(configuration, Some(notify), linear_color)
    }

    fn spawn_configured(configuration: Configuration, notify: Option<WakeSignal>, linear_color: bool) -> std::io::Result<Self> {
        Self::spawn_prepared(configuration, notify, linear_color, |_| {})
    }

    #[cfg(all(test, feature = "nested", feature = "server-tests"))]
    pub(crate) fn spawn_with_setup(configuration: Configuration, prepare: impl FnOnce(&mut Presentation) + Send + 'static) -> std::io::Result<Self> {
        Self::spawn_prepared(configuration, None, false, prepare)
    }

    fn spawn_prepared(configuration: Configuration, notify: Option<WakeSignal>, linear_color: bool, prepare: impl FnOnce(&mut Presentation) + Send + 'static) -> std::io::Result<Self> {
        let (scene, scenes) = latest_channel();
        let (images, frames) = latest_channel();
        let (starting, started) = latest_channel();
        let (inputs, events) = sync_channel(128);
        let (pointer, motion) = latest_channel();
        let (commands, requests) = sync_channel(64);
        let (failed, failures) = sync_channel(1);
        let running = Arc::new(AtomicBool::new(true));
        let active = running.clone();
        let dirty = Arc::new(AtomicBool::new(true));
        let changed = dirty.clone();
        let input_activity = lince_interface::host::input::InputActivity::default();
        let worker_activity = input_activity.clone();
        let native_focus = lince_interface::host::input::InputActivity::default();
        let worker_focus = native_focus.clone();
        let retired_input = Arc::new(AtomicU64::new(0));
        let worker_retired = retired_input.clone();
        let progress = InputProgress::default();
        let worker_progress = progress.clone();
        #[cfg(test)]
        let idle = Arc::new(AtomicBool::new(false));
        #[cfg(test)]
        let quiescent = idle.clone();
        #[cfg(test)]
        let publications = Arc::new(AtomicU64::new(0));
        #[cfg(test)]
        let published = publications.clone();
        let join = std::thread::Builder::new()
            .name("park-canvas".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let active = Active {
                    running: active,
                    notify: notify.clone(),
                    input_activity: worker_activity,
                    native_focus: worker_focus,
                    retired_input: worker_retired,
                    progress: worker_progress,
                    #[cfg(test)]
                    idle: quiescent,
                    #[cfg(test)]
                    publications: published,
                };
                if let Err(error) = run(
                    configuration,
                    linear_color,
                    scenes,
                    events,
                    motion,
                    images,
                    starting,
                    commands,
                    &active,
                    &changed,
                    notify.as_ref(),
                    prepare,
                ) {
                    let _ = failed.try_send(error);
                    if let Some(notify) = &notify {
                        notify.ring();
                    }
                }
            })?;
        let thread = join.thread().clone();
        Ok(Self {
            scene,
            inputs,
            next_input: AtomicU64::new(1),
            progress,
            pointer,
            next_pointer: AtomicU64::new(1),
            frames,
            started,
            requests,
            failures,
            running,
            dirty,
            input_activity,
            native_focus,
            retired_input,
            #[cfg(test)]
            idle,
            #[cfg(test)]
            publications,
            thread,
            join: Some(join),
        })
    }

    pub fn scene(&self, scene: Scene) -> std::io::Result<()> {
        self.scene
            .publish(scene)
            .map_err(|_| std::io::Error::other("the Park canvas has stopped"))?;
        self.wake();
        Ok(())
    }

    pub fn input(&self, input: Input) -> Result<u64, TrySendError<Input>> {
        if let Input::Focus(focused) = &input { self.focus_native(*focused); }
        let serial =
            match self
                .next_input
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |serial| {
                    serial.checked_add(1)
                }) {
                Ok(serial) => serial,
                Err(_) => return Err(TrySendError::Full(input)),
            };
        self.inputs
            .try_send((serial, input))
            .map_err(|error| match error {
                TrySendError::Full((_, input)) => TrySendError::Full(input),
                TrySendError::Disconnected((_, input)) => TrySendError::Disconnected(input),
            })?;
        self.progress.queued(serial);
        self.wake();
        Ok(serial)
    }

    pub fn frame(&self) -> Option<Completed> {
        self.frames.take()
    }

    pub(crate) fn focus_native(&self, focused: bool) {
        self.native_focus.set_active(focused);
        if !focused { self.retired_input.fetch_max(self.next_input.load(Ordering::Acquire).saturating_sub(1), Ordering::AcqRel); }
        self.wake();
    }

    #[cfg(feature = "nested")]
    pub(crate) fn host_active(&self, active: bool) {
        let previous = self.input_activity.revision();
        self.input_activity.set_active(active);
        if self.input_activity.revision() != previous {
            if !active { self.focus_native(false); } else { self.wake(); }
        }
    }

    pub fn started(&self) -> Option<Started> {
        self.started.take()
    }

    pub fn pointer(&self, position: [f32; 2]) -> std::io::Result<u64> {
        self.publish_pointer(position, None)
    }

    #[cfg(feature = "nested")]
    pub(crate) fn pointer_with_epoch(&self, position: [f32; 2], epoch: u64) -> std::io::Result<u64> {
        self.publish_pointer(position, Some(epoch))
    }

    fn publish_pointer(&self, position: [f32; 2], epoch: Option<u64>) -> std::io::Result<u64> {
        if position.iter().any(|value| !value.is_finite()) {
            return Err(std::io::Error::other("invalid Park pointer position"));
        }
        let serial = self
            .next_pointer
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |serial| {
                serial.checked_add(1)
            })
            .map_err(|_| std::io::Error::other("Park pointer serials exhausted"))?;
        self.pointer
            .publish((serial, position, epoch.unwrap_or(serial)))
            .map_err(|_| std::io::Error::other("the Park pointer receiver is unavailable"))?;
        self.wake();
        Ok(serial)
    }
    pub fn request(&self) -> Option<HostRequest> {
        self.requests.try_recv().ok()
    }
    pub fn failure(&self) -> Option<ProbeFailure> {
        self.failures.try_recv().ok()
    }
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    #[cfg(all(test, feature = "nested", any(feature = "server-tests", feature = "accessibility-gate")))]
    pub(crate) fn test_activity(&self) -> (bool, u64) { (self.idle.load(Ordering::Acquire), self.publications.load(Ordering::Acquire)) }

    pub fn wake(&self) {
        #[cfg(test)]
        self.idle.store(false, Ordering::Release);
        self.dirty.store(true, Ordering::Release);
        self.thread.unpark();
    }

    pub fn stop(&mut self) -> std::io::Result<()> {
        self.running.store(false, Ordering::Release);
        self.thread.unpark();
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| std::io::Error::other("the Park canvas thread panicked"))?;
        }
        if let Some(failure) = self.failure() {
            return Err(std::io::Error::other(failure.to_string()));
        }
        Ok(())
    }
}

impl Drop for CanvasWorker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn run(
    configuration: Configuration,
    linear_color: bool,
    scenes: LatestReceiver<Scene>,
    events: Receiver<(u64, Input)>,
    motion: LatestReceiver<(u64, [f32; 2], u64)>,
    images: LatestSender<Completed>,
    starting: LatestSender<Started>,
    commands: SyncSender<HostRequest>,
    active: &Active,
    dirty: &Arc<AtomicBool>,
    notify: Option<&WakeSignal>,
    prepare: impl FnOnce(&mut Presentation),
) -> Result<(), ProbeFailure> {
    let running = &active.running;
    let runtime = configuration.runtime.clone();
    let _entered = runtime.enter();
    if let Some(cell) = &configuration.cell {
        runtime
            .block_on(lince_interface::native::app::initialize_language(&cell.0))
            .map_err(|error| failure(error.to_string()))?;
    }
    let thread = std::thread::current();
    let changed = dirty.clone();
    #[cfg(test)]
    let idle = active.idle.clone();
    let wake = WakeSignal::new(move || {
        #[cfg(test)]
        idle.store(false, Ordering::Release);
        changed.store(true, Ordering::Release);
        thread.unpark();
    });
    let connected = configuration.cell.is_some();
    let mut canvas = Presentation::new_with_cell(
        configuration.size,
        configuration.directory.join("interface-assets"),
        wake,
        configuration.cell.map(|cell| (cell, configuration.directory.clone())),
    )?;
    canvas.world_mut().insert_resource(active.input_activity.clone());
    canvas.world_mut().insert_resource(active.progress.clone());
    canvas.prepare_linear_output(linear_color)?;
    canvas.host_requests(commands.clone());
    canvas.enable_accessibility();
    let mut showoff = configuration.showoff;
    canvas
        .output_preferences(configuration.directory.join("park-outputs.json"))
        .map_err(|error| failure(error.to_string()))?;
    if !connected {
        canvas
            .world_mut()
            .insert_resource(lince_interface::native::workspace::WorkspaceFile::new(
                configuration.directory.join("interface.json"),
            ));
    }
    prepare(&mut canvas);
    let mut windows = Arc::new(Vec::new());
    let mut scene_revision = 0;
    let mut started = false;
    let mut pending = false;
    let mut settle = 8;
    let mut input_serial = 0;
    let mut awaiting: Option<Completed> = None;
    let mut button_pending = false;
    let mut keyboard_pending = false;
    let mut queued_event = None;
    let mut focus_revision = None;
    let synchronize_focus = |canvas: &mut Presentation, revision: &mut Option<(u64, u64)>| -> Result<(), ProbeFailure> {
        canvas.synchronize_input_activity()?;
        let current = (active.input_activity.revision(), active.native_focus.revision());
        if *revision != Some(current) {
            canvas.focus(current.0 & 1 != 0 && current.1 & 1 != 0)?;
            *revision = Some(current);
        }
        Ok(())
    };
    loop {
        #[cfg(test)]
        active.idle.store(false, Ordering::Release);
        let stopping = !running.load(Ordering::Acquire);
        let changed = dirty.swap(false, Ordering::AcqRel);
        synchronize_focus(&mut canvas, &mut focus_revision)?;
        if !started {
            canvas.frame()?;
            started = true;
        }
        if let Some(scene) = scenes.take() {
            scene_revision = scene.revision;
            if let Some(display) = scene.display {
                canvas.world_mut().insert_resource(display);
            }
            canvas.outputs(scene.outputs)?;
            if showoff {
                canvas.world_mut().insert_resource(lince_interface::native::laboratory::showoff::Launch);
                showoff = false;
            }
            pending = canvas.sync_windows(&scene.windows)?;
            windows = scene.windows;
            settle = settle.max(3);
        }
        let mut input = false;
        let pointer = if button_pending { None } else { motion.take() };
        if let Some((_, point, epoch)) = pointer && active.input_activity.active() {
            lince_interface::native::layout::desktop::focus::pointer_epoch(canvas.world_mut(), epoch);
            canvas.pointer(point)?;
            input = true;
        }
        let mut pointer_stamp = None;
        while !button_pending {
            let Some((serial, event)) = queued_event.take().or_else(|| events.try_recv().ok()) else {
                break;
            };
            let absolute_focus = {
                #[cfg(feature = "session")]
                { matches!(&event, Input::AbsoluteFocus(_)) }
                #[cfg(not(feature = "session"))]
                { false }
            };
            let inspecting = {
                #[cfg(test)]
                { matches!(&event, Input::Test(_)) }
                #[cfg(not(test))]
                { false }
            };
            if keyboard_pending && (absolute_focus || inspecting || matches!(&event, Input::Control(_))) {
                queued_event = Some((serial, event));
                break;
            }
            #[cfg(feature = "session")]
            if let Input::AbsoluteFocus(request) = &event
                && canvas.prepare_absolute_focus(request)?
            {
                queued_event = Some((serial, event));
                button_pending = true;
                input = true;
                break;
            }
            input_serial = serial;
            input = true;
            synchronize_focus(&mut canvas, &mut focus_revision)?;
            if serial <= active.retired_input.load(Ordering::Acquire)
                && matches!(&event, Input::Pointer(_) | Input::Button(..) | Input::CapturedButton(..) | Input::Key { .. } | Input::Scroll(..) | Input::CapturedScroll(..) | Input::NativeEdit(_))
            {
                #[cfg(test)]
                if matches!(&event, Input::Key { .. }) { eprintln!("Native key retired before delivery: serial={serial}, retired={}", active.retired_input.load(Ordering::Acquire)); }
                continue;
            }
            keyboard_pending |= matches!(&event, Input::Key { .. } | Input::NativeEdit(_));
            let button_edge = absolute_focus || inspecting
                || matches!(&event, Input::Action(_) | Input::Control(_) | Input::Inspect(_) | Input::Application(_)
                    | Input::Activate(_) | Input::InitialActivate(_) | Input::ReapplyOpening(_) | Input::Workspace { .. }
                    | Input::CreateWorkspace { .. } | Input::Output(_) | Input::Window(..) | Input::Logout)
                || matches!(&event, Input::Button(..) | Input::CapturedButton(..) | Input::Scroll(..) | Input::CapturedScroll(..) | Input::CancelTouch | Input::CancelTablet | Input::CancelKeys)
                || matches!(&event, Input::Touch(touch) if !matches!(touch.phase, TouchPhase::Moved))
                || matches!(&event, Input::Tablet(tablet) if tablet.event.phase != lince_interface::host::input::TabletPhase::Motion);
            let absolute_press = absolute_focus || matches!(&event, Input::Touch(touch) if matches!(touch.phase, TouchPhase::Started))
                || matches!(&event, Input::Tablet(tablet) if matches!(tablet.event.phase, lince_interface::host::input::TabletPhase::Tip(true) | lince_interface::host::input::TabletPhase::Button { pressed: true, .. }));
            if absolute_press || matches!(
                &event,
                Input::Manipulation(..)
                    | Input::NativeDrop(_)
                    | Input::FileDrag(_)
                    | Input::DragActive(_)
                    | Input::OverviewHover { .. }
                    | Input::NativeEdit(_)
                    | Input::Button(..)
                    | Input::CapturedButton(..)
                    | Input::Key { .. }
                    | Input::Scroll(..)
                    | Input::CapturedScroll(..)
                    | Input::Focus(false)
                    | Input::Action(_)
                    | Input::Inspect(_)
                    | Input::Logout
                    | Input::Application(_)
                    | Input::Activate(_)
                    | Input::InitialActivate(_)
                    | Input::ReapplyOpening(_)
                    | Input::Workspace { .. }
                    | Input::CreateWorkspace { .. }
                    | Input::Window(..)
                    | Input::Output(_)
            ) {
                canvas.cancel_gesture();
            }
            match event {
                #[cfg(test)]
                Input::Test(test) => test(&mut canvas),
                Input::Manipulation(event, stamp) => {
                    canvas.manipulation(event)?;
                    pointer_stamp = Some(stamp);
                }
                Input::Pointer(point) => canvas.pointer(point)?,
                Input::Touch(event) => { canvas.touch(event)?; }
                Input::CancelTouch => canvas.cancel_touch(),
                Input::Tablet(event) => { canvas.tablet(event)?; }
                Input::CancelTablet => canvas.cancel_tablet(),
                #[cfg(feature = "session")]
                Input::AbsoluteFocus(event) => canvas.absolute_focus(*event)?,
                Input::CancelKeys => lince_interface::native::cancel_keyboard_input(canvas.world_mut()),
                Input::Button(button, pressed, position, stamp) => {
                    canvas.pointer(position)?;
                    canvas.button(button, pressed)?;
                    pointer_stamp = Some(stamp);
                }
                Input::CapturedButton(button, pressed, position, stamp, target) => {
                    if canvas.native_pointer_current(&target) {
                        canvas.pointer(position)?;
                        canvas.button(button, pressed)?;
                    }
                    pointer_stamp = Some(stamp);
                }
                Input::Key {
                    physical,
                    logical,
                    text,
                    pressed,
                    repeat,
                } => canvas.key(physical, logical, text, pressed, repeat)?,
                Input::Scroll(delta, unit, phase, position, stamp) => {
                    canvas.pointer(position)?;
                    canvas.scroll(delta, unit, phase)?;
                    pointer_stamp = Some(stamp);
                }
                Input::CapturedScroll(delta, unit, phase, position, stamp, target) => {
                    if canvas.native_pointer_current(&target) {
                        canvas.pointer(position)?;
                        canvas.scroll(delta, unit, phase)?;
                    }
                    pointer_stamp = Some(stamp);
                }
                Input::Focus(focused) => canvas.focus(focused)?,
                Input::Modifiers(pressed) => canvas.modifiers(&pressed)?,
                Input::Action(action) => canvas.desktop_action(action)?,
                Input::Control(work) => {
                    canvas.control(work);
                    if let Some(notify) = notify { notify.ring(); }
                }
                Input::ControlStatus(status) => {
                    if status.available { canvas.accessibility_availability(status.active && !status.locked); }
                    if status.available && (status.locked || !status.active) {
                        lince_interface::native::layout::desktop::cancel_scratch_interactions(canvas.world_mut());
                    }
                    canvas.world_mut().insert_resource(status);
                }
                Input::CapturePolicy(gate) => { canvas.world_mut().insert_resource(gate); canvas.publish_capture_policy(); }
                Input::CaptureStatus(status) => { canvas.world_mut().insert_resource(status); }
                Input::CaptureDemand(demand) => { canvas.world_mut().insert_resource(demand); }
                Input::Inspect(target) => canvas.inspect_window(target)?,
                Input::Logout => match canvas.finish() {
                    Ok(()) => {
                        commands
                            .try_send(HostRequest::Exit)
                            .map_err(|_| failure("Could not confirm Park logout".into()))?;
                        if let Some(notify) = notify {
                            notify.ring();
                        }
                        while running.load(Ordering::Acquire) {
                            std::thread::park();
                        }
                        return Ok(());
                    }
                    Err(error) => {
                        eprintln!("Park logout: {error}");
                        lince_interface::native::notifications::report(
                            canvas.world_mut(),
                            "Park",
                            &error.detail,
                        );
                    }
                },
                Input::Keyboard(snapshot) => { canvas.world_mut().insert_resource(snapshot); }
                Input::ExternalSettings(snapshot) => { canvas.world_mut().insert_resource(snapshot); }
                Input::Magnification(snapshot) => { canvas.world_mut().insert_resource(snapshot); }
                Input::MagnificationCaret(caret) => { canvas.magnification_caret(caret); }
                Input::PointerSettings(snapshot) => { canvas.world_mut().insert_resource(snapshot); }
                Input::AbsoluteSettings(snapshot) => { canvas.world_mut().insert_resource(snapshot); }
                Input::Gesture(delivery) => canvas.gesture(delivery)?,
                Input::GestureSupport(available) => {
                    if !available { canvas.cancel_gesture(); }
                    canvas.world_mut().insert_resource(lince_interface::native::layout::desktop::gestures::GestureSupport(available));
                }
                Input::CursorWarpSupport(available) => {
                    canvas.world_mut().insert_resource(lince_interface::native::layout::desktop::focus::CursorWarpSupport(available));
                }
                Input::PointerFocus(target, epoch) => canvas.pointer_focus(target, epoch)?,
                Input::Feedback(message) => lince_interface::native::notifications::report(
                    canvas.world_mut(),
                    "park",
                    &message,
                ),
                Input::NativeEdit(edit) => {
                    if !canvas.suspended() { crate::native_input::enqueue(canvas.world_mut(), edit); }
                }
                Input::NativeDrop(request) => {
                    if canvas.suspended() {
                        request.cancelled.store(true, Ordering::Release);
                    } else {
                        crate::native_input::transfer::enqueue(canvas.world_mut(), request);
                    }
                }
                Input::FileDrag(context) => canvas.file_drag(context)?,
                Input::DragActive(active) => { canvas.world_mut().insert_resource(lince_interface::host::input::DragActive(active)); }
                Input::OverviewHover { request, accepted } => canvas.overview_hover(request, accepted)?,
                Input::Application(surface) => canvas.focus_application(surface),
                Input::Activate(surface) => canvas.activate_application(surface)?,
                Input::InitialActivate(surface) => canvas.initial_activate_application(surface)?,
                Input::ReapplyOpening(target) => canvas.reapply_opening(target)?,
                Input::Workspace { owner, id } => canvas.activate_workspace(owner, id)?,
                Input::CreateWorkspace { owner } => canvas.create_workspace(owner)?,
                Input::Window(surface, request) => canvas.window_request(surface, request),
                Input::Output(output) => canvas.focus_output(output)?,
            }
            if button_edge {
                button_pending = true;
            }
        }
        if let Some((serial, point, epoch)) = pointer
            && pointer_stamp.is_some_and(|stamp| stamp < serial)
        {
            lince_interface::native::layout::desktop::focus::pointer_epoch(canvas.world_mut(), epoch);
            canvas.pointer(point)?;
        }
        if stopping {
            break;
        }
        if input {
            settle = settle.max(3);
        } else if changed {
            settle = settle.max(1);
        }
        if pending && canvas.imports_ready() { settle = settle.max(1); }
        if let Some(completed) = awaiting.as_ref() {
            if !completed.frame.ready()? {
                std::thread::park();
                continue;
            }
            if images.publish(awaiting.take().unwrap()).is_err() { return canvas.finish(); }
            #[cfg(test)]
            active.publications.fetch_add(1, Ordering::Release);
            if let Some(notify) = notify { notify.ring(); }
        }
        if settle == 0 {
            #[cfg(test)]
            active.idle.store(true, Ordering::Release);
            std::thread::park();
            continue;
        }
        if pending {
            pending = canvas.sync_windows(&windows)?;
        }
        if starting.publish(Started { scene_revision, windows: canvas.painted_windows() }).is_err() { break; }
        if let Some(notify) = notify { notify.ring(); }
        let frame = canvas.frame()?;
        active.progress.applied.fetch_max(input_serial, Ordering::Release);
        canvas.accessibility_input_completed();
        let frame_sequence = NEXT_FRAME_SEQUENCE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |sequence| sequence.checked_add(1))
            .map_err(|_| failure("The completed frame sequence is exhausted.".into()))?;
        button_pending = false;
        keyboard_pending = false;
        awaiting = Some(Completed {
                sequence: frame_sequence,
                scene_revision,
                frame,
                windows: Arc::new(canvas.painted_windows()),
                rejected: canvas.rejected_windows(),
                input_serial,
            });
        settle -= 1;
    }
    canvas.finish()
}

fn failure(detail: String) -> ProbeFailure {
    ProbeFailure {
        stage: "canvas_worker".into(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod availability;
    mod logout;
    mod hover;
    mod configuration;

    #[test]
    fn canvas_panic_wakes_the_compositor_after_marking_the_worker_stopped() {
        let running = Arc::new(AtomicBool::new(true));
        let stopped = running.clone();
        let notified = Arc::new(AtomicBool::new(false));
        let observed = notified.clone();
        let notify = WakeSignal::new(move || {
            assert!(!stopped.load(Ordering::Acquire));
            observed.store(true, Ordering::Release);
        });
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _active = Active {
                running: running.clone(),
                notify: Some(notify),
                input_activity: Default::default(),
                native_focus: Default::default(),
                retired_input: Arc::new(AtomicU64::new(0)),
                progress: InputProgress::default(),
                idle: Arc::new(AtomicBool::new(false)),
                publications: Arc::new(AtomicU64::new(0)),
            };
            panic!("canvas failure");
        }));
        assert!(outcome.is_err());
        assert!(!running.load(Ordering::Acquire));
        assert!(notified.load(Ordering::Acquire));
    }
}
