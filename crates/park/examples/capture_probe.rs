#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let report = arguments.next().ok_or("Provide a report path, or --help")?;
    if report == "--help" { println!("capture_probe REPORT [--image PATH] [--application APP_ID]: capture an authorized output and optionally require a mapped application"); return Ok(()); }
    let mut image = None;
    let mut application = None;
    while let Some(option) = arguments.next() {
        if option == "--image" { image = Some(arguments.next().ok_or("--image needs a new PPM filename")?); }
        else if option == "--application" { application = Some(arguments.next().ok_or("--application needs an app ID")?.into_string().map_err(|_| "Invalid application ID")?); }
        else { return Err("Unexpected capture argument".into()); }
    }
    client::run(std::path::Path::new(&report), image.as_deref().map(std::path::Path::new), application.as_deref())?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod client {
    use std::{fs::OpenOptions, io::{self, Read, Seek, Write}, os::{fd::AsFd, unix::fs::OpenOptionsExt}, path::Path, time::{Duration, Instant}};
    use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
        protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool}};
    use wayland_protocols::ext::{image_capture_source::v1::client::{ext_image_capture_source_v1::ExtImageCaptureSourceV1 as Source,
        ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1 as Factory},
        image_copy_capture::v1::client::{ext_image_copy_capture_manager_v1::{self as manager, ExtImageCopyCaptureManagerV1 as Manager},
            ext_image_copy_capture_session_v1::{self as session, ExtImageCopyCaptureSessionV1 as Session},
            ext_image_copy_capture_frame_v1::{self as frame, ExtImageCopyCaptureFrameV1 as Frame}}};
    use wayland_protocols::ext::foreign_toplevel_list::v1::client::{ext_foreign_toplevel_list_v1 as list, ext_foreign_toplevel_handle_v1 as handle};

    #[derive(Default)]
    struct State {
        output: Option<wl_output::WlOutput>, shm: Option<wl_shm::WlShm>, factory: Option<Factory>, manager: Option<Manager>,
        size: Option<[u32; 2]>, argb: bool, constraints: bool, ready: bool, normal: bool, time: bool, damage: bool, failure: Option<String>,
        foreign: Option<list::ExtForeignToplevelListV1>,
        windows: Vec<(handle::ExtForeignToplevelHandleV1, String, String)>,
    }

    pub(super) fn run(report: &Path, image: Option<&Path>, application: Option<&str>) -> io::Result<()> {
        let connection = Connection::connect_to_env().map_err(io::Error::other)?;
        let mut queue = connection.new_event_queue();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        let mut state = State::default();
        until(&connection, &mut queue, &mut state, |state| state.output.is_some() && state.shm.is_some() && state.factory.is_some() && state.manager.is_some())?;
        if let Some(application) = application {
            until(&connection, &mut queue, &mut state, |state| state.windows.iter().any(|(_, app_id, _)| app_id.to_lowercase().contains(&application.to_lowercase())))?;
        }
        let source = state.factory.as_ref().unwrap().create_source(state.output.as_ref().unwrap(), &qh, ());
        let session = state.manager.as_ref().unwrap().create_session(&source, manager::Options::empty(), &qh, ());
        until(&connection, &mut queue, &mut state, |state| state.constraints)?;
        let size = state.size.filter(|size| size.iter().all(|size| *size > 0)).ok_or_else(|| io::Error::other("Capture size is unavailable"))?;
        if !state.argb { return Err(io::Error::other("The capture provider did not advertise ARGB8888 SHM")); }
        let length = u64::from(size[0]).checked_mul(u64::from(size[1])).and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes <= 64 * 1024 * 1024).ok_or_else(|| io::Error::other("Capture exceeds the probe memory budget"))?;
        let mut file = tempfile::tempfile()?;
        file.set_len(length)?;
        let pool = state.shm.as_ref().unwrap().create_pool(file.as_fd(), length as i32, &qh, ());
        let buffer = pool.create_buffer(0, size[0] as i32, size[1] as i32, size[0] as i32 * 4, wl_shm::Format::Argb8888, &qh, ());
        pool.destroy();
        let frame = session.create_frame(&qh, ());
        frame.attach_buffer(&buffer);
        frame.capture();
        until(&connection, &mut queue, &mut state, |state| state.ready)?;
        if !state.normal || !state.time || !state.damage { return Err(io::Error::other("Capture completed without its transform, timestamp or initial damage")); }
        file.rewind()?;
        let mut pixels = vec![0; length as usize];
        file.read_exact(&mut pixels)?;
        let opaque = pixels.chunks_exact(4).filter(|pixel| pixel[3] == 255).count();
        let colored = pixels.chunks_exact(4).filter(|pixel| pixel[..3].iter().any(|channel| *channel != 0)).count();
        if opaque == 0 || colored == 0 { return Err(io::Error::other("The captured Park scene is empty or black")); }
        if let Some(path) = image {
            let mut image = OpenOptions::new().create_new(true).write(true).mode(0o600).open(path)?;
            write!(image, "P6\n{} {}\n255\n", size[0], size[1])?;
            let rgb: Vec<_> = pixels.chunks_exact(4).flat_map(|pixel| [pixel[2], pixel[1], pixel[0]]).collect();
            image.write_all(&rgb)?;
            image.sync_all()?;
        }
        let result = serde_json::json!({"status": "passed", "size": size, "opaque_pixels": opaque, "colored_pixels": colored,
            "applications": state.windows.iter().map(|(_, app_id, title)| serde_json::json!({"app_id": app_id, "title": title})).collect::<Vec<_>>(),
            "checks": ["authorized-output-source", "shm-export", "ready", "normal-transform", "monotonic-timestamp", "initial-damage", "nonempty-scene"],
            "limits": "This does not establish visual fidelity, cursor policy, window capture, portal consent or physical scanout."});
        let temporary = report.with_extension("pending");
        let mut report_file = OpenOptions::new().create_new(true).write(true).mode(0o600).open(&temporary)?;
        report_file.write_all(&serde_json::to_vec_pretty(&result).map_err(io::Error::other)?)?;
        report_file.write_all(b"\n")?;
        report_file.sync_all()?;
        std::fs::rename(temporary, report)?;
        frame.destroy();
        session.destroy();
        source.destroy();
        buffer.destroy();
        if let Err(error) = connection.flush()
            && !matches!(&error, wayland_client::backend::WaylandError::Io(error) if error.kind() == io::ErrorKind::WouldBlock) {
            return Err(io::Error::other(error));
        }
        println!("PASS: {}x{} output captured through the authorized image-copy protocol into SHM", size[0], size[1]);
        Ok(())
    }

    fn until(connection: &Connection, queue: &mut EventQueue<State>, state: &mut State, done: impl Fn(&State) -> bool) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            queue.dispatch_pending(state).map_err(io::Error::other)?;
            if let Some(failure) = state.failure.take() { return Err(io::Error::other(failure)); }
            if done(state) { return Ok(()); }
            if Instant::now() >= deadline { return Err(io::Error::other("Output capture probe timed out")); }
            if let Err(error) = connection.flush()
                && !matches!(&error, wayland_client::backend::WaylandError::Io(error) if error.kind() == io::ErrorKind::WouldBlock) {
                return Err(io::Error::other(error));
            }
            if let Some(read) = queue.prepare_read() {
                let fd = read.connection_fd();
                let mut descriptors = [rustix::event::PollFd::new(&fd, rustix::event::PollFlags::IN)];
                rustix::event::poll(&mut descriptors, Some(&rustix::time::Timespec { tv_sec: 0, tv_nsec: 100_000_000 })).map_err(io::Error::other)?;
                let flags = descriptors[0].revents();
                if flags.intersects(rustix::event::PollFlags::ERR | rustix::event::PollFlags::HUP) { return Err(io::Error::other("Park disconnected the capture probe")); }
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
                    "wl_output" if state.output.is_none() => state.output = Some(registry.bind(name, version.min(4), qh, ())),
                    "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                    "ext_output_image_capture_source_manager_v1" => state.factory = Some(registry.bind(name, 1, qh, ())),
                    "ext_image_copy_capture_manager_v1" => state.manager = Some(registry.bind(name, 1, qh, ())),
                    "ext_foreign_toplevel_list_v1" => state.foreign = Some(registry.bind(name, 1, qh, ())),
                    _ => {}
                }
            }
        }
    }

    impl Dispatch<list::ExtForeignToplevelListV1, ()> for State {
        fn event(state: &mut Self, _: &list::ExtForeignToplevelListV1, event: list::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let list::Event::Toplevel { toplevel } = event { state.windows.push((toplevel, String::new(), String::new())); }
        }
        wayland_client::event_created_child!(State, list::ExtForeignToplevelListV1, [0 => (handle::ExtForeignToplevelHandleV1, ())]);
    }

    impl Dispatch<handle::ExtForeignToplevelHandleV1, ()> for State {
        fn event(state: &mut Self, proxy: &handle::ExtForeignToplevelHandleV1, event: handle::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            let Some((_, app_id, title)) = state.windows.iter_mut().find(|(window, _, _)| window == proxy) else { return };
            match event {
                handle::Event::AppId { app_id: value } => *app_id = value,
                handle::Event::Title { title: value } => *title = value,
                handle::Event::Closed => state.windows.retain(|(window, _, _)| window != proxy),
                _ => {}
            }
        }
    }

    impl Dispatch<Session, ()> for State {
        fn event(state: &mut Self, _: &Session, event: session::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            match event {
                session::Event::BufferSize { width, height } => { state.size = Some([width, height]); state.argb = false; state.constraints = false; }
                session::Event::ShmFormat { format } => state.argb |= format == wayland_client::WEnum::Value(wl_shm::Format::Argb8888),
                session::Event::Done => state.constraints = true,
                session::Event::Stopped => state.failure = Some("The output capture session stopped".into()),
                _ => {}
            }
        }
    }

    impl Dispatch<Frame, ()> for State {
        fn event(state: &mut Self, _: &Frame, event: frame::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            match event {
                frame::Event::Transform { transform } => state.normal = transform == wayland_client::WEnum::Value(wl_output::Transform::Normal),
                frame::Event::PresentationTime { tv_sec_hi, tv_sec_lo, tv_nsec } => state.time = (tv_sec_hi != 0 || tv_sec_lo != 0) && tv_nsec < 1_000_000_000,
                frame::Event::Damage { x, y, width, height } => state.damage |= state.size.is_some_and(|size| x == 0 && y == 0 && width == size[0] as i32 && height == size[1] as i32),
                frame::Event::Ready => state.ready = true,
                frame::Event::Failed { reason } => state.failure = Some(format!("Output capture failed: {reason:?}")),
                _ => {}
            }
        }
    }

    delegate_noop!(State: ignore wl_output::WlOutput);
    delegate_noop!(State: ignore wl_shm::WlShm);
    delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
    delegate_noop!(State: ignore wl_buffer::WlBuffer);
    delegate_noop!(State: ignore Factory);
    delegate_noop!(State: ignore Manager);
    delegate_noop!(State: ignore Source);
}
