use crate::{
    desktop::{Desktop, error, renderer::Composition},
    launch::Application,
    runtime::{CanvasWorker, Completed, Configuration, Input},
    server::{Monitor, Server},
};
use lince_interface::{
    activation::InstanceGuard,
    host::{DesktopAction, OutputId, WakeSignal},
};
use smithay::{
    backend::{
        allocator::{
            Fourcc,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{
            DrmDevice, DrmDeviceFd, DrmEvent, DrmNode,
            compositor::FrameFlags,
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
        },
        egl::{EGLContext, EGLDisplay},
        input::{
            AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event as _, InputEvent, KeyState,
            KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
            GestureBeginEvent, GestureEndEvent, GestureSwipeUpdateEvent, GesturePinchUpdateEvent,
            TouchEvent,
            TabletToolProximityEvent, TabletToolTipEvent, TabletToolButtonEvent, ProximityState, TabletToolTipState,
        },
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            Bind, Offscreen, Texture,
            gles::{GlesRenderer, GlesTexture},
        },
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent, primary_gpu},
    },
    input::pointer::AxisFrame,
    reexports::{
        calloop::{
            EventLoop, Interest, Mode as PollMode, PostAction, generic::Generic, ping::make_ping,
        },
        drm::control::{self, Device as _, ModeTypeFlags, connector, crtc},
        input::Libinput,
        rustix::fs::OFlags,
    },
    utils::DeviceFd,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

mod display;
mod devices;
mod power;
mod readiness;
#[cfg(all(test, feature = "server-tests"))]
pub(crate) use power::Frame as PowerFrame;
#[cfg(all(test, feature = "server-tests"))]
pub(crate) use power::request_current as power_request_current;
#[cfg(all(test, feature = "server-tests"))]
pub(crate) use power::State as PowerState;
pub(crate) use devices::tablet::Report as TabletReport;
#[cfg(all(test, feature = "server-tests"))]
pub(crate) use devices::absolute::reject_removed_calibration;
use display::{OutputPlan, PlannedOutput};

struct Queued {
    id: OutputId,
    frame: Arc<Completed>,
    scene: Arc<crate::desktop::renderer::Presented>,
    adaptive_sync: Option<bool>,
    color: display::color::Submitted,
    power: power::Frame,
}

type Output =
    DrmOutput<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, Queued, DrmDeviceFd>;
type Outputs = DrmOutputManager<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    Queued,
    DrmDeviceFd,
>;

enum Event {
    Input(InputEvent<LibinputInputBackend>),
    Session(SessionEvent),
    Device(UdevEvent),
    Presented(crtc::Handle, std::time::Duration, bool),
    Error(String),
}

struct Screen {
    protected: bool,
    prepared: Option<Prepared>,
    id: OutputId,
    connector: connector::Handle,
    mode: control::Mode,
    output: Output,
    origin: [i32; 2],
    logical_size: [i32; 2],
    pending: bool,
    texture: Option<GlesTexture>,
    adaptive_sync: lince_interface::host::display::AdaptiveSyncStatus,
    color: display::color::State,
    power: power::State,
}

struct Prepared {
    frame: Queued,
    sync: smithay::backend::renderer::sync::SyncPoint,
    _watch: crate::graphics::wait::Watch,
}

struct Hardware {
    devices: devices::Devices,
    catalog: Vec<crate::server::OutputHead>,
    configurations: std::collections::VecDeque<(crate::server::OutputRequest, Instant)>,
    power_requests: std::collections::VecDeque<crate::server::PowerRequest>,
    retired: Vec<Prepared>,
    preferences: BTreeMap<String, crate::commands::OutputPreference>,
    preview_preferences: display::PreferencePreview,
    outputs: Outputs,
    renderer: GlesRenderer,
    screens: HashMap<crtc::Handle, Screen>,
    identities: BTreeMap<String, OutputId>,
    next_id: u64,
    size: [u32; 2],
    active: bool,
    native_color: Option<bool>,
}

pub use crate::environment::{environment_ready, reexec_environment};

fn active_vt() -> io::Result<i32> {
    std::fs::read_to_string("/sys/class/tty/tty0/active")?
        .trim()
        .strip_prefix("tty")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|vt| (1..=63).contains(vt))
        .ok_or_else(|| io::Error::other("The kernel did not identify an active virtual terminal."))
}

