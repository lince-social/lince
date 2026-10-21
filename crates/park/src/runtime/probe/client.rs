use super::*;
use std::{
    io::{Seek, SeekFrom, Write},
    os::fd::AsFd,
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_subcompositor,
        wl_subsurface, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

#[derive(Default)]
struct Client {
    compositor: Option<wl_compositor::WlCompositor>,
    subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    shell: Option<xdg_wm_base::XdgWmBase>,
    shm: Option<wl_shm::WlShm>,
    surface: Option<wl_surface::WlSurface>,
    xdg: Option<xdg_surface::XdgSurface>,
    role: Option<xdg_toplevel::XdgToplevel>,
    drawn: bool,
    closed: bool,
    releases: Option<Arc<AtomicUsize>>,
    error: Option<ProbeFailure>,
    files: Vec<std::fs::File>,
    root_buffer: Option<wl_buffer::WlBuffer>,
    remapping: bool,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    bars: Vec<(
        wl_surface::WlSurface,
        zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    )>,
    popup: Option<(
        wl_surface::WlSurface,
        xdg_surface::XdgSurface,
        xdg_popup::XdgPopup,
    )>,
}

pub(super) enum Control {
    Unmap,
    Remap,
    Fullscreen(bool),
    Maximized(bool),
}

pub(super) fn run(
    stream: UnixStream,
    stop: Arc<AtomicBool>,
    releases: Arc<AtomicUsize>,
    control: Receiver<Control>,
    bars: bool,
) -> Result<(), ProbeFailure> {
    let connection = Connection::from_socket(stream).map_err(|error| failure(error.to_string()))?;
    let mut queue: EventQueue<Client> = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut client = Client {
        releases: Some(releases),
        ..Default::default()
    };
    let deadline = Instant::now() + Duration::from_secs(90);
    while !stop.load(Ordering::Acquire) && !client.closed && Instant::now() < deadline {
        while let Ok(command) = control.try_recv() {
            match command {
                Control::Unmap => {
                    let surface = client.surface.as_ref().unwrap();
                    surface.attach(None, 0, 0);
                    surface.commit();
                    let file = &mut client.files[1];
                    file.seek(SeekFrom::Start(0))
                        .map_err(|error| failure(error.to_string()))?;
                    file.write_all(&vec![0; 80 * 40 * 4])
                        .map_err(|error| failure(error.to_string()))?;
                }
                Control::Remap => {
                    client.remapping = true;
                    client.surface.as_ref().unwrap().commit();
                }
                Control::Fullscreen(enabled) => {
                    if let Some(role) = &client.role {
                        if enabled {
                            role.set_fullscreen(None);
                        } else {
                            role.unset_fullscreen();
                        }
                    }
                }
                Control::Maximized(enabled) => {
                    if let Some(role) = &client.role {
                        if enabled {
                            role.set_maximized();
                        } else {
                            role.unset_maximized();
                        }
                    }
                }
            }
        }
        if let (Some(compositor), Some(shell), Some(_), Some(_)) = (
            &client.compositor,
            &client.shell,
            &client.subcompositor,
            &client.shm,
        ) && client.surface.is_none()
        {
            let surface = compositor.create_surface(&qh, ());
            let xdg = shell.get_xdg_surface(&surface, &qh, ());
            let role = xdg.get_toplevel(&qh, ());
            role.set_title("Park worker proof".into());
            role.set_app_id("social.lince.park.worker-proof".into());
            role.set_min_size(180, 100);
            if bars {
                role.set_max_size(180, 100);
            }
            surface.commit();
            client.surface = Some(surface);
            client.xdg = Some(xdg);
            client.role = Some(role);
        }
        if bars
            && client.bars.is_empty()
            && let (Some(shell), Some(compositor)) = (&client.layer_shell, &client.compositor)
        {
            for (index, edge, height) in [
                (0, zwlr_layer_surface_v1::Anchor::Top, 32),
                (1, zwlr_layer_surface_v1::Anchor::Bottom, 24),
            ] {
                let surface = compositor.create_surface(&qh, ());
                let role = shell.get_layer_surface(
                    &surface,
                    None,
                    zwlr_layer_shell_v1::Layer::Top,
                    "park-smoke-bar".into(),
                    &qh,
                    index,
                );
                role.set_anchor(
                    edge | zwlr_layer_surface_v1::Anchor::Left
                        | zwlr_layer_surface_v1::Anchor::Right,
                );
                role.set_size(0, height);
                role.set_exclusive_zone(height as i32);
                surface.commit();
                client.bars.push((surface, role));
            }
        }
        if bars && client.drawn && client.popup.is_none() {
            let compositor = client.compositor.as_ref().unwrap();
            let shell = client.shell.as_ref().unwrap();
            let positioner = shell.create_positioner(&qh, ());
            positioner.set_size(40, 24);
            positioner.set_anchor_rect(178, 10, 1, 1);
            positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
            positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
            positioner.set_constraint_adjustment(
                xdg_positioner::ConstraintAdjustment::SlideX
                    | xdg_positioner::ConstraintAdjustment::SlideY,
            );
            positioner.set_reactive();
            let surface = compositor.create_surface(&qh, ());
            let xdg = shell.get_xdg_surface(&surface, &qh, ());
            let popup = xdg.get_popup(client.xdg.as_ref(), &positioner, &qh, ());
            surface.commit();
            client.popup = Some((surface, xdg, popup));
        }
        if let Err(error) = connection.flush()
            && !matches!(&error, wayland_client::backend::WaylandError::Io(error) if error.kind() == std::io::ErrorKind::WouldBlock) {
            return Err(failure(error.to_string()));
        }
        queue
            .dispatch_pending(&mut client)
            .map_err(|error| failure(error.to_string()))?;
        if let Some(read) = queue.prepare_read() {
            let fd = read.connection_fd();
            let mut poll = [rustix::event::PollFd::new(
                &fd,
                rustix::event::PollFlags::IN,
            )];
            rustix::event::poll(
                &mut poll,
                Some(&rustix::time::Timespec {
                    tv_sec: 0,
                    tv_nsec: 20_000_000,
                }),
            )
            .map_err(|error| failure(error.to_string()))?;
            if poll[0].revents().contains(rustix::event::PollFlags::IN) {
                if let Err(error) = read.read() {
                    if matches!(&error, wayland_client::backend::WaylandError::Io(error) if error.kind() == std::io::ErrorKind::WouldBlock) {
                        continue;
                    }
                    if stop.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    return Err(failure(error.to_string()));
                }
            }
        }
        queue
            .dispatch_pending(&mut client)
            .map_err(|error| failure(error.to_string()))?;
        if let Some(error) = client.error.take() {
            return Err(error);
        }
    }
    Ok(())
}

impl Client {
    fn buffer(
        &mut self,
        size: [i32; 2],
        format: wl_shm::Format,
        pixels: &[u8],
        qh: &QueueHandle<Self>,
    ) -> Result<wl_buffer::WlBuffer, ProbeFailure> {
        let mut file = tempfile::tempfile().map_err(|error| failure(error.to_string()))?;
        file.write_all(pixels)
            .map_err(|error| failure(error.to_string()))?;
        let pool =
            self.shm
                .as_ref()
                .unwrap()
                .create_pool(file.as_fd(), pixels.len() as i32, qh, ());
        let buffer = pool.create_buffer(0, size[0], size[1], size[0] * 4, format, qh, ());
        pool.destroy();
        self.files.push(file);
        Ok(buffer)
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) -> Result<(), ProbeFailure> {
        let root = self.buffer(
            [400, 240],
            wl_shm::Format::Xrgb8888,
            &[255, 0, 0, 0].repeat(400 * 240),
            qh,
        )?;
        let mut pixels = Vec::with_capacity(80 * 40 * 4);
        for y in 0..40 {
            for x in 0..80 {
                pixels.extend_from_slice(match (x < 40, y < 20) {
                    (true, true) => &[0, 0, 255, 255],
                    (false, true) => &[0, 255, 0, 255],
                    (true, false) => &[255, 0, 0, 255],
                    (false, false) => &[255, 255, 255, 255],
                });
            }
        }
        let buffer = self.buffer([80, 40], wl_shm::Format::Argb8888, &pixels, qh)?;
        let surface = self.surface.as_ref().unwrap();
        let child = self.compositor.as_ref().unwrap().create_surface(qh, ());
        let sub = self
            .subcompositor
            .as_ref()
            .unwrap()
            .get_subsurface(&child, surface, qh, ());
        sub.set_position(20, 20);
        child.set_buffer_scale(2);
        child.set_buffer_transform(wl_output::Transform::_90);
        child.attach(Some(&buffer), 0, 0);
        child.damage_buffer(0, 0, 80, 40);
        child.commit();
        self.xdg
            .as_ref()
            .unwrap()
            .set_window_geometry(5, 7, 180, 100);
        surface.set_buffer_scale(2);
        surface.attach(Some(&root), 0, 0);
        surface.damage_buffer(0, 0, 400, 240);
        surface.commit();
        self.root_buffer = Some(root);
        self.drawn = true;
        Ok(())
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()))
                }
                "wl_subcompositor" => state.subcompositor = Some(registry.bind(name, 1, qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "xdg_wm_base" => state.shell = Some(registry.bind(name, version.min(6), qh, ())),
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
    fn event(
        _: &mut Self,
        proxy: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            proxy.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for Client {
    fn event(
        state: &mut Self,
        proxy: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            proxy.ack_configure(serial);
            if let Some((surface, xdg, _)) = &state.popup
                && xdg == proxy
            {
                let surface = surface.clone();
                match state.buffer(
                    [40, 24],
                    wl_shm::Format::Argb8888,
                    &[80, 0, 255, 255].repeat(40 * 24),
                    qh,
                ) {
                    Ok(buffer) => {
                        proxy.set_window_geometry(0, 0, 40, 24);
                        surface.attach(Some(&buffer), 0, 0);
                        surface.damage_buffer(0, 0, 40, 24);
                        surface.commit();
                    }
                    Err(error) => state.error = Some(error),
                }
                return;
            }
            if state.remapping {
                proxy.set_window_geometry(5, 7, 180, 100);
                let surface = state.surface.as_ref().unwrap();
                surface.attach(state.root_buffer.as_ref(), 0, 0);
                surface.commit();
                state.remapping = false;
            }
            if !state.drawn
                && let Err(error) = state.draw(qh)
            {
                state.error = Some(error);
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for Client {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Close = event {
            state.closed = true;
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            state
                .releases
                .as_ref()
                .unwrap()
                .fetch_add(1, Ordering::AcqRel);
        }
    }
}

delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore xdg_positioner::XdgPositioner);
delegate_noop!(Client: ignore xdg_popup::XdgPopup);
delegate_noop!(Client: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(Client: ignore wl_subsurface::WlSubsurface);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Client: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, usize> for Client {
    fn event(
        state: &mut Self,
        proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        index: &usize,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            proxy.ack_configure(serial);
            let color = if *index == 0 {
                [0, 160, 240, 255]
            } else {
                [160, 80, 0, 255]
            };
            match state.buffer(
                [width as i32, height as i32],
                wl_shm::Format::Argb8888,
                &color.repeat(width as usize * height as usize),
                qh,
            ) {
                Ok(buffer) => {
                    let surface = &state.bars[*index].0;
                    surface.attach(Some(&buffer), 0, 0);
                    surface.damage_buffer(0, 0, width as i32, height as i32);
                    surface.commit();
                }
                Err(error) => state.error = Some(error),
            }
        }
    }
}
