use std::{
    fs::{self, File},
    io::{self, IoSlice, Write},
    mem::MaybeUninit,
    os::{
        fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
        unix::{fs::FileExt, net::UnixStream, process::CommandExt},
    },
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use smithay::reexports::wayland_server::{
    self as server, Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, New,
    Resource, ResourceData,
    backend::{ClientData, ObjectData, ObjectId},
    protocol::{wl_callback, wl_compositor, wl_region, wl_shm, wl_shm_pool, wl_surface},
};
use wayland_backend::handoff::{
    Checkpoint, ClientKey, ConnectionDecoder, ConnectionEncoder, ObjectKey, RestoreContext,
};
use wayland_client::{
    self as client, Connection, EventQueue, Proxy, QueueHandle, delegate_noop,
    protocol::{
        wl_callback as client_callback, wl_compositor as client_compositor, wl_registry,
        wl_shm as client_shm, wl_surface as client_surface,
    },
};

#[derive(Default)]
struct MiniState {
    commits: usize,
    increment: usize,
    frames: Vec<wl_callback::WlCallback>,
    pools: Vec<File>,
}

#[derive(Debug)]
struct MiniClientData;
impl ClientData for MiniClientData {}

impl GlobalDispatch<wl_compositor::WlCompositor, ()> for MiniState {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<wl_compositor::WlCompositor>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }
}

impl GlobalDispatch<wl_shm::WlShm, ()> for MiniState {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<wl_shm::WlShm>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        let shm = init.init(resource, ());
        shm.format(wl_shm::Format::Argb8888);
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for MiniState {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &wl_compositor::WlCompositor,
        request: wl_compositor::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_compositor::Request::CreateSurface { id } => {
                init.init(id, ());
            }
            wl_compositor::Request::CreateRegion { id } => {
                init.init(id, ());
            }
            _ => unreachable!(),
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for MiniState {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_surface::WlSurface,
        request: wl_surface::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_surface::Request::Commit => state.commits += state.increment,
            wl_surface::Request::Frame { callback } => state.frames.push(init.init(callback, ())),
            _ => {}
        }
    }
}

impl Dispatch<wl_shm::WlShm, ()> for MiniState {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_shm::WlShm,
        request: wl_shm::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let wl_shm::Request::CreatePool { id, fd, size } = request {
            assert!(size > 0);
            init.init(id, ());
            state.pools.push(File::from(fd));
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for MiniState {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}
impl Dispatch<wl_region::WlRegion, ()> for MiniState {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &wl_region::WlRegion,
        _: wl_region::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for MiniState {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &wl_callback::WlCallback,
        _: wl_callback::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}

#[derive(Default)]
struct MiniClient {
    compositor: Option<client_compositor::WlCompositor>,
    shm: Option<client_shm::WlShm>,
    formats: Vec<u32>,
}

impl client::Dispatch<wl_registry::WlRegistry, ()> for MiniClient {
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
                _ => {}
            }
        }
    }
}

impl client::Dispatch<client_callback::WlCallback, Arc<AtomicBool>> for MiniClient {
    fn event(
        _: &mut Self,
        _: &client_callback::WlCallback,
        _: client_callback::Event,
        completed: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        completed.store(true, Ordering::Release);
    }
}

delegate_noop!(MiniClient: ignore client_compositor::WlCompositor);
impl client::Dispatch<client_shm::WlShm, ()> for MiniClient {
    fn event(
        state: &mut Self,
        _: &client_shm::WlShm,
        event: client_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let client_shm::Event::Format { format } = event {
            state.formats.push(format.into());
        }
    }
}
delegate_noop!(MiniClient: ignore client_surface::WlSurface);

struct Codec;

impl ConnectionEncoder<MiniState> for Codec {
    fn client(
        &mut self,
        _: server::backend::ClientId,
        _: &Arc<dyn ClientData>,
        _: &mut Vec<OwnedFd>,
    ) -> io::Result<Vec<u8>> {
        Ok(Vec::new())
    }
    fn object(
        &mut self,
        id: ObjectId,
        _: &Arc<dyn ObjectData<MiniState>>,
        _: &mut Vec<OwnedFd>,
    ) -> io::Result<Vec<u8>> {
        match id.interface().name {
            "wl_compositor" | "wl_shm" | "wl_surface" | "wl_region" | "wl_callback" => {
                Ok(Vec::new())
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "This fixture has no adapter for that resource",
            )),
        }
    }
}

