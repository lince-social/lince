use super::*;
use smithay::{
    delegate_compositor, delegate_dmabuf, delegate_seat, delegate_shm, delegate_xdg_shell,
    input::{Seat, SeatHandler, SeatState, pointer::MotionEvent},
    reexports::wayland_server::{
        Client, Display,
        backend::{ClientData, ClientId, DisconnectReason},
        protocol::{wl_buffer, wl_seat, wl_surface::WlSurface},
    },
    utils::Serial,
    wayland::{
        buffer::BufferHandler,
        compositor::{
            BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
            SurfaceAttributes, with_states,
        },
        dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier, get_dmabuf},
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
        },
        shm::{ShmHandler, ShmState, with_buffer_contents},
    },
};
use std::{
    os::unix::net::UnixStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::{Duration, Instant},
};

mod client;

struct Commit {
    buffer: wl_buffer::WlBuffer,
    content: Content,
}

enum Content {
    Dma(Dmabuf),
    Shm { pixels: Vec<u8>, size: (i32, i32) },
}

fn shm_content(buffer: &wl_buffer::WlBuffer) -> std::result::Result<Content, String> {
    with_buffer_contents(buffer, |pointer, length, data| {
        if data.width <= 0
            || data.height <= 0
            || data.width > 4096
            || data.height > 4096
            || data.offset < 0
            || data.stride < data.width * 4
            || data.format != smithay::reexports::wayland_server::protocol::wl_shm::Format::Argb8888
        {
            return Err("unsupported SHM geometry or format".into());
        }
        let row = data.width as usize * 4;
        let end = (data.stride as usize)
            .checked_mul(data.height as usize - 1)
            .and_then(|value| value.checked_add(data.offset as usize))
            .and_then(|value| value.checked_add(row))
            .ok_or("SHM dimensions overflow")?;
        if end > length {
            return Err("SHM contents exceed the mapped pool".into());
        }
        let mut pixels = vec![0; row * data.height as usize];
        for y in 0..data.height as usize {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pointer.add(data.offset as usize + y * data.stride as usize),
                    pixels.as_mut_ptr().add(y * row),
                    row,
                )
            };
        }
        Ok(Content::Shm {
            pixels,
            size: (data.width, data.height),
        })
    })
    .map_err(|error| error.to_string())?
}

struct State {
    compositor: CompositorState,
    shell: XdgShellState,
    shm: ShmState,
    dmabuf: DmabufState,
    seats: SeatState<Self>,
    _seat: Seat<Self>,
    toplevel: Option<ToplevelSurface>,
    frames: SyncSender<Commit>,
    failure: Option<String>,
}

#[derive(Default)]
struct ClientState(CompositorClientState);

impl ClientData for ClientState {
    fn initialized(&self, _: ClientId) {}
    fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _: &wl_buffer::WlBuffer) {}
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }
    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().0
    }
    fn commit(&mut self, surface: &WlSurface) {
        let buffer = with_states(surface, |states| {
            states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .buffer
                .take()
        });
        if let Some(BufferAssignment::NewBuffer(buffer)) = buffer {
            let content = match get_dmabuf(&buffer) {
                Ok(dmabuf) => Ok(Content::Dma(dmabuf.clone())),
                Err(_) => shm_content(&buffer),
            };
            match content {
                Ok(content) => {
                    let commit = Commit {
                        buffer: buffer.clone(),
                        content,
                    };
                    if self.frames.try_send(commit).is_err() {
                        self.failure =
                            Some("graphics gate frame queue overflowed or closed".into());
                        buffer.release();
                    }
                }
                Err(error) => {
                    self.failure = Some(error);
                    buffer.release();
                }
            }
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf
    }
    fn dmabuf_imported(&mut self, _: &DmabufGlobal, buffer: Dmabuf, notifier: ImportNotifier) {
        use smithay::backend::allocator::{Buffer, Fourcc, Modifier};
        if buffer.num_planes() == 1
            && buffer.format().code == Fourcc::Argb8888
            && buffer.format().modifier == Modifier::Linear
        {
            if notifier.successful::<Self>().is_err() {
                self.failure = Some("client disappeared during buffer creation".into());
            }
        } else {
            notifier.failed();
        }
    }
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;
    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seats
    }
    fn focus_changed(&mut self, _: &Seat<Self>, _: Option<&WlSurface>) {}
    fn cursor_image(&mut self, _: &Seat<Self>, _: smithay::input::pointer::CursorImageStatus) {}
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.shell
    }
    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|state| {
            state.size = Some((128, 128).into());
        });
        surface.send_configure();
        self.toplevel = Some(surface);
    }
    fn new_popup(&mut self, surface: PopupSurface, _: PositionerState) {
        surface.send_popup_done();
    }
    fn grab(&mut self, surface: PopupSurface, _: wl_seat::WlSeat, _: Serial) {
        surface.send_popup_done();
    }
    fn reposition_request(&mut self, surface: PopupSurface, _: PositionerState, _: u32) {
        surface.send_popup_done();
    }
}

delegate_compositor!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_dmabuf!(State);
delegate_xdg_shell!(State);

