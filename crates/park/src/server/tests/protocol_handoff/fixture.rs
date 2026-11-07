use super::*;
use std::{
    fs::File,
    os::{fd::AsFd, unix::fs::FileExt},
    time::Duration,
};
use wayland_client::{
    self as client, Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

#[derive(Default)]
pub(super) struct Client {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    shell: Option<xdg_wm_base::XdgWmBase>,
    pub acked: Vec<u32>,
    pub sizes: Vec<(u32, [i32; 2])>,
    pub releases: usize,
    pub closes: usize,
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
                    state.compositor = Some(registry.bind(name, version.min(6), qh, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, version.min(1), qh, ())),
                "xdg_wm_base" => state.shell = Some(registry.bind(name, version.min(6), qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
    fn event(
        _: &mut Self,
        shell: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            shell.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, u32> for Client {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.acked.push(serial);
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, u32> for Client {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        tag: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                state.sizes.push((*tag, [width, height]))
            }
            xdg_toplevel::Event::Close => state.closes += 1,
            _ => {}
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
            state.releases += 1;
        }
    }
}

impl Dispatch<wl_callback::WlCallback, Arc<AtomicBool>> for Client {
    fn event(
        _: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        done: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        done.store(true, Ordering::Release);
    }
}

delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Client: ignore wl_surface::WlSurface);

pub(super) struct Window {
    pub surface: wl_surface::WlSurface,
    pub xdg: xdg_surface::XdgSurface,
    pub role: xdg_toplevel::XdgToplevel,
    pub buffer: Option<wl_buffer::WlBuffer>,
    pub pixels: Option<File>,
}

pub(super) struct Peer {
    pub connection: Connection,
    pub queue: EventQueue<Client>,
    pub client: Client,
}

impl Peer {
    pub fn connect(stream: UnixStream) -> Self {
        let connection = Connection::from_socket(stream).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        Self {
            connection,
            queue,
            client: Client::default(),
        }
    }

    pub fn sync(&mut self, mut step: impl FnMut()) -> io::Result<()> {
        let done = Arc::new(AtomicBool::new(false));
        self.connection
            .display()
            .sync(&self.queue.handle(), done.clone());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done.load(Ordering::Acquire) {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Production protocol roundtrip timed out",
                ));
            }
            self.connection.flush().map_err(io::Error::other)?;
            step();
            self.queue
                .dispatch_pending(&mut self.client)
                .map_err(io::Error::other)?;
            if let Some(read) = self.queue.prepare_read() {
                let fd = read.connection_fd();
                let mut descriptors = [rustix::event::PollFd::new(
                    &fd,
                    rustix::event::PollFlags::IN,
                )];
                rustix::event::poll(
                    &mut descriptors,
                    Some(&rustix::time::Timespec {
                        tv_sec: 0,
                        tv_nsec: 1_000_000,
                    }),
                )?;
                if descriptors[0].revents().intersects(
                    rustix::event::PollFlags::IN
                        | rustix::event::PollFlags::ERR
                        | rustix::event::PollFlags::HUP,
                ) {
                    match read.read() {
                        Ok(_) => {}
                        Err(client::backend::WaylandError::Io(error))
                            if error.kind() == io::ErrorKind::WouldBlock => {}
                        Err(error) => return Err(io::Error::other(error)),
                    }
                }
            }
            self.queue
                .dispatch_pending(&mut self.client)
                .map_err(io::Error::other)?;
        }
        Ok(())
    }

    pub fn window(&self, tag: u32) -> Window {
        let qh = self.queue.handle();
        let surface = self
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&qh, ());
        let xdg = self
            .client
            .shell
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &qh, tag);
        let role = xdg.get_toplevel(&qh, tag);
        role.set_title(format!("Private handoff window {tag}"));
        role.set_app_id("social.lince.handoff-proof".into());
        role.set_min_size(16, 12);
        role.set_max_size(2048, 2048);
        surface.commit();
        Window {
            surface,
            xdg,
            role,
            buffer: None,
            pixels: None,
        }
    }

    pub fn attach(&self, window: &mut Window, value: u8) {
        let file = tempfile::tempfile().unwrap();
        file.set_len(64 * 48 * 4).unwrap();
        file.write_all_at(&vec![value; 64 * 48 * 4], 0).unwrap();
        let qh = self.queue.handle();
        let pool =
            self.client
                .shm
                .as_ref()
                .unwrap()
                .create_pool(file.as_fd(), 64 * 48 * 4, &qh, ());
        let buffer = pool.create_buffer(0, 64, 48, 64 * 4, wl_shm::Format::Argb8888, &qh, ());
        pool.destroy();
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, 64, 48);
        window.surface.commit();
        window.buffer = Some(buffer);
        window.pixels = Some(file);
    }
}

