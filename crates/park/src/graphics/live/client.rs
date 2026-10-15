use super::*;
use smithay::backend::allocator::Buffer;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::{
    wp::linux_dmabuf::zv1::client::{zwp_linux_buffer_params_v1, zwp_linux_dmabuf_v1},
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

struct State {
    renderer: VulkanRenderer,
    compositor: Option<wl_compositor::WlCompositor>,
    shell: Option<xdg_wm_base::XdgWmBase>,
    dmabuf: Option<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1>,
    shm: Option<wl_shm::WlShm>,
    surface: Option<wl_surface::WlSurface>,
    size: (u32, u32),
    buffers: Vec<(wl_buffer::WlBuffer, Option<ExportedImage>)>,
    releases: usize,
    pointer_seen: bool,
    failure: Option<ProbeFailure>,
}

pub(super) fn run(stream: UnixStream) -> Result<()> {
    let connection = Connection::from_socket(stream)
        .map_err(|error| ProbeFailure::at("client_connection", error))?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    let mut state = State {
        renderer: VulkanRenderer::new()?,
        compositor: None,
        shell: None,
        dmabuf: None,
        shm: None,
        surface: None,
        size: (128, 128),
        buffers: Vec::new(),
        releases: 0,
        pointer_seen: false,
        failure: None,
    };
    connection.display().get_registry(&qh, ());
    queue
        .roundtrip(&mut state)
        .map_err(|error| ProbeFailure::at("client_registry", error))?;
    let surface = state
        .compositor
        .as_ref()
        .ok_or_else(|| ProbeFailure::at("client_registry", "missing compositor"))?
        .create_surface(&qh, ());
    let xdg = state
        .shell
        .as_ref()
        .ok_or_else(|| ProbeFailure::at("client_registry", "missing xdg-shell"))?
        .get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_title("Park graphics gate".into());
    toplevel.set_app_id("social.lince.park.graphics-gate".into());
    surface.commit();
    state.surface = Some(surface);
    while state.releases < 3 || !state.pointer_seen {
        queue
            .blocking_dispatch(&mut state)
            .map_err(|error| ProbeFailure::at("client_dispatch", error))?;
        if let Some(error) = state.failure.take() {
            return Err(error);
        }
    }
    Ok(())
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => state.compositor = Some(registry.bind(name, 4, qh, ())),
                "xdg_wm_base" => state.shell = Some(registry.bind(name, 1, qh, ())),
                "zwp_linux_dmabuf_v1" => state.dmabuf = Some(registry.bind(name, 3, qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "wl_seat" => {
                    registry
                        .bind::<wl_seat::WlSeat, _, _>(name, 5, qh, ())
                        .get_pointer(qh, ());
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
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

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Configure { width, height, .. } = event {
            if width > 0 && height > 0 {
                state.size = (width as u32, height as u32);
            }
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
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
            let result = (|| {
                if state.size.0 == 64 {
                    use std::{io::Write, os::fd::AsFd};
                    let fd = rustix::fs::memfd_create(
                        c"park-shm-proof",
                        rustix::fs::MemfdFlags::CLOEXEC,
                    )
                    .map_err(|error| ProbeFailure::at("client_shm", error))?;
                    let mut file = std::fs::File::from(fd);
                    let pixels = [255_u8, 64, 0, 255].repeat(64 * 64);
                    file.write_all(&pixels)
                        .map_err(|error| ProbeFailure::at("client_shm", error))?;
                    let pool = state
                        .shm
                        .as_ref()
                        .ok_or_else(|| ProbeFailure::at("client_shm", "missing SHM global"))?
                        .create_pool(file.as_fd(), pixels.len() as i32, qh, ());
                    let buffer =
                        pool.create_buffer(0, 64, 64, 256, wl_shm::Format::Argb8888, qh, ());
                    pool.destroy();
                    let surface = state.surface.as_ref().unwrap();
                    surface.attach(Some(&buffer), 0, 0);
                    surface.damage_buffer(0, 0, 64, 64);
                    surface.commit();
                    state.buffers.push((buffer, None));
                    return Ok(());
                }
                let image = state
                    .renderer
                    .create_shared_image(state.size.0, state.size.1)?;
                let alpha = if state.size.0 == 192 { 0.5 } else { 1.0 };
                let image = state.renderer.clear(
                    image,
                    wgpu::Color {
                        r: 0.0,
                        g: 0.25 * alpha,
                        b: alpha,
                        a: alpha,
                    },
                )?;
                let params = state
                    .dmabuf
                    .as_ref()
                    .ok_or_else(|| ProbeFailure::at("client_buffer", "missing DMA-BUF global"))?
                    .create_params(qh, ());
                params.add(
                    image.dmabuf.handles().next().unwrap(),
                    0,
                    image.dmabuf.offsets().next().unwrap(),
                    image.dmabuf.strides().next().unwrap(),
                    0,
                    0,
                );
                let buffer = params.create_immed(
                    state.size.0 as i32,
                    state.size.1 as i32,
                    image.dmabuf.format().code as u32,
                    zwp_linux_buffer_params_v1::Flags::empty(),
                    qh,
                    (),
                );
                params.destroy();
                let surface = state.surface.as_ref().unwrap();
                surface.attach(Some(&buffer), 0, 0);
                surface.damage_buffer(0, 0, state.size.0 as i32, state.size.1 as i32);
                surface.commit();
                state.buffers.push((buffer, Some(image)));
                Ok(())
            })();
            if let Err(error) = result {
                state.failure = Some(error);
            }
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            if let Some(index) = state
                .buffers
                .iter()
                .position(|(buffer, _)| buffer.id() == proxy.id())
            {
                let (buffer, _) = state.buffers.swap_remove(index);
                buffer.destroy();
                state.releases += 1;
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Enter {
            surface_x,
            surface_y,
            ..
        } = event
        {
            state.pointer_seen = surface_x == 40.0 && surface_y == 32.0;
            if !state.pointer_seen {
                state.failure = Some(ProbeFailure::at(
                    "client_pointer",
                    format!("expected [40,32], got [{surface_x},{surface_y}]"),
                ));
            }
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1);
delegate_noop!(State: ignore zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1);