fn serve(
    stream: UnixStream,
    frames: SyncSender<Commit>,
    control: Receiver<(i32, i32)>,
    stop: &AtomicBool,
) -> Result<()> {
    let mut display =
        Display::<State>::new().map_err(|error| ProbeFailure::at("wayland_display", error))?;
    let mut handle = display.handle();
    let mut seats = SeatState::new();
    let mut seat = seats.new_wl_seat(&handle, "park-gate");
    let pointer = seat.add_pointer();
    let mut dmabuf = DmabufState::new();
    let _global = dmabuf.create_global::<State>(
        &handle,
        [smithay::backend::allocator::Format {
            code: smithay::backend::allocator::Fourcc::Argb8888,
            modifier: smithay::backend::allocator::Modifier::Linear,
        }],
    );
    let mut state = State {
        compositor: CompositorState::new::<State>(&handle),
        shell: XdgShellState::new::<State>(&handle),
        shm: ShmState::new::<State>(&handle, vec![]),
        dmabuf,
        seats,
        _seat: seat,
        toplevel: None,
        frames,
        failure: None,
    };
    let _client = handle
        .insert_client(stream, Arc::new(ClientState::default()))
        .map_err(|error| ProbeFailure::at("wayland_client", error))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !stop.load(Ordering::Acquire) {
        display
            .dispatch_clients(&mut state)
            .map_err(|error| ProbeFailure::at("wayland_dispatch", error))?;
        if let Some(failure) = state.failure.take() {
            return Err(ProbeFailure::at("wayland_commit", failure));
        }
        if let Ok(size) = control.try_recv() {
            let surface = state
                .toplevel
                .clone()
                .ok_or_else(|| ProbeFailure::at("wayland_resize", "no configured toplevel"))?;
            surface.with_pending_state(|state| state.size = Some(size.into()));
            surface.send_configure();
            let mapping = lince_interface::host::ContentMapping { corners: None,
                canvas: lince_interface::host::Rectangle::new([320.0, 16.0], [256.0, 256.0])
                    .unwrap(),
                surface: lince_interface::host::Rectangle::new([0.0, 0.0], [128.0, 128.0]).unwrap(),
            };
            let point = mapping.surface_point([400.0, 80.0]).unwrap();
            pointer.motion(
                &mut state,
                Some((surface.wl_surface().clone(), (0.0, 0.0).into())),
                &MotionEvent {
                    location: (point[0], point[1]).into(),
                    serial: 1.into(),
                    time: 1,
                },
            );
            pointer.frame(&mut state);
        }
        display
            .flush_clients()
            .map_err(|error| ProbeFailure::at("wayland_flush", error))?;
        if Instant::now() >= deadline {
            return Err(ProbeFailure::at(
                "wayland_timeout",
                "live client proof exceeded 30 seconds",
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

pub(crate) fn with_client(
    renderer: &VulkanRenderer,
    report: &mut ProbeReport,
    mut render: impl FnMut(&wgpu::Texture) -> Result<()>,
) -> Result<()> {
    let (server_stream, client_stream) =
        UnixStream::pair().map_err(|error| ProbeFailure::at("wayland_socket", error))?;
    let (frames_tx, frames_rx) = sync_channel(2);
    let (control_tx, control_rx) = sync_channel(1);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let server = scope.spawn(|| serve(server_stream, frames_tx, control_rx, &stop));
        let client = scope.spawn(|| client::run(client_stream));
        let result = (|| {
            for expected in [(128, 128), (192, 96), (64, 64)] {
                let frame = frames_rx
                    .recv_timeout(Duration::from_secs(20))
                    .map_err(|error| ProbeFailure::at("wayland_frame", error))?;
                use smithay::backend::allocator::Buffer;
                let (texture, size) = match &frame.content {
                    Content::Dma(buffer) => (
                        renderer.import_dmabuf(buffer)?,
                        (buffer.size().w, buffer.size().h),
                    ),
                    Content::Shm { pixels, size } => {
                        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                            label: Some("park_shm_content"),
                            size: wgpu::Extent3d {
                                width: size.0 as u32,
                                height: size.1 as u32,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Bgra8Unorm,
                            usage: wgpu::TextureUsages::COPY_DST
                                | wgpu::TextureUsages::COPY_SRC
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        });
                        renderer.queue.write_texture(
                            texture.as_image_copy(),
                            pixels,
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(size.0 as u32 * 4),
                                rows_per_image: Some(size.1 as u32),
                            },
                            texture.size(),
                        );
                        (texture, *size)
                    }
                };
                if size != expected {
                    return Err(ProbeFailure::at(
                        "wayland_resize",
                        "client did not commit the configured dimensions",
                    ));
                }
                render(&texture)?;
                frame.buffer.release();
                if expected != (64, 64) {
                    control_tx
                        .send(if expected == (128, 128) {
                            (192, 96)
                        } else {
                            (64, 64)
                        })
                        .map_err(|error| ProbeFailure::at("wayland_control", error))?;
                }
            }
            Ok(())
        })();
        if result.is_err() {
            stop.store(true, Ordering::Release);
        }
        let client_result = client
            .join()
            .map_err(|_| ProbeFailure::at("wayland_client", "client thread panicked"))?;
        stop.store(true, Ordering::Release);
        let server_result = server
            .join()
            .map_err(|_| ProbeFailure::at("wayland_server", "server thread panicked"))?;
        result?;
        server_result?;
        client_result?;
        report.completed.push("live xdg-shell client: opaque and translucent DMA-BUF, SHM upload, configure/ack resize, mapped pointer coordinates, release after GPU completion".into());
        Ok(())
    })
}
