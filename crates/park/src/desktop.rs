use crate::{
    launch::Application,
    runtime::{CanvasWorker, Completed, Configuration, Input, Scene},
    server::{LayerSnapshot, Server},
};
use lince_interface::host::{
    DesktopAction, FileDragContext, HostRequest, ItemId, OutputId, OutputViewport, OverviewHoverRequest, Rectangle,
};
use smithay::{
    backend::{
        renderer::gles::GlesRenderer,
        winit::{self, WinitEvent},
    },
    reexports::{
        wayland_server::protocol::wl_surface::WlSurface,
        winit::{dpi::LogicalSize, window::WindowAttributes},
    },
    utils::SERIAL_COUNTER,
    wayland::shell::wlr_layer::{KeyboardInteractivity, Layer},
};
use std::{
    collections::{HashMap, HashSet},
    io,
    process::Child,
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "nested/compose.rs"]
mod compose;
#[path = "nested/input.rs"]
pub(crate) mod input;
#[path = "nested/renderer.rs"]
pub(crate) mod renderer;
#[path = "nested/smoke.rs"]
mod smoke;
mod manipulation;
mod file_drag;
mod focus_policy;
mod control;
mod magnification;
mod mapping;
#[cfg(feature = "session")]
mod absolute;
#[cfg(feature = "session")]
mod tablet;
mod outputs;
#[cfg(feature = "session")]
mod gestures;

