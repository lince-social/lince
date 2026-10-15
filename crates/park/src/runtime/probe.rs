use super::*;
use crate::server::Server;
use lince_interface::host::{ItemId, OutputId, OutputViewport, Rectangle};
use std::{os::unix::net::UnixStream, sync::atomic::AtomicUsize, time::Instant};

mod client;

pub fn demo_client() -> Result<(), ProbeFailure> {
    let socket = std::env::var_os("WAYLAND_DISPLAY")
        .ok_or_else(|| failure("WAYLAND_DISPLAY is missing".into()))?;
    let stream = UnixStream::connect(socket).map_err(|error| failure(error.to_string()))?;
    let (_sender, receiver) = sync_channel(1);
    client::run(
        stream,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
        receiver,
        true,
    )
}

pub fn probe() -> Result<(), ProbeFailure> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| failure(error.to_string()))?;
    let directory = tempfile::tempdir().map_err(|error| failure(error.to_string()))?;
    let mut worker = CanvasWorker::spawn(Configuration {
        size: [800, 480],
        directory: directory.path().into(),
        runtime: runtime.handle().clone(),
        cell: None,
    })
    .map_err(|error| failure(error.to_string()))?;
    let mut server = Server::new([800, 480]).map_err(|error| failure(error.to_string()))?;
    let (server_stream, client_stream) =
        UnixStream::pair().map_err(|error| failure(error.to_string()))?;
    server
        .insert_client(server_stream)
        .map_err(|error| failure(error.to_string()))?;
    let stop = Arc::new(AtomicBool::new(false));
    let releases = Arc::new(AtomicUsize::new(0));
    let client_stop = stop.clone();
    let client_releases = releases.clone();
    let (control, commands) = sync_channel(2);
    let client = std::thread::spawn(move || {
        client::run(client_stream, client_stop, client_releases, commands, false)
    });
    let result = (|| {
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut revision = 0;
        let mut pixels_seen = false;
        let mut operation = false;
        let mut modal_seen = false;
        let mut close_requested = false;
        let mut unmap_requested = false;
        let mut remap_requested = false;
        let mut remapped = false;
        let mut identity = None;
        let mut fullscreen_requested = false;
        let mut maximize_requested = false;
        let mut maximize_seen = false;
        let mut maximize_restored = false;
        let mut fullscreen_seen = false;
        let mut restored = false;
        let mut status = 0;
        let mut reported_pixels = false;
        let mut last_pixels = String::new();
        let mut last_frame = Vec::new();
        let mut last_size = [0; 2];
        loop {
            if Instant::now() > deadline {
                if !last_frame.is_empty() {
                    let mut image =
                        format!("P6\n{} {}\n255\n", last_size[0], last_size[1]).into_bytes();
                    image.extend(
                        last_frame
                            .chunks_exact(4)
                            .flat_map(|pixel| pixel[..3].iter().copied()),
                    );
                    if let Ok(mut artifact) = tempfile::Builder::new()
                        .prefix("lince-park-worker-")
                        .suffix(".ppm")
                        .tempfile()
                        && std::io::Write::write_all(&mut artifact, &image).is_ok()
                        && let Ok((_, path)) = artifact.keep()
                    {
                        eprintln!("Park worker frame: {}", path.display());
                    }
                }
                return Err(failure(format!(
                    "the worker proof timed out: pixels={pixels_seen}, unmap_requested={unmap_requested}, remap_requested={remap_requested}, remap={remapped}, fullscreen={fullscreen_seen}, restored={restored}, operation={operation}, modal={modal_seen}, close={close_requested}, last={last_pixels}, releases={}",
                    releases.load(Ordering::Acquire)
                )));
            }
            server
                .dispatch()
                .map_err(|error| failure(error.to_string()))?;
            for (surface, request) in server.requests() {
                worker.input(Input::Window(surface, request)).map_err(|_| {
                    failure("could not forward a client window-state request".into())
                })?;
            }
            if let Some(error) = worker.failure() {
                return Err(error);
            }
            if !worker.is_running() {
                return Err(failure(
                    "the worker stopped before finishing the proof".into(),
                ));
            }
            if revision != server.revision() {
                let bounds = Rectangle::new([0.0; 2], [800.0, 480.0]).unwrap();
                worker
                    .scene(Scene {
                        outputs: vec![crate::presentation::Output {
                            name: "Park-1".into(),
                            viewport: OutputViewport {
                                id: OutputId(1),
                                bounds,
                                usable: Rectangle::new([0.0, 32.0], [800.0, 424.0]),
                                physical_size: [800, 480],
                                workspace: 1,
                            },
                        }],
                        windows: Arc::new(server.windows()),
                    })
                    .map_err(|error| failure(error.to_string()))?;
                revision = server.revision();
            }
            if let Some(completed) = worker.frame() {
                for window in &completed.frame.windows {
                    server
                        .configure_state(
                            window.surface,
                            window.size.map(|value| value as i32),
                            window.state,
                        )
                        .map_err(|error| failure(error.to_string()))?;
                }
                use smithay::backend::allocator::Buffer;
                let expected = if completed.frame.fullscreen {
                    [800, 480]
                } else {
                    [800, 424]
                };
                if maximize_requested
                    && !maximize_seen
                    && completed
                        .frame
                        .windows
                        .iter()
                        .any(|window| window.state.maximized)
                {
                    maximize_seen = true;
                    control
                        .try_send(client::Control::Maximized(false))
                        .map_err(|_| {
                            failure("could not restore the maximized application".into())
                        })?;
                } else if maximize_seen
                    && completed
                        .frame
                        .windows
                        .iter()
                        .all(|window| !window.state.maximized)
                {
                    maximize_restored = true;
                }
                if completed.frame.dmabuf().size() != (expected[0], expected[1]).into() {
                    return Err(failure(
                        "the native view does not fit its reserved output area".into(),
                    ));
                }
                if fullscreen_requested && completed.frame.fullscreen && !fullscreen_seen {
                    fullscreen_seen = true;
                    control
                        .try_send(client::Control::Fullscreen(false))
                        .map_err(|_| {
                            failure("could not restore the reserved canvas view".into())
                        })?;
                } else if fullscreen_seen && !completed.frame.fullscreen {
                    restored = true;
                }
                if !completed.rejected.is_empty() {
                    return Err(failure(
                        "the worker rejected a probe surface; inspect Park diagnostics".into(),
                    ));
                }
                if let Some(geometry) = &completed.frame.geometry {
                    if unmap_requested
                        && !remap_requested
                        && completed.windows.iter().any(|window| !window.mapped)
                        && geometry
                            .items
                            .iter()
                            .all(|item| !matches!(item.id, ItemId::Application(_)))
                    {
                        control
                            .try_send(client::Control::Remap)
                            .map_err(|_| failure("could not remap the probe window".into()))?;
                        remap_requested = true;
                        reported_pixels = false;
                    }
                    if let Some(item) = geometry
                        .items
                        .iter()
                        .find(|item| matches!(item.id, ItemId::Application(_)))
                    {
                        let ItemId::Application(id) = item.id else {
                            unreachable!()
                        };
                        if let Some(window) =
                            completed.windows.iter().find(|window| window.id == id)
                        {
                            let pixels =
                                crate::presentation::probe_pixels(completed.frame.dmabuf())?;
                            let sample = |content: [f64; 2]| {
                                let point: [f64; 2] = std::array::from_fn(|axis| {
                                    item.mapping.canvas.origin()[axis]
                                        + content[axis] / item.mapping.surface.size()[axis]
                                            * item.mapping.canvas.size()[axis]
                                });
                                let point: [f64; 2] = std::array::from_fn(|axis| {
                                    point[axis] - completed.frame.origin[axis]
                                });
                                let offset = (point[1] as usize * expected[0] as usize
                                    + point[0] as usize)
                                    * 4;
                                pixels.get(offset..offset + 4)
                            };
                            let blue = sample([140.0, 70.0]);
                            let green = sample([20.0, 18.0]);
                            let blue_top = sample([140.0, 30.0]);
                            last_pixels = format!(
                                "blue={blue:?}, blue_top={blue_top:?}, green={green:?}, mapping={:?}, nodes={}",
                                item.mapping,
                                window.nodes.len()
                            );
                            if !reported_pixels {
                                eprintln!(
                                    "Park worker pixels: blue={blue:?}, green={green:?}, origin={:?}, view={expected:?}",
                                    completed.frame.origin
                                );
                                reported_pixels = true;
                            }
                            if blue.is_some_and(|pixel| {
                                pixel[2] > 235 && pixel[0] < 20 && pixel[1] < 20
                            }) && green.is_some_and(|pixel| {
                                pixel[1] > 235 && pixel[0] < 20 && pixel[2] < 20
                            }) {
                                if let Some(previous) = identity {
                                    if previous != id {
                                        return Err(failure(
                                            "unmap/remap changed the live window identity".into(),
                                        ));
                                    }
                                } else {
                                    identity = Some(id);
                                }
                                pixels_seen = true;
                                if remap_requested {
                                    remapped = true;
                                }
                                let (_, local) =
                                    window.hit_content([20.0, 18.0]).ok_or_else(|| {
                                        failure(
                                            "the rendered subsurface has no input target".into(),
                                        )
                                    })?;
                                if local != [5.0, 5.0] {
                                    return Err(failure(
                                        "surface input and rendered geometry disagree".into(),
                                    ));
                                }
                            }
                            if pixels_seen
                                && releases.load(Ordering::Acquire) >= 2
                                && !unmap_requested
                            {
                                control.try_send(client::Control::Unmap).map_err(|_| {
                                    failure("could not unmap the probe window".into())
                                })?;
                                unmap_requested = true;
                            }
                            if remapped && !maximize_requested {
                                control.try_send(client::Control::Maximized(true)).map_err(
                                    |_| failure("could not maximize the probe application".into()),
                                )?;
                                maximize_requested = true;
                            }
                            if remapped && maximize_restored && !fullscreen_requested {
                                control
                                    .try_send(client::Control::Fullscreen(true))
                                    .map_err(|_| {
                                        failure("could not fullscreen the probe application".into())
                                    })?;
                                fullscreen_requested = true;
                            }
                            if restored && !operation {
                                worker
                                    .input(Input::Action(DesktopAction::OpenOperation))
                                    .map_err(|_| failure("could not open Operation".into()))?;
                                operation = true;
                            }
                            if remapped && completed.frame.input.modal && !modal_seen {
                                modal_seen = true;
                                worker
                                    .input(Input::Key {
                                        physical: KeyCode::Escape,
                                        logical: Key::Escape,
                                        text: None,
                                        pressed: true,
                                        repeat: false,
                                    })
                                    .map_err(|_| failure("could not close Operation".into()))?;
                                worker
                                    .input(Input::Key {
                                        physical: KeyCode::Escape,
                                        logical: Key::Escape,
                                        text: None,
                                        pressed: false,
                                        repeat: false,
                                    })
                                    .map_err(|_| failure("could not release Escape".into()))?;
                            }
                            if modal_seen
                                && !completed.frame.input.modal
                                && releases.load(Ordering::Acquire) >= 2
                                && !close_requested
                            {
                                server.close(id);
                                close_requested = true;
                            }
                            last_frame = pixels;
                            last_size = expected;
                        }
                    }
                }
            }
            if close_requested && server.windows().is_empty() {
                break;
            }
            let next = u8::from(pixels_seen)
                | u8::from(remapped) << 1
                | u8::from(fullscreen_seen) << 2
                | u8::from(restored) << 3
                | u8::from(modal_seen) << 4
                | u8::from(close_requested) << 5;
            if status != next {
                status = next;
                eprintln!(
                    "Park worker check: pixels={pixels_seen}, remap={remapped}, fullscreen={fullscreen_seen}, restored={restored}, modal={modal_seen}, close={close_requested}"
                );
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if !pixels_seen
            || !modal_seen
            || !remapped
            || !restored
            || !maximize_seen
            || !maximize_restored
        {
            return Err(failure(
                "surface pixels or native modal routing were not observed".into(),
            ));
        }
        Ok(())
    })();
    stop.store(true, Ordering::Release);
    drop(server);
    let stopped = worker.stop().map_err(|error| failure(error.to_string()));
    let client_result = client
        .join()
        .map_err(|_| failure("the probe client panicked".into()))?;
    result?;
    stopped?;
    client_result
}
