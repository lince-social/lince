use super::*;
use std::{io::Write, os::fd::AsFd, process::{Child, Command, Stdio}, time::{Duration, Instant}};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, protocol::wl_registry};
use wayland_protocols::wp::security_context::v1::client::{wp_security_context_manager_v1, wp_security_context_v1};

#[derive(Default)]
struct Registry {
    globals: Vec<String>,
    security: Option<wp_security_context_manager_v1::WpSecurityContextManagerV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Registry {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, handle: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            if interface == "wp_security_context_manager_v1" { state.security = Some(registry.bind(name, version.min(1), handle, ())); }
            state.globals.push(interface);
        }
    }
}

delegate_noop!(Registry: ignore wp_security_context_manager_v1::WpSecurityContextManagerV1);
delegate_noop!(Registry: ignore wp_security_context_v1::WpSecurityContextV1);

fn clipboard_globals(registry: &Registry, allowed: bool) {
    for name in ["zwlr_data_control_manager_v1", "ext_data_control_manager_v1", "wp_security_context_manager_v1"] {
        assert_eq!(registry.globals.iter().any(|interface| interface == name), allowed, "Unexpected visibility for {name}");
    }
    assert!(registry.globals.iter().any(|name| name == "wl_data_device_manager"));
    for name in ["zwlr_output_power_manager_v1", "zwlr_foreign_toplevel_manager_v1", "zwlr_screencopy_manager_v1"] {
        assert!(!registry.globals.iter().any(|interface| interface == name), "Clipboard access granted unrelated {name}");
    }
}

#[test]
#[ignore = "spawned by the private clipboard security-context test"]
fn clipboard_registry_probe_child() {
    let socket = std::env::var_os("PARK_CLIPBOARD_TEST_SOCKET").unwrap();
    let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
    let mut queue = connection.new_event_queue();
    connection.display().get_registry(&queue.handle(), ());
    let mut registry = Registry::default();
    queue.roundtrip(&mut registry).unwrap();
    clipboard_globals(&registry, true);
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("sandbox");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let (close_reader, close_writer) = UnixStream::pair().unwrap();
    let context = registry.security.as_ref().unwrap().create_listener(listener.as_fd(), close_reader.as_fd(), &queue.handle(), ());
    context.set_sandbox_engine("park-test".into());
    context.set_app_id("clipboard-private-probe".into());
    context.commit();
    queue.roundtrip(&mut registry).unwrap();
    drop(listener);
    drop(close_reader);
    let sandbox = Connection::from_socket(UnixStream::connect(&socket).unwrap()).unwrap();
    let mut sandbox_queue = sandbox.new_event_queue();
    sandbox.display().get_registry(&sandbox_queue.handle(), ());
    let mut restricted = Registry::default();
    sandbox_queue.roundtrip(&mut restricted).unwrap();
    clipboard_globals(&restricted, false);
    drop(close_writer);
    let deadline = Instant::now() + Duration::from_secs(5);
    while UnixStream::connect(&socket).is_ok() {
        queue.roundtrip(&mut registry).unwrap();
        assert!(Instant::now() < deadline, "Closed sandbox listener continued accepting clients");
        std::thread::sleep(Duration::from_millis(1));
    }
    let (invalid_listener, mut trigger) = UnixStream::pair().unwrap();
    let (close_reader, _close_writer) = UnixStream::pair().unwrap();
    let malformed = registry.security.as_ref().unwrap().create_listener(invalid_listener.as_fd(), close_reader.as_fd(), &queue.handle(), ());
    malformed.commit();
    trigger.write_all(b"invalid listening socket").unwrap();
    queue.roundtrip(&mut registry).unwrap();
    queue.roundtrip(&mut registry).unwrap();
}

struct ChildGuard(Option<Child>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 { let _ = child.kill(); let _ = child.wait(); }
    }
}

#[test]
fn normal_clients_receive_clipboard_automatically_and_security_context_clients_do_not() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("display");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut server = Server::new([640, 480]).unwrap();
    let mut child = ChildGuard(Some(Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "server::clipboard_tests::clipboard_registry_probe_child", "--ignored", "--nocapture", "--test-threads=1"])
        .env("PARK_CLIPBOARD_TEST_SOCKET", &socket).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap()));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        while let Ok((stream, _)) = listener.accept() {
            let client = server.insert_client(stream).unwrap();
            assert!(client.get_data::<ClientState>().unwrap().capabilities.is_empty());
        }
        server.dispatch().unwrap();
        if child.0.as_mut().unwrap().try_wait().unwrap().is_some() { break; }
        assert!(Instant::now() < deadline, "Clipboard registry test timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    let output = child.0.take().unwrap().wait_with_output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

fn transfer(mime: &str, contents: &[u8]) {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("display");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut server = Server::new([640, 480]).unwrap();
    let mut source = ChildGuard(Some(Command::new("wl-copy").args(["--foreground", "--type", mime])
        .env("WAYLAND_DISPLAY", &socket).env_remove("WAYLAND_SOCKET").env_remove("DISPLAY")
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap()));
    source.0.as_mut().unwrap().stdin.take().unwrap().write_all(contents).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut destination = ChildGuard(None);
    loop {
        while let Ok((stream, _)) = listener.accept() { server.insert_client(stream).unwrap(); }
        server.dispatch().unwrap();
        assert!(source.0.as_mut().unwrap().try_wait().unwrap().is_none(), "The clipboard source exited before transfer");
        if destination.0.is_none() {
            destination.0 = Some(Command::new("wl-paste").args(["--no-newline", "--type", mime])
                .env("WAYLAND_DISPLAY", &socket).env_remove("WAYLAND_SOCKET").env_remove("DISPLAY")
                .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap());
        }
        if destination.0.as_mut().unwrap().try_wait().unwrap().is_some() {
            let output = destination.0.take().unwrap().wait_with_output().unwrap();
            if output.status.success() {
                assert_eq!(output.stdout, contents);
                break;
            }
            assert!(Instant::now() < deadline, "{}", String::from_utf8_lossy(&output.stderr));
        }
        assert!(Instant::now() < deadline, "The private clipboard transfer timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(server.state.seat.get_keyboard().unwrap().current_focus().is_none());
    assert!(server.windows().is_empty(), "Clipboard tools created fallback focus windows");
}

#[test]
#[ignore = "requires installed wl-clipboard tools"]
fn fresh_clipboard_transfers_text_without_configuration_or_focus() {
    transfer("text/plain;charset=utf-8", "Park private clipboard test: ação λ\n".as_bytes());
}

#[test]
#[ignore = "requires installed wl-clipboard tools"]
fn fresh_clipboard_transfers_png_without_configuration_or_focus() {
    transfer("image/png", b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\x0d\x49\x44\x41\x54\x78\x9c\x63\x10\xa9\xb8\xf3\x1f\x00\x04\x74\x02\x68\x62\x8a\xea\xc3\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82");
}