#[derive(Default)]
pub struct Options {
    pub duration: Option<Duration>,
    pub smoke: bool,
    pub outputs: usize,
    pub ready_hook: bool,
    pub activation: Option<lince_interface::activation::InstanceEvents>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Ready {
    #[default]
    Waiting,
    Dispatched,
    Failed,
}

pub(crate) struct Desktop {
    pub(crate) server: Server,
    pub(crate) worker: CanvasWorker,
    pub(crate) size: [u32; 2],
    pub(crate) output_count: usize,
    pub(crate) current: Option<Arc<Completed>>,
    pub(crate) pending: Option<Arc<Completed>>,
    output_frames: HashMap<OutputId, (Arc<Completed>, Arc<renderer::Presented>)>,
    capture_times: HashMap<OutputId, Duration>,
    retired_sequence: u64,
    scene_serial: u64,
    pub(crate) layers: Vec<LayerSnapshot>,
    pub(crate) popups: Vec<renderer::PopupScene>,
    pub(crate) clips: Vec<(OutputId, Rectangle)>,
    pub(crate) pointer: [f64; 2],
    display_pointer: Option<[f64; 2]>,
    pub(crate) pointer_sent: Option<[f64; 2]>,
    pub(crate) pointer_stamp: u64,
    pub(crate) pointer_epoch: u64,
    pub(crate) pointer_target: Option<(WlSurface, [f64; 2])>,
    pub(crate) grabbed: Option<input::Target>,
    pub(crate) buttons: HashSet<u32>,
    manipulation: manipulation::State,
    #[cfg(feature = "session")]
    gesture_devices: [Option<String>; 3],
    #[cfg(feature = "session")]
    gestures: gestures::State,
    pub(crate) keys: HashMap<u32, crate::server::KeyEvent>,
    pub(crate) consumed: HashSet<u32>,
    pub(crate) consumed_actions: HashMap<u32, DesktopAction>,
    pub(crate) modal_keys: HashSet<u32>,
    pub(crate) native_keyboard: bool,
    pub(crate) focused: bool,
    pub(crate) layer_focus: Option<WlSurface>,
    pub(crate) pending_focus: Option<(u64, Option<lince_interface::host::SurfaceId>)>,
    pub(crate) pending_output: Option<(u64, Option<OutputId>)>,
    pub(crate) deferred: std::collections::VecDeque<input::Deferred>,
    pub(crate) scene_revision: u64,
    pub(crate) server_revision: u64,
    pub(crate) viewports: Vec<OutputViewport>,
    display: Option<lince_interface::host::display::DisplaySnapshot>,
    keyboard_snapshot: Option<lince_interface::host::input::KeyboardSnapshot>,
    pointer_snapshot: Option<lince_interface::host::input::PointerSnapshot>,
    absolute_snapshot: Option<lince_interface::host::input::AbsoluteSnapshot>,
    absolute_sent: Option<lince_interface::host::input::AbsoluteSnapshot>,
    #[cfg(feature = "session")]
    absolute: absolute::State,
    #[cfg(feature = "session")]
    tablets: tablet::State,
    control_status: Option<lince_interface::host::control::Status>,
    capture_gate_pending: bool,
    capture_status: Option<lince_interface::host::capture::Status>,
    capture_demand: Option<lince_interface::host::capture::Demand>,
    logout_pending: bool,
    external: crate::external_commands::ExternalCommands,
    magnification: crate::magnification::State,
    magnification_sent: Option<lince_interface::host::magnification::Snapshot>,
    magnification_caret: Option<lince_interface::host::magnification::Caret>,
    #[cfg(feature = "session")]
    pub(crate) pointer_requests: Option<std::collections::VecDeque<lince_interface::host::input::PointerRequest>>,
    #[cfg(feature = "session")]
    pub(crate) absolute_requests: Option<std::collections::VecDeque<lince_interface::host::input::AbsoluteCommand>>,
    pub(crate) redraw: bool,
    pub(crate) running: bool,
    command_file: Option<std::path::PathBuf>,
    socket: Option<std::path::PathBuf>,
    children: Vec<Child>,
    ready_state: Ready,
    locked: bool,
    file_drag_sent: Option<FileDragContext>,
    drag_active_sent: bool,
    hover_reply: Option<(OverviewHoverRequest, bool)>,
    pointer_focus_item: Option<lince_interface::host::input::FocusTarget>,
    cursor_warp_sequence: u64,
    pub(crate) compose: compose::Compose,
    #[cfg(feature = "session")]
    repeat: Option<(u32, Instant)>,
    #[cfg(feature = "session")]
    keyboard: crate::commands::Keyboard,
}

#[derive(Debug)]
pub struct Report {
    pub frames: u64,
    pub applications: usize,
    pub layers: usize,
    pub elapsed: Duration,
    pub checked: bool,
}

pub fn run(
    configuration: Configuration,
    applications: Vec<Application>,
    options: Options,
) -> io::Result<Report> {
    let attributes = WindowAttributes::default()
        .with_title("Lince Park — nested development")
        .with_inner_size(LogicalSize::new(
            configuration.size[0],
            configuration.size[1],
        ));
    let (mut backend, events) =
        winit::init_from_attributes::<GlesRenderer>(attributes).map_err(error)?;
    use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic, ping::make_ping};
    let mut event_loop = EventLoop::<Vec<WinitEvent>>::try_new().map_err(error)?;
    let handle = event_loop.handle();
    handle.insert_source(events, |event, _, pending| pending.push(event)).map_err(error)?;
    let (ping, source) = make_ping().map_err(error)?;
    handle.insert_source(source, |_, _, _| {}).map_err(error)?;
    let wake = lince_interface::host::WakeSignal::new(move || ping.ping());
    if let Some(activation) = &options.activation {
        activation.attach(wake.clone());
    }
    let physical = backend.window_size();
    let size = [physical.w as u32, physical.h as u32];
    let mut server = Server::new([physical.w, physical.h])?;
    let output_count = options.outputs.clamp(1, 16);
    configure_outputs(&mut server, size, output_count)?;
    let socket = server.listen()?;
    for fd in server.event_fds()? {
        handle.insert_source(Generic::new(fd, Interest::READ, Mode::Level), |_, _, _| Ok(PostAction::Continue)).map_err(error)?;
    }
    eprintln!("Park Wayland display: {}", socket.display());
    let command_file = configuration.directory.join("park.json");
    let mut composition = renderer::Composition::new(wake.clone())?;
    composition.prepare_color(backend.renderer())?;
    let worker = CanvasWorker::spawn_for_compositor(Configuration {
        size,
        ..configuration
    }, wake, composition.linear_color_available())?;
    let mut desktop = Desktop::new(
        server,
        worker,
        size,
        output_count,
        backend.window().has_focus(),
    );
    desktop.commands(command_file, socket.clone());
    let mut applications = Some(applications);
    let mut children: Vec<Child> = Vec::new();
    let started = Instant::now();
    let mut frames = 0;
    let mut seen_windows = HashSet::new();
    let mut seen_layers = HashSet::new();
    let mut smoke = smoke::Check::default();
    let mut input = Vec::new();
    let mut awaiting_presented: Option<renderer::Presented> = None;
    let mut draw_allowed = false;
    let mut redraw_requested = false;
    let result: io::Result<()> = (|| {
        while desktop.running
            && options
                .duration
                .is_none_or(|duration| started.elapsed() < duration)
        {
            for event in input.drain(..) {
                if matches!(event, WinitEvent::Redraw) {
                    draw_allowed = true;
                    redraw_requested = false;
                }
                desktop.event(event)?;
            }
            if options.activation.as_ref().is_some_and(|events| events.take_show()) {
                desktop.send(Input::Action(lince_interface::host::DesktopAction::OpenOperation))?;
            }
            desktop.dispatch()?;
            for request in desktop.server.output_requests() {
                let accepted = desktop.server.apply_virtual_output_configuration(&request, desktop.size)?;
                desktop.server.output_configuration_finished(request.token, accepted);
                if accepted && !request.test { desktop.invalidate_outputs()?; }
            }
            desktop.redraw |= composition.imports_changed();
            if awaiting_presented.is_none() && desktop.accept(&mut composition, backend.renderer())? {
                if let Some(completed) = desktop.pending.as_ref().or(desktop.current.as_ref()).cloned() {
                    composition.enable_client_formats(backend.renderer(), &mut desktop.server, &completed.frame)?;
                }
                if desktop.opening_ready() && let Some(applications) = applications.take() {
                    for application in applications {
                        match desktop.server.spawn_foreground(application, &socket) {
                            Ok(child) => children.push(child),
                            Err(error) => {
                                let message =
                                    format!("Could not start the requested application: {error}");
                                eprintln!("{message}");
                                let _ = desktop.send(Input::Feedback(message));
                            }
                        }
                    }
                }
            }
            if desktop.redraw && !draw_allowed && !redraw_requested {
                backend.window().request_redraw();
                redraw_requested = true;
            }
            if awaiting_presented.is_none() && desktop.redraw && draw_allowed && backend.window_size().w > 0 && backend.window_size().h > 0 {
                let presented = composition.present(
                    &mut backend,
                    &mut desktop.server,
                    desktop.pending.as_deref().or(desktop.current.as_deref()),
                    desktop.pointer,
                )?;
                desktop.redraw = false;
                draw_allowed = false;
                awaiting_presented = Some(presented);
            }
            if awaiting_presented.as_ref().is_some_and(|presented| presented.sync.is_reached()) {
                let presented = awaiting_presented.take().unwrap();
                if presented.locked { desktop.server.lock_presented(); }
                desktop.presented(presented, started.elapsed().as_millis() as u32)?;
                if options.ready_hook { desktop.ready(); }
                frames += 1;
                seen_layers.extend(desktop.layers.iter().map(|layer| layer.surface.clone()));
                if let Some(geometry) = desktop
                    .current
                    .as_ref()
                    .and_then(|current| current.frame.geometry.as_ref())
                {
                    seen_windows.extend(geometry.items.iter().filter_map(|item| {
                        if let ItemId::Application(id) = item.id
                            && desktop.current.as_ref().is_some_and(|current| {
                                current.windows.iter().any(|window| {
                                    window.id == id && window.mapped && !window.nodes.is_empty()
                                })
                            })
                        {
                            Some(id)
                        } else {
                            None
                        }
                    }));
                }

                if options.smoke {
                    smoke.advance(&mut desktop)?;
                }
            }
            children.retain_mut(|child| match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        eprintln!("A Park application exited with {status}");
                    }
                    false
                }
                Ok(None) => true,
                Err(error) => {
                    eprintln!("Could not read a Park application status: {error}");
                    false
                }
            });
            desktop.server.dispatch()?;
            composition.capture(backend.renderer(), &mut desktop, true)?;
            let timeout = options.duration.map(|duration| duration.saturating_sub(started.elapsed()));
            let timeout = if !children.is_empty() || !desktop.children.is_empty() || options.smoke {
                Some(timeout.unwrap_or(Duration::from_secs(1)).min(Duration::from_secs(1)))
            } else {
                timeout
            };
            if desktop.running {
                let timeout = timeout.into_iter().chain(desktop.server.idle_timeout()).chain(desktop.server.native_timeout())
                    .chain(desktop.server.capture_timeout()).chain(desktop.server.display_timeout()).chain(desktop.server.control_timeout()).chain(desktop.scene_timeout()).chain(composition.capture_timeout()).min();
                event_loop.dispatch(timeout, &mut input).map_err(error)?;
            }
        }
        Ok(())
    })();
    desktop
        .server
        .clear_input(started.elapsed().as_millis() as u32);
    let stopped = desktop.worker.stop();
    result?;
    stopped?;
    Ok(Report {
        frames,
        applications: seen_windows.len(),
        layers: seen_layers.len(),
        elapsed: started.elapsed(),
        checked: smoke.complete() && !desktop.running,
    })
}

impl Desktop {
    pub(crate) fn scene_timeout(&self) -> Option<Duration> {
        (self.server_revision != self.server.revision()).then_some(Duration::ZERO)
    }

