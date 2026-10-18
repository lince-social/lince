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
        },
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            Bind, Offscreen,
            element::{
                Kind,
                texture::{TextureBuffer, TextureRenderElement},
            },
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
    utils::{DeviceFd, Transform},
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
use display::{OutputPlan, PlannedOutput};

struct Queued {
    id: OutputId,
    frame: Arc<Completed>,
    scene: Arc<crate::desktop::renderer::Presented>,
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
    Presented(crtc::Handle, std::time::Duration),
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
}

struct Prepared {
    frame: Queued,
    sync: smithay::backend::renderer::sync::SyncPoint,
    _watch: crate::graphics::wait::Watch,
}

struct Hardware {
    catalog: Vec<crate::server::OutputHead>,
    configurations: std::collections::VecDeque<(crate::server::OutputRequest, Instant)>,
    retired: Vec<Prepared>,
    preferences: BTreeMap<String, crate::commands::OutputPreference>,
    outputs: Outputs,
    renderer: GlesRenderer,
    screens: HashMap<crtc::Handle, Screen>,
    identities: BTreeMap<String, OutputId>,
    next_id: u64,
    size: [u32; 2],
    texture: Option<GlesTexture>,
    active: bool,
}

pub use crate::environment::{environment_ready, reexec_environment};

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
    let renderer = unsafe { GlesRenderer::new(context) }.map_err(error)?;
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
        [Fourcc::Argb8888, Fourcc::Xrgb8888],
        formats,
    );
    let mut hardware = Hardware {
        catalog: Vec::new(),
        configurations: std::collections::VecDeque::new(),
        retired: Vec::new(),
        preferences: crate::commands::Commands::load(&configuration.directory.join("park.json"))?.outputs,
        outputs,
        renderer,
        screens: HashMap::new(),
        identities: BTreeMap::new(),
        next_id: 1,
        size: configuration.size,
        texture: None,
        active: true,
    };
    let mut server = Server::new([configuration.size[0] as i32, configuration.size[1] as i32])?;
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
                    smithay::backend::drm::DrmEventTime::Monotonic(time) => Some(time),
                    smithay::backend::drm::DrmEventTime::Realtime(_) => None,
                }).unwrap_or_else(crate::desktop::renderer::capture::monotonic);
                events.push(Event::Presented(crtc, time));
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
    let worker = CanvasWorker::spawn_notifying(
        Configuration {
            size: hardware.size,
            ..configuration
        },
        Some(wake.clone()),
    )?;
    let mut desktop = Desktop::new(server, worker, hardware.size, hardware.screens.len(), true);
    desktop.commands(command_file, socket.clone());
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
    let mut composition = Composition::new(wake.clone())?;
    let mut applications = Some(applications);
    let mut children = Vec::new();
    let mut events = Vec::new();
    let mut vt_keys = HashSet::new();
    let mut lock_frame_pending = false;
    let mut environment_ready = false;
    let mut logout_requested = false;
    let started = Instant::now();
    eprintln!("Park Wayland display: {}", socket.display());
    let result: io::Result<()> = (|| {
        while desktop.running {
            if stopped.load(Ordering::Acquire) && !logout_requested {
                desktop.send(Input::Logout)?;
                logout_requested = true;
            }
            for event in events.drain(..) {
                match event {
                    Event::Session(SessionEvent::PauseSession) => {
                        hardware.active = false;
                        for screen in hardware.screens.values_mut() {
                            if let Some(prepared) = screen.prepared.take() { hardware.retired.push(prepared); }
                        }
                        libinput.suspend();
                        hardware.outputs.pause();
                        desktop.event(smithay::backend::winit::WinitEvent::Focus(false))?;
                    }
                    Event::Session(SessionEvent::ActivateSession) => {
                        hardware.outputs.activate(false).map_err(error)?;
                        libinput
                            .resume()
                            .map_err(|_| io::Error::other("Could not resume input"))?;
                        hardware.active = true;
                        for screen in hardware.screens.values_mut() {
                            screen.pending = false;
                            screen.protected = false;
                            screen.output.reset_buffers();
                        }
                        hardware.rescan(&mut desktop.server)?;
                        desktop.size = hardware.size;
                        desktop.invalidate_outputs()?;
                        desktop.event(smithay::backend::winit::WinitEvent::Focus(true))?;
                    }
                    Event::Presented(crtc, presentation_time) => {
                        if let Some(screen) = hardware.screens.get_mut(&crtc) {
                            if let Some(queued) = screen.output.frame_submitted().map_err(error)? {
                                screen.protected = queued.scene.locked;
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
                    }
                    Event::Device(event) if hardware.active => {
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
                    Event::Input(event) if hardware.active => {
                        hardware.input(&mut desktop, event, &mut session, &mut vt_keys)?
                    }
                    Event::Error(message) => eprintln!("Park display: {message}"),
                    Event::Device(_) | Event::Input(_) => {}
                }
            }
            if activation.take_show() {
                desktop.send(Input::Action(DesktopAction::OpenOperation))?;
            }
            desktop.dispatch()?;
            hardware.configure_requests(&mut desktop)?;
            desktop.redraw |= composition.imports_changed();
            desktop.repeat_key(started.elapsed().as_millis() as u32)?;
            if hardware.active {
                hardware.flush_prepared()?;
                if lock_frame_pending && composition.ready() && hardware.screens.values().all(|screen| !screen.pending && screen.protected) {
                    desktop.server.lock_presented();
                    lock_frame_pending = false;
                }
                if hardware.screens.values().any(|screen| !screen.pending) && composition.ready() {
                desktop.accept(&mut composition, &mut hardware.renderer)?;
                if desktop.rendering().is_some() {
                    if let Some(applications) = applications.take() {
                        desktop.server.enable_dmabuf();
                        for application in applications {
                            match application.spawn(&socket) {
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
                        let submitted = hardware.render(
                            &mut composition,
                            &mut desktop,
                            started.elapsed().as_millis() as u32,
                        )?;
                        lock_frame_pending = submitted && desktop.server.locked();
                        if lock_frame_pending
                            && composition.ready()
                            && hardware.screens.values().all(|screen| !screen.pending && screen.protected)
                        {
                            desktop.server.lock_presented();
                            lock_frame_pending = false;
                        }
                        if submitted && !environment_ready {
                            desktop.ready();
                            environment_ready = true;
                        }
                    }
                }
                }
            }
            children.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
            desktop.server.dispatch()?;
            composition.capture(&mut hardware.renderer, &mut desktop, hardware.active)?;
            if desktop.running {
                event_loop
                    .dispatch(desktop.repeat_timeout().into_iter().chain(hardware.configuration_timeout())
                        .chain(desktop.server.capture_timeout()).chain(composition.capture_timeout()).min(), &mut events)
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
            catalog.push(crate::server::OutputHead { id, name: name.clone(),
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
            let plan = OutputPlan { screens: Vec::new(), catalog, size: self.size };
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
                let logical_width = (f64::from(width) / preference.scale).round() as i32;
                let logical_height = (f64::from(current_height) / preference.scale).round() as i32;
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
            let output = server.output(id).filter(|output| output.name() == monitor.name).unwrap_or_else(|| {
                smithay::output::Output::new(monitor.name.clone(), smithay::output::PhysicalProperties {
                    size: (0, 0).into(), subpixel: smithay::output::Subpixel::Unknown, make: "Lince".into(), model: "Park".into(),
                })
            });
            let actual_mode = smithay::output::Mode::from(mode);
            if output.current_mode().is_none() {
                output.change_current_state(Some(actual_mode), Some(Transform::Normal), Some(smithay::output::Scale::Fractional(scale)),
                    Some((monitor.origin[0], monitor.origin[1]).into()));
                output.set_preferred(actual_mode);
            }
            if let Some(head) = catalog.iter_mut().find(|head| head.id == id) { head.position = monitor.origin; }
            screens.push(PlannedOutput { crtc, connector: info.handle(), mode, id, output, monitor: monitor.clone(), scale });
        }
        let size = [x as u32, height as u32];
        let plan = OutputPlan { screens, catalog, size };
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
        if self.texture.is_none() {
            self.texture = Some(
                self.renderer
                    .create_buffer(
                        Fourcc::Argb8888,
                        (self.size[0] as i32, self.size[1] as i32).into(),
                    )
                    .map_err(error)?,
            );
        }
        let texture = self.texture.as_mut().unwrap();
        let presented = {
            let mut target = self.renderer.bind(texture).map_err(error)?;
            composition.render(
                &mut self.renderer,
                &mut target,
                (self.size[0] as i32, self.size[1] as i32).into(),
                Transform::Normal,
                &mut desktop.server,
                Some(completed.as_ref()),
                desktop.pointer,
            )?
        };
        let presented = Arc::new(presented);
        let buffer = TextureBuffer::from_texture(
            &self.renderer,
            texture.clone(),
            1,
            Transform::Normal,
            None,
        );
        for screen in self.screens.values_mut() {
            if screen.pending { continue; }
            let [width, height] = screen.logical_size;
            let element = TextureRenderElement::from_texture_buffer(
                (0.0, 0.0),
                &buffer,
                None,
                Some(smithay::utils::Rectangle::new(
                    (f64::from(screen.origin[0]), f64::from(screen.origin[1])).into(),
                    (f64::from(width), f64::from(height)).into(),
                )),
                Some((width, height).into()),
                Kind::Unspecified,
            );
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
                let sync = if frame.needs_sync()
                    && let smithay::backend::drm::compositor::PrimaryPlaneElement::Swapchain(
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
                    };
                if let Some(sync) = sync.filter(|sync| !sync.is_reached()) {
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

    fn flush_prepared(&mut self) -> io::Result<()> {
        self.retired.retain(|prepared| !prepared.sync.is_reached());
        for screen in self.screens.values_mut() {
            if screen.prepared.as_ref().is_some_and(|prepared| prepared.sync.is_reached()) {
                let prepared = screen.prepared.take().unwrap();
                screen.output.queue_frame(prepared.frame).map_err(error)?;
            }
        }
        Ok(())
    }

    fn input(
        &mut self,
        desktop: &mut Desktop,
        event: InputEvent<LibinputInputBackend>,
        session: &mut LibSeatSession,
        vt_keys: &mut HashSet<u32>,
    ) -> io::Result<()> {
        if matches!(&event, InputEvent::Keyboard { .. } | InputEvent::PointerMotion { .. }
            | InputEvent::PointerMotionAbsolute { .. } | InputEvent::PointerButton { .. }
            | InputEvent::PointerAxis { .. }) { desktop.server.activity(); }
        match event {
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
                        session.change_vt(vt as i32).map_err(error)?;
                        return Ok(());
                    }
                }
                desktop.key(code, pressed, event.time_msec())?;
            }
            InputEvent::PointerMotion { event } => {
                let delta = event.delta();
                let unaccelerated = event.delta_unaccel();
                desktop.relative_motion([delta.x, delta.y], [unaccelerated.x, unaccelerated.y], event.time());
                let mut proposed = [desktop.pointer[0] + delta.x, desktop.pointer[1] + delta.y];
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