pub fn run(
    configuration: Configuration,
    instance: &InstanceGuard,
    applications: Vec<Application>,
) -> io::Result<()> {
    if !environment_ready() {
        return Err(io::Error::other(
            "Start the standalone session with lince --park",
        ));
    }
    let mut readiness = readiness::Readiness::new()?;
    let mut event_loop = EventLoop::<Vec<Event>>::try_new().map_err(error)?;
    let handle = event_loop.handle();
    let (ping, source) = make_ping().map_err(error)?;
    handle.insert_source(source, |_, _, _| {}).map_err(error)?;
    let wake = WakeSignal::new(move || ping.ping());
    let activation = instance.events();
    activation.attach(wake.clone());
    let (mut session, notifier) = LibSeatSession::new().map_err(error)?;
    handle
        .insert_source(notifier, |event, _, events| {
            events.push(Event::Session(event))
        })
        .map_err(error)?;
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    let mut initial_events = Vec::new();
    while !session.is_active() {
        if Instant::now() >= deadline {
            return Err(io::Error::other(
                "The seat did not become active; start Park from an active TTY or display-manager session",
            ));
        }
        event_loop
            .dispatch(
                Some(deadline.saturating_duration_since(Instant::now())),
                &mut initial_events,
            )
            .map_err(error)?;
        initial_events.clear();
    }
    let seat = session.seat();
    let path = match std::env::var_os("LINCE_PARK_DRM_DEVICE") {
        Some(path) => PathBuf::from(path),
        None => primary_gpu(&seat)
            .map_err(error)?
            .ok_or_else(|| io::Error::other("No graphics device is available for this seat"))?,
    };
    let fd = session
        .open(
            &path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(error)?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (drm, notifier_drm) = DrmDevice::new(fd.clone(), true).map_err(error)?;
    let gbm = GbmDevice::new(fd).map_err(error)?;
    let egl = unsafe { EGLDisplay::new(gbm.clone()) }.map_err(error)?;
    let context = EGLContext::new(&egl).map_err(error)?;
    let mut renderer = unsafe { GlesRenderer::new(context) }.map_err(error)?;
    let mut composition = Composition::new(wake.clone())?;
    composition.prepare_color(&mut renderer)?;
    let formats = renderer.egl_context().dmabuf_render_formats().clone();
    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let node = DrmNode::from_path(&path).map_err(error)?;
    eprintln!("Park graphics device: {}", path.display());
    let exporter = GbmFramebufferExporter::new(gbm.clone(), Some(node));
    let outputs = DrmOutputManager::new(
        drm,
        allocator,
        exporter,
        Some(gbm),
        [Fourcc::Argb8888, Fourcc::Xrgb8888, Fourcc::Abgr2101010, Fourcc::Argb2101010],
        formats,
    );
    let commands = crate::commands::Commands::load(&configuration.directory.join("park.json"))?;
    let mut hardware = Hardware {
        devices: devices::Devices::new(commands.pointer),
        catalog: Vec::new(),
        configurations: std::collections::VecDeque::new(),
        power_requests: Default::default(),
        retired: Vec::new(),
        preferences: commands.outputs,
        preview_preferences: Default::default(),
        outputs,
        renderer,
        screens: HashMap::new(),
        identities: BTreeMap::new(),
        next_id: 1,
        size: configuration.size,
        active: true,
        native_color: None,
    };
    hardware.devices.absolute_preferences(commands.devices);
    let mut server = Server::new([configuration.size[0] as i32, configuration.size[1] as i32])?;
    server.output_power_backend(true);
    hardware.rescan(&mut server)?;
    let socket = server.listen_named(&format!("wayland-park-{}", std::process::id()))?;
    for fd in server.event_fds()? {
        handle
            .insert_source(
                Generic::new(fd, Interest::READ, PollMode::Level),
                |_, _, _| Ok(PostAction::Continue),
            )
            .map_err(error)?;
    }
    handle
        .insert_source(notifier_drm, |event, metadata, events| match event {
            DrmEvent::VBlank(crtc) => {
                let time = metadata.as_ref().and_then(|metadata| match metadata.time {
                    smithay::backend::drm::DrmEventTime::Monotonic(time) => Some((time, true)),
                    smithay::backend::drm::DrmEventTime::Realtime(_) => None,
                }).unwrap_or_else(|| (crate::desktop::renderer::capture::monotonic(), false));
                events.push(Event::Presented(crtc, time.0, time.1));
            }
            DrmEvent::Error(error) => events.push(Event::Error(error.to_string())),
        })
        .map_err(error)?;
    let udev = UdevBackend::new(&seat).map_err(error)?;
    handle
        .insert_source(udev, |event, _, events| events.push(Event::Device(event)))
        .map_err(error)?;
    let mut libinput =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput
        .udev_assign_seat(&seat)
        .map_err(|_| io::Error::other("Could not assign the input seat"))?;
    handle
        .insert_source(
            LibinputInputBackend::new(libinput.clone()),
            |event, _, events| events.push(Event::Input(event)),
        )
        .map_err(error)?;
    let command_file = configuration.directory.join("park.json");
    let worker = CanvasWorker::spawn_for_compositor(
        Configuration {
            size: hardware.size,
            ..configuration
        },
        wake.clone(),
        composition.linear_color_available(),
    )?;
    let mut desktop = Desktop::new(server, worker, hardware.size, hardware.screens.len(), true);
    desktop.send(crate::runtime::Input::CursorWarpSupport(true))?;
    desktop.server.display_bounds(None);
    desktop.commands(command_file, socket.clone());
    desktop.pointer_requests = Some(Default::default());
    desktop.absolute_requests = Some(Default::default());
    desktop.pointer_settings(hardware.devices.snapshot())?;
    desktop.absolute_settings(hardware.devices.absolute_snapshot())?;
    desktop.pointer = [
        f64::from(hardware.size[0]) / 2.0,
        f64::from(hardware.size[1]) / 2.0,
    ];
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut child_exit = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::child())?;
    let signal_wake = wake.clone();
    let signal = tokio::spawn(async move {
        loop {
            let shutdown = tokio::select! {
                _ = tokio::signal::ctrl_c() => true,
                _ = terminate.recv() => true,
                _ = child_exit.recv() => false,
            };
            if shutdown {
                stop.store(true, Ordering::Release);
            }
            signal_wake.ring();
            if shutdown {
                break;
            }
        }
    });
    let mut applications = Some(applications);
    let mut children = Vec::new();
    let mut events = Vec::new();
    let mut vt_keys = HashSet::new();
    let mut lock_frame_pending = false;
    let mut logout_requested = false;
    let started = Instant::now();
    eprintln!("Park Wayland display: {}", socket.display());
    let result: io::Result<()> = (|| {
        while desktop.running {
            let mut vt_request = None;
            if stopped.load(Ordering::Acquire) && !logout_requested {
                desktop.logout()?;
                logout_requested = true;
            }
            for event in events.drain(..) {
                match event {
                    Event::Session(SessionEvent::PauseSession) => {
                        hardware.pause(&mut desktop, &mut libinput, &mut vt_keys)?;
                        lock_frame_pending = false;
                    }
                    Event::Session(SessionEvent::ActivateSession) => {
                        hardware.resume(&mut desktop, &mut libinput)?;
                    }
                    Event::Presented(crtc, presentation_time, hardware_clock) if hardware.active && session.is_active() => {
                        let mut color_fallback = None;
                        if let Some(screen) = hardware.screens.get_mut(&crtc) {
                            let submitted = match screen.output.frame_submitted() {
                                Ok(submitted) => submitted,
                                Err(failure) => {
                                    screen.pending = false;
                                    if !desktop.server.recover_display_preview(failure.to_string()) { return Err(error(failure)); }
                                    desktop.redraw = true;
                                    None
                                }
                            };
                            if let Some(queued) = submitted {
                                if crate::runtime::tracing_frames() {
                                    let observed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default().as_secs_f64();
                                    let clock = if hardware_clock { "drm" } else { "dispatch" };
                                    eprintln!("Park scanout: {} {:.9} {observed:.6} {clock}", queued.id.0, presentation_time.as_secs_f64());
                                }
                                let current = queued.power.current(&desktop.server, screen.id, screen.power.epoch)
                                    && queued.scene.locked == desktop.server.locked();
                                if current {
                                    display::observe_adaptive_sync(hardware.outputs.device(), crtc, &mut screen.adaptive_sync, queued.adaptive_sync);
                                    desktop.server.output_adaptive_sync(screen.id, screen.adaptive_sync.clone());
                                    if display::color::observe(hardware.outputs.device(), screen, &queued.color) {
                                        color_fallback = screen.color.status.reason.clone();
                                    }
                                    desktop.server.output_color(screen.id, screen.color.status.clone());
                                    if let Some(message) = power::presented(hardware.outputs.device(), screen, crtc, queued.power.epoch, queued.scene.locked, &mut desktop.server) {
                                        desktop.send(Input::Feedback(message))?;
                                    }
                                } else {
                                    if crate::runtime::tracing_frames() {
                                        eprintln!("Park completion unconfirmed: {} epoch {} current_epoch {} protected {} locked {}",
                                            screen.id.0, queued.power.epoch, screen.power.epoch, queued.scene.locked, desktop.server.locked());
                                    }
                                    if screen.power.on { screen.power.reconfigure(); }
                                    screen.output.reset_buffers();
                                    desktop.redraw = true;
                                }
                                screen.protected = current && queued.scene.locked;
                                let stale = desktop.pending.as_ref().or(desktop.current.as_ref())
                                    .is_some_and(|latest| !Arc::ptr_eq(latest, &queued.frame));
                                desktop.output_presented(
                                    queued.id,
                                    queued.frame,
                                    queued.scene,
                                    presentation_time,
                                    started.elapsed().as_millis() as u32,
                                )?;
                                desktop.redraw |= stale;
                            }
                            screen.pending = false;
                        }
                        if let Some(reason) = color_fallback {
                            if !desktop.server.recover_display_preview(reason.clone()) {
                                hardware.rescan(&mut desktop.server)?;
                                desktop.send(Input::Feedback(format!("HDR output could not be confirmed; SDR was restored. {reason}")))?;
                            }
                            desktop.redraw = true;
                        }
                    }
                    Event::Device(event) if hardware.active && session.is_active() => {
                        let device_id = match event {
                            UdevEvent::Added { device_id, .. }
                            | UdevEvent::Changed { device_id }
                            | UdevEvent::Removed { device_id } => device_id,
                        };
                        if device_id != node.dev_id() {
                            continue;
                        }
                        hardware.rescan(&mut desktop.server)?;
                        desktop.size = hardware.size;
                        desktop.invalidate_outputs()?;
                    }
                    Event::Input(event) if hardware.active && session.is_active() && vt_request.is_none() => {
                        hardware.input(&mut desktop, event, &mut vt_request, &mut vt_keys)?
                    }
                    Event::Error(message) => eprintln!("Park display: {message}"),
                    Event::Presented(..) | Event::Device(_) | Event::Input(_) => {}
                }
            }
            if !session.is_active() {
                hardware.pause(&mut desktop, &mut libinput, &mut vt_keys)?;
                lock_frame_pending = false;
            }
            if let Some(vt) = vt_request {
                match active_vt() {
                    Ok(current) if current == vt => {}
                    Ok(_) => {
                        hardware.pause(&mut desktop, &mut libinput, &mut vt_keys)?;
                        lock_frame_pending = false;
                        if let Err(failure) = session.change_vt(vt) {
                            if session.is_active() { hardware.resume(&mut desktop, &mut libinput)?; }
                            desktop.send(Input::Feedback(format!("Could not switch virtual terminal: {failure}")))?;
                        }
                    }
                    Err(failure) => desktop.send(Input::Feedback(format!("Could not determine the active virtual terminal: {failure}")))?,
                }
            }
            if activation.take_show() {
                desktop.send(Input::Action(DesktopAction::OpenOperation))?;
            }
            desktop.dispatch()?;
            hardware.configure_pointer(&mut desktop)?;
            hardware.configure_requests(&mut desktop)?;
            hardware.configure_power(&mut desktop)?;
            desktop.redraw |= composition.imports_changed();
            desktop.repeat_key(started.elapsed().as_millis() as u32)?;
            if hardware.active {
                let submission_ready = match hardware.flush_prepared(&mut desktop.server) {
                    Ok(invalidated) => { desktop.redraw |= invalidated; true },
                    Err(failure) => {
                        if !desktop.server.recover_display_preview(failure.to_string()) { return Err(failure); }
                        desktop.redraw = true;
                        false
                    }
                };
                if lock_frame_pending && composition.ready() && hardware.screens.values().all(|screen| !screen.power.on || !screen.pending && screen.protected) {
                    desktop.server.lock_presented();
                    lock_frame_pending = false;
                }
                if submission_ready && hardware.configurations.is_empty() && hardware.screens.values().any(|screen| !screen.pending) && composition.ready() {
                desktop.accept(&mut composition, &mut hardware.renderer)?;
                if let Some(completed) = desktop.pending.as_ref().or(desktop.current.as_ref()).cloned() {
                    composition.enable_client_formats(&hardware.renderer, &mut desktop.server, &completed.frame)?;
                }
                if desktop.rendering().is_some() {
                    if hardware.color_ready() && desktop.opening_ready() && let Some(applications) = applications.take() {
                        for application in applications {
                            match desktop.server.spawn_foreground(application, &socket) {
                                Ok(child) => children.push(child),
                                Err(error) => {
                                    let message = format!(
                                        "Could not start the requested application: {error}"
                                    );
                                    eprintln!("{message}");
                                    let _ = desktop.send(Input::Feedback(message));
                                }
                            }
                        }
                    }
                    if desktop.redraw && !hardware.screens.is_empty() {
                        let submitted = match hardware.render(
                            &mut composition,
                            &mut desktop,
                            started.elapsed().as_millis() as u32,
                        ) {
                            Ok(submitted) => submitted,
                            Err(failure) => {
                                if !desktop.server.recover_display_preview(failure.to_string()) { return Err(failure); }
                                desktop.redraw = true;
                                false
                            }
                        };
                        lock_frame_pending = submitted && desktop.server.locked();
                        if lock_frame_pending
                            && composition.ready()
                            && hardware.screens.values().all(|screen| !screen.power.on || !screen.pending && screen.protected)
                        {
                            desktop.server.lock_presented();
                            lock_frame_pending = false;
                        }
                    }
                }
                }
                if hardware.color_ready() {
                    let ready = desktop.ready();
                    readiness.advance(ready == crate::desktop::Ready::Dispatched)?;
                }
            }
            readiness.advance(false)?;
            children.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
            desktop.server.dispatch()?;
            composition.capture(&mut hardware.renderer, &mut desktop, hardware.active)?;
            if desktop.running {
                event_loop
                    .dispatch(desktop.repeat_timeout().into_iter().chain(hardware.configuration_timeout()).chain(hardware.power_timeout()).chain(desktop.server.output_power_timeout()).chain(desktop.server.display_timeout()).chain(desktop.server.control_timeout()).chain(desktop.scene_timeout())
                        .chain(desktop.server.capture_timeout()).chain(composition.capture_timeout()).chain(readiness.timeout()).min(), &mut events)
                    .map_err(error)?;
            }
        }
        Ok(())
    })();
    signal.abort();
    desktop
        .server
        .clear_input(started.elapsed().as_millis() as u32);
    let worker = desktop.worker.stop();
    result?;
    worker?;
    eprintln!("Park orderly logout completed");
    Ok(())
}

