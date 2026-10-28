use super::*;
use crate::{control_client::ControlClient, launch::{Capability, Description}};
use std::{os::unix::fs::MetadataExt, sync::mpsc::{Receiver, TryRecvError, channel}};

fn socket(server: &mut Server) -> UnixStream {
    let (server_socket, client_socket) = UnixStream::pair().unwrap();
    rustix::net::sockopt::set_socket_send_buffer_size(&client_socket, 4096).unwrap();
    server.insert_authorized_client(server_socket, vec![Capability::WindowList, Capability::DesktopControl], false).unwrap();
    client_socket
}

fn receive<T>(server: &mut Server, receiver: &Receiver<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        match receiver.try_recv() {
            Ok(value) => return value,
            Err(TryRecvError::Disconnected) => panic!("The public SDK fixture disconnected before reporting"),
            Err(TryRecvError::Empty) => {}
        }
        assert!(Instant::now() < deadline, "The public SDK fixture did not finish");
        server.dispatch().unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn the_public_sdk_recovers_from_actual_socket_write_backpressure_without_resending_a_snapshot() {
    let mut server = Server::new([640, 480]).unwrap();
    let stream = socket(&mut server);
    let (ready, started) = channel();
    let (resume, proceed) = channel();
    let (blocked, pressure) = channel();
    let (finished, result) = channel();
    let thread = std::thread::spawn(move || {
        let connection = Connection::from_socket(stream).unwrap();
        let mut client = ControlClient::from_connection(connection.clone()).unwrap();
        ready.send(()).unwrap();
        proceed.recv_timeout(Duration::from_secs(5)).unwrap();
        let queue = connection.new_event_queue::<Client>();
        let mut saturated = false;
        for _ in 0..4096 {
            connection.display().sync(&queue.handle(), Arc::new(AtomicBool::new(false)));
            match connection.flush() {
                Ok(()) => {}
                Err(wayland_client::backend::WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => { saturated = true; break; }
                Err(error) => panic!("Unexpected Wayland write failure: {error}"),
            }
        }
        assert!(saturated, "The fixture must observe a real full socket before claiming write-pressure recovery");
        blocked.send(()).unwrap();
        let response = client.snapshot(0, 0).unwrap();
        assert!(matches!(response, Response::Snapshot { next: None, .. }));
        client.close().unwrap();
        finished.send(()).unwrap();
    });
    receive(&mut server, &started);
    resume.send(()).unwrap();
    pressure.recv_timeout(Duration::from_secs(5)).unwrap();
    receive(&mut server, &result);
    thread.join().unwrap();
    server.dispatch().unwrap();
    assert!(server.control_work().is_empty());
    assert_eq!(server.control_status().pending, 0);
    assert_eq!(server.control_status().readers, 0);
}

#[test]
fn a_public_sdk_mutation_timeout_never_replays_the_command_and_a_late_reply_cannot_replace_a_fresh_snapshot() {
    let mut server = Server::new([640, 480]).unwrap();
    let stream = socket(&mut server);
    let (ready, started) = channel();
    let (resume, proceed) = channel();
    let (expired, timeout) = channel();
    let (refresh, refreshed) = channel();
    let (finished, result) = channel();
    let thread = std::thread::spawn(move || {
        let mut client = ControlClient::from_connection(Connection::from_socket(stream).unwrap()).unwrap();
        ready.send(()).unwrap();
        proceed.recv_timeout(Duration::from_secs(5)).unwrap();
        let error = client.invoke(&call()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("outcome may be unknown"));
        expired.send(()).unwrap();
        refreshed.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(client.snapshot(0, 0).unwrap(), Response::Snapshot { .. }));
        finished.send(()).unwrap();
    });
    receive(&mut server, &started);
    resume.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let work = loop {
        server.dispatch().unwrap();
        if let Some(work) = server.control_work().pop() { break work; }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    timeout.recv_timeout(Duration::from_secs(14)).unwrap();
    assert!(server.control_work().is_empty());
    work.complete(Ok(Disposition::Applied));
    server.dispatch().unwrap();
    refresh.send(()).unwrap();
    receive(&mut server, &result);
    thread.join().unwrap();
    server.dispatch().unwrap();
    assert_eq!(server.control_status().pending, 0);
    assert!(server.control_work().is_empty());
}

#[test]
fn the_public_sdk_consumes_resync_and_reads_the_final_revision_after_a_real_slow_observer_flood() {
    let mut server = Server::new([640, 480]).unwrap();
    let stream = socket(&mut server);
    let (ready, started) = channel();
    let (resume, proceed) = channel();
    let (finished, result) = channel();
    let thread = std::thread::spawn(move || {
        let mut client = ControlClient::from_connection(Connection::from_socket(stream).unwrap()).unwrap();
        client.subscribe(true).unwrap();
        ready.send(()).unwrap();
        proceed.recv_timeout(Duration::from_secs(5)).unwrap();
        let first = client.next_change(Duration::from_secs(5)).unwrap();
        let final_change = client.next_change(Duration::from_secs(5)).unwrap();
        assert!(final_change.0 > first.0);
        assert_ne!(final_change.1 & CHANGE_RESYNC, 0);
        let Response::Snapshot { revision, entries, next } = client.snapshot(0, 0).unwrap() else { panic!("Resync must produce a fresh actual snapshot"); };
        assert_eq!(revision, final_change.0);
        assert_eq!(next, None);
        assert!(entries.iter().any(|entry| matches!(entry, Entry::Workspace { name, .. } if name == "SDK final workspace")));
        assert_eq!(client.next_change(Duration::from_millis(30)).unwrap_err().kind(), io::ErrorKind::TimedOut);
        client.close().unwrap();
        finished.send(()).unwrap();
    });
    receive(&mut server, &started);
    server.dispatch().unwrap();
    for index in 0..EVENT_LIMIT + 10 {
        server.workspaces(WorkspaceSnapshot { owner: 11, entries: vec![WorkspaceEntry { id: 1, name: format!("SDK workspace {index}"), active: true }] });
        server.dispatch().unwrap();
    }
    server.workspaces(WorkspaceSnapshot { owner: 11, entries: vec![WorkspaceEntry { id: 1, name: "SDK final workspace".into(), active: true }] });
    server.dispatch().unwrap();
    resume.send(()).unwrap();
    receive(&mut server, &result);
    thread.join().unwrap();
    server.dispatch().unwrap();
    assert_eq!(server.control_status().pending, 0);
}

#[test]
#[ignore = "only spawned by the authorized helper restart fixture"]
fn authorized_helper_child() {
    let raw = std::env::var("WAYLAND_SOCKET").expect("The helper must inherit its authorized FD").parse::<u32>().unwrap();
    let inode = std::fs::metadata(format!("/proc/self/fd/{raw}")).unwrap().ino();
    let mut client = ControlClient::connect().unwrap();
    assert!(matches!(client.snapshot(0, 0).unwrap(), Response::Snapshot { .. }));
    let response = client.invoke(&call()).unwrap();
    std::fs::write("helper-result.json", serde_json::to_vec(&serde_json::json!({"pid":std::process::id(), "inode":inode, "response":response})).unwrap()).unwrap();
}

#[test]
fn restarting_an_owner_selected_helper_uses_a_fresh_authorized_fd_and_only_its_new_connection_grants() {
    let directory = tempfile::tempdir().unwrap();
    let mut server = Server::new([640, 480]).unwrap();
    let mut reports = Vec::new();
    for generation in 0..2 {
        let path = directory.path().join(format!("helper-{generation}"));
        std::fs::create_dir(&path).unwrap();
        let capabilities = if generation == 0 { vec![Capability::WindowList, Capability::DesktopControl] } else { vec![Capability::WindowList] };
        let description = Description { program: std::env::current_exe().unwrap().to_str().unwrap().into(), arguments: vec!["--exact".into(), "server::tests::control_tests::sdk::authorized_helper_child".into(), "--ignored".into(), "--test-threads=1".into()], directory: Some(path.clone()), capabilities: capabilities.clone() };
        let mut child = server.spawn_helper(&description, &directory.path().join("wayland-authorized-fixture")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut requests = 0;
        let mut observed = false;
        loop {
            server.dispatch().unwrap();
            observed |= server.control_status().readers == 1;
            for work in server.control_work() {
                requests += 1;
                assert_eq!(work.grants(), &capabilities);
                work.complete(Ok(Disposition::Dispatched));
            }
            if let Some(status) = child.try_wait().unwrap() { assert!(status.success()); break; }
            if Instant::now() >= deadline { child.kill().unwrap(); child.wait().unwrap(); panic!("The selected helper did not finish"); }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(observed);
        assert_eq!(requests, usize::from(generation == 0));
        let report: serde_json::Value = serde_json::from_slice(&std::fs::read(path.join("helper-result.json")).unwrap()).unwrap();
        let response: Response = serde_json::from_value(report["response"].clone()).unwrap();
        if generation == 0 { assert!(matches!(response, Response::Complete { outcome: Disposition::Dispatched })); }
        else { assert_eq!(failed(&response), Error::Denied); }
        reports.push(report);
        let deadline = Instant::now() + Duration::from_secs(5);
        while server.control_status().readers != 0 {
            server.dispatch().unwrap();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(server.control_status().controllers, 0);
        assert_eq!(server.control_status().pending, 0);
    }
    assert_ne!(reports[0]["pid"], reports[1]["pid"]);
    assert_ne!(reports[0]["inode"], reports[1]["inode"]);
}
