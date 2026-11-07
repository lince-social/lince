use super::fixture::{Peer, Window};
use super::*;
use lince_refresh::checkpoint::{
    CheckpointFile, CheckpointScope, DescriptorPurpose, DescriptorSpec, Envelope,
    ExecutableIdentity,
};
use rustix::{
    event::{PollFd, PollFlags, poll},
    io::{FdFlags, fcntl_setfd},
    net::{RecvFlags, SendFlags, recv, send},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    os::{
        fd::{AsFd, AsRawFd, FromRawFd},
        unix::{fs::MetadataExt, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

const CHILD: &str = "server::tests::protocol_handoff_tests::exec::protocol_exec_child";
const PACKET_BYTES: usize = 16384;

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn wait(fd: impl AsFd, events: PollFlags, deadline: Instant) -> io::Result<()> {
    loop {
        let duration = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Private protocol control timed out",
                )
            })?;
        let mut descriptors = [PollFd::new(&fd, events)];
        match poll(
            &mut descriptors,
            Some(&rustix::time::Timespec {
                tv_sec: duration
                    .as_secs()
                    .try_into()
                    .map_err(|_| invalid("Control deadline overflow"))?,
                tv_nsec: duration.subsec_nanos().into(),
            }),
        ) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Private protocol control timed out",
                ));
            }
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn write_packet(fd: impl AsFd, value: &serde_json::Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > PACKET_BYTES {
        return Err(invalid("Private control packet exceeds limit"));
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        wait(fd.as_fd(), PollFlags::OUT, deadline)?;
        match send(
            fd.as_fd(),
            &bytes,
            SendFlags::DONTWAIT | SendFlags::NOSIGNAL,
        ) {
            Ok(size) if size == bytes.len() => return Ok(()),
            Ok(_) => return Err(invalid("Truncated private control packet")),
            Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn read_packet(fd: impl AsFd) -> io::Result<serde_json::Value> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        wait(fd.as_fd(), PollFlags::IN, deadline)?;
        let mut bytes = [0_u8; PACKET_BYTES];
        match recv(
            fd.as_fd(),
            &mut bytes[..],
            RecvFlags::DONTWAIT | RecvFlags::TRUNC,
        ) {
            Ok((_, size)) if size > 0 && size <= PACKET_BYTES => {
                return serde_json::from_slice(&bytes[..size]).map_err(io::Error::other);
            }
            Ok(_) => return Err(invalid("Closed or truncated private control channel")),
            Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn query(fd: impl AsFd, operation: &str) -> serde_json::Value {
    write_packet(fd.as_fd(), &serde_json::json!({"operation":operation})).unwrap();
    read_packet(fd).unwrap()
}

fn identity(path: &Path) -> io::Result<ExecutableIdentity> {
    let metadata = fs::metadata(path)?;
    let revision = std::env::var("LINCE_PROTOCOL_HANDOFF_SOURCE_REVISION").ok();
    Ok(ExecutableIdentity {
        revision,
        sha256: lince_refresh::checkpoint::sha256(&fs::read(path)?),
        device: metadata.dev(),
        inode: metadata.ino(),
        target: format!(
            "{}-unknown-linux-{}",
            std::env::consts::ARCH,
            if cfg!(target_env = "musl") {
                "musl"
            } else {
                "gnu"
            }
        ),
    })
}

fn start_ticks() -> io::Result<u64> {
    let stat = fs::read_to_string("/proc/self/stat")?;
    stat.rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .ok_or_else(|| invalid("Missing process start identity"))?
        .parse()
        .map_err(io::Error::other)
}

fn status(server: &Server, image: &ExecutableIdentity, executable: &Path) -> serde_json::Value {
    let windows = server.windows().iter().map(|window| serde_json::json!({
        "id":window.id.0, "surface":window.surface.id().handoff_key(),
        "buffer":server.state.buffers.get(&window.surface).map(|buffer| buffer.buffer().id().handoff_key()),
        "buffer_version":server.state.buffers.get(&window.surface).map(|buffer| buffer.version),
        "generation":server.state.generations.get(&window.surface),
        "mapped":window.mapped, "pixels":server.state.buffers.contains_key(&window.surface).then(|| fixture::pixels(server, window.id)),
    })).collect::<Vec<_>>();
    serde_json::json!({"pid":std::process::id(),"start_ticks":start_ticks().unwrap(),"sha256":image.sha256,"source_revision":image.revision,
        "executable":executable,"revision_step":revision_step(),"revision":server.revision(),"windows":windows,
        "serial_counter":smithay::utils::SERIAL_COUNTER.handoff_value(),
        "pending_frames":server.handoff_inventory().unwrap().pending_frame_callbacks,"swap_complete":false})
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    state: schema::State,
    raw_descriptors: Vec<i32>,
}

fn descriptor_table(snapshot: &Snapshot) -> io::Result<Vec<DescriptorSpec>> {
    let mut purposes = vec![None; snapshot.descriptors.len()];
    for client in &snapshot.state.connections.clients {
        purposes[client.socket.descriptor] = Some((
            DescriptorPurpose::WaylandClient,
            format!("client/{}/{}", client.key.slot, client.key.serial),
        ));
        for (index, descriptor) in client
            .socket
            .incoming_fds
            .iter()
            .chain(&client.socket.outgoing_fds)
            .enumerate()
        {
            purposes[*descriptor] = Some((
                DescriptorPurpose::QueuedProtocol,
                format!(
                    "client/{}/{}/queued/{index}",
                    client.key.slot, client.key.serial
                ),
            ));
        }
    }
    for (index, pool) in snapshot.state.pools.iter().enumerate() {
        if purposes[pool.descriptor].is_some() {
            return Err(invalid("Checkpoint descriptor has two ownership roles"));
        }
        purposes[pool.descriptor] = Some((DescriptorPurpose::ShmPool, format!("shm/{index}")));
    }
    purposes
        .into_iter()
        .zip(&snapshot.descriptors)
        .map(|(purpose, descriptor)| {
            let (purpose, owner) =
                purpose.ok_or_else(|| invalid("Checkpoint descriptor has no transfer owner"))?;
            DescriptorSpec::capture(purpose, owner, descriptor)
        })
        .collect()
}

fn replace(
    server: &mut Server,
    control: &OwnedFd,
    source: &ExecutableIdentity,
    candidate: &Path,
) -> io::Result<()> {
    let snapshot = capture(server)?;
    let candidate_identity = identity(candidate)?;
    let raw_descriptors = snapshot
        .descriptors
        .iter()
        .map(AsRawFd::as_raw_fd)
        .collect::<Vec<_>>();
    let payload = serde_json::to_vec(&Bootstrap {
        state: snapshot.state.clone(),
        raw_descriptors,
    })?;
    let transaction = lince_refresh::checkpoint::sha256(
        format!(
            "{}:{}:{}",
            std::process::id(),
            clock_ns()?,
            candidate_identity.sha256
        )
        .as_bytes(),
    );
    let checkpoint = CheckpointFile::create(
        Envelope {
            schema: 1,
            scope: CheckpointScope::ProtocolExperiment,
            transaction,
            source: source.clone(),
            candidate: candidate_identity,
            source_generation: 0,
            candidate_generation: 1,
            descriptors: descriptor_table(&snapshot)?,
            payload_length: 0,
            payload_sha256: String::new(),
        },
        &payload,
    )?;
    for descriptor in &snapshot.descriptors {
        fcntl_setfd(descriptor, FdFlags::empty())?;
    }
    fcntl_setfd(checkpoint.descriptor(), FdFlags::empty())?;
    fcntl_setfd(control, FdFlags::empty())?;
    let error = Command::new(candidate)
        .args(["--ignored", "--exact", CHILD, "--nocapture"])
        .env("LINCE_PROTOCOL_HANDOFF_AFTER", "1")
        .env(
            "LINCE_PROTOCOL_HANDOFF_META",
            checkpoint.descriptor().as_raw_fd().to_string(),
        )
        .exec();
    fcntl_setfd(control, FdFlags::CLOEXEC)?;
    Err(error)
}

fn inherited_raw(name: &str) -> io::Result<i32> {
    let value: i32 = std::env::var(name)
        .map_err(io::Error::other)?
        .parse()
        .map_err(io::Error::other)?;
    if value <= 2 {
        return Err(invalid("Invalid inherited private descriptor"));
    }
    Ok(value)
}

fn resumed(control: &OwnedFd) -> io::Result<Server> {
    let raw = inherited_raw("LINCE_PROTOCOL_HANDOFF_META")?;
    if raw == control.as_raw_fd() {
        return Err(invalid("Checkpoint and control descriptor overlap"));
    }
    let checkpoint = CheckpointFile::open(unsafe { OwnedFd::from_raw_fd(raw) })?;
    let active = identity(&std::env::current_exe()?)?;
    if checkpoint.envelope().scope != CheckpointScope::ProtocolExperiment
        || active != checkpoint.envelope().candidate
    {
        return Err(invalid(
            "Active executable differs from the private protocol candidate",
        ));
    }
    let payload: Bootstrap = deserialize(&checkpoint.payload()?)?;
    if payload.raw_descriptors.len() != checkpoint.envelope().descriptors.len() {
        return Err(invalid("Inherited descriptor table length differs"));
    }
    let mut unique = BTreeSet::new();
    for descriptor in &payload.raw_descriptors {
        if *descriptor <= 2
            || *descriptor == raw
            || *descriptor == control.as_raw_fd()
            || !unique.insert(*descriptor)
        {
            return Err(invalid("Invalid inherited descriptor table"));
        }
    }
    let descriptors = payload
        .raw_descriptors
        .into_iter()
        .map(|descriptor| unsafe { OwnedFd::from_raw_fd(descriptor) })
        .collect::<Vec<_>>();
    for descriptor in &descriptors {
        fcntl_setfd(descriptor, FdFlags::CLOEXEC)?;
    }
    checkpoint.envelope().validate_descriptors(&descriptors)?;
    let snapshot = Snapshot {
        state: payload.state,
        descriptors,
    };
    let server = restore(&snapshot, true)?;
    drop(snapshot);
    drop(checkpoint);
    Ok(server)
}

#[test]
#[ignore = "private production protocol exec helper"]
fn protocol_exec_child() {
    let raw = inherited_raw("LINCE_PROTOCOL_HANDOFF_CONTROL").unwrap();
    let control = unsafe { OwnedFd::from_raw_fd(raw) };
    fcntl_setfd(&control, FdFlags::CLOEXEC).unwrap();
    let after = std::env::var_os("LINCE_PROTOCOL_HANDOFF_AFTER").is_some();
    let mut server = if after {
        resumed(&control).unwrap()
    } else {
        let raw = inherited_raw("LINCE_PROTOCOL_HANDOFF_WAYLAND").unwrap();
        assert_ne!(raw, control.as_raw_fd());
        let stream = unsafe { UnixStream::from_raw_fd(raw) };
        fcntl_setfd(&stream, FdFlags::CLOEXEC).unwrap();
        let mut server = Server::new([640, 480]).unwrap();
        server.insert_client(stream).unwrap();
        server
    };
    let executable = fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
    let image = identity(&executable).unwrap();
    if after {
        write_packet(&control, &status(&server, &image, &executable)).unwrap();
    }
    let events = server
        .display
        .backend()
        .poll_fd()
        .try_clone_to_owned()
        .unwrap();
    loop {
        server.dispatch().unwrap();
        let mut descriptors = [
            PollFd::new(&control, PollFlags::IN),
            PollFd::new(&events, PollFlags::IN),
        ];
        poll(&mut descriptors, None).unwrap();
        if descriptors[0]
            .revents()
            .intersects(PollFlags::HUP | PollFlags::ERR)
        {
            return;
        }
        if !descriptors[0].revents().contains(PollFlags::IN) {
            continue;
        }
        let request = read_packet(&control).unwrap();
        match request["operation"].as_str().unwrap() {
            "status" => write_packet(&control, &status(&server, &image, &executable)).unwrap(),
            "prepare" => {
                server
                    .configure_state(SurfaceId(1), [320, 240], WindowState::default())
                    .unwrap();
                write_packet(&control, &status(&server, &image, &executable)).unwrap();
            }
            "present" => {
                let ids = server
                    .windows()
                    .iter()
                    .map(|window| window.id)
                    .collect::<Vec<_>>();
                server.presented(&ids);
                server.display.flush_clients().unwrap();
                write_packet(&control, &status(&server, &image, &executable)).unwrap();
            }
            "exec" => {
                let candidate =
                    PathBuf::from(std::env::var_os("LINCE_PROTOCOL_HANDOFF_CANDIDATE").unwrap());
                if let Err(error) = replace(&mut server, &control, &image, &candidate) {
                    write_packet(&control, &serde_json::json!({"outcome":"exec_failed","kind":format!("{:?}",error.kind()),"swap_complete":false})).unwrap();
                }
            }
            "stop" => {
                write_packet(&control, &serde_json::json!({"stopped":true})).unwrap();
                return;
            }
            _ => panic!("Unknown private protocol operation"),
        }
    }
}

fn mapped(peer: &mut Peer, tag: u32, color: u8) -> Window {
    let mut window = peer.window(tag);
    peer.sync(|| {}).unwrap();
    peer.sync(|| {}).unwrap();
    peer.attach(&mut window, color);
    peer.sync(|| {}).unwrap();
    peer.sync(|| {}).unwrap();
    window
}

#[test]
fn production_protocol_state_survives_exec() {
    use std::os::unix::fs::FileExt;
    let directory = tempfile::tempdir().unwrap();
    let initial = directory.path().join("initial");
    let candidate = directory.path().join("candidate");
    let distinct = std::env::var_os("LINCE_PROTOCOL_HANDOFF_CANDIDATE_BINARY");
    fs::copy(std::env::current_exe().unwrap(), &initial).unwrap();
    fs::copy(
        distinct
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| initial.clone()),
        &candidate,
    )
    .unwrap();
    let initial_hash = identity(&initial).unwrap().sha256;
    let candidate_hash = identity(&candidate).unwrap().sha256;
    if distinct.is_some() {
        assert_ne!(initial_hash, candidate_hash);
    }
    let (wayland, client) = UnixStream::pair().unwrap();
    let (control, child_control) = lince_refresh::transport::pair().unwrap();
    let wayland_raw = wayland.as_raw_fd();
    let control_raw = child_control.as_raw_fd();
    let log = File::create(directory.path().join("child.log")).unwrap();
    let mut command = Command::new(&initial);
    command
        .args(["--ignored", "--exact", CHILD, "--nocapture"])
        .env("LINCE_PROTOCOL_HANDOFF_CONTROL", control_raw.to_string())
        .env("LINCE_PROTOCOL_HANDOFF_WAYLAND", wayland_raw.to_string())
        .env("LINCE_PROTOCOL_HANDOFF_CANDIDATE", &candidate)
        .env_remove("LINCE_PROTOCOL_HANDOFF_AFTER")
        .env_remove("LINCE_PROTOCOL_HANDOFF_META")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log));
    unsafe {
        command.pre_exec(move || {
            fcntl_setfd(&wayland, FdFlags::empty())?;
            fcntl_setfd(&child_control, FdFlags::empty())?;
            Ok(())
        });
    }
    let mut process = Process(command.spawn().unwrap());
    drop(command);
    let mut peer = Peer::connect(client);
    peer.sync(|| {}).unwrap();
    peer.sync(|| {}).unwrap();
    let first = mapped(&mut peer, 1, 17);
    let second = mapped(&mut peer, 2, 29);
    let first_frame = Arc::new(AtomicBool::new(false));
    let second_frame = Arc::new(AtomicBool::new(false));
    first
        .surface
        .frame(&peer.queue.handle(), first_frame.clone());
    first.surface.commit();
    second
        .surface
        .frame(&peer.queue.handle(), second_frame.clone());
    second.surface.commit();
    peer.sync(|| {}).unwrap();
    let before = query(&control, "prepare");
    assert_eq!(before["pid"], process.0.id());
    assert_eq!(before["windows"].as_array().unwrap().len(), 2);
    assert_eq!(before["pending_frames"], 2);
    let acknowledgements = peer.client.acked.len();
    let after = query(&control, "exec");
    assert_eq!(after["pid"], before["pid"]);
    assert_eq!(after["sha256"], candidate_hash);
    assert_eq!(after["windows"], before["windows"]);
    assert_eq!(after["pending_frames"], 2);
    assert_eq!(after["serial_counter"], before["serial_counter"]);
    if distinct.is_some() {
        assert_eq!(before["revision_step"], 1);
        assert_eq!(after["revision_step"], 10);
    }
    peer.sync(|| {}).unwrap();
    peer.sync(|| {}).unwrap();
    assert!(peer.client.acked.len() > acknowledgements);
    assert_eq!(peer.client.releases, 0);
    query(&control, "present");
    peer.sync(|| {}).unwrap();
    assert!(first_frame.load(Ordering::Acquire) && second_frame.load(Ordering::Acquire));
    let before_commit = query(&control, "status")["revision"].as_u64().unwrap();
    second
        .pixels
        .as_ref()
        .unwrap()
        .write_all_at(&[41; 4], 0)
        .unwrap();
    second.surface.damage_buffer(0, 0, 1, 1);
    second.surface.commit();
    peer.sync(|| {}).unwrap();
    let progress = query(&control, "status");
    assert_eq!(
        progress["windows"][1]["pixels"],
        serde_json::json!([41, 41, 41, 41])
    );
    let step = progress["revision_step"].as_u64().unwrap();
    assert!(progress["revision"].as_u64().unwrap() >= before_commit + step);
    let third = mapped(&mut peer, 3, 67);
    assert_eq!(
        query(&control, "status")["windows"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    third.role.destroy();
    third.xdg.destroy();
    third.surface.destroy();
    peer.sync(|| {}).unwrap();
    let final_state = query(&control, "status");
    assert_eq!(final_state["windows"].as_array().unwrap().len(), 2);
    assert_eq!(peer.client.closes, 0);
    for _ in 0..4 {
        first.surface.commit();
        second.surface.commit();
        peer.sync(|| {}).unwrap();
    }
    let receipt = serde_json::json!({"schema":1,"experiment":"production_park_protocol_exec","scope":"Production Park and Smithay compositor/XDG/SHM with two private mapped windows; not native, seat/GPU, other protocol families, or a complete session", "outcome":"protocol_handoff_verified",
        "distinct_builds":distinct.is_some(),"before":before,"after":after,"progress":progress,"final":final_state,"initial_sha256":initial_hash,"candidate_sha256":candidate_hash,
        "pending_configure_acknowledged":true,"retained_frames_completed":2,"mapped_windows_preserved":true,"buffer_pixels_preserved":true,"new_and_destroyed_window":true,"swap_complete":false});
    if let Some(path) = std::env::var_os("LINCE_PROTOCOL_HANDOFF_REPORT") {
        fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
    eprintln!("{}", serde_json::to_string(&receipt).unwrap());
    query(&control, "stop");
    assert!(process.0.wait().unwrap().success());
}