impl Hardware {
    fn pause(&mut self, desktop: &mut Desktop, libinput: &mut Libinput, vt_keys: &mut HashSet<u32>) -> io::Result<()> {
        if !self.active { return Ok(()); }
        self.active = false;
        self.devices.active(false);
        vt_keys.clear();
        desktop.server.tablet_reset(crate::desktop::renderer::capture::monotonic().as_millis() as u32);
        for screen in self.screens.values_mut() {
            screen.power.pause(&mut desktop.server, screen.pending && screen.prepared.is_none());
            if let Some(prepared) = screen.prepared.take() { self.retired.push(prepared); }
            screen.adaptive_sync.effective = None;
            screen.adaptive_sync.pending = false;
            screen.adaptive_sync.reason = Some("The physical session is inactive; adaptive sync will be confirmed after resume.".into());
            screen.color.invalidate();
        }
        self.publish_adaptive_sync(&mut desktop.server);
        self.publish_color(&mut desktop.server);
        libinput.suspend();
        self.outputs.pause();
        desktop.event(smithay::backend::winit::WinitEvent::Focus(false))
    }

    fn resume(&mut self, desktop: &mut Desktop, libinput: &mut Libinput) -> io::Result<()> {
        if self.active { return Ok(()); }
        self.outputs.activate(false).map_err(error)?;
        libinput.resume().map_err(|_| io::Error::other("Could not resume input"))?;
        self.active = true;
        self.devices.active(true);
        self.resume_output_requests(&desktop.server);
        for screen in self.screens.values_mut() {
            screen.pending = false;
            screen.protected = false;
            screen.output.reset_buffers();
            screen.power.reconfigure();
        }
        self.rescan(&mut desktop.server)?;
        desktop.size = self.size;
        desktop.invalidate_outputs()?;
        desktop.event(smithay::backend::winit::WinitEvent::Focus(true))
    }