pub(super) struct Fixture {
    pub server: Server,
    pub peer: Peer,
}

impl Fixture {
    pub fn new() -> Self {
        let mut server = Server::new([640, 480]).unwrap();
        let (source, target) = UnixStream::pair().unwrap();
        server.insert_client(source).unwrap();
        let peer = Peer::connect(target);
        let mut fixture = Self { server, peer };
        fixture.sync();
        fixture.sync();
        fixture
    }

    pub fn sync(&mut self) {
        self.peer.sync(|| self.server.dispatch().unwrap()).unwrap();
    }

    pub fn mapped_window(&mut self, tag: u32, color: u8) -> Window {
        let mut window = self.peer.window(tag);
        self.sync();
        self.sync();
        self.peer.attach(&mut window, color);
        self.sync();
        self.sync();
        window
    }
}

pub(super) fn pixels(server: &Server, id: SurfaceId) -> Vec<u8> {
    let content = server
        .state
        .buffers
        .get(server.state.windows.get(&id).unwrap().role.wl_surface())
        .unwrap();
    smithay::wayland::shm::with_buffer_contents(content.buffer(), |pointer, length, data| {
        assert!(length >= 4 && data.offset >= 0 && (data.offset as usize) < length - 3);
        unsafe { std::slice::from_raw_parts(pointer.add(data.offset as usize), 4).to_vec() }
    })
    .unwrap()
}

#[test]
fn production_protocol_restores_mapped_windows_buffers_configures_frames_and_further_requests() {
    let mut fixture = Fixture::new();
    let first = fixture.mapped_window(1, 17);
    let mut second = fixture.mapped_window(2, 29);
    let ids = fixture
        .server
        .windows()
        .iter()
        .map(|window| window.id)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    assert_eq!(pixels(&fixture.server, ids[0]), [17; 4]);
    assert_eq!(pixels(&fixture.server, ids[1]), [29; 4]);
    let frame = Arc::new(AtomicBool::new(false));
    first
        .surface
        .frame(&fixture.peer.queue.handle(), frame.clone());
    first.surface.commit();
    fixture.sync();
    let acks = fixture.peer.client.acked.len();
    fixture
        .server
        .configure_state(ids[0], [320, 240], WindowState::default())
        .unwrap();
    let snapshot = capture(&mut fixture.server).unwrap();
    let snapshot = Snapshot::decode(&snapshot.encode().unwrap(), snapshot.descriptors).unwrap();
    let replacement = restore(&snapshot, false).unwrap();
    let source = std::mem::replace(&mut fixture.server, replacement);
    drop(source);
    fixture.sync();
    fixture.sync();
    assert!(fixture.peer.client.acked.len() > acks);
    assert_eq!(fixture.peer.client.closes, 0);
    assert_eq!(fixture.peer.client.releases, 0);
    assert_eq!(
        fixture
            .server
            .windows()
            .iter()
            .map(|window| window.id)
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(pixels(&fixture.server, ids[0]), [17; 4]);
    assert_eq!(pixels(&fixture.server, ids[1]), [29; 4]);
    fixture.server.presented(&ids);
    fixture.sync();
    assert!(frame.load(Ordering::Acquire));
    second
        .pixels
        .as_ref()
        .unwrap()
        .write_all_at(&[41; 4], 0)
        .unwrap();
    second.surface.damage_buffer(0, 0, 1, 1);
    second.surface.commit();
    fixture.sync();
    assert_eq!(pixels(&fixture.server, ids[1]), [41; 4]);
    fixture.peer.attach(&mut second, 53);
    fixture.sync();
    assert_eq!(pixels(&fixture.server, ids[1]), [53; 4]);
    let third = fixture.mapped_window(3, 67);
    assert_eq!(fixture.server.windows().len(), 3);
    third.role.destroy();
    third.xdg.destroy();
    third.surface.destroy();
    fixture.sync();
    assert_eq!(fixture.server.windows().len(), 2);
    first.surface.damage_buffer(0, 0, 1, 1);
    first.surface.commit();
    fixture.sync();
}

#[test]
fn production_protocol_rejects_an_incomplete_graph_and_retains_the_original_server() {
    let mut fixture = Fixture::new();
    let first = fixture.mapped_window(1, 79);
    let snapshot = capture(&mut fixture.server).unwrap();
    let mut invalid = snapshot.duplicate().unwrap();
    invalid.state.server.surfaces.values.clear();
    assert!(restore(&invalid, false).is_err());
    first.surface.damage_buffer(0, 0, 1, 1);
    first.surface.commit();
    fixture.sync();
    assert_eq!(fixture.server.windows().len(), 1);
    assert_eq!(pixels(&fixture.server, SurfaceId(1)), [79; 4]);
    assert_eq!(fixture.peer.client.closes, 0);
}
