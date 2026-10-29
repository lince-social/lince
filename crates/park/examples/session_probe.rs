#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|argument| argument == "--help") {
        println!("session_probe [--hold-until RELEASE_FILE]: bounded native window lifecycle acceptance on WAYLAND_DISPLAY");
        return Ok(());
    }
    let release = match arguments.as_slice() {
        [] => None,
        [option, path] if option == "--hold-until" => Some(std::path::Path::new(path)),
        _ => return Err("Use session_probe [--hold-until RELEASE_FILE]".into()),
    };
    client::run(release)?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod client {
    use std::{fs::File, io::{self, Write}, os::fd::AsFd, time::{Duration, Instant}};
    use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop, protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
    }};
    use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

    #[derive(Default)]
    struct State {
        compositor: Option<wl_compositor::WlCompositor>,
        shell: Option<xdg_wm_base::XdgWmBase>,
        shm: Option<wl_shm::WlShm>,
        windows: Vec<Window>,
        synchronized: bool,
        failure: Option<String>,
    }

    struct Window {
        surface: wl_surface::WlSurface,
        xdg: xdg_surface::XdgSurface,
        role: xdg_toplevel::XdgToplevel,
        size: [i32; 2],
        fullscreen: bool,
        frames: usize,
        buffers: Vec<(wl_buffer::WlBuffer, File)>,
    }

    #[derive(Clone, Copy)]
    enum Callback {
        Frame(usize),
        Sync,
    }

    pub(super) fn run(release: Option<&std::path::Path>) -> io::Result<()> {
        let connection = Connection::connect_to_env().map_err(io::Error::other)?;
        let mut queue = connection.new_event_queue();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        let mut state = State::default();
        until(&connection, &mut queue, &mut state, |state| state.compositor.is_some() && state.shell.is_some() && state.shm.is_some())?;
        for index in 0..2 {
            let surface = state.compositor.as_ref().unwrap().create_surface(&qh, ());
            let xdg = state.shell.as_ref().unwrap().get_xdg_surface(&surface, &qh, index);
            let role = xdg.get_toplevel(&qh, index);
            role.set_title(format!("Park validation {}", index + 1));
            role.set_app_id("social.lince.park.validation".into());
            surface.commit();
            state.windows.push(Window { surface, xdg, role, size: [320, 200], fullscreen: false, frames: 0, buffers: Vec::new() });
        }
        until(&connection, &mut queue, &mut state, |state| state.windows.iter().all(|window| !window.buffers.is_empty()) && state.windows.iter().any(|window| window.frames > 0))?;
        println!("PASS: two native windows configured and a visible window received a presentation callback");
        if let Some(release) = release {
            use std::os::unix::fs::OpenOptionsExt;
            if !release.is_absolute() || release.exists() || release.with_extension("ready").exists() {
                return Err(io::Error::other("The native-window hold needs a new absolute private release path"));
            }
            let mut ready = std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(release.with_extension("pending"))?;
            ready.write_all(std::process::id().to_string().as_bytes())?;
            ready.sync_all()?;
            std::fs::rename(release.with_extension("pending"), release.with_extension("ready"))?;
            until_before(&connection, &mut queue, &mut state, |_| release.is_file(), Instant::now() + Duration::from_secs(60))?;
        }
        let before = state.windows[0].frames;
        eprintln!("Checking fullscreen entry on the first window while the second is focused");
        state.windows[0].role.set_fullscreen(None);
        until(&connection, &mut queue, &mut state, |state| state.windows[0].fullscreen && state.windows[0].frames > before)?;
        let before = state.windows[0].frames;
        eprintln!("Checking fullscreen restore");
        state.windows[0].role.unset_fullscreen();
        until(&connection, &mut queue, &mut state, |state| !state.windows[0].fullscreen && state.windows[0].frames > before)?;
        println!("PASS: fullscreen state entered and restored with new content callbacks");
        let window = &state.windows[1];
        window.surface.attach(None, 0, 0);
        window.surface.commit();
        state.synchronized = false;
        connection.display().sync(&qh, Callback::Sync);
        until(&connection, &mut queue, &mut state, |state| state.synchronized)?;
        let before = state.windows[1].frames;
        state.windows[1].surface.commit();
        state.windows[1].role.set_fullscreen(None);
        until(&connection, &mut queue, &mut state, |state| state.windows[1].fullscreen && state.windows[1].frames > before)?;
        println!("PASS: native window unmapped and remapped");
        for window in &state.windows {
            window.role.destroy();
            window.xdg.destroy();
            window.surface.destroy();
        }
        state.synchronized = false;
        connection.display().sync(&qh, Callback::Sync);
        until(&connection, &mut queue, &mut state, |state| state.synchronized)?;
        println!("PASS: both native windows destroyed; display still responds");
        Ok(())
    }

    fn until(connection: &Connection, queue: &mut EventQueue<State>, state: &mut State, finished: impl Fn(&State) -> bool) -> io::Result<()> {
        until_before(connection, queue, state, finished, Instant::now() + Duration::from_secs(12))
    }

    fn until_before(connection: &Connection, queue: &mut EventQueue<State>, state: &mut State, finished: impl Fn(&State) -> bool, deadline: Instant) -> io::Result<()> {
        loop {
            queue.dispatch_pending(state).map_err(io::Error::other)?;
            if let Some(error) = state.failure.take() {
                return Err(io::Error::other(error));
            }
            if finished(state) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::other("Native session probe phase timed out"));
            }
            if let Err(error) = connection.flush()
                && !matches!(&error, wayland_client::backend::WaylandError::Io(error) if error.kind() == io::ErrorKind::WouldBlock) {
                return Err(io::Error::other(error));
            }
            if let Some(read) = queue.prepare_read() {
                let fd = read.connection_fd();
                let mut descriptors = [rustix::event::PollFd::new(&fd, rustix::event::PollFlags::IN)];
                rustix::event::poll(&mut descriptors, Some(&rustix::time::Timespec { tv_sec: 0, tv_nsec: 100_000_000 })).map_err(io::Error::other)?;
                let flags = descriptors[0].revents();
                if flags.intersects(rustix::event::PollFlags::ERR | rustix::event::PollFlags::HUP) {
                    return Err(io::Error::other("Park disconnected the session probe"));
                }
                if flags.contains(rustix::event::PollFlags::IN) {
                    match read.read() {
                        Ok(_) => {}
                        Err(wayland_client::backend::WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
                        Err(error) => return Err(io::Error::other(error)),
                    }
                }
            }
        }
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for State {
        fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
            if let wl_registry::Event::Global { name, interface, version } = event {
                match interface.as_str() {
                    "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(4), qh, ())),
                    "xdg_wm_base" => state.shell = Some(registry.bind(name, version.min(6), qh, ())),
                    "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                    _ => {}
                }
            }
        }
    }

    impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
        fn event(_: &mut Self, shell: &xdg_wm_base::XdgWmBase, event: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let xdg_wm_base::Event::Ping { serial } = event {
                shell.pong(serial);
            }
        }
    }

    impl Dispatch<xdg_toplevel::XdgToplevel, usize> for State {
        fn event(state: &mut Self, _: &xdg_toplevel::XdgToplevel, event: xdg_toplevel::Event, index: &usize, _: &Connection, _: &QueueHandle<Self>) {
            match event {
                xdg_toplevel::Event::Configure { width, height, states } => {
                    eprintln!("Window {index}: configure {width}x{height}, states {states:?}");
                    if width > 8192 || height > 8192 || width < 0 || height < 0 {
                        state.failure = Some("Invalid or oversized configure".into());
                        return;
                    }
                    let window = &mut state.windows[*index];
                    if width > 0 { window.size[0] = width; }
                    if height > 0 { window.size[1] = height; }
                    window.fullscreen = states.chunks_exact(4).any(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()) == xdg_toplevel::State::Fullscreen as u32);
                }
                xdg_toplevel::Event::Close => state.failure = Some("A probe window was unexpectedly closed".into()),
                _ => {}
            }
        }
    }

    impl Dispatch<xdg_surface::XdgSurface, usize> for State {
        fn event(state: &mut Self, surface: &xdg_surface::XdgSurface, event: xdg_surface::Event, index: &usize, _: &Connection, qh: &QueueHandle<Self>) {
            if let xdg_surface::Event::Configure { serial } = event {
                surface.ack_configure(serial);
                let result = (|| -> io::Result<()> {
                    let window = &mut state.windows[*index];
                    if window.buffers.len() >= 32 {
                        return Err(io::Error::other("Probe configure/buffer limit exceeded"));
                    }
                    let [width, height] = window.size;
                    let length = width.checked_mul(height).and_then(|pixels| pixels.checked_mul(4)).filter(|length| *length <= 256 * 1024 * 1024)
                        .ok_or_else(|| io::Error::other("Probe buffer size limit exceeded"))?;
                    let mut file = tempfile::tempfile()?;
                    let pixel = if *index == 0 { [0x40, 0x90, 0xe0, 0xff] } else { [0x90, 0xd0, 0x40, 0xff] };
                    let row = pixel.repeat(width as usize);
                    for _ in 0..height { file.write_all(&row)?; }
                    let pool = state.shm.as_ref().unwrap().create_pool(file.as_fd(), length, qh, ());
                    let buffer = pool.create_buffer(0, width, height, width * 4, wl_shm::Format::Argb8888, qh, ());
                    pool.destroy();
                    window.surface.attach(Some(&buffer), 0, 0);
                    window.surface.damage_buffer(0, 0, width, height);
                    window.surface.frame(qh, Callback::Frame(*index));
                    window.surface.commit();
                    window.buffers.push((buffer, file));
                    Ok(())
                })();
                if let Err(error) = result { state.failure = Some(error.to_string()); }
            }
        }
    }

    impl Dispatch<wl_callback::WlCallback, Callback> for State {
        fn event(state: &mut Self, _: &wl_callback::WlCallback, event: wl_callback::Event, callback: &Callback, _: &Connection, _: &QueueHandle<Self>) {
            if let wl_callback::Event::Done { .. } = event {
                match callback {
                    Callback::Frame(index) => state.windows[*index].frames += 1,
                    Callback::Sync => state.synchronized = true,
                }
            }
        }
    }

    delegate_noop!(State: ignore wl_compositor::WlCompositor);
    delegate_noop!(State: ignore wl_shm::WlShm);
    delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
    delegate_noop!(State: ignore wl_buffer::WlBuffer);
    delegate_noop!(State: ignore wl_surface::WlSurface);
}
