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
}

pub enum Input {
    NativeDrop(crate::native_input::transfer::Request),
    NativeEdit(crate::native_input::Edit),
    Pointer([f32; 2]),
    Button(MouseButton, bool, [f32; 2], u64),
    Key {
        physical: KeyCode,
        logical: Key,
        text: Option<String>,
        pressed: bool,
        repeat: bool,
    },
    Scroll([f32; 2], MouseScrollUnit, TouchPhase, [f32; 2], u64),
    Focus(bool),
    Action(DesktopAction),
    Logout,
    Feedback(String),
    Application(SurfaceId),
    Activate(SurfaceId),
    Workspace { owner: u64, id: u64 },
    Window(SurfaceId, lince_interface::host::WindowRequest),
    Output(lince_interface::host::OutputId),
}

pub struct Completed {
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

pub struct CanvasWorker {
    scene: LatestSender<Scene>,
    inputs: SyncSender<(u64, Input)>,
    next_input: AtomicU64,
    pointer: LatestSender<(u64, [f32; 2])>,
    next_pointer: AtomicU64,
    frames: LatestReceiver<Completed>,
    started: LatestReceiver<Started>,
    requests: Receiver<HostRequest>,
    failures: Receiver<ProbeFailure>,
    running: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
    thread: Thread,
    join: Option<JoinHandle<()>>,
}

struct Active {
    running: Arc<AtomicBool>,
    notify: Option<WakeSignal>,
}

impl Drop for Active {
    fn drop(&mut self) {
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
        let join = std::thread::Builder::new()
            .name("park-canvas".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let active = Active { running: active, notify: notify.clone() };
                if let Err(error) = run(
                    configuration,
                    scenes,
                    events,
                    motion,
                    images,
                    starting,
                    commands,
                    &active.running,
                    &changed,
                    notify.as_ref(),
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
            pointer,
            next_pointer: AtomicU64::new(1),
            frames,
            started,
            requests,
            failures,
            running,
            dirty,
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
        self.wake();
        Ok(serial)
    }

    pub fn frame(&self) -> Option<Completed> {
        self.frames.take()
    }

    pub fn started(&self) -> Option<Started> {
        self.started.take()
    }

    pub fn pointer(&self, position: [f32; 2]) -> std::io::Result<u64> {
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
            .publish((serial, position))
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

    pub fn wake(&self) {
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
    scenes: LatestReceiver<Scene>,
    events: Receiver<(u64, Input)>,
    motion: LatestReceiver<(u64, [f32; 2])>,
    images: LatestSender<Completed>,
    starting: LatestSender<Started>,
    commands: SyncSender<HostRequest>,
    running: &AtomicBool,
    dirty: &Arc<AtomicBool>,
    notify: Option<&WakeSignal>,
) -> Result<(), ProbeFailure> {
    let runtime = configuration.runtime.clone();
    let _entered = runtime.enter();
    let thread = std::thread::current();
    let changed = dirty.clone();
    let wake = WakeSignal::new(move || {
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
    canvas.host_requests(commands.clone());
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
    let mut windows = Arc::new(Vec::new());
    let mut scene_revision = 0;
    let mut started = false;
    let mut pending = false;
    let mut settle = 8;
    let mut input_serial = 0;
    let mut awaiting: Option<Completed> = None;
    loop {
        let stopping = !running.load(Ordering::Acquire);
        let changed = dirty.swap(false, Ordering::AcqRel);
        if !started {
            canvas.frame()?;
            started = true;
        }
        if let Some(scene) = scenes.take() {
            scene_revision = scene.revision;
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
        let pointer = motion.take();
        if let Some((_, point)) = pointer {
            canvas.pointer(point)?;
            input = true;
        }
        let mut pointer_stamp = None;
        for (serial, event) in events.try_iter() {
            input_serial = serial;
            input = true;
            match event {
                Input::Pointer(point) => canvas.pointer(point)?,
                Input::Button(button, pressed, position, stamp) => {
                    canvas.pointer(position)?;
                    canvas.button(button, pressed)?;
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
                Input::Focus(focused) => canvas.focus(focused)?,
                Input::Action(action) => canvas.desktop_action(action)?,
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
                Input::Feedback(message) => lince_interface::native::notifications::report(
                    canvas.world_mut(),
                    "park",
                    &message,
                ),
                Input::NativeEdit(edit) => crate::native_input::enqueue(canvas.world_mut(), edit),
                Input::NativeDrop(request) => crate::native_input::transfer::enqueue(canvas.world_mut(), request),
                Input::Application(surface) => canvas.focus_application(surface),
                Input::Activate(surface) => canvas.activate_application(surface)?,
                Input::Workspace { owner, id } => canvas.activate_workspace(owner, id)?,
                Input::Window(surface, request) => canvas.window_request(surface, request),
                Input::Output(output) => canvas.focus_output(output)?,
            }
        }
        if let Some((serial, point)) = pointer
            && pointer_stamp.is_some_and(|stamp| stamp < serial)
        {
            canvas.pointer(point)?;
        }
        if stopping {
            break;
        }
        if input || changed {
            settle = settle.max(3);
        }
        if pending && canvas.imports_ready() { settle = settle.max(1); }
        if let Some(completed) = awaiting.as_ref() {
            if !completed.frame.ready()? {
                std::thread::park();
                continue;
            }
            if images.publish(awaiting.take().unwrap()).is_err() { return canvas.finish(); }
            if let Some(notify) = notify { notify.ring(); }
        }
        if settle == 0 {
            std::thread::park();
            continue;
        }
        if pending {
            pending = canvas.sync_windows(&windows)?;
        }
        if starting.publish(Started { scene_revision, windows: canvas.painted_windows() }).is_err() { break; }
        if let Some(notify) = notify { notify.ring(); }
        let frame = canvas.frame()?;
        awaiting = Some(Completed {
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
            let _active = Active { running: running.clone(), notify: Some(notify) };
            panic!("canvas failure");
        }));
        assert!(outcome.is_err());
        assert!(!running.load(Ordering::Acquire));
        assert!(notified.load(Ordering::Acquire));
    }
}
