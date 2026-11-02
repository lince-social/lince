use super::*;
use wayland_protocols_wlr::output_power_management::v1::client::{
    zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1 as Manager,
    zwlr_output_power_v1::{self as power, ZwlrOutputPowerV1 as Power},
};

#[derive(Default)]
pub(super) struct Capture {
    pub(in crate::server::tests) manager: Option<Manager>,
    pub(in crate::server::tests) modes: Vec<(u32, bool)>,
    pub(in crate::server::tests) failed: Vec<u32>,
}

pub(super) fn bind(state: &mut Client, registry: &wl_registry::WlRegistry, name: u32, version: u32, qh: &QueueHandle<Client>) {
    state.power.manager = Some(registry.bind(name, version.min(1), qh, ()));
}

delegate_noop!(Client: ignore Manager);

impl Dispatch<Power, ()> for Client {
    fn event(state: &mut Self, object: &Power, event: power::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            power::Event::Mode { mode: wayland_client::WEnum::Value(mode) } => state.power.modes.push((object.id().protocol_id(), mode == power::Mode::On)),
            power::Event::Failed => state.power.failed.push(object.id().protocol_id()),
            _ => {}
        }
    }
}

fn harness(physical: bool) -> Harness {
    let mut server = Server::new([640, 480]).unwrap();
    server.output_power_backend(physical);
    if physical { server.output_power_state(OutputId(1), Some(true)); }
    let mut harness = Harness::connect_authorized(server, Some(vec![crate::launch::Capability::OutputControl]));
    harness.server.registered_output_catalog().unwrap();
    harness
}

fn control(harness: &mut Harness) -> Power {
    let resource = harness.client.power.manager.as_ref().unwrap().get_output_power(
        harness.client.output.as_ref().unwrap(), &harness.queue.handle(), ());
    harness.sync();
    resource
}

#[test]
fn output_power_requires_its_own_connection_authority_and_nested_outputs_report_unavailable() {
    let ordinary = Harness::new();
    assert!(ordinary.client.power.manager.is_none());
    assert!(!ordinary.client.globals.iter().any(|name| name == "zwlr_output_power_manager_v1"));
    let mut nested = harness(false);
    let resource = control(&mut nested);
    assert_eq!(nested.client.power.failed, vec![resource.id().protocol_id()]);
    assert!(nested.client.power.modes.is_empty());
    resource.set_mode(power::Mode::Off);
    nested.sync();
    assert!(nested.server.output_power_requests().is_empty());
    assert!(nested.server.output_powered(OutputId(1)));
    resource.destroy(); nested.sync();
}