impl ConnectionDecoder<MiniState> for Codec {
    fn interface(&self, name: &str) -> Option<&'static wayland_backend::protocol::Interface> {
        match name {
            "wl_compositor" => Some(wl_compositor::WlCompositor::interface()),
            "wl_shm" => Some(wl_shm::WlShm::interface()),
            "wl_surface" => Some(wl_surface::WlSurface::interface()),
            "wl_region" => Some(wl_region::WlRegion::interface()),
            "wl_callback" => Some(wl_callback::WlCallback::interface()),
            _ => None,
        }
    }
    fn client(
        &mut self,
        _: ClientKey,
        payload: &[u8],
        _: &[OwnedFd],
    ) -> io::Result<Arc<dyn ClientData>> {
        if !payload.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected fixture client payload",
            ));
        }
        Ok(Arc::new(MiniClientData))
    }
    fn object(
        &mut self,
        _: ObjectKey,
        info: wayland_backend::protocol::ObjectInfo,
        payload: &[u8],
        _: &[OwnedFd],
    ) -> io::Result<Arc<dyn ObjectData<MiniState>>> {
        if !payload.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected fixture resource payload",
            ));
        }
        match info.interface.name {
            "wl_compositor" => Ok(Arc::new(
                ResourceData::<wl_compositor::WlCompositor, ()>::new(()),
            )),
            "wl_shm" => Ok(Arc::new(ResourceData::<wl_shm::WlShm, ()>::new(()))),
            "wl_surface" => Ok(Arc::new(ResourceData::<wl_surface::WlSurface, ()>::new(()))),
            "wl_region" => Ok(Arc::new(ResourceData::<wl_region::WlRegion, ()>::new(()))),
            "wl_callback" => Ok(Arc::new(ResourceData::<wl_callback::WlCallback, ()>::new(
                (),
            ))),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Unknown fixture resource",
            )),
        }
    }
}

fn display() -> Display<MiniState> {
    let display = Display::<MiniState>::new().unwrap();
    display
        .handle()
        .create_global::<MiniState, wl_compositor::WlCompositor, _>(6, ());
    display
        .handle()
        .create_global::<MiniState, wl_shm::WlShm, _>(1, ());
    display
}

fn exchange(
    display: &mut Display<MiniState>,
    state: &mut MiniState,
    connection: &Connection,
    queue: &mut EventQueue<MiniClient>,
    client: &mut MiniClient,
) {
    remote_exchange(connection, queue, client, || {
        display.dispatch_clients(state).unwrap();
        display.flush_clients().unwrap();
    });
}

fn remote_exchange(
    connection: &Connection,
    queue: &mut EventQueue<MiniClient>,
    client: &mut MiniClient,
    mut step: impl FnMut(),
) {
    let completed = Arc::new(AtomicBool::new(false));
    connection
        .display()
        .sync(&queue.handle(), completed.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !completed.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "Backend fixture roundtrip timed out"
        );
        connection.flush().unwrap();
        step();
        queue.dispatch_pending(client).unwrap();
        if let Some(read) = queue.prepare_read() {
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
            )
            .unwrap();
            if descriptors[0]
                .revents()
                .contains(rustix::event::PollFlags::IN)
            {
                match read.read() {
                    Ok(_) => {}
                    Err(client::backend::WaylandError::Io(error))
                        if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => panic!("Backend fixture disconnected: {error}"),
                }
            }
        }
        queue.dispatch_pending(client).unwrap();
    }
}

