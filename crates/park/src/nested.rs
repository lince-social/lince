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

mod input;
mod renderer;
mod smoke;

#[derive(Default)]
pub struct Options {
    pub duration: Option<Duration>,
    pub smoke: bool,
    pub outputs: usize,
}

struct Desktop {
    server: Server,
    worker: CanvasWorker,
    size: [u32; 2],
    output_count: usize,
    current: Option<Completed>,
    layers: Vec<LayerSnapshot>,
    popups: Vec<renderer::PopupScene>,
    clips: Vec<(OutputId, Rectangle)>,
    pointer: [f64; 2],
    pointer_sent: Option<[f64; 2]>,
    pointer_stamp: u64,
    pointer_target: Option<(WlSurface, [f64; 2])>,
    grabbed: Option<input::Target>,
    buttons: HashSet<u32>,
    keys: HashMap<u32, crate::server::KeyEvent>,
    consumed: HashSet<u32>,
    native_keyboard: bool,
    focused: bool,
    layer_focus: Option<WlSurface>,
    pending_focus: Option<(u64, Option<lince_interface::host::SurfaceId>)>,
    pending_output: Option<(u64, Option<OutputId>)>,
    deferred: std::collections::VecDeque<input::Deferred>,
    scene_revision: u64,
    server_revision: u64,
    viewports: Vec<OutputViewport>,
    redraw: bool,
    running: bool,
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
    let (mut backend, mut events) =
        winit::init_from_attributes::<GlesRenderer>(attributes).map_err(error)?;
    let physical = backend.window_size();
    let size = [physical.w as u32, physical.h as u32];
    let mut server = Server::new([physical.w, physical.h])?;
    let output_count = options.outputs.clamp(1, 16);
    configure_outputs(&mut server, size, output_count)?;
    let socket = server.listen()?;
    eprintln!("Park Wayland display: {}", socket.display());
    let worker = CanvasWorker::spawn(Configuration {
        size,
        ..configuration
    })?;
    let mut desktop = Desktop {
        server,
        worker,
        size,
        output_count,
        current: None,
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
        focused: backend.window().has_focus(),
        layer_focus: None,
        pending_focus: None,
        pending_output: None,
        deferred: Default::default(),
        scene_revision: 0,
        server_revision: 0,
        viewports: Vec::new(),
        redraw: true,
        running: true,
    };
    let mut composition = renderer::Composition::default();
    let mut applications = Some(applications);
    let mut children: Vec<Child> = Vec::new();
    let started = Instant::now();
    let mut frames = 0;
    let mut seen_windows = HashSet::new();
    let mut seen_layers = HashSet::new();
    let mut smoke = smoke::Check::default();
    let result = (|| {
        while desktop.running
            && options
                .duration
                .is_none_or(|duration| started.elapsed() < duration)
        {
            let mut input = Vec::new();
            if let smithay::reexports::winit::platform::pump_events::PumpStatus::Exit(_) =
                events.dispatch_new_events(|event| input.push(event))
            {
                desktop.running = false;
            }
            for event in input {
                desktop.event(event)?;
            }
            desktop.server.dispatch()?;
            for (surface, request) in desktop.server.requests() {
                desktop.send(Input::Window(surface, request))?;
            }
            for surface in desktop.server.activations() {
                let serial = desktop.send_tracked(Input::Activate(surface))?;
                desktop.pending_output = Some((serial, None));
                desktop.pending_focus = Some((serial, Some(surface)));
            }
            if let Some(failure) = desktop.worker.failure() {
                return Err(error(failure));
            }
            if !desktop.worker.is_running() {
                return Err(io::Error::other("the Park canvas stopped"));
            }
            if desktop.server_revision != desktop.server.revision() {
                let outputs: Vec<_> = desktop
                    .server
                    .output_viewports()
                    .into_iter()
                    .map(|(name, viewport)| crate::presentation::Output { name, viewport })
                    .collect();
                let viewports: Vec<_> = outputs
                    .iter()
                    .map(|output| output.viewport.clone())
                    .collect();
                if desktop.scene_revision != desktop.server.window_revision()
                    || desktop.viewports != viewports
                {
                    desktop.worker.scene(Scene {
                        outputs,
                        windows: Arc::new(desktop.server.windows()),
                    })?;
                    desktop.scene_revision = desktop.server.window_revision();
                    desktop.viewports = viewports;
                }
                desktop.server_revision = desktop.server.revision();
                desktop.redraw = true;
            }
            while let Some(request) = desktop.worker.request() {
                match request {
                    HostRequest::Close(surface) => desktop.server.close(surface),
                    HostRequest::Action(DesktopAction::Logout) => desktop.running = false,
                    HostRequest::Action(
                        action @ (DesktopAction::FocusOutput(_)
                        | DesktopAction::MoveToOutput(_)
                        | DesktopAction::MoveWorkspaceToOutput(_)),
                    ) => desktop.send(Input::Action(action))?,
                    HostRequest::Action(action @ DesktopAction::InspectWindowRecord) => {
                        desktop.send(Input::Action(action))?
                    }
                    HostRequest::Action(_) => {
                        eprintln!("This desktop action is not available in nested development yet.")
                    }
                }
            }
            if let Some(completed) = desktop.worker.frame() {
                if desktop
                    .current
                    .as_ref()
                    .is_some_and(|current| current.frame.output != completed.frame.output)
                {
                    desktop.native_keyboard = false;
                }
                if desktop
                    .pending_focus
                    .is_some_and(|(serial, _)| completed.input_serial >= serial)
                {
                    desktop.pending_focus = None;
                }
                for id in &completed.rejected {
                    desktop.server.reject_window(*id);
                }
                for window in &completed.frame.windows {
                    if desktop
                        .server
                        .configure_state(
                            window.surface,
                            [window.size[0] as i32, window.size[1] as i32],
                            window.state,
                        )
                        .is_err()
                    {
                        desktop.server.reject_window(window.surface);
                    }
                }
                composition.canvas(&mut backend, &completed.frame)?;
                desktop.current = Some(completed);
                desktop.redraw = true;
                if let Some(applications) = applications.take() {
                    desktop.server.enable_dmabuf();
                    for application in applications {
                        match application.spawn(&socket) {
                            Ok(child) => children.push(child),
                            Err(error) => eprintln!("Could not start a Park application: {error}"),
                        }
                    }
                }
            }
            if desktop.redraw && backend.window_size().w > 0 && backend.window_size().h > 0 {
                let presented = composition.present(
                    &mut backend,
                    &mut desktop.server,
                    desktop.current.as_ref(),
                    desktop.pointer,
                )?;
                desktop.layers = presented.layers;
                desktop.popups = presented.popups;
                desktop.clips = presented.clips;
                frames += 1;
                seen_layers.extend(desktop.layers.iter().map(|layer| layer.surface.clone()));
                if let Some(geometry) = desktop
                    .current
                    .as_ref()
                    .and_then(|current| current.frame.geometry.as_ref())
                {
                    desktop.server.presented_outputs(
                        geometry,
                        desktop
                            .current
                            .as_ref()
                            .and_then(|current| current.frame.output),
                    );
                    seen_windows.extend(geometry.items.iter().filter_map(|item| {
                        if let ItemId::Application(id) = item.id {
                            Some(id)
                        } else {
                            None
                        }
                    }));
                }
                desktop.redraw = false;
                desktop.replay()?;
                desktop.focus()?;
                desktop.motion(started.elapsed().as_millis() as u32)?;
                if options.smoke && smoke.advance(&mut desktop)? {
                    desktop.running = false;
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
            std::thread::sleep(Duration::from_millis(4));
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
        checked: smoke.complete(),
    })
}

impl Desktop {
    fn send(&self, input: Input) -> io::Result<()> {
        self.send_tracked(input).map(|_| ())
    }

    fn send_tracked(&self, input: Input) -> io::Result<u64> {
        self.worker
            .input(input)
            .map_err(|_| io::Error::other("the Park input queue is full or unavailable"))
    }

    fn focus(&mut self) -> io::Result<()> {
        if !self.focused {
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
            self.server.focus(application, SERIAL_COUNTER.next_serial());
        }
        Ok(())
    }

    fn fullscreen(&self, output: OutputId) -> bool {
        self.current.as_ref().is_some_and(|current| {
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

fn error(error: impl std::fmt::Display) -> io::Error {
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
    server.configure_outputs(&monitors)
}