    fn tablet_input<E>(&mut self, desktop: &mut Desktop, event: &E, phase: lince_interface::host::input::TabletPhase)
    where E: smithay::backend::input::TabletToolEvent<LibinputInputBackend> + smithay::reexports::input::event::tablet_tool::TabletToolEventTrait {
        if let Some(report) = self.devices.tablet_event(event, phase)
            && let Some(reason) = desktop.tablet_event(report) { self.devices.tablet_problem(&event.device(), reason); }
    }
    fn configure_pointer(&mut self, desktop: &mut Desktop) -> io::Result<()> {
        let protected = !self.active || desktop.server.locked();
        self.devices.absolute_contacts(desktop.touch_devices());
        if let Some(requests) = &mut desktop.pointer_requests {
            for request in requests.drain(..) { self.devices.request(request, protected); }
        }
        if let Some(requests) = &mut desktop.absolute_requests {
            for request in requests.drain(..) {
                match request {
                    lince_interface::host::input::AbsoluteCommand::Save(request) => self.devices.absolute_request(request, protected),
                    lince_interface::host::input::AbsoluteCommand::Reload(request) => self.devices.absolute_reload(request, protected),
                }
            }
        }
        let idle = desktop.buttons.is_empty() && desktop.grabbed.is_none()
            && !desktop.server.pointer_grabbed() && !desktop.server.dragging();
        self.devices.advance(protected, idle, |settings, expected| desktop.save_pointer(settings, expected));
        self.devices.advance_absolute(protected, |settings, expected| desktop.save_devices(settings, expected), || desktop.load_devices());
        desktop.gesture_support(self.devices.gestures_available())?;
        desktop.absolute_settings(self.devices.absolute_snapshot())?;
        desktop.pointer_settings(self.devices.snapshot())
    }