    pub(crate) fn new(
        mut server: Server,
        worker: CanvasWorker,
        size: [u32; 2],
        output_count: usize,
        focused: bool,
    ) -> Self {
        server.display_bounds(Some(size));
        server.control_active(focused);
        server.capture_policy_gate().invalidate();
        Self {
            server,
            worker,
            size,
            output_count,
            current: None,
            pending: None,
            output_frames: HashMap::new(),
            capture_times: HashMap::new(),
            retired_sequence: 0,
            scene_serial: 0,
            layers: Vec::new(),
            popups: Vec::new(),
            clips: Vec::new(),
            pointer: [0.0; 2],
            display_pointer: None,
            pointer_sent: None,
            pointer_stamp: 0,
            pointer_epoch: 0,
            pointer_target: None,
            file_drag_sent: None,
            drag_active_sent: false,
            hover_reply: None,
            pointer_focus_item: None,
            cursor_warp_sequence: 0,
            grabbed: None,
            buttons: HashSet::new(),
            manipulation: Default::default(),
            #[cfg(feature = "session")]
            gesture_devices: Default::default(),
            #[cfg(feature = "session")]
            gestures: Default::default(),
            keys: HashMap::new(),
            consumed: HashSet::new(),
            consumed_actions: HashMap::new(),
            modal_keys: HashSet::new(),
            native_keyboard: true,
            focused,
            layer_focus: None,
            pending_focus: None,
            pending_output: None,
            deferred: Default::default(),
            scene_revision: 0,
            server_revision: 0,
            viewports: Vec::new(),
            display: None,
            keyboard_snapshot: None,
            pointer_snapshot: None,
            absolute_snapshot: None,
            absolute_sent: None,
            #[cfg(feature = "session")]
            absolute: Default::default(),
            #[cfg(feature = "session")]
            tablets: Default::default(),
            control_status: None,
            capture_gate_pending: true,
            capture_status: None,
            capture_demand: None,
            logout_pending: false,
            external: Default::default(),
            magnification: Default::default(),
            magnification_sent: None,
            magnification_caret: None,
            #[cfg(feature = "session")]
            pointer_requests: None,
            #[cfg(feature = "session")]
            absolute_requests: None,
            redraw: true,
            running: true,
            command_file: None,
            socket: None,
            children: Vec::new(),
            ready_state: Ready::Waiting,
            locked: false,
            compose: Default::default(),
            #[cfg(feature = "session")]
            repeat: None,
            #[cfg(feature = "session")]
            keyboard: Default::default(),
        }
    }