const EXEC_CHILD: &str = "server::tests::backend_handoff_tests::backend_exec_child";

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn state_file(directory: &Path, name: &str, value: &serde_json::Value) {
    let temporary = directory.join(format!("{name}.tmp"));
    fs::write(&temporary, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(temporary, directory.join(format!("{name}.json"))).unwrap();
}

fn wait_file(directory: &Path, name: &str, process: &mut Process) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(bytes) = fs::read(directory.join(format!("{name}.json"))) {
            return serde_json::from_slice(&bytes).unwrap();
        }
        assert!(
            Instant::now() < deadline && process.0.try_wait().unwrap().is_none(),
            "Backend child failed or timed out: {}",
            fs::read_to_string(directory.join("child.log")).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn compiled_generation() -> &'static str {
    match option_env!("LINCE_REFRESH_PROBE_GENERATION") {
        Some("next") => "next",
        _ => "initial",
    }
}

#[test]
#[ignore = "private subprocess for backend connection-state exec proof"]
fn backend_exec_child() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("LINCE_BACKEND_EXEC_DIRECTORY").expect("private backend proof directory"),
    );
    let after = std::env::var_os("LINCE_BACKEND_EXEC_AFTER").is_some();
    let mut display = display();
    let mut state = MiniState {
        increment: if compiled_generation() == "next" {
            10
        } else {
            1
        },
        ..MiniState::default()
    };
    if after {
        let wire: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.join("checkpoint.json")).unwrap()).unwrap();
        let descriptors = wire["descriptors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|number| unsafe {
                OwnedFd::from_raw_fd(i32::try_from(number.as_i64().unwrap()).unwrap())
            })
            .collect();
        let checkpoint = Checkpoint {
            state: serde_json::from_value(wire["state"].clone()).unwrap(),
            descriptors,
        };
        state.commits = usize::try_from(wire["commits"].as_u64().unwrap()).unwrap();
        display
            .backend()
            .restore_connections(&checkpoint, &mut Codec)
            .unwrap();
        drop(checkpoint);
        let handle = display.handle();
        let backend = handle.backend_handle();
        let mut clients = Vec::new();
        backend.with_all_clients(|client| clients.push(client));
        for client in clients {
            let mut objects = Vec::new();
            backend
                .with_all_objects_for(client, |object| objects.push(object))
                .unwrap();
            for object in objects {
                if object.interface().name == "wl_callback" {
                    wl_callback::WlCallback::from_id(&handle, object)
                        .unwrap()
                        .done(9001);
                }
            }
        }
        state_file(
            &directory,
            "after",
            &serde_json::json!({"pid":std::process::id(), "executable":std::env::current_exe().unwrap(), "commits":state.commits, "generation":compiled_generation(), "increment":state.increment}),
        );
    } else {
        let descriptor = std::env::var("LINCE_BACKEND_EXEC_SOCKET")
            .unwrap()
            .parse::<i32>()
            .unwrap();
        let stream = unsafe { UnixStream::from_raw_fd(descriptor) };
        rustix::io::fcntl_setfd(&stream, rustix::io::FdFlags::CLOEXEC).unwrap();
        display
            .handle()
            .insert_client(stream, Arc::new(MiniClientData))
            .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut reported = false;
    let mut previous = usize::MAX;
    while Instant::now() < deadline {
        display.dispatch_clients(&mut state).unwrap();
        display.flush_clients().unwrap();
        if !after && !reported && state.frames.len() == 2 && state.commits >= 2 * state.increment {
            state_file(
                &directory,
                "before",
                &serde_json::json!({"pid":std::process::id(), "executable":std::env::current_exe().unwrap(), "commits":state.commits, "generation":compiled_generation()}),
            );
            reported = true;
        }
        if after && previous != state.commits {
            state_file(
                &directory,
                "progress",
                &serde_json::json!({"commits":state.commits}),
            );
            previous = state.commits;
        }
        if !after && directory.join("replace").exists() {
            assert!(reported);
            let checkpoint = display
                .backend()
                .checkpoint_connections(&mut Codec)
                .unwrap();
            fs::write(directory.join("checkpoint.json"), serde_json::to_vec(&serde_json::json!({"state":checkpoint.state, "descriptors":checkpoint.descriptors.iter().map(AsRawFd::as_raw_fd).collect::<Vec<_>>(), "commits":state.commits})).unwrap()).unwrap();
            for descriptor in &checkpoint.descriptors {
                rustix::io::fcntl_setfd(descriptor, rustix::io::FdFlags::empty()).unwrap();
            }
            panic!(
                "Backend replacement exec failed: {}",
                Command::new(directory.join("replacement"))
                    .args(["--exact", EXEC_CHILD, "--ignored", "--nocapture"])
                    .env("LINCE_BACKEND_EXEC_AFTER", "1")
                    .exec()
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("Private backend proof reached its lifetime limit");
}

fn executable_hash(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut source = File::open(path).unwrap();
    let mut digest = Sha256::new();
    let mut bytes = [0_u8; 65536];
    loop {
        let count = source.read(&mut bytes).unwrap();
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn backend_connection_state_survives_exec() {
    let directory = tempfile::tempdir().unwrap();
    let initial = std::env::current_exe().unwrap();
    let candidate = std::env::var_os("LINCE_BACKEND_EXEC_CANDIDATE").map(std::path::PathBuf::from);
    let replacement = directory.path().join("replacement");
    fs::copy(candidate.as_ref().unwrap_or(&initial), &replacement).unwrap();
    let initial_hash = executable_hash(&initial);
    let candidate_hash = executable_hash(&replacement);
    if candidate.is_some() {
        assert_ne!(
            initial_hash, candidate_hash,
            "The distinct-build proof requires different executable images"
        );
    }
    let (server, socket) = UnixStream::pair().unwrap();
    let descriptor = server.as_raw_fd();
    let log = File::create(directory.path().join("child.log")).unwrap();
    let mut command = Command::new(&initial);
    command
        .args(["--exact", EXEC_CHILD, "--ignored", "--nocapture"])
        .env("LINCE_BACKEND_EXEC_DIRECTORY", directory.path())
        .env("LINCE_BACKEND_EXEC_SOCKET", descriptor.to_string())
        .env_remove("LINCE_BACKEND_EXEC_AFTER")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    unsafe {
        command.pre_exec(move || {
            let fd = std::os::fd::BorrowedFd::borrow_raw(descriptor);
            rustix::io::fcntl_setfd(fd, rustix::io::FdFlags::empty())?;
            Ok(())
        });
    }
    let mut process = Process(command.spawn().unwrap());
    drop(server);
    let connection = Connection::from_socket(socket).unwrap();
    let mut queue = connection.new_event_queue();
    connection.display().get_registry(&queue.handle(), ());
    let mut client = MiniClient::default();
    remote_exchange(&connection, &mut queue, &mut client, || {});
    remote_exchange(&connection, &mut queue, &mut client, || {});
    let qh = queue.handle();
    let first = client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let second = client.compositor.as_ref().unwrap().create_surface(&qh, ());
    let frame_one = Arc::new(AtomicBool::new(false));
    let frame_two = Arc::new(AtomicBool::new(false));
    first.frame(&qh, frame_one.clone());
    second.frame(&qh, frame_two.clone());
    first.commit();
    second.commit();
    remote_exchange(&connection, &mut queue, &mut client, || {});
    let before = wait_file(directory.path(), "before", &mut process);
    fs::write(directory.path().join("replace"), []).unwrap();
    let after = wait_file(directory.path(), "after", &mut process);
    assert_eq!(before["pid"], after["pid"]);
    assert_eq!(after["executable"], serde_json::json!(replacement));
    if candidate.is_some() {
        assert_eq!(after["generation"], "next");
    }
    first.commit();
    second.commit();
    remote_exchange(&connection, &mut queue, &mut client, || {});
    assert!(frame_one.load(Ordering::Acquire));
    assert!(frame_two.load(Ordering::Acquire));
    let third = client.compositor.as_ref().unwrap().create_surface(&qh, ());
    third.commit();
    remote_exchange(&connection, &mut queue, &mut client, || {});
    third.destroy();
    first.destroy();
    second.destroy();
    remote_exchange(&connection, &mut queue, &mut client, || {});
    let expected = before["commits"].as_u64().unwrap() + 3 * after["increment"].as_u64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let progress = loop {
        let progress = wait_file(directory.path(), "progress", &mut process);
        if progress["commits"].as_u64() == Some(expected) {
            break progress;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    };
    let report = serde_json::json!({"schema":1, "experiment":"backend_connection_exec", "scope":"Generic Wayland connection backend with a minimal protocol fixture; not the production Park/Smithay state graph, native interface, GPU or full session", "outcome":"backend_handoff_verified", "backend_connections_preserved":true, "distinct_builds":candidate.is_some(), "before":before, "after":after, "progress":progress, "initial_sha256":initial_hash, "candidate_sha256":candidate_hash, "restored_frame_callbacks":2, "post_exec_surface_commits":3, "post_exec_new_and_destroyed_objects":true, "swap_complete":false});
    if let Some(path) = std::env::var_os("LINCE_BACKEND_HANDOFF_REPORT") {
        fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!("{}", serde_json::to_string(&report).unwrap());
}

struct Fixture {
    display: Display<MiniState>,
    state: MiniState,
    connection: Connection,
    queue: EventQueue<MiniClient>,
    client: MiniClient,
    raw: UnixStream,
}

impl Fixture {
    fn new() -> Self {
        let display = display();
        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        let raw = client_socket.try_clone().unwrap();
        display
            .handle()
            .insert_client(server_socket, Arc::new(MiniClientData))
            .unwrap();
        let connection = Connection::from_socket(client_socket).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut fixture = Self {
            display,
            state: MiniState {
                increment: 1,
                ..MiniState::default()
            },
            connection,
            queue,
            client: MiniClient::default(),
            raw,
        };
        fixture.exchange();
        fixture.exchange();
        fixture
    }

    fn exchange(&mut self) {
        exchange(
            &mut self.display,
            &mut self.state,
            &self.connection,
            &mut self.queue,
            &mut self.client,
        );
    }

    fn checkpoint(&mut self) -> Checkpoint {
        let mut checkpoint = self
            .display
            .backend()
            .checkpoint_connections(&mut Codec)
            .unwrap();
        let wire = serde_json::to_vec(&checkpoint.state).unwrap();
        checkpoint.state = serde_json::from_slice(&wire).unwrap();
        checkpoint
    }

    fn restore(&mut self, checkpoint: &Checkpoint) {
        let mut next = display();
        next.backend()
            .restore_connections(checkpoint, &mut Codec)
            .unwrap();
        self.display = next;
        self.state.increment = 10;
        self.state.frames.clear();
    }
}

#[test]
fn backend_restores_ids_serials_handlers_callbacks_and_a_partially_read_request() {
    let mut fixture = Fixture::new();
    let qh = fixture.queue.handle();
    let first = fixture
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let second = fixture
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    first.commit();
    second.commit();
    let first_frame = Arc::new(AtomicBool::new(false));
    let second_frame = Arc::new(AtomicBool::new(false));
    first.frame(&qh, first_frame.clone());
    second.frame(&qh, second_frame.clone());
    fixture.exchange();
    assert_eq!(fixture.state.commits, 2);
    let mut request = first.id().protocol_id().to_ne_bytes().to_vec();
    request.extend_from_slice(&((8_u32 << 16) | 6).to_ne_bytes());
    fixture.raw.write_all(&request[..4]).unwrap();
    fixture
        .display
        .dispatch_clients(&mut fixture.state)
        .unwrap();
    let checkpoint = fixture.checkpoint();
    assert_eq!(checkpoint.state.clients[0].socket.incoming, request[..4]);
    assert_eq!(checkpoint.state.registries.len(), 1);
    let mut invalid = Checkpoint {
        state: checkpoint.state.clone(),
        descriptors: checkpoint
            .descriptors
            .iter()
            .map(|fd| fd.try_clone().unwrap())
            .collect(),
    };
    invalid.state.clients[0]
        .objects
        .iter_mut()
        .find(|object| object.interface == "wl_surface")
        .unwrap()
        .protocol
        .push_str(" changed");
    let mut rejected = display();
    assert!(
        rejected
            .backend()
            .restore_connections(&invalid, &mut Codec)
            .is_err()
    );
    let mut inserted = 0;
    rejected
        .backend()
        .handle()
        .with_all_clients(|_| inserted += 1);
    assert_eq!(inserted, 0);
    fixture.restore(&checkpoint);
    fixture.raw.write_all(&request[4..]).unwrap();
    fixture.exchange();
    assert_eq!(fixture.state.commits, 12);
    let handle = fixture.display.handle();
    let backend = handle.backend_handle();
    let mut clients = Vec::new();
    backend.with_all_clients(|client| clients.push(client));
    let mut objects = Vec::new();
    backend
        .with_all_objects_for(clients[0].clone(), |object| objects.push(object))
        .unwrap();
    for object in objects {
        if object.interface().name == "wl_callback" {
            wl_callback::WlCallback::from_id(&handle, object)
                .unwrap()
                .done(9001);
        }
    }
    first.commit();
    second.commit();
    fixture.exchange();
    assert!(first_frame.load(Ordering::Acquire));
    assert!(second_frame.load(Ordering::Acquire));
    assert_eq!(fixture.state.commits, 32);
    let third = fixture
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    third.commit();
    fixture.exchange();
    assert_eq!(fixture.state.commits, 42);
    third.destroy();
    first.destroy();
    second.destroy();
    fixture.exchange();
    drop(checkpoint);
    fixture.exchange();
}

#[test]
fn backend_restores_a_partially_read_request_with_its_queued_scm_rights_descriptor() {
    let mut fixture = Fixture::new();
    let baseline = fixture.checkpoint();
    let new_id = baseline.state.clients[0].client_slots as u32 + 1;
    let mut request = fixture
        .client
        .shm
        .as_ref()
        .unwrap()
        .id()
        .protocol_id()
        .to_ne_bytes()
        .to_vec();
    request.extend_from_slice(&(16_u32 << 16).to_ne_bytes());
    request.extend_from_slice(&new_id.to_ne_bytes());
    request.extend_from_slice(&16_i32.to_ne_bytes());
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"preserved bytes!").unwrap();
    let borrowed = [file.as_fd()];
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut ancillary = rustix::net::SendAncillaryBuffer::new(&mut space);
    assert!(ancillary.push(rustix::net::SendAncillaryMessage::ScmRights(&borrowed)));
    assert_eq!(
        rustix::net::sendmsg(
            &fixture.raw,
            &[IoSlice::new(&request[..4])],
            &mut ancillary,
            rustix::net::SendFlags::NOSIGNAL
        )
        .unwrap(),
        4
    );
    fixture
        .display
        .dispatch_clients(&mut fixture.state)
        .unwrap();
    let checkpoint = fixture.checkpoint();
    assert_eq!(checkpoint.state.clients[0].socket.incoming, request[..4]);
    assert_eq!(checkpoint.state.clients[0].socket.incoming_fds.len(), 1);
    fixture.restore(&checkpoint);
    fixture.raw.write_all(&request[4..]).unwrap();
    fixture
        .display
        .dispatch_clients(&mut fixture.state)
        .unwrap();
    assert_eq!(fixture.state.pools.len(), 1);
    let mut contents = [0_u8; 16];
    fixture.state.pools[0]
        .read_exact_at(&mut contents, 0)
        .unwrap();
    assert_eq!(&contents, b"preserved bytes!");
    let before = rustix::fs::fstat(&file).unwrap();
    let after = rustix::fs::fstat(&fixture.state.pools[0]).unwrap();
    assert_eq!((before.st_dev, before.st_ino), (after.st_dev, after.st_ino));
}

#[test]
fn backend_restores_unsent_events_and_delete_id_ordering() {
    let mut fixture = Fixture::new();
    let qh = fixture.queue.handle();
    let surface = fixture
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    let frame = Arc::new(AtomicBool::new(false));
    surface.frame(&qh, frame.clone());
    surface.commit();
    fixture.exchange();
    for callback in fixture.state.frames.drain(..) {
        callback.done(777);
    }
    let marker = Arc::new(AtomicBool::new(false));
    fixture.connection.display().sync(&qh, marker.clone());
    fixture.connection.flush().unwrap();
    fixture
        .display
        .dispatch_clients(&mut fixture.state)
        .unwrap();
    assert!(!frame.load(Ordering::Acquire));
    assert!(!marker.load(Ordering::Acquire));
    let checkpoint = fixture.checkpoint();
    assert!(!checkpoint.state.clients[0].socket.outgoing.is_empty());
    fixture.restore(&checkpoint);
    fixture.exchange();
    assert!(frame.load(Ordering::Acquire));
    assert!(marker.load(Ordering::Acquire));
    let next = fixture
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    next.commit();
    fixture.exchange();
    assert_eq!(fixture.state.commits, 11);
    drop(checkpoint);
    surface.destroy();
    next.destroy();
    fixture.exchange();
}

#[test]
fn backend_rejects_duplicate_slots_and_registry_corruption_before_import() {
    let mut fixture = Fixture::new();
    let qh = fixture.queue.handle();
    let surface = fixture
        .client
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&qh, ());
    surface.commit();
    fixture.exchange();
    let mut checkpoint = fixture.checkpoint();
    let original = checkpoint.state.clone();
    let mut duplicate = checkpoint.state.clients[0]
        .objects
        .iter()
        .find(|object| object.interface == "wl_surface")
        .unwrap()
        .clone();
    duplicate.serial = duplicate.serial.wrapping_add(1);
    checkpoint.state.clients[0].objects.push(duplicate);
    assert!(checkpoint.validate().is_err());
    checkpoint.state = original.clone();
    checkpoint.state.registries[0].serial = checkpoint.state.registries[0].serial.wrapping_add(1);
    assert!(checkpoint.validate().is_err());
    checkpoint.state = original.clone();
    checkpoint.state.client_capacity = usize::MAX;
    assert!(checkpoint.validate().is_err());
    checkpoint.state = original;
    let mut next = display();
    next.handle()
        .create_global::<MiniState, wl_shm::WlShm, _>(1, ());
    assert!(
        next.backend()
            .restore_connections(&checkpoint, &mut Codec)
            .is_err()
    );
    surface.commit();
    fixture.exchange();
    assert_eq!(fixture.state.commits, 2);
}

#[derive(Debug)]
struct FinalizedSurface {
    marker: u32,
    peer: Option<wl_surface::WlSurface>,
}

impl Dispatch<wl_surface::WlSurface, FinalizedSurface> for MiniState {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_surface::WlSurface,
        request: wl_surface::Request,
        data: &FinalizedSurface,
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        if let wl_surface::Request::Commit = request {
            assert_eq!(data.marker, 91);
            if let Some(peer) = &data.peer {
                assert!(peer.is_alive());
                assert_eq!(peer.data::<FinalizedSurface>().unwrap().marker, 91);
                state.commits += 100;
            } else {
                state.commits += 10;
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Finalization {
    Restore,
    Reject,
    QueueEvent,
    FlushEvent,
    Panic,
}

struct FinalizingCodec {
    surfaces: Vec<ObjectKey>,
    shm: ObjectKey,
    mode: Finalization,
}

impl FinalizingCodec {
    fn new(checkpoint: &Checkpoint, mode: Finalization) -> Self {
        let mut surfaces = Vec::new();
        let mut shm = None;
        for client in &checkpoint.state.clients {
            for object in &client.objects {
                let key = ObjectKey {
                    client: client.key,
                    id: object.id,
                    serial: object.serial,
                };
                match object.interface.as_str() {
                    "wl_surface" => surfaces.push(key),
                    "wl_shm" => shm = Some(key),
                    _ => {}
                }
            }
        }
        Self {
            surfaces,
            shm: shm.unwrap(),
            mode,
        }
    }
}

impl ConnectionDecoder<MiniState> for FinalizingCodec {
    fn interface(&self, name: &str) -> Option<&'static wayland_backend::protocol::Interface> {
        ConnectionDecoder::<MiniState>::interface(&Codec, name)
    }

    fn client(
        &mut self,
        key: ClientKey,
        payload: &[u8],
        descriptors: &[OwnedFd],
    ) -> io::Result<Arc<dyn ClientData>> {
        ConnectionDecoder::<MiniState>::client(&mut Codec, key, payload, descriptors)
    }

    fn object(
        &mut self,
        key: ObjectKey,
        info: wayland_backend::protocol::ObjectInfo,
        payload: &[u8],
        descriptors: &[OwnedFd],
    ) -> io::Result<Arc<dyn ObjectData<MiniState>>> {
        ConnectionDecoder::<MiniState>::object(&mut Codec, key, info, payload, descriptors)
    }

    fn finish(&mut self, context: &RestoreContext, _: &[OwnedFd]) -> io::Result<()> {
        let handle = DisplayHandle::from(context.backend_handle().clone());
        let mut peer = None;
        for key in &self.surfaces {
            let id = context.object(*key)?;
            context
                .backend_handle()
                .set_object_data::<MiniState>(
                    id.clone(),
                    Arc::new(
                        ResourceData::<wl_surface::WlSurface, FinalizedSurface>::new(
                            FinalizedSurface {
                                marker: 91,
                                peer: peer.clone(),
                            },
                        ),
                    ),
                )
                .map_err(io::Error::other)?;
            let surface = wl_surface::WlSurface::from_id(&handle, id).map_err(io::Error::other)?;
            assert_eq!(surface.data::<FinalizedSurface>().unwrap().marker, 91);
            peer = Some(surface);
        }
        let mut stale = self.shm;
        stale.serial = stale.serial.wrapping_add(1);
        assert!(context.object(stale).is_err());
        match self.mode {
            Finalization::Restore => Ok(()),
            Finalization::Reject => Err(io::Error::other("Injected finalization rejection")),
            Finalization::Panic => panic!("Injected finalizer panic"),
            Finalization::QueueEvent | Finalization::FlushEvent => {
                let shm = wl_shm::WlShm::from_id(&handle, context.object(self.shm)?)
                    .map_err(io::Error::other)?;
                shm.format(wl_shm::Format::Argb8888);
                if matches!(self.mode, Finalization::FlushEvent) {
                    context.backend_handle().flush(None)?;
                }
                Ok(())
            }
        }
    }
}

#[test]
fn backend_finalization_rebuilds_typed_resource_references_before_dispatch() {
    let mut fixture = Fixture::new();
    let qh = fixture.queue.handle();
    let compositor = fixture.client.compositor.as_ref().unwrap();
    let first = compositor.create_surface(&qh, ());
    let second = compositor.create_surface(&qh, ());
    fixture.exchange();
    let checkpoint = fixture.checkpoint();
    let mut next = display();
    next.backend()
        .restore_connections(
            &checkpoint,
            &mut FinalizingCodec::new(&checkpoint, Finalization::Restore),
        )
        .unwrap();
    fixture.display = next;
    first.commit();
    second.commit();
    fixture.exchange();
    assert_eq!(fixture.state.commits, 110);
    first.destroy();
    second.destroy();
    fixture.exchange();
}

#[test]
fn backend_finalization_errors_panics_and_protocol_io_leave_source_usable_and_allow_retry() {
    for mode in [
        Finalization::Reject,
        Finalization::QueueEvent,
        Finalization::FlushEvent,
        Finalization::Panic,
    ] {
        let mut fixture = Fixture::new();
        let qh = fixture.queue.handle();
        let surface = fixture
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&qh, ());
        fixture.exchange();
        let formats = fixture.client.formats.len();
        let checkpoint = fixture.checkpoint();
        let mut next = display();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            next.backend()
                .restore_connections(&checkpoint, &mut FinalizingCodec::new(&checkpoint, mode))
        }));
        assert!(matches!(outcome, Ok(Err(_)) | Err(_)));
        let mut client_count = 0;
        next.backend()
            .handle()
            .with_all_clients(|_| client_count += 1);
        assert_eq!(client_count, 0);
        surface.commit();
        fixture.exchange();
        assert_eq!(fixture.state.commits, 1);
        assert_eq!(fixture.client.formats.len(), formats);
        let checkpoint = fixture.checkpoint();
        next.backend()
            .restore_connections(&checkpoint, &mut Codec)
            .unwrap();
        fixture.display = next;
        surface.commit();
        fixture.exchange();
        assert_eq!(fixture.state.commits, 2);
        assert_eq!(fixture.client.formats.len(), formats);
    }
}
