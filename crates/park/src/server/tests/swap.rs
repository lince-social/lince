use super::*;
use std::{
    fs::{self, File},
    io::Write,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    path::Path,
    process::{Child, Command, Stdio},
};

const CHILD: &str = "server::tests::swap_tests::exec_child";

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Peer {
    connection: Connection,
    queue: EventQueue<Client>,
    client: Client,
}

impl Peer {
    fn new(socket: UnixStream) -> Self {
        let connection = Connection::from_socket(socket).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut peer = Self {
            connection,
            queue,
            client: Client::default(),
        };
        peer.sync().unwrap();
        peer.sync().unwrap();
        peer
    }

    fn sync(&mut self) -> Result<(), String> {
        let done = Arc::new(AtomicBool::new(false));
        self.connection
            .display()
            .sync(&self.queue.handle(), done.clone());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done.load(Ordering::Acquire) {
            if Instant::now() >= deadline {
                return Err("Wayland roundtrip timed out".into());
            }
            self.connection.flush().map_err(|error| error.to_string())?;
            self.queue
                .dispatch_pending(&mut self.client)
                .map_err(|error| error.to_string())?;
            if let Some(read) = self.queue.prepare_read() {
                let fd = read.connection_fd();
                let mut poll = [rustix::event::PollFd::new(
                    &fd,
                    rustix::event::PollFlags::IN,
                )];
                rustix::event::poll(
                    &mut poll,
                    Some(&rustix::time::Timespec {
                        tv_sec: 0,
                        tv_nsec: 10_000_000,
                    }),
                )
                .map_err(|error| error.to_string())?;
                if poll[0].revents().intersects(
                    rustix::event::PollFlags::IN
                        | rustix::event::PollFlags::HUP
                        | rustix::event::PollFlags::ERR,
                ) {
                    match read.read() {
                        Ok(_) => {}
                        Err(wayland_client::backend::WaylandError::Io(error))
                            if error.kind() == io::ErrorKind::WouldBlock => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
            }
            self.queue
                .dispatch_pending(&mut self.client)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn window(&mut self, title: &str) -> (Window, wl_buffer::WlBuffer) {
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
            .get_xdg_surface(&surface, &qh, ());
        let role = xdg.get_toplevel(&qh, ());
        role.set_title(title.into());
        surface.commit();
        self.sync().unwrap();
        let mut pixels = tempfile::tempfile().unwrap();
        pixels.write_all(&vec![255; 64 * 48 * 4]).unwrap();
        let pool =
            self.client
                .shm
                .as_ref()
                .unwrap()
                .create_pool(pixels.as_fd(), 64 * 48 * 4, &qh, ());
        let buffer = pool.create_buffer(0, 64, 48, 64 * 4, wl_shm::Format::Argb8888, &qh, ());
        pool.destroy();
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, 64, 48);
        surface.commit();
        self.sync().unwrap();
        (Window { surface, xdg, role }, buffer)
    }
}

fn write_state(directory: &Path, generation: &str, server: &Server) {
    let windows = server.windows();
    let state = serde_json::json!({
        "pid": std::process::id(),
        "executable": std::env::current_exe().unwrap(),
        "windows": windows.iter().map(|window| serde_json::json!({"title": window.title, "mapped": window.mapped})).collect::<Vec<_>>(),
    });
    let temporary = directory.join(format!("{generation}.tmp"));
    fs::write(&temporary, serde_json::to_vec(&state).unwrap()).unwrap();
    fs::rename(temporary, directory.join(format!("{generation}.json"))).unwrap();
}

fn state(directory: &Path, generation: &str, process: &mut Process) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    let path = directory.join(format!("{generation}.json"));
    loop {
        if let Ok(bytes) = fs::read(&path) {
            return serde_json::from_slice(&bytes).unwrap();
        }
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "Probe child exited: {}",
            fs::read_to_string(directory.join("child.log")).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "Probe child did not report {generation}: {}",
            fs::read_to_string(directory.join("child.log")).unwrap()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "private subprocess launched only by the isolated exec probe"]
fn exec_child() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("LINCE_SWAP_DIRECTORY").expect("private probe directory"),
    );
    let descriptor = std::env::var("LINCE_SWAP_FD")
        .unwrap()
        .parse::<i32>()
        .unwrap();
    let stream = unsafe { UnixStream::from_raw_fd(descriptor) };
    let mut server = Server::new([640, 480]).unwrap();
    server.insert_client(stream.try_clone().unwrap()).unwrap();
    let replacement = directory.join("replacement");
    let after = std::env::var_os("LINCE_SWAP_AFTER").is_some();
    if after {
        write_state(&directory, "after", &server);
    }
    let stream = if after {
        drop(stream);
        None
    } else {
        Some(stream)
    };
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut reported = false;
    while Instant::now() < deadline {
        server.dispatch().unwrap();
        if !after
            && !reported
            && server
                .windows()
                .iter()
                .filter(|window| window.mapped)
                .count()
                == 2
        {
            write_state(&directory, "before", &server);
            reported = true;
        }
        if !after && directory.join("swap").exists() {
            assert!(reported);
            rustix::io::fcntl_setfd(stream.as_ref().unwrap(), rustix::io::FdFlags::empty())
                .unwrap();
            let error = Command::new(&replacement)
                .args(["--exact", CHILD, "--ignored", "--nocapture"])
                .env("LINCE_SWAP_AFTER", "1")
                .exec();
            panic!("Replacement exec failed: {error}");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("Probe child reached its private lifetime limit");
}

#[test]
fn socket_preserving_exec_probe() {
    let directory = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let replacement = directory.path().join("replacement");
    fs::copy(&executable, &replacement).unwrap();
    let (server, client) = UnixStream::pair().unwrap();
    let descriptor = server.as_raw_fd();
    let log = File::create(directory.path().join("child.log")).unwrap();
    let mut command = Command::new(&executable);
    command
        .args(["--exact", CHILD, "--ignored", "--nocapture"])
        .env("LINCE_SWAP_DIRECTORY", directory.path())
        .env("LINCE_SWAP_FD", descriptor.to_string())
        .env_remove("LINCE_SWAP_AFTER")
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
    let mut peer = Peer::new(client);
    let first = peer.window("First surviving-window candidate");
    let second = peer.window("Second surviving-window candidate");
    let before = state(directory.path(), "before", &mut process);
    assert_eq!(before["windows"].as_array().unwrap().len(), 2);
    for window in [&first.0, &second.0] {
        window.surface.damage_buffer(0, 0, 32, 24);
        window.surface.commit();
        peer.sync().unwrap();
    }
    fs::write(directory.path().join("swap"), []).unwrap();
    let after = state(directory.path(), "after", &mut process);
    assert_eq!(before["pid"], after["pid"]);
    assert_eq!(after["executable"], serde_json::json!(replacement));
    assert_eq!(after["windows"].as_array().unwrap().len(), 0);
    first.0.surface.damage_buffer(0, 0, 32, 24);
    first.0.surface.commit();
    let error = peer.sync().expect_err(
        "An inherited socket alone must not appear to preserve the old surface objects",
    );
    assert!(
        !error.contains("timed out"),
        "The experiment needs a definite client failure, not a timeout: {error}"
    );
    let report = serde_json::json!({
        "schema": 1,
        "experiment": "park_socket_preserving_exec",
        "scope": "Headless production Park protocol server; two mapped synthetic XDG windows; exec into a separate copy of the same test executable. No physical display, GPU rendering or real terminal-agent continuity tested.",
        "outcome": "unsupported",
        "swap_complete": false,
        "control_roundtrips_passed": true,
        "pid_preserved": true,
        "client_socket_inherited": true,
        "before": before,
        "after": after,
        "client_error": error,
        "required": "Preserve the Wayland object graph and compositor state through a persistent host or an implemented state handoff before attempting live activation.",
    });
    if let Some(path) = std::env::var_os("LINCE_PARK_SWAP_REPORT") {
        fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!("{}", serde_json::to_string(&report).unwrap());
}