    pub(crate) fn dispatch(&mut self) -> io::Result<()> {
        #[cfg(feature = "session")]
        { self.reconcile_touch(); self.reconcile_tablet(); self.flush_touch()?; }
        if let Some(snapshot) = self.absolute_snapshot.clone() { self.absolute_settings(snapshot)?; }
        #[cfg(feature = "session")]
        self.flush_gesture()?;
        self.flush_magnification()?;
        let mut failures = Vec::new();
        self.children.retain_mut(|child| match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    failures.push(format!("A Park command exited with {status}"));
                }
                false
            }
            Ok(None) => true,
            Err(error) => {
                failures.push(format!("Could not read a Park command status: {error}"));
                false
            }
        });
        for failure in failures {
            eprintln!("{failure}");
            self.send(Input::Feedback(failure))?;
        }
        self.server.dispatch()?;
        self.worker.host_active(self.focused && !self.server.locked());
        let caret = self.server.magnification_caret();
        if self.magnification_caret != caret {
            match self.worker.input(Input::MagnificationCaret(caret)) {
                Ok(_) => self.magnification_caret = caret,
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas is unavailable.")),
            }
        }
        self.reconcile_file_drag()?;
        for request in self.server.manipulations() {
            self.begin_application_manipulation(request)?;
        }
        self.server.advance_display(Instant::now());
        for event in self.server.native_events() {
            match event {
                crate::native_input::client::Event::Drop(request) => {
                    let token = request.token;
                    let output = self.current.as_ref().and_then(|current| current.frame.output);
                    if !self.focused || self.server.locked() || self.input_pending()
                        || output.is_some_and(|output| output != request.target.output)
                        || self.send(Input::NativeDrop(request)).is_err() {
                        self.server.finish_native_drop(token, false);
                    }
                }
                crate::native_input::client::Event::DropError(message) => {
                    if self.focused && !self.server.locked() { self.send(Input::Feedback(message))?; }
                }
                crate::native_input::client::Event::Edit(edit) if self.native_keyboard && self.focused && !self.server.locked()
                    && !self.current.as_ref().is_some_and(|current| current.frame.input.keyboard_capture) => {
                    self.send(Input::NativeEdit(edit))?;
                }
                crate::native_input::client::Event::Key { code, pressed, event } => {
                    self.native_key(code, pressed, false, &event)?;
                }
                _ => {}
            }
        }
        for (surface, request) in self.server.requests() {
            self.send(Input::Window(surface, request))?;
        }
        for (surface, initial) in self.server.activation_requests() {
            self.activate(surface, initial)?;
        }
        for (owner, id) in self.server.workspace_activations() {
            let serial = self.send_tracked(Input::Workspace { owner, id })?;
            self.pending_output = Some((serial, None));
            self.pending_focus = Some((serial, None));
        }
        if let Some(started) = self.worker.started()
            && started.scene_revision == self.scene_serial
            && !self.server.locked()
            && let Some(geometry) = self.current.as_ref().and_then(|current| current.frame.geometry.clone())
        {
            for item in &geometry.items {
                let ItemId::Application(id) = item.id else { continue; };
                let visible = geometry.outputs.iter().find(|output| output.id == item.output)
                    .and_then(|output| if self.fullscreen(output.id) { Some(output.bounds) } else { output.usable })
                    .is_some_and(|clip| (0..2).all(|axis| {
                        item.mapping.canvas.origin()[axis] < clip.origin()[axis] + clip.size()[axis]
                            && item.mapping.canvas.origin()[axis] + item.mapping.canvas.size()[axis] > clip.origin()[axis]
                    }));
                if visible && let Some(window) = started.windows.iter().find(|window| window.id == id && window.mapped) {
                    let nodes = self.server.callback_nodes(window);
                    self.server.frame_started_nodes(&nodes);
                }
            }
        }
        if let Some(failure) = self.worker.failure() {
            return Err(error(failure));
        }
        if !self.worker.is_running() {
            return Err(io::Error::other("the Park canvas stopped"));
        }
        if self.server_revision != self.server.revision() {
            let outputs: Vec<_> = self
                .server
                .output_viewports()
                .into_iter()
                .map(|(name, viewport)| crate::presentation::Output { name, viewport })
                .collect();
            let viewports: Vec<_> = outputs
                .iter()
                .map(|output| output.viewport.clone())
                .collect();
            let display = self.server.display_snapshot();
            if self.magnification.reconcile(&display, self.command_file.is_some()) {
                self.flush_magnification()?;
            }
            if self.scene_revision != self.server.window_revision() || self.viewports != viewports || self.display.as_ref() != Some(&display) {
                if self.viewports != viewports || self.scene_serial == 0 {
                    self.scene_serial = self
                        .scene_serial
                        .checked_add(1)
                        .ok_or_else(|| io::Error::other("Park scene revisions exhausted"))?;
                }
                self.worker.scene(Scene {
                    revision: self.scene_serial,
                    outputs,
                    windows: Arc::new(self.server.windows()),
                    display: Some(display.clone()),
                })?;
                self.scene_revision = self.server.window_revision();
                self.viewports = viewports;
                self.display = Some(display);
            }
            self.server_revision = self.server.revision();
            self.redraw = true;
        }
        while let Some(request) = self.worker.request() {
            match request {
                HostRequest::ControlRevoke => self.server.revoke_control(),
                HostRequest::StopCapture(id) => { if self.focused && !self.server.locked() { self.server.stop_capture_session(id); } }
                HostRequest::OverviewHover(request) => self.hover_reply = Some((request, self.file_drag_context() == Some(request.drag))),
                HostRequest::DropFinished { token, accepted } => self.server.finish_native_drop(token, accepted),
                HostRequest::Close(surface) => { self.server.close(surface); }
                HostRequest::Keyboard(request) => self.server.keyboard_request(request),
                HostRequest::External(request) => {
                    let path = self.command_file.clone();
                    let changed = self.external.request(request, self.server.locked(), |settings, expected| {
                        let path = path.as_deref().ok_or_else(|| io::Error::other("The local Park configuration is unavailable"))?;
                        crate::commands::Commands::save_external(path, settings, expected)
                    });
                    if changed { self.stop_external_repeats(); }
                    self.send(Input::ExternalSettings(self.external.snapshot.clone()))?;
                }
                HostRequest::ExternalReload => self.reload_external()?,
                HostRequest::Magnification(request) => self.magnification_request(request)?,
                HostRequest::MagnificationReload(request) => self.magnification_reload(request)?,
                HostRequest::Absolute(request) => {
                    #[cfg(feature = "session")]
                    if let Some(pending) = &mut self.absolute_requests && pending.len() < 8 {
                        pending.push_back(lince_interface::host::input::AbsoluteCommand::Save(request));
                        continue;
                    }
                    let mut snapshot = self.absolute_snapshot.clone().unwrap_or_default();
                    snapshot.last_request = snapshot.last_request.max(request.id);
                    snapshot.response = Some(lince_interface::host::input::InputResponse {
                        request: request.id,
                        outcome: lince_interface::host::input::InputOutcome::Failed(if snapshot.available {
                            "Park is busy; retry the preserved device draft.".into()
                        } else {
                            "Physical touch/tablet settings belong to the host compositor in Sandbox or nested Park.".into()
                        }),
                    });
                    self.absolute_settings(snapshot)?;
                }
                HostRequest::AbsoluteReload(request) => {
                    #[cfg(feature = "session")]
                    if let Some(pending) = &mut self.absolute_requests && pending.len() < 8 {
                        pending.push_back(lince_interface::host::input::AbsoluteCommand::Reload(request));
                        continue;
                    }
                    let mut snapshot = self.absolute_snapshot.clone().unwrap_or_default();
                    snapshot.last_request = snapshot.last_request.max(request.id);
                    snapshot.response = Some(lince_interface::host::input::InputResponse {
                        request: request.id,
                        outcome: lince_interface::host::input::InputOutcome::Failed("Park is busy or physical device settings belong to another host; the draft is preserved.".into()),
                    });
                    self.absolute_settings(snapshot)?;
                }
                HostRequest::Pointer(request) => {
                    #[cfg(feature = "session")]
                    if let Some(pending) = &mut self.pointer_requests
                        && pending.len() < 8 {
                        pending.push_back(request);
                        continue;
                    }
                    let mut snapshot = self.pointer_snapshot.clone().unwrap_or_default();
                    snapshot.last_request = snapshot.last_request.max(request.id);
                    snapshot.response = Some(lince_interface::host::input::InputResponse {
                        request: request.id,
                        outcome: lince_interface::host::input::InputOutcome::Failed(if snapshot.available {
                            "Park is busy; retry the preserved pointer draft.".into()
                        } else {
                            "Physical pointer settings require a standalone Park seat; nested Park uses its parent compositor's devices.".into()
                        }),
                    });
                    self.pointer_settings(snapshot)?;
                }
                HostRequest::Display(request) => {
                    let path = self.command_file.clone();
                    let mut warning = None;
                    self.server.display_request(request, |heads| {
                        let path = path.as_deref().ok_or_else(|| io::Error::other("The local Park configuration is unavailable"))?;
                        warning = crate::commands::Commands::save_outputs(path, heads)?;
                        Ok(())
                    });
                    if let Some(warning) = warning { self.send(Input::Feedback(warning))?; }
                }
                HostRequest::Action(DesktopAction::Logout) => self.logout()?,
                HostRequest::Workspace { owner, id } => {
                    let serial = self.send_tracked(Input::Workspace { owner, id })?;
                    self.pending_output = Some((serial, None));
                    self.native_keyboard = false;
                }
                HostRequest::CreateWorkspace { owner } => {
                    let serial = self.send_tracked(Input::CreateWorkspace { owner })?;
                    self.pending_output = Some((serial, None));
                    self.native_keyboard = false;
                }
                HostRequest::Exit => self.running = false,
                HostRequest::Action(
                    action @ (DesktopAction::OpenTerminal
                    | DesktopAction::OpenLauncher
                    | DesktopAction::External(_)
                    | DesktopAction::Lock),
                ) => self.command(action, false),
                HostRequest::Action(
                    action @ (DesktopAction::FocusOutput(_)
                    | DesktopAction::CommitRecent { .. }
                    | DesktopAction::SelectPlacement { .. }
                    | DesktopAction::MoveToOutput(_)
                    | DesktopAction::MoveRowToOutput(_)
                    | DesktopAction::Workspace(_)
                    | DesktopAction::PreviousWorkspace
                    | DesktopAction::NextWorkspace
                    | DesktopAction::MoveWorkspaceEarlier
                    | DesktopAction::MoveWorkspaceLater
                    | DesktopAction::MoveToWorkspace(_)
                    | DesktopAction::MoveWorkspaceToOutput(_)),
                ) => {
                    let serial = self.send_tracked(Input::Action(action))?;
                    self.pending_output = Some((serial, None));
                    self.native_keyboard = false;
                }
                HostRequest::Action(action @ DesktopAction::InspectWindowRecord) => {
                    self.send(Input::Action(action))?
                }
                HostRequest::Inspect(target) => self.send(Input::Inspect(target))?,
                HostRequest::ReapplyOpening(target) => {
                    if self.focused && !self.server.locked() {
                        let serial = self.send_tracked(Input::ReapplyOpening(target))?;
                        self.pending_output = Some((serial, None));
                    }
                }
                HostRequest::Action(_) => {
                    eprintln!("This desktop action is not available in nested development yet.")
                }
            }
        }
        self.flush_logout()?;
        self.control()?;
        if self.capture_gate_pending
            && self.worker.input(Input::CapturePolicy(self.server.capture_policy_gate())).is_ok()
        { self.capture_gate_pending = false; }
        let status = self.server.capture_status();
        if self.capture_status.as_ref() != Some(&status)
            && self.worker.input(Input::CaptureStatus(status.clone())).is_ok()
        { self.capture_status = Some(status); }
        let demand = self.server.capture_demand();
        if self.capture_demand.as_ref() != Some(&demand)
            && self.worker.input(Input::CaptureDemand(demand.clone())).is_ok()
        { self.capture_demand = Some(demand); }
        let path = self.command_file.as_deref();
        let _ = self.server.advance_keyboard(|settings, expected| {
            let path = path.ok_or_else(|| io::Error::other("The local Park configuration is unavailable"))?;
            crate::commands::Commands::save_keyboard(path, settings, expected)
        });
        let keyboard = self.server.keyboard_snapshot();
        if self.keyboard_snapshot.as_ref() != Some(&keyboard) {
            if self.keyboard_snapshot.as_ref().is_none_or(|previous| previous.revision != keyboard.revision) {
                self.compose.reset();
                #[cfg(feature = "session")]
                {
                    self.repeat = None;
                    self.keyboard = keyboard.settings.clone();
                }
            }
            self.send(Input::Keyboard(keyboard.clone()))?;
            self.keyboard_snapshot = Some(keyboard);
        }
        if self.pointer_snapshot.is_none() { self.pointer_settings(Default::default())?; }
        if self.absolute_snapshot.is_none() { self.absolute_settings(Default::default())?; }
        self.reconcile_file_drag()?;
        Ok(())
    }

    pub(crate) fn pointer_settings(&mut self, snapshot: lince_interface::host::input::PointerSnapshot) -> io::Result<()> {
        if self.pointer_snapshot.as_ref() != Some(&snapshot) {
            match self.worker.input(Input::PointerSettings(snapshot.clone())) {
                Ok(_) => self.pointer_snapshot = Some(snapshot),
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas is unavailable for pointer settings.")),
            }
        }
        Ok(())
    }

    pub(crate) fn absolute_settings(&mut self, snapshot: lince_interface::host::input::AbsoluteSnapshot) -> io::Result<()> {
        self.absolute_snapshot = Some(snapshot.clone());
        if self.absolute_sent.as_ref() != Some(&snapshot) {
            match self.worker.input(Input::AbsoluteSettings(snapshot.clone())) {
                Ok(_) => self.absolute_sent = Some(snapshot),
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => return Err(io::Error::other("The Park canvas is unavailable for device settings.")),
            }
        }
        Ok(())
    }

    #[cfg(feature = "session")]
    pub(crate) fn save_devices(&self, settings: &lince_interface::host::input::AbsolutePreferences, expected: &lince_interface::host::input::AbsolutePreferences) -> io::Result<Option<String>> {
        let path = self.command_file.as_deref().ok_or_else(|| io::Error::other("The local Park configuration is unavailable"))?;
        crate::commands::Commands::save_devices(path, settings, expected)
    }

    #[cfg(feature = "session")]
    pub(crate) fn load_devices(&self) -> io::Result<lince_interface::host::input::AbsolutePreferences> {
        let path = self.command_file.as_deref().ok_or_else(|| io::Error::other("The local Park configuration is unavailable"))?;
        Ok(crate::commands::Commands::load(path)?.devices)
    }

    #[cfg(feature = "session")]
    pub(crate) fn save_pointer(&self, settings: &lince_interface::host::input::PointerSettings, expected: &lince_interface::host::input::PointerSettings) -> io::Result<Option<String>> {
        let path = self.command_file.as_deref().ok_or_else(|| io::Error::other("The local Park configuration is unavailable"))?;
        crate::commands::Commands::save_pointer(path, settings, expected)
    }

    pub(crate) fn accept(
        &mut self,
        composition: &mut renderer::Composition,
        renderer: &mut GlesRenderer,
    ) -> io::Result<bool> {
        if let Some(completed) = self.worker.frame() {
            if completed.scene_revision != self.scene_serial {
                return Ok(false);
            }
            for id in &completed.rejected {
                self.server.reject_window(*id);
            }
            for window in &completed.frame.windows {
                if self
                    .server
                    .configure_presented_state(
                        window.surface,
                        [window.size[0] as i32, window.size[1] as i32],
                        window.state,
                        completed.frame.geometry.as_ref().is_some_and(|geometry| {
                            geometry.items.iter().any(|item| item.id == ItemId::Application(window.surface))
                        }),
                    )
                    .is_err()
                {
                    self.server.reject_window(window.surface);
                }
            }
            composition.canvas(renderer, &completed.frame)?;
            if completed.frame.dmabuf().is_none() {
                self.pending = None;
                self.output_frames.clear();
                self.capture_times.clear();
                self.adopt(Arc::new(completed));
                self.refresh_presented(0)?;
                self.redraw = false;
                return Ok(true);
            }
            self.pending = Some(Arc::new(completed));
            self.redraw = true;
            return Ok(true);
        }
        Ok(false)
    }

    #[cfg(feature = "session")]
    pub(crate) fn rendering(&self) -> Option<&Completed> {
        self.pending.as_deref().or(self.current.as_deref())
    }

    pub(crate) fn frame_at(&self, point: [f64; 2]) -> Option<&Completed> {
        self.output_frames
            .iter()
            .find_map(|(id, (frame, _))| {
                frame
                    .frame
                    .geometry
                    .as_ref()?
                    .outputs
                    .iter()
                    .any(|output| output.id == *id && output.bounds.contains(point))
                    .then_some(frame.as_ref())
            })
            .or(self.current.as_deref())
    }

    pub(crate) fn presented(
        &mut self,
        presented: renderer::Presented,
        time: u32,
    ) -> io::Result<()> {
        let Some(frame) = self.pending.take().or_else(|| self.current.clone()) else {
            return Ok(());
        };
        if frame.sequence <= self.retired_sequence || self.current.as_ref().is_some_and(|current| current.sequence > frame.sequence) { return Ok(()); }
        self.server.presented_groups(presented.callbacks.iter().map(|(_, nodes)| nodes.as_slice()));
        let presented = Arc::new(presented);
        let ids: Vec<_> = frame
            .frame
            .geometry
            .as_ref()
            .into_iter()
            .flat_map(|geometry| geometry.outputs.iter())
            .map(|output| output.id)
            .collect();
        self.output_frames.retain(|id, _| ids.contains(id));
        self.capture_times.retain(|id, _| ids.contains(id));
        for id in ids {
            if !presented.locked && frame.frame.has_output_image(id) {
                self.server.presented_output_mapping(id, frame.frame.input.mappings.get(id));
            }
            if !presented.locked && frame.frame.has_output_image(id) && self.magnification.presented(id, &frame.frame.input.mappings) {
                self.flush_magnification()?;
            }
            if !presented.locked && frame.frame.has_output_image(id)
                && let Some(receipt) = &frame.frame.input.accessibility
            { receipt.presented(id); }
            self.capture_times.insert(id, renderer::capture::monotonic());
            self.output_frames
                .insert(id, (frame.clone(), presented.clone()));
        }
        self.adopt(frame);
        self.refresh_presented(time)
    }

    pub(crate) fn output_presented(
        &mut self,
        id: OutputId,
        frame: Arc<Completed>,
        presented: Arc<renderer::Presented>,
        presentation_time: Duration,
        time: u32,
    ) -> io::Result<()> {
        if !self.server.has_output(id) || frame.sequence <= self.retired_sequence
            || self.output_frames.get(&id).is_some_and(|(latest, _)| latest.sequence > frame.sequence)
            || self.capture_times.get(&id).is_some_and(|latest| presentation_time < *latest)
        { return Ok(()); }
        self.server.presented_groups(presented.callbacks.iter()
            .filter(|(output, _)| output.is_none_or(|output| output == id))
            .map(|(_, nodes)| nodes.as_slice()));
        if !presented.locked && frame.frame.has_output_image(id) { self.server.presented_output_mapping(id, frame.frame.input.mappings.get(id)); }
        if !presented.locked && frame.frame.has_output_image(id) && self.magnification.presented(id, &frame.frame.input.mappings) {
            self.flush_magnification()?;
        }
        if !presented.locked && frame.frame.has_output_image(id)
            && let Some(receipt) = &frame.frame.input.accessibility
        { receipt.presented(id); }
        self.output_frames.insert(id, (frame.clone(), presented));
        self.capture_times.insert(id, presentation_time);
        if frame.frame.output == Some(id) || self.current.is_none() {
            self.adopt(frame);
        }
        self.refresh_presented(time)
    }

    pub(crate) fn adopt(&mut self, frame: Arc<Completed>) {
        if frame.sequence <= self.retired_sequence || self.current.as_ref().is_some_and(|current| current.sequence > frame.sequence) { return; }
        #[cfg(feature = "session")]
        self.observe_gesture(&frame);
        self.manipulation.observe(&frame);
        if self.current.as_ref().is_some_and(|current| current.frame.input.keyboard_capture != frame.frame.input.keyboard_capture) {
            self.compose.reset();
        }
        if frame.frame.input.keyboard_modal {
            self.modal_keys.extend(self.keys.keys().copied());
        }
        self.server.workspaces(frame.frame.input.workspaces.clone());
        self.server.control_catalog(frame.frame.input.control.clone());
        if self
            .current
            .as_ref()
            .is_none_or(|current| current.frame.output != frame.frame.output)
        {
            self.native_keyboard = false;
        }
        if self
            .pending_focus
            .is_some_and(|(serial, _)| frame.input_serial >= serial)
        {
            self.pending_focus = None;
        }
        self.current = Some(frame);
    }

    fn refresh_presented(&mut self, time: u32) -> io::Result<()> {
        self.layers.clear();
        self.popups.clear();
        self.clips.clear();
        let mut outputs = Vec::new();
        let mut items = Vec::new();
        let mut revision = 0;
        for (id, (frame, presented)) in &self.output_frames {
            self.layers.extend(
                presented
                    .layers
                    .iter()
                    .filter(|layer| layer.output == *id)
                    .cloned(),
            );
            self.popups.extend(
                presented
                    .popups
                    .iter()
                    .filter(|popup| popup.output == *id)
                    .cloned(),
            );
            self.clips.extend(
                presented
                    .clips
                    .iter()
                    .filter(|(output, _)| output == id)
                    .copied(),
            );
            if let Some(geometry) = &frame.frame.geometry {
                outputs.extend(
                    geometry
                        .outputs
                        .iter()
                        .filter(|output| output.id == *id)
                        .cloned(),
                );
                items.extend(
                    geometry
                        .items
                        .iter()
                        .filter(|item| item.output == *id)
                        .cloned(),
                );
                revision = revision.max(geometry.revision);
            }
        }
        let focused = self
            .current
            .as_ref()
            .and_then(|current| current.frame.geometry.as_ref())
            .and_then(|geometry| geometry.focused);
        self.server.presented_outputs(
            &lince_interface::host::GeometrySnapshot {
                revision,
                outputs,
                items,
                focused,
            },
            self.current
                .as_ref()
                .and_then(|current| current.frame.output),
        );
        self.refresh_pointer_mapping();
        self.replay()?;
        self.focus()?;
        self.warp_pointer(time)?;
        self.motion(time)?;
        #[cfg(feature = "session")]
        { self.refresh_touch_projection(time); self.refresh_tablet_projection(time); }
        self.reconcile_file_drag()
    }

    pub(crate) fn invalidate_outputs(&mut self) -> io::Result<()> {
        #[cfg(feature = "session")]
        self.cancel_absolute();
        self.retired_sequence = self.current.iter().chain(self.pending.iter())
            .chain(self.output_frames.values().map(|(frame, _)| frame)).map(|frame| frame.sequence).fold(self.retired_sequence, u64::max);
        self.output_frames.clear();
        self.capture_times.clear();
        self.current = None;
        self.pending = None;
        self.layers.clear();
        self.popups.clear();
        self.clips.clear();
        self.scene_revision = 0;
        self.server_revision = 0;
        self.viewports.clear();
        let focused = self.focused;
        self.event(WinitEvent::Focus(false))?;
        self.focused = focused;
        self.server.control_active(focused);
        self.redraw = true;
        Ok(())
    }

    pub(crate) fn commands(&mut self, path: std::path::PathBuf, socket: std::path::PathBuf) {
        let result = crate::commands::Commands::load(&path).and_then(|commands| {
                if let Err(error) = self.restore_virtual_preferences(&commands.outputs) {
                    let message = format!("Saved virtual monitor settings could not be applied: {error}. The saved preferences are unchanged.");
                    eprintln!("{message}");
                    let _ = self.send(Input::Feedback(message));
                }
                self.magnification.configure(commands.outputs.iter().map(|(name, output)| (name.clone(), output.magnification)).collect());
                self.external.configure(commands.external);
                self.server.configure_keyboard(commands.keyboard.clone())?;
                #[cfg(feature = "session")]
                {
                    self.keyboard = commands.keyboard;
                }
                Ok(())
        });
        if let Err(error) = result {
            let message = format!("Could not load Park settings: {error}");
            if !self.external.snapshot.available { self.external.snapshot.message = Some(message.clone()); }
            eprintln!("{message}");
            let _ = self.send(Input::Feedback(message));
        }
        self.command_file = Some(path);
        self.socket = Some(socket);
        self.magnification.reconcile(&self.server.display_snapshot(), self.command_file.is_some());
        let _ = self.flush_magnification();
        let _ = self.send(Input::ExternalSettings(self.external.snapshot.clone()));
    }

    fn command(&mut self, action: DesktopAction, repeating: bool) {
        if let Err(error) = self.launch_command(action, repeating, None) {
            let name = match action {
                DesktopAction::OpenTerminal => "the terminal",
                DesktopAction::OpenLauncher => "the application launcher",
                DesktopAction::Lock => "the screen locker",
                DesktopAction::External(action) => action.label(),
                _ => "the selected program",
            };
            let message = format!("Could not start {name}: {error}");
            eprintln!("{message}");
            let _ = self.send(Input::Feedback(message));
        }
    }

    fn launch_command(&mut self, action: DesktopAction, repeating: bool, grants: Option<&[crate::launch::Capability]>) -> io::Result<()> {
        let child = (|| {
            if self.children.len() >= 128 { return Err(io::Error::other("Too many command helpers are still running; close or stop one before retrying.")); }
            let path = self
                .command_file
                .as_ref()
                .ok_or_else(|| io::Error::other("Park command configuration is unavailable"))?;
            let socket = self
                .socket
                .as_ref()
                .ok_or_else(|| io::Error::other("Park display is unavailable"))?.clone();
            let commands = crate::commands::Commands::load(path)?;
            let description = if let DesktopAction::External(external) = action {
                let changed = self.external.configure(commands.external.clone());
                if changed {
                    self.stop_external_repeats();
                    let _ = self.send(Input::ExternalSettings(self.external.snapshot.clone()));
                }
                if changed && repeating {
                    return Err(io::Error::other("Release the key before using the changed command."));
                }
                commands.external.command(external, self.server.locked()).map_err(io::Error::other)?.clone()
            } else {
                if self.server.locked() { return Err(io::Error::other("This command is unavailable while locked")); }
                commands.command(action)?
            };
            if let Some(grants) = grants {
                if !grants.contains(&crate::launch::Capability::DesktopControl) || !grants.contains(&crate::launch::Capability::Launch)
                    || description.capabilities.iter().any(|capability| !grants.contains(capability))
                { return Err(io::Error::new(io::ErrorKind::PermissionDenied, "The management connection cannot grant this configured helper's privileges")); }
                if matches!(action, DesktopAction::External(lince_interface::host::external::Action::ScreenshotScreen | lince_interface::host::external::Action::ScreenshotRegion))
                    && !grants.contains(&crate::launch::Capability::Capture)
                { return Err(io::Error::new(io::ErrorKind::PermissionDenied, "The management connection lacks capture permission")); }
            }
            if matches!(action, DesktopAction::OpenTerminal | DesktopAction::OpenLauncher
                | DesktopAction::External(lince_interface::host::external::Action::Editor)) {
                self.server.spawn_foreground_helper(&description, &socket)
            } else {
                self.server.spawn_helper(&description, &socket)
            }
        })()?;
        self.children.push(child);
        Ok(())
    }

    fn stop_external_repeats(&mut self) {
        #[cfg(feature = "session")]
        if self.repeat.is_some_and(|(code, _)| matches!(self.consumed_actions.get(&code), Some(DesktopAction::External(_)))) {
            self.repeat = None;
        }
        self.consumed_actions.retain(|_, action| !matches!(action, DesktopAction::External(_)));
    }

    fn reload_external(&mut self) -> io::Result<()> {
        if self.server.locked() { return Ok(()); }
        let result = self.command_file.as_deref()
            .ok_or_else(|| io::Error::other("The local Park configuration is unavailable"))
            .and_then(crate::commands::Commands::load);
        match result {
            Ok(commands) => {
                self.magnification.configure(commands.outputs.iter().map(|(name, output)| (name.clone(), output.magnification)).collect());
                self.flush_magnification()?;
                if self.external.configure(commands.external) { self.stop_external_repeats(); }
            }
            Err(error) => self.external.snapshot.message = Some(error.to_string()),
        }
        self.send(Input::ExternalSettings(self.external.snapshot.clone()))
    }

    pub(crate) fn opening_ready(&self) -> bool {
        self.pending.as_ref().or(self.current.as_ref()).is_some_and(|frame| frame.frame.input.opening_ready)
    }

    pub(crate) fn ready(&mut self) -> Ready {
        if self.ready_state != Ready::Waiting { return self.ready_state; }
        if self.server.locked() || self.viewports.is_empty()
            || self.viewports.iter().any(|viewport| self.output_frames.get(&viewport.id).is_none_or(|(frame, _)| !frame.frame.input.opening_ready))
        { return Ready::Waiting; }
        let result = (|| {
            let path = self
                .command_file
                .as_ref()
                .ok_or_else(|| io::Error::other("Park command configuration is unavailable"))?;
            let socket = self
                .socket
                .as_ref()
                .ok_or_else(|| io::Error::other("Park display is unavailable"))?;
            crate::commands::Commands::load(path)?
                .ready
                .map(|command| self.server.spawn_helper(&command, socket))
                .transpose()
        })();
        match result {
            Ok(Some(child)) => { self.children.push(child); self.ready_state = Ready::Dispatched; }
            Ok(None) => { self.ready_state = Ready::Dispatched; }
            Err(error) => {
                self.ready_state = Ready::Failed;
                let message = format!("Could not publish Park session readiness: {error}");
                eprintln!("{message}");
                let _ = self.send(Input::Feedback(message));
            }
        }
        self.ready_state
    }

    pub(crate) fn send(&self, input: Input) -> io::Result<()> {
        self.send_tracked(input).map(|_| ())
    }

    pub(crate) fn activate(&mut self, surface: lince_interface::host::SurfaceId, initial: bool) -> io::Result<()> {
        let serial = self.send_tracked(if initial { Input::InitialActivate(surface) } else { Input::Activate(surface) })?;
        if !initial { self.layer_focus = None; }
        self.pending_output = Some((serial, None));
        self.pending_focus = (!initial).then_some((serial, Some(surface)));
        Ok(())
    }

    fn send_tracked(&self, input: Input) -> io::Result<u64> {
        self.worker
            .input(input)
            .map_err(|_| io::Error::other("the Park input queue is full or unavailable"))
    }

    pub(crate) fn blur_native(&mut self) -> io::Result<()> {
        if !self.worker.is_running() { return Err(io::Error::other("The Park canvas disconnected during input suspension.")); }
        self.worker.focus_native(false);
        Ok(())
    }

    pub(crate) fn logout(&mut self) -> io::Result<()> {
        if !self.logout_pending {
            self.cancel_input()?;
            self.logout_pending = true;
        }
        self.flush_logout()
    }

    fn flush_logout(&mut self) -> io::Result<()> {
        if !self.logout_pending { return Ok(()); }
        match self.worker.input(Input::Logout) {
            Ok(_) => { self.logout_pending = false; Ok(()) }
            Err(std::sync::mpsc::TrySendError::Full(_)) => Ok(()),
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => Err(io::Error::other("The Park canvas disconnected before logout could save its state.")),
        }
    }

    fn cancel_input(&mut self) -> io::Result<()> {
        #[cfg(feature = "session")]
        self.cancel_absolute();
        self.invalidate_pointer_policy()?;
        #[cfg(feature = "session")]
        self.cancel_client_gestures(0);
        self.cancel_manipulation()?;
        self.server.clear_input(0);
        self.keys.clear();
        self.compose.reset();
        #[cfg(feature = "session")]
        { self.repeat = None; }
        self.buttons.clear();
        self.consumed.clear();
        self.consumed_actions.clear();
        self.modal_keys.clear();
        self.grabbed = None;
        self.layer_focus = None;
        self.pending_focus = None;
        self.pending_output = None;
        self.deferred.clear();
        self.pointer_target = None;
        Ok(())
    }

    pub(crate) fn focus(&mut self) -> io::Result<()> {
        let locked = self.server.locked();
        self.worker.host_active(self.focused && !locked);
        if locked != self.locked {
            self.cancel_input()?;
            self.blur_native()?;
            self.native_keyboard = false;
            self.locked = locked;
        }
        if locked {
            self.server.suspend_native_drop();
            self.server.native_editor(None);
            self.server.focus_lock(
                self.current
                    .as_ref()
                    .and_then(|current| current.frame.output),
            );
            return Ok(());
        }
        if !self.focused || self.current.as_ref().is_some_and(|current| current.frame.dmabuf().is_none()) {
            #[cfg(feature = "session")]
            self.cancel_client_gestures(0);
            self.cancel_manipulation()?;
            self.server.suspend_native_drop();
            self.server.native_editor(None);
            if self.native_keyboard {
                self.blur_native()?;
                self.native_keyboard = false;
            }
            self.server.focus(None, SERIAL_COUNTER.next_serial());
            return Ok(());
        }
        #[cfg(feature = "session")]
        if self.absolute_waiting() { return Ok(()); }
        let visible =
            |layer: &LayerSnapshot| layer.layer == Layer::Overlay || !self.fullscreen(layer.output);
        let exclusive = self
            .layers
            .iter()
            .rev()
            .find(|layer| {
                visible(layer)
                    && self
                        .current
                        .as_ref()
                        .is_none_or(|current| current.frame.output == Some(layer.output))
                    && layer.keyboard == KeyboardInteractivity::Exclusive
                    && matches!(layer.layer, Layer::Top | Layer::Overlay)
            })
            .map(|layer| layer.surface.clone());
        let layer = exclusive.or_else(|| {
            self.layer_focus
                .as_ref()
                .filter(|surface| {
                    self.layers
                        .iter()
                        .any(|layer| visible(layer) && &layer.surface == *surface)
                })
                .cloned()
        }).filter(|_| !self.current.as_ref().is_some_and(|current| current.frame.input.modal || current.frame.input.keyboard_modal));
        if layer.is_some() || self.current.as_ref().is_some_and(|frame| {
            frame.frame.input.modal || frame.frame.input.keyboard_modal
        }) {
            self.cancel_manipulation()?;
        }
        let native = self.manipulation.active() || layer.is_none()
            && self.pending_focus.map_or_else(
                || {
                    self.current.as_ref().is_none_or(|current| {
                        current.frame.input.keyboard
                            || current.frame.geometry.as_ref().is_none_or(|geometry| {
                                !matches!(geometry.focused, Some(ItemId::Application(_)))
                            })
                    })
                },
                |(_, application)| application.is_none(),
            );
        if native != self.native_keyboard {
            self.compose.reset();
            if native { self.worker.focus_native(true); } else { self.blur_native()?; }
            self.native_keyboard = native;
            if native {
                self.restore_modifiers()?;
            }
        }
        if let Some(layer) = layer {
            self.server
                .focus_surface(Some(layer), SERIAL_COUNTER.next_serial());
        } else {
            let application = self
                .pending_focus
                .and_then(|(_, application)| application)
                .or_else(|| {
                    (!native)
                        .then(|| self.current.as_ref()?.frame.geometry.as_ref()?.focused)
                        .flatten()
                        .and_then(|item| {
                            if let ItemId::Application(id) = item {
                                Some(id)
                            } else {
                                None
                            }
                        })
                });
            if native {
                let editor = self.current.as_ref().and_then(|current| current.frame.input.editor.clone());
                self.server.native_editor(editor);
                self.server.native_focus();
            } else {
                self.server.native_editor(None);
                self.server.focus(application, SERIAL_COUNTER.next_serial());
            }
        }
        Ok(())
    }

    fn fullscreen(&self, output: OutputId) -> bool {
        self.output_frames
            .get(&output)
            .map(|(frame, _)| frame)
            .or(self.current.as_ref())
            .is_some_and(|current| {
                if current.frame.output == Some(output) {
                    current.frame.fullscreen
                } else {
                    current
                        .frame
                        .outputs
                        .iter()
                        .find(|frame| frame.id == output)
                        .is_some_and(|frame| frame.fullscreen)
                }
            })
    }
}

pub(crate) fn error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

fn configure_outputs(server: &mut Server, size: [u32; 2], count: usize) -> io::Result<()> {
    let width = size[0] / count as u32;
    if width == 0 {
        return Err(io::Error::other(
            "the nested window is too narrow for its outputs",
        ));
    }
    let monitors: Vec<_> = (0..count)
        .map(|index| crate::server::Monitor {
            id: OutputId(index as u64 + 1),
            name: format!("Park-{}", index + 1),
            origin: [(index as u32 * width) as i32, 0],
            size: [
                if index + 1 == count {
                    (size[0] - index as u32 * width) as i32
                } else {
                    width as i32
                },
                size[1] as i32,
            ],
        })
        .collect();
    server.configure_outputs(&monitors)?;
    server.registered_output_catalog()
}