#[test]
#[ignore = "spawns a bounded helper on a private simulated output, not physical DPMS"]
fn external_power_helper_uses_its_inherited_authority_and_public_clients_remain_denied() {
    use std::process::Stdio;
    let mut server = Server::new([640, 480]).unwrap();
    server.output_power_backend(true);
    server.output_power_state(OutputId(1), Some(true));
    let socket = server.listen().unwrap();
    let directory = tempfile::tempdir().unwrap();
    for (index, on, authorized, confirm) in [(0, false, true, true), (1, true, true, true), (2, false, false, false), (3, false, true, false)] {
        let path = directory.path().join(format!("power-{index}.log"));
        let report = directory.path().join(format!("power-{index}.result"));
        let mut arguments = vec![format!("LINCE_PARK_POWER_MODE={}", if on { "on" } else { "off" }),
            format!("LINCE_PARK_POWER_REPORT={}", report.display())];
        if let Ok(libraries) = std::env::var("LD_LIBRARY_PATH") { arguments.push(format!("LD_LIBRARY_PATH={libraries}")); }
        arguments.extend([std::env::current_exe().unwrap().to_string_lossy().into_owned(), "--exact".into(),
            "server::tests::power_tests::external_power_helper_child".into(), "--ignored".into(), "--test-threads=1".into()]);
        let description = crate::launch::Description {
            program: "env".into(), arguments,
            directory: None, capabilities: if authorized { vec![crate::launch::Capability::OutputControl] } else { Vec::new() },
        };
        let mut application = crate::launch::Application::from_description(&description).unwrap().command(&socket).unwrap();
        application.stdout(std::fs::File::create(&path).unwrap()).stderr(Stdio::from(std::fs::OpenOptions::new().append(true).open(&path).unwrap()));
        let mut child = if authorized { server.spawn_helper(&description, &socket).unwrap() } else { application.spawn().unwrap() };
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut count = 0;
        let mut pending = None;
        loop {
            server.dispatch().unwrap();
            for request in server.output_power_requests() {
                assert!(authorized);
                assert_eq!(request.output, OutputId(1));
                assert_eq!(request.on, on);
                assert!(server.output_power_current(&request));
                assert!(request.deadline <= Instant::now() + Duration::from_secs(2));
                count += 1;
                pending = Some((Instant::now() + Duration::from_millis(100), request));
            }
            if confirm && pending.as_ref().is_some_and(|(after, _)| Instant::now() >= *after) {
                let (_, request) = pending.take().unwrap();
                assert!(server.output_power_current(&request), "The power helper disconnected before backend confirmation");
                server.output_power_state(request.output, Some(request.on));
            }
            if let Some(status) = child.try_wait().unwrap() {
                if confirm { assert!(pending.is_none(), "The power helper exited before backend confirmation"); }
                assert_eq!(status.success(), authorized && confirm, "{}", std::fs::read_to_string(&path).unwrap());
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap(); child.wait().unwrap();
                panic!("The external power helper did not finish");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(count, usize::from(authorized));
        assert_eq!(std::fs::read_to_string(report).unwrap(), if confirm { "ok" } else if authorized { "TimedOut" } else { "PermissionDenied" });
        assert_eq!(server.output_powered(OutputId(1)), if authorized && confirm { on } else { true });
    }
}

#[test]
#[ignore = "invoked by the private helper fixture with an inherited connection"]
fn external_power_helper_child() -> std::io::Result<()> {
    let result = crate::power_client::run(std::env::var("LINCE_PARK_POWER_MODE").unwrap() == "on");
    let report = result.as_ref().err().map(|error| format!("{:?}", error.kind())).unwrap_or_else(|| "ok".into());
    std::fs::write(std::env::var("LINCE_PARK_POWER_REPORT").unwrap(), report)?;
    result
}

#[test]
fn output_power_reports_only_backend_observations_and_keeps_windows_and_output_configuration() {
    let mut harness = harness(true);
    let window = harness.window();
    let _buffer = harness.attach(&window, [80, 60]);
    let before = harness.server.display_snapshot();
    let windows = harness.server.windows().iter().map(|window| window.id).collect::<Vec<_>>();
    let resource = control(&mut harness);
    let id = resource.id().protocol_id();
    assert_eq!(harness.client.power.modes, vec![(id, true)]);
    for (index, on) in [false, true, false, true].into_iter().enumerate() {
        let count = harness.client.power.modes.len();
        resource.set_mode(if on { power::Mode::On } else { power::Mode::Off });
        harness.sync();
        let request = harness.server.output_power_requests().pop().unwrap();
        assert_eq!(request.output, OutputId(1));
        assert_eq!(request.on, on);
        assert!(request.deadline > Instant::now());
        assert!(request.deadline <= Instant::now() + Duration::from_secs(2));
        assert!(harness.server.output_power_current(&request));
        assert_eq!(harness.client.power.modes.len(), count);
        harness.server.output_power_state(OutputId(1), Some(on));
        harness.sync();
        assert_eq!(harness.client.power.modes.len(), index + 2);
        assert_eq!(harness.client.power.modes.last(), Some(&(id, on)));
        assert_eq!(harness.server.display_snapshot().configuration(), before.configuration());
        assert_eq!(harness.server.display_snapshot().revision, before.revision);
        assert_eq!(harness.server.windows().iter().map(|window| window.id).collect::<Vec<_>>(), windows);
        assert!(!harness.server.locked());
    }
    resource.destroy(); harness.sync();
}

#[test]
fn replacing_an_uncommitted_power_request_does_not_let_its_failure_poison_the_new_request() {
    let mut harness = harness(true);
    let resource = control(&mut harness);
    resource.set_mode(power::Mode::Off); harness.sync();
    let before = harness.server.output_power_requests().pop().unwrap();
    resource.set_mode(power::Mode::On); harness.sync();
    let current = harness.server.output_power_requests().pop().unwrap();
    assert!(!harness.server.output_power_current(&before));
    assert!(harness.server.output_power_current(&current));
    harness.server.output_power_failed(before); harness.sync();
    assert!(harness.client.power.failed.is_empty());
    assert!(harness.server.output_power_current(&current));
    resource.destroy(); harness.sync();
    assert!(!harness.server.output_power_current(&current));
}

#[test]
fn destroying_the_manager_keeps_its_controls_valid_and_output_replacement_retires_the_old_target() {
    let mut harness = harness(true);
    let resource = control(&mut harness);
    harness.client.power.manager.take().unwrap().destroy(); harness.sync();
    resource.set_mode(power::Mode::Off); harness.sync();
    let request = harness.server.output_power_requests().pop().unwrap();
    assert!(harness.server.output_power_current(&request));
    harness.server.configure_outputs(&[Monitor { id: OutputId(2), name: "Other".into(), origin: [0, 0], size: [640, 480] }]).unwrap();
    harness.sync();
    assert!(!harness.server.output_power_current(&request));
    assert_eq!(harness.client.power.failed, vec![resource.id().protocol_id()]);
    let count = harness.client.power.modes.len();
    harness.server.configure_outputs(&[Monitor { id: OutputId(1), name: "Replacement".into(), origin: [0, 0], size: [640, 480] }]).unwrap();
    harness.server.output_power_state(OutputId(1), Some(false)); harness.sync();
    assert!(!harness.server.output_power_current(&request));
    assert_eq!(harness.client.power.modes.len(), count);
    assert_eq!(harness.client.power.failed.len(), 1);
    resource.destroy(); harness.sync();
}

#[test]
fn sleep_wake_and_helper_failure_cannot_release_the_session_lock() {
    let mut harness = harness(true);
    let resource = control(&mut harness);
    let lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    for on in [false, true] {
        resource.set_mode(if on { power::Mode::On } else { power::Mode::Off }); harness.sync();
        let request = harness.server.output_power_requests().pop().unwrap();
        assert!(harness.server.output_power_current(&request));
        harness.server.output_power_state(OutputId(1), Some(on)); harness.sync();
        assert!(harness.server.locked());
    }
    resource.set_mode(power::Mode::Off); harness.sync();
    let request = harness.server.output_power_requests().pop().unwrap();
    harness.server.output_power_failed(request); harness.sync();
    assert!(harness.server.locked());
    assert_eq!(harness.client.power.failed.len(), 1);
    assert!(harness.client.lock_replies.is_empty());
    resource.destroy(); lock.destroy(); harness.sync();
    assert!(harness.server.locked());
}

#[test]
fn disconnected_power_helpers_cannot_replay_a_captured_request_on_a_new_connection() {
    let (mut server, request) = {
        let mut harness = harness(true);
        let resource = control(&mut harness);
        resource.set_mode(power::Mode::Off); harness.sync();
        let request = harness.server.output_power_requests().pop().unwrap();
        assert!(harness.server.output_power_current(&request));
        (harness.server, request)
    };
    server.dispatch().unwrap();
    assert!(!server.output_power_current(&request));
    let mut replacement = Harness::connect_authorized(server, Some(vec![crate::launch::Capability::OutputControl]));
    let resource = control(&mut replacement);
    assert!(!replacement.server.output_power_current(&request));
    assert_eq!(replacement.client.power.modes.last(), Some(&(resource.id().protocol_id(), true)));
    resource.destroy(); replacement.sync();
}

#[test]
fn a_full_power_queue_fails_only_the_excess_control_and_keeps_requests_bounded() {
    let mut harness = harness(true);
    let objects: Vec<_> = (0..17).map(|_| control(&mut harness)).collect();
    for object in &objects { object.set_mode(power::Mode::Off); }
    harness.sync();
    let requests = harness.server.output_power_requests();
    assert_eq!(requests.len(), 16);
    assert_eq!(harness.client.power.failed, vec![objects[16].id().protocol_id()]);
    assert!(requests.iter().all(|request| harness.server.output_power_current(request)));
    assert!(harness.server.output_powered(OutputId(1)));
    for object in objects { object.destroy(); }
    harness.sync();
    assert!(requests.iter().all(|request| !harness.server.output_power_current(request)));
}

#[test]
fn malformed_power_modes_disconnect_only_the_offending_connection_and_fresh_clients_still_work() {
    let mut harness = harness(true);
    let resource = control(&mut harness);
    resource.send_request(power::Request::SetMode { mode: wayland_client::WEnum::Unknown(999) }).unwrap();
    let failure = synchronize(&mut harness.server, &harness.connection, &mut harness.queue, &mut harness.client).unwrap_err();
    assert!(!failure.contains("timed out"));
    assert!(harness.server.output_power_requests().is_empty());
    assert!(harness.server.output_powered(OutputId(1)));
    let mut recovered = Harness::connect_authorized(harness.server, Some(vec![crate::launch::Capability::OutputControl]));
    let replacement = control(&mut recovered);
    replacement.set_mode(power::Mode::Off); recovered.sync();
    assert_eq!(recovered.server.output_power_requests().len(), 1);
    replacement.destroy(); recovered.sync();
}

#[test]
fn control_object_pressure_has_a_bounded_connection_failure_and_does_not_starve_a_new_helper() {
    let mut harness = harness(true);
    let manager = harness.client.power.manager.as_ref().unwrap();
    let output = harness.client.output.as_ref().unwrap();
    let _objects: Vec<_> = (0..33).map(|_| manager.get_output_power(output, &harness.queue.handle(), ())).collect();
    let failure = synchronize(&mut harness.server, &harness.connection, &mut harness.queue, &mut harness.client).unwrap_err();
    assert!(!failure.contains("timed out"));
    assert!(harness.server.output_powered(OutputId(1)));
    let mut recovered = Harness::connect_authorized(harness.server, Some(vec![crate::launch::Capability::OutputControl]));
    let resource = control(&mut recovered);
    assert!(recovered.client.power.failed.is_empty());
    assert_eq!(recovered.client.power.modes.last(), Some(&(resource.id().protocol_id(), true)));
    resource.destroy(); recovered.sync();
}