    fn output_plan(&mut self, server: &Server, preferences: &BTreeMap<String, crate::commands::OutputPreference>, strict: bool) -> io::Result<OutputPlan> {
        let resources = self.outputs.device().resource_handles().map_err(error)?;
        let mut connected = Vec::new();
        let mut catalog = Vec::new();
        let mut controller_choices = Vec::new();
        for connector in resources.connectors() {
            let info = self
                .outputs
                .device()
                .get_connector(*connector, true)
                .map_err(error)?;
            if info.state() != connector::State::Connected {
                continue;
            }
            let name = format!("{}-{}", info.interface().as_str(), info.interface_id());
            let id = *self.identities.entry(name.clone()).or_insert_with(|| {
                let id = OutputId(self.next_id);
                self.next_id += 1;
                id
            });
            let preference = preferences.get(&name).cloned().unwrap_or_default();
            let modes: Vec<_> = info.modes().iter().copied().filter(|mode| {
                let (width, height) = mode.size();
                width > 0 && height > 0 && width <= 16384 && height <= 16384
            }).take(128).collect();
            if modes.is_empty() { continue; }
            catalog.push(crate::server::OutputHead { color: preference.color, adaptive_sync: preference.adaptive_sync, rotation: preference.rotation, id, name: name.clone(),
                modes: modes.iter().map(|mode| {
                    let mode_value = smithay::output::Mode::from(*mode);
                    crate::server::OutputMode { size: [mode_value.size.w, mode_value.size.h], refresh_mhz: mode_value.refresh,
                        preferred: mode.mode_type().contains(ModeTypeFlags::PREFERRED) }
                }).collect(), current: None, position: preference.position.unwrap_or([0, 0]), scale: preference.scale });
            if !preference.enabled { continue; }
            let matches = |mode: &&control::Mode| {
                preference.mode.is_none_or(|size| {
                    let (width, height) = mode.size();
                    size == [u32::from(width), u32::from(height)]
                }) && preference.refresh_mhz.is_none_or(|refresh| {
                    let difference = (i64::from(smithay::output::Mode::from(**mode).refresh) - i64::from(refresh)).abs();
                    if strict { difference == 0 } else { difference <= 500 }
                })
            };
            let Some(mode) = modes.iter().filter(matches)
                .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
                .or_else(|| modes.iter().find(matches))
                .or_else(|| (!strict).then(|| modes.iter().find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED)).or_else(|| modes.first())).flatten()).copied()
            else {
                if preference.mode.is_some() || preference.refresh_mhz.is_some() {
                    return Err(io::Error::other(format!("Requested mode is unavailable on {name}")));
                }
                continue;
            };
            let existing = self
                .screens
                .iter()
                .find(|(_, screen)| screen.connector == *connector)
                .map(|(crtc, _)| *crtc);
            let mut choices = Vec::new();
            for encoder in info.encoders() {
                let encoder = self.outputs.device().get_encoder(*encoder).map_err(error)?;
                for crtc in resources.filter_crtcs(encoder.possible_crtcs()) {
                    if !choices.contains(&crtc) { choices.push(crtc); }
                }
            }
            if let Some(index) = existing.and_then(|existing| choices.iter().position(|crtc| *crtc == existing)) { choices.swap(0, index); }
            let Some(crtc) = choices.first().copied() else {
                if strict { return Err(io::Error::other(format!("No display controller is available for {name}"))); }
                continue;
            };
            catalog.last_mut().unwrap().current = modes.iter().position(|current| *current == mode);
            connected.push((crtc, info, mode, id, name));
            controller_choices.push(choices);
        }
        let assignments = display::assign_controllers(&controller_choices);
        let mut assigned = Vec::new();
        for (mut connected, assignment) in connected.into_iter().zip(assignments) {
            if let Some(crtc) = assignment { connected.0 = crtc; assigned.push(connected); }
            else if strict { return Err(io::Error::other("The requested outputs cannot share the available display controllers")); }
            else if let Some(head) = catalog.iter_mut().find(|head| head.id == connected.3) { head.current = None; }
        }
        let connected = assigned;
        if connected.is_empty() {
            let plan = OutputPlan { screens: Vec::new(), catalog, size: self.size, strict };
            plan.validate()?;
            return Ok(plan);
        }
        let mut x = 0_i32;
        let mut height = 0_i32;
        let monitors: Vec<_> = connected
            .iter()
            .map(|(_, _, mode, id, name)| {
                let (width, current_height) = mode.size();
                let preference = preferences.get(name).cloned().unwrap_or_default();
                let source = preference.rotation.source_size([u32::from(width), u32::from(current_height)]);
                let logical_width = (f64::from(source[0]) / preference.scale).round() as i32;
                let logical_height = (f64::from(source[1]) / preference.scale).round() as i32;
                let origin = preference.position.unwrap_or([x, 0]);
                let monitor = Monitor {
                    id: *id,
                    name: name.clone(),
                    origin,
                    size: [i32::from(width), i32::from(current_height)],
                };
                x = x.max(origin[0] + logical_width);
                height = height.max(origin[1] + logical_height);
                monitor
            })
            .collect();
        if x <= 0 || height <= 0 || x > 16384 || height > 16384 {
            return Err(io::Error::other("Combined logical output bounds exceed Park's supported size"));
        }
        let mut screens = Vec::new();
        for ((crtc, info, mode, id, _), monitor) in connected.into_iter().zip(&monitors) {
            let scale = preferences.get(&monitor.name).map_or(1.0, |preference| preference.scale);
            let rotation = preferences.get(&monitor.name).map_or(Default::default(), |preference| preference.rotation);
            let output = server.output(id).filter(|output| output.name() == monitor.name).unwrap_or_else(|| {
                smithay::output::Output::new(monitor.name.clone(), smithay::output::PhysicalProperties {
                    size: (0, 0).into(), subpixel: smithay::output::Subpixel::Unknown, make: "Lince".into(), model: "Park".into(),
                })
            });
            let actual_mode = smithay::output::Mode::from(mode);
            if output.current_mode().is_none() {
                output.change_current_state(Some(actual_mode), Some(crate::server::output_transform(rotation)), Some(smithay::output::Scale::Fractional(scale)),
                    Some((monitor.origin[0], monitor.origin[1]).into()));
                output.set_preferred(actual_mode);
            }
            if let Some(head) = catalog.iter_mut().find(|head| head.id == id) { head.position = monitor.origin; }
            let adaptive_sync = preferences.get(&monitor.name).is_some_and(|preference| preference.adaptive_sync);
            let color = preferences.get(&monitor.name).map_or(Default::default(), |preference| preference.color);
            screens.push(PlannedOutput { crtc, connector: info.handle(), mode, id, output, monitor: monitor.clone(), scale, rotation, adaptive_sync, color, color_restore: None });
        }
        let size = [x as u32, height as u32];
        let plan = OutputPlan { screens, catalog, size, strict };
        plan.validate()?;
        Ok(plan)
    }

    fn render(
        &mut self,
        composition: &mut Composition,
        desktop: &mut Desktop,
        time: u32,
    ) -> io::Result<bool> {
        let Some(completed) = desktop
            .pending
            .as_ref()
            .or(desktop.current.as_ref())
            .cloned()
        else {
            return Ok(false);
        };
        if self.native_color.is_none() {
            let ready = composition.linear_color_available()
                && completed.frame.encoding == lince_interface::host::color::NativeEncoding::LinearRelative;
            self.native_color = Some(ready);
            if ready { desktop.server.enable_hdr_color_management()?; }
            else { desktop.server.enable_sdr_color_management(); }
            self.rescan(&mut desktop.server)?;
        }
        let viewports = desktop.server.output_viewports();
        for screen in self.screens.values_mut() {
            if !screen.power.on { continue; }
            if screen.pending { continue; }
            let Some((_, viewport)) = viewports.iter().find(|(_, viewport)| viewport.id == screen.id) else { continue };
            let source = viewport.source_size();
            let size = (source[0] as i32, source[1] as i32);
            let format = screen.color.format();
            if screen.texture.as_ref().is_none_or(|texture| texture.size().w != size.0 || texture.size().h != size.1 || texture.format() != Some(format)) {
                screen.texture = Some(self.renderer.create_buffer(format, size.into()).map_err(error)?);
            }
            let texture = screen.texture.as_mut().unwrap();
            let presented = {
                let mut target = self.renderer.bind(texture).map_err(error)?;
                composition.render_output(&mut self.renderer, &mut target, viewport, &mut desktop.server, Some(completed.as_ref()), desktop.pointer, screen.color.output())?
            };
            let presented = Arc::new(presented);
            let element = crate::desktop::renderer::output_element(&self.renderer, texture.clone(), viewport.raster.scale[0]);
            let elements = [element];
            let frame = screen
                .output
                .render_frame(
                    &mut self.renderer,
                    &elements,
                    smithay::backend::renderer::Color32F::BLACK,
                    FrameFlags::empty(),
                )
                .map_err(error)?;
            if !frame.is_empty {
                let needs_sync = frame.needs_sync();
                let rendered_sync = if let smithay::backend::drm::compositor::PrimaryPlaneElement::Swapchain(
                        element,
                    ) = &frame.primary_element
                {
                    Some(element.sync.clone())
                } else { None };
                drop(frame);
                let queued = Queued {
                        id: screen.id,
                        frame: completed.clone(),
                        scene: presented.clone(),
                        adaptive_sync: screen.adaptive_sync.prepared,
                        color: screen.color.submitted(),
                        power: power::Frame::capture(&desktop.server, screen.id, screen.power.epoch)
                            .ok_or_else(|| io::Error::other("The rendered output owner was retired before submission."))?,
                    };
                if !power::may_submit(screen, &desktop.server)
                    || !queued.power.current(&desktop.server, screen.id, screen.power.epoch)
                    || queued.scene.locked != desktop.server.locked()
                {
                    let sync = rendered_sync.unwrap_or_else(|| presented.sync.clone());
                    if !sync.is_reached() {
                        let watch = composition.watch(sync.clone())?;
                        self.retired.push(Prepared { frame: queued, sync, _watch: watch });
                    }
                    power::cancel_unsent(screen, &mut desktop.server);
                    continue;
                }
                if let Some(sync) = rendered_sync.filter(|sync| needs_sync && !sync.is_reached()) {
                    let watch = composition.watch(sync.clone())?;
                    screen.prepared = Some(Prepared { frame: queued, sync, _watch: watch });
                } else {
                    screen.output.queue_frame(queued).map_err(error)?;
                }
                screen.pending = true;
            } else {
                screen.protected = presented.locked;
                desktop.output_presented(screen.id, completed.clone(), presented.clone(), crate::desktop::renderer::capture::monotonic(), time)?;
            }
        }
        desktop.redraw = false;
        Ok(true)
    }

    fn flush_prepared(&mut self, server: &mut Server) -> io::Result<bool> {
        let mut invalidated = false;
        self.retired.retain(|prepared| !prepared.sync.is_reached());
        for screen in self.screens.values_mut() {
            if screen.prepared.is_some() && !power::may_submit(screen, server) {
                self.retired.push(screen.prepared.take().unwrap());
                power::cancel_unsent(screen, server);
                continue;
            }
            if screen.prepared.as_ref().is_some_and(|prepared| !prepared.frame.power.current(server, screen.id, screen.power.epoch)) {
                let prepared = screen.prepared.take().unwrap();
                if crate::runtime::tracing_frames() {
                    eprintln!("Park prepared frame retired: {} epoch {} current_epoch {} protected {} locked {}",
                        screen.id.0, prepared.frame.power.epoch, screen.power.epoch, prepared.frame.scene.locked, server.locked());
                }
                self.retired.push(prepared);
                screen.pending = false;
                screen.protected = false;
                screen.output.reset_buffers();
                invalidated = true;
                continue;
            }
            if screen.prepared.as_ref().is_some_and(|prepared| prepared.sync.is_reached()) {
                let prepared = screen.prepared.take().unwrap();
                if let Err(failure) = screen.output.queue_frame(prepared.frame) {
                    screen.pending = false;
                    return Err(error(failure));
                }
            }
        }
        Ok(invalidated)
    }

    fn input(
        &mut self,
        desktop: &mut Desktop,
        event: InputEvent<LibinputInputBackend>,
        vt_request: &mut Option<i32>,
        vt_keys: &mut HashSet<u32>,
    ) -> io::Result<()> {
        desktop.flush_touch()?;
        self.devices.absolute_contacts(desktop.touch_devices());
        if matches!(&event, InputEvent::Keyboard { .. } | InputEvent::PointerMotion { .. }
            | InputEvent::PointerMotionAbsolute { .. } | InputEvent::PointerButton { .. }
            | InputEvent::PointerAxis { .. } | InputEvent::GestureSwipeBegin { .. }
            | InputEvent::GestureSwipeUpdate { .. } | InputEvent::GestureSwipeEnd { .. }
            | InputEvent::GesturePinchBegin { .. } | InputEvent::GesturePinchUpdate { .. }
            | InputEvent::GesturePinchEnd { .. } | InputEvent::GestureHoldBegin { .. }
            | InputEvent::GestureHoldEnd { .. } | InputEvent::TouchDown { .. } | InputEvent::TouchMotion { .. }
            | InputEvent::TouchUp { .. } | InputEvent::TabletToolAxis { .. } | InputEvent::TabletToolProximity { .. }
            | InputEvent::TabletToolTip { .. } | InputEvent::TabletToolButton { .. }) { desktop.server.activity(); }
        match event {
            InputEvent::DeviceAdded { device } => {
                if let Some(generation) = self.devices.generation(device.sysname()) {
                    desktop.tablet_device_removed(device.sysname(), generation, crate::desktop::renderer::capture::monotonic().as_millis() as u32);
                }
                desktop.touch_device_removed(device.sysname());
                self.devices.added(device.clone());
                if device.has_capability(smithay::reexports::input::DeviceCapability::TabletTool)
                    && let Some(snapshot) = self.devices.absolute_device(&device)
                    && !desktop.server.tablet_device(snapshot.generation, smithay::wayland::tablet_manager::TabletDescriptor::from(&device)) {
                    self.devices.tablet_problem(&device, "Park's tablet inventory is full; this device is unavailable.");
                }
            }
            InputEvent::DeviceRemoved { device } => {
                if !self.devices.registered(&device) { return Ok(()); }
                desktop.touch_device_removed(device.sysname());
                if let Some(snapshot) = self.devices.absolute_device(&device) {
                    desktop.tablet_device_removed(device.sysname(), snapshot.generation, crate::desktop::renderer::capture::monotonic().as_millis() as u32);
                }
                desktop.cancel_device_gestures(device.sysname(), crate::desktop::renderer::capture::monotonic().as_millis() as u32);
                self.devices.removed(device.sysname());
            }
            InputEvent::TouchDown { event } => {
                if let Some(device) = self.devices.absolute_device(&event.device()) {
                    let point = event.position_transformed((1, 1).into());
                    desktop.touch_down(&device, event.slot().into(), [point.x, point.y], event.time_msec());
                }
            }
            InputEvent::TouchMotion { event } => {
                if let Some(device) = self.devices.absolute_device(&event.device()) {
                    let point = event.position_transformed((1, 1).into());
                    desktop.touch_motion(&device, event.slot().into(), [point.x, point.y], event.time_msec());
                }
            }
            InputEvent::TouchUp { event } => {
                if let Some(device) = self.devices.absolute_device(&event.device()) { desktop.touch_up(&device, event.slot().into(), event.time_msec()); }
            }
            InputEvent::TouchCancel { event } => { if self.devices.registered(&event.device()) { desktop.cancel_touch(); } }
            InputEvent::TouchFrame { event } => { if self.devices.registered(&event.device()) { desktop.server.touch_frame(); } }
            InputEvent::TabletToolProximity { event } => {
                let phase = if event.state() == ProximityState::In { lince_interface::host::input::TabletPhase::Enter } else { lince_interface::host::input::TabletPhase::Leave };
                self.tablet_input(desktop, &event, phase);
            }
            InputEvent::TabletToolAxis { event } => {
                self.tablet_input(desktop, &event, lince_interface::host::input::TabletPhase::Motion);
            }
            InputEvent::TabletToolTip { event } => {
                let phase = lince_interface::host::input::TabletPhase::Tip(TabletToolTipEvent::tip_state(&event) == TabletToolTipState::Down);
                self.tablet_input(desktop, &event, phase);
            }
            InputEvent::TabletToolButton { event } => {
                let phase = lince_interface::host::input::TabletPhase::Button { code: event.button(), pressed: TabletToolButtonEvent::button_state(&event) == ButtonState::Pressed };
                self.tablet_input(desktop, &event, phase);
            }
            InputEvent::GestureSwipeBegin { event } => {
                self.devices.gesture(event.device().sysname(), 0, true);
                desktop.gesture_swipe_begin(event.device().sysname(), event.fingers(), event.time_msec());
            }
            InputEvent::GestureSwipeUpdate { event } => {
                desktop.gesture_swipe_update(event.device().sysname(), [event.delta_x(), event.delta_y()], event.time_msec());
            }
            InputEvent::GestureSwipeEnd { event } => {
                desktop.gesture_swipe_end(event.device().sysname(), event.cancelled(), event.time_msec());
                self.devices.gesture(event.device().sysname(), 0, false);
            }
            InputEvent::GesturePinchBegin { event } => {
                self.devices.gesture(event.device().sysname(), 1, true);
                desktop.gesture_pinch_begin(event.device().sysname(), event.fingers(), event.time_msec());
            }
            InputEvent::GesturePinchUpdate { event } => {
                desktop.gesture_pinch_update(event.device().sysname(), [event.delta_x(), event.delta_y()], event.scale(), event.rotation(), event.time_msec());
            }
            InputEvent::GesturePinchEnd { event } => {
                desktop.gesture_pinch_end(event.device().sysname(), event.cancelled(), event.time_msec());
                self.devices.gesture(event.device().sysname(), 1, false);
            }
            InputEvent::GestureHoldBegin { event } => {
                self.devices.gesture(event.device().sysname(), 2, true);
                desktop.gesture_hold_begin(event.device().sysname(), event.fingers(), event.time_msec());
            }
            InputEvent::GestureHoldEnd { event } => {
                desktop.gesture_hold_end(event.device().sysname(), event.cancelled(), event.time_msec());
                self.devices.gesture(event.device().sysname(), 2, false);
            }
            InputEvent::Keyboard { event } => {
                let code = event.key_code().raw().saturating_sub(8);
                let pressed = event.state() == KeyState::Pressed;
                if !pressed && vt_keys.remove(&code) {
                    return Ok(());
                }
                let symbol = desktop.server.key_symbol(code)?;
                if pressed && symbol.modifiers.ctrl && symbol.modifiers.alt {
                    let vt = match code {
                        59..=68 => Some(code - 58),
                        87 => Some(11),
                        88 => Some(12),
                        _ => None,
                    };
                    if let Some(vt) = vt {
                        vt_keys.insert(code);
                        *vt_request = Some(vt as i32);
                        return Ok(());
                    }
                }
                desktop.key(code, pressed, event.time_msec())?;
            }
            InputEvent::PointerMotion { event } => {
                let delta = event.delta();
                let unaccelerated = event.delta_unaccel();
                desktop.relative_motion([delta.x, delta.y], [unaccelerated.x, unaccelerated.y], event.time());
                let displayed = desktop.displayed_pointer();
                let mut proposed = [displayed[0] + delta.x, displayed[1] + delta.y];
                self.clamp_pointer(&mut proposed);
                desktop.move_pointer(proposed, event.time_msec())?;
                desktop.redraw = true;
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let point =
                    event.position_transformed((self.size[0] as i32, self.size[1] as i32).into());
                let mut proposed = [point.x, point.y];
                self.clamp_pointer(&mut proposed);
                desktop.move_pointer(proposed, event.time_msec())?;
                desktop.redraw = true;
            }
            InputEvent::PointerButton { event } => desktop.button(
                event.button_code(),
                event.state() == ButtonState::Pressed,
                event.time_msec(),
            )?,
            InputEvent::PointerAxis { event } => {
                let mut delta = [0.0; 2];
                let mut frame = AxisFrame::new(event.time_msec()).source(event.source());
                for (index, axis) in [Axis::Horizontal, Axis::Vertical].into_iter().enumerate() {
                    if event.source() == AxisSource::Finger && let Some(amount) = event.amount(axis) {
                        self.devices.scroll(event.device().sysname(), index as u8, amount != 0.0);
                    }
                    delta[index] = event
                        .amount(axis)
                        .unwrap_or_else(|| event.amount_v120(axis).unwrap_or(0.0) * 15.0 / 120.0);
                    frame = frame.value(axis, delta[index]);
                    if let Some(value) = event.amount_v120(axis) {
                        frame = frame.v120(axis, value as i32);
                    }
                    if delta[index] == 0.0 && event.source() == AxisSource::Finger {
                        frame = frame.stop(axis);
                    }
                }
                desktop.axis(delta, frame)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn clamp_pointer(&self, point: &mut [f64; 2]) {
        let nearest = self
            .screens
            .values()
            .map(|screen| {
                let [width, height] = screen.logical_size;
                let nearest = [
                    point[0].clamp(
                        f64::from(screen.origin[0]),
                        f64::from(screen.origin[0]) + f64::from(width) - 1.0,
                    ),
                    point[1].clamp(
                        f64::from(screen.origin[1]),
                        f64::from(screen.origin[1]) + f64::from(height) - 1.0,
                    ),
                ];
                let distance = (nearest[0] - point[0]).powi(2) + (nearest[1] - point[1]).powi(2);
                (nearest, distance)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((nearest, _)) = nearest {
            *point = nearest;
        }
    }
}

impl Drop for Hardware {
    fn drop(&mut self) {
        for prepared in self.retired.iter().chain(self.screens.values().filter_map(|screen| screen.prepared.as_ref())) {
            if !prepared.sync.is_reached() { let _ = prepared.sync.wait(); }
        }
    }
}
