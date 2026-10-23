use crate::{
    launch::Application,
    runtime::{CanvasWorker, Completed, Configuration, Input, Scene},
    server::{LayerSnapshot, Server},
};
use lince_interface::host::{
    DesktopAction, HostRequest, ItemId, OutputId, OutputViewport, Rectangle,
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

#[derive(Default)]
pub struct Options {
    pub duration: Option<Duration>,
    pub smoke: bool,
    pub outputs: usize,
    pub ready_hook: bool,
    pub activation: Option<lince_interface::activation::InstanceEvents>,
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
    scene_serial: u64,
    pub(crate) layers: Vec<LayerSnapshot>,
    pub(crate) popups: Vec<renderer::PopupScene>,
    pub(crate) clips: Vec<(OutputId, Rectangle)>,
    pub(crate) pointer: [f64; 2],
    pub(crate) pointer_sent: Option<[f64; 2]>,
    pub(crate) pointer_stamp: u64,
    pub(crate) pointer_target: Option<(WlSurface, [f64; 2])>,
    pub(crate) grabbed: Option<input::Target>,
    pub(crate) buttons: HashSet<u32>,
    pub(crate) keys: HashMap<u32, crate::server::KeyEvent>,
    pub(crate) consumed: HashSet<u32>,
    pub(crate) native_keyboard: bool,
    pub(crate) focused: bool,
    pub(crate) layer_focus: Option<WlSurface>,
    pub(crate) pending_focus: Option<(u64, Option<lince_interface::host::SurfaceId>)>,
    pub(crate) pending_output: Option<(u64, Option<OutputId>)>,
    pub(crate) deferred: std::collections::VecDeque<input::Deferred>,
    pub(crate) scene_revision: u64,
    pub(crate) server_revision: u64,
    pub(crate) viewports: Vec<OutputViewport>,
    pub(crate) redraw: bool,
    pub(crate) running: bool,
    command_file: Option<std::path::PathBuf>,
    socket: Option<std::path::PathBuf>,
    children: Vec<Child>,
    locked: bool,
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
    let worker = CanvasWorker::spawn_notifying(Configuration {
        size,
        ..configuration
    }, Some(wake.clone()))?;
    let mut desktop = Desktop::new(
        server,
        worker,
        size,
        output_count,
        backend.window().has_focus(),
    );
    desktop.commands(command_file, socket.clone());
    let mut composition = renderer::Composition::new(wake)?;
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
    let mut environment_ready = false;
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
                if let Some(applications) = applications.take() {
                    desktop.server.enable_dmabuf();
                    for application in applications {
                        match application.spawn(&socket) {
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
                if options.ready_hook && !environment_ready && !desktop.viewports.is_empty()
                    && desktop.viewports.iter().all(|viewport| desktop.output_frames.contains_key(&viewport.id)) {
                    desktop.ready();
                    environment_ready = true;
                }
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
                    .chain(desktop.server.capture_timeout()).chain(composition.capture_timeout()).min();
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
    pub(crate) fn new(
        server: Server,
        worker: CanvasWorker,
        size: [u32; 2],
        output_count: usize,
        focused: bool,
    ) -> Self {
        Self {
            server,
            worker,
            size,
            output_count,
            current: None,
            pending: None,
            output_frames: HashMap::new(),
            capture_times: HashMap::new(),
            scene_serial: 0,
            layers: Vec::new(),
            popups: Vec::new(),
            clips: Vec::new(),
            pointer: [0.0; 2],
            pointer_sent: None,
            pointer_stamp: 0,
            pointer_target: None,
            grabbed: None,
            buttons: HashSet::new(),
            keys: HashMap::new(),
            consumed: HashSet::new(),
            native_keyboard: true,
            focused,
            layer_focus: None,
            pending_focus: None,
            pending_output: None,
            deferred: Default::default(),
            scene_revision: 0,
            server_revision: 0,
            viewports: Vec::new(),
            redraw: true,
            running: true,
            command_file: None,
            socket: None,
            children: Vec::new(),
            locked: false,
            compose: Default::default(),
            #[cfg(feature = "session")]
            repeat: None,
            #[cfg(feature = "session")]
            keyboard: Default::default(),
        }
    }

    pub(crate) fn dispatch(&mut self) -> io::Result<()> {
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
        for event in self.server.native_events() {
            match event {
                crate::native_input::client::Event::Drop(request) => {
                    let token = request.token;
                    let output = self.current.as_ref().and_then(|current| current.frame.output);
                    if !self.focused || self.server.locked() || self.pending_output.is_some()
                        || output.is_some_and(|output| output != request.target.output)
                        || self.send(Input::NativeDrop(request)).is_err() {
                        self.server.finish_native_drop(token, false);
                    }
                }
                crate::native_input::client::Event::DropError(message) => {
                    if self.focused && !self.server.locked() { self.send(Input::Feedback(message))?; }
                }
                crate::native_input::client::Event::Edit(edit) if self.native_keyboard && self.focused && !self.server.locked() => {
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
        for surface in self.server.activations() {
            let serial = self.send_tracked(Input::Activate(surface))?;
            self.pending_output = Some((serial, None));
            self.pending_focus = Some((serial, Some(surface)));
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
            if self.scene_revision != self.server.window_revision() || self.viewports != viewports {
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
                })?;
                self.scene_revision = self.server.window_revision();
                self.viewports = viewports;
            }
            self.server_revision = self.server.revision();
            self.redraw = true;
        }
        while let Some(request) = self.worker.request() {
            match request {
                HostRequest::DropFinished { token, accepted } => self.server.finish_native_drop(token, accepted),
                HostRequest::Close(surface) => self.server.close(surface),
                HostRequest::Action(DesktopAction::Logout) => self.send(Input::Logout)?,
                HostRequest::Exit => self.running = false,
                HostRequest::Action(
                    action @ (DesktopAction::OpenTerminal
                    | DesktopAction::OpenLauncher
                    | DesktopAction::Lock),
                ) => self.command(action),
                HostRequest::Action(
                    action @ (DesktopAction::FocusOutput(_)
                    | DesktopAction::MoveToOutput(_)
                    | DesktopAction::MoveWorkspaceToOutput(_)),
                ) => self.send(Input::Action(action))?,
                HostRequest::Action(action @ DesktopAction::InspectWindowRecord) => {
                    self.send(Input::Action(action))?
                }
                HostRequest::Action(_) => {
                    eprintln!("This desktop action is not available in nested development yet.")
                }
            }
        }
        Ok(())
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
                    .configure_state(
                        window.surface,
                        [window.size[0] as i32, window.size[1] as i32],
                        window.state,
                    )
                    .is_err()
                {
                    self.server.reject_window(window.surface);
                }
            }
            composition.canvas(renderer, &completed.frame)?;
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
        self.server.presented_groups(presented.callbacks.iter().map(|(_, nodes)| nodes.as_slice()));
        let Some(frame) = self.pending.take().or_else(|| self.current.clone()) else {
            return Ok(());
        };
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
        self.server.presented_groups(presented.callbacks.iter()
            .filter(|(output, _)| output.is_none_or(|output| output == id))
            .map(|(_, nodes)| nodes.as_slice()));
        self.output_frames.insert(id, (frame.clone(), presented));
        self.capture_times.insert(id, presentation_time);
        if frame.frame.output == Some(id) || self.current.is_none() {
            self.adopt(frame);
        }
        self.refresh_presented(time)
    }

    fn adopt(&mut self, frame: Arc<Completed>) {
        self.server.workspaces(frame.frame.input.workspaces.clone());
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
        self.replay()?;
        self.focus()?;
        self.motion(time)
    }

    pub(crate) fn invalidate_outputs(&mut self) -> io::Result<()> {
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
        self.redraw = true;
        Ok(())
    }

    pub(crate) fn commands(&mut self, path: std::path::PathBuf, socket: std::path::PathBuf) {
        match crate::commands::Commands::load(&path) {
            Ok(commands) => {
                self.server.keyboard_repeat(
                    commands.keyboard.repeat_rate,
                    commands.keyboard.repeat_delay_ms,
                );
                #[cfg(feature = "session")]
                {
                    self.keyboard = commands.keyboard;
                }
            }
            Err(error) => eprintln!("Could not load Park commands: {error}"),
        }
        self.command_file = Some(path);
        self.socket = Some(socket);
    }

    fn command(&mut self, action: DesktopAction) {
        let result = (|| {
            let path = self
                .command_file
                .as_ref()
                .ok_or_else(|| io::Error::other("Park command configuration is unavailable"))?;
            let socket = self
                .socket
                .as_ref()
                .ok_or_else(|| io::Error::other("Park display is unavailable"))?;
            let description = crate::commands::Commands::load(path)?.command(action)?;
            self.server.spawn_helper(&description, socket)
        })();
        match result {
            Ok(child) => self.children.push(child),
            Err(error) => {
                let name = match action {
                    DesktopAction::OpenTerminal => "the terminal",
                    DesktopAction::OpenLauncher => "the application launcher",
                    DesktopAction::Lock => "the screen locker",
                    _ => "the selected program",
                };
                let message = format!("Could not start {name}: {error}");
                eprintln!("{message}");
                let _ = self.send(Input::Feedback(message));
            }
        }
    }

    pub(crate) fn ready(&mut self) {
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
            Ok(Some(child)) => self.children.push(child),
            Ok(None) => {}
            Err(error) => {
                let message = format!("Could not publish Park session readiness: {error}");
                eprintln!("{message}");
                let _ = self.send(Input::Feedback(message));
            }
        }
    }

    pub(crate) fn send(&self, input: Input) -> io::Result<()> {
        self.send_tracked(input).map(|_| ())
    }

    fn send_tracked(&self, input: Input) -> io::Result<u64> {
        self.worker
            .input(input)
            .map_err(|_| io::Error::other("the Park input queue is full or unavailable"))
    }

    pub(crate) fn focus(&mut self) -> io::Result<()> {
        let locked = self.server.locked();
        if locked != self.locked {
            self.server.clear_input(0);
            self.keys.clear();
            self.compose.reset();
            #[cfg(feature = "session")]
            {
                self.repeat = None;
            }
            self.buttons.clear();
            self.consumed.clear();
            self.grabbed = None;
            self.layer_focus = None;
            self.pending_focus = None;
            self.pending_output = None;
            self.deferred.clear();
            self.pointer_target = None;
            self.send(Input::Focus(false))?;
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
        if !self.focused {
            self.server.suspend_native_drop();
            self.server.native_editor(None);
            if self.native_keyboard {
                self.send(Input::Focus(false))?;
                self.native_keyboard = false;
            }
            self.server.focus(None, SERIAL_COUNTER.next_serial());
            return Ok(());
        }
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
        });
        let native = layer.is_none()
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
            self.send(Input::Focus(native))?;
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
