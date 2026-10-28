use super::*;
use crate::control_protocol::client::lince_park_control_v1::{self as protocol, LinceParkControlV1 as Manager};
use lince_interface::host::control::{Arguments, Catalog, Context, Disposition, Entry, Error, Invocation, Response, CHANGE_RESYNC, EVENT_LIMIT, IN_FLIGHT_LIMIT};

#[path = "control/sdk.rs"]
mod sdk;
#[path = "control/outputs.rs"]
mod outputs;

#[derive(Default)]
pub(super) struct Capture {
    pub(super) manager: Option<Manager>,
    pub(super) replies: BTreeMap<u32, Response>,
    updates: Vec<(u64, u32)>,
    hold_ack: bool,
    acknowledgement: Option<(String, String)>,
}

impl Dispatch<Manager, ()> for Client {
    fn event(state: &mut Self, manager: &Manager, event: protocol::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let capture = &mut state.control;
        let (serial, token) = match event {
            protocol::Event::Reply { id, body, serial, token } => {
                let body = crate::control_protocol::read_response(body).unwrap();
                capture.replies.insert(id, serde_json::from_slice(&body).unwrap());
                (serial, token)
            }
            protocol::Event::Changed { revision, scope, serial, token } => {
                capture.updates.push((revision.parse().unwrap(), scope));
                (serial, token)
            }
        };
        if capture.hold_ack { capture.acknowledgement = Some((serial, token)); }
        else { manager.acknowledge(serial, token); }
    }
}

fn call() -> Invocation {
    Invocation { command: "operation.open".into(), context: Context { owner: 11, canvas: "canvas-control-fixture".into(), workspace: 1, output: Some(1), item: None }, arguments: Arguments::None }
}

fn controller() -> Harness {
    Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::WindowList, crate::launch::Capability::DesktopControl]))
}

fn failed(response: &Response) -> Error {
    match response { Response::Failed { failure } => failure.code, other => panic!("Expected a typed refusal, got {other:?}") }
}

#[test]
fn the_management_global_and_command_authority_are_connection_scoped() {
    use crate::launch::Capability;
    for grants in [vec![], vec![Capability::Clipboard], vec![Capability::WindowList], vec![Capability::DesktopControl], vec![Capability::WindowControl]] {
        let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(grants.clone()));
        assert_eq!(harness.client.control.manager.is_some(), grants.contains(&Capability::WindowList) || grants.contains(&Capability::DesktopControl));
        let Some(manager) = harness.client.control.manager.clone() else { continue; };
        manager.snapshot(1, "0".into(), 0);
        manager.invoke(2, serde_json::to_string(&call()).unwrap());
        harness.sync();
        if !grants.contains(&Capability::WindowList) { assert_eq!(failed(&harness.client.control.replies[&1]), Error::Denied); }
        else { assert!(matches!(harness.client.control.replies[&1], Response::Snapshot { .. })); }
        if !grants.contains(&Capability::DesktopControl) {
            assert_eq!(failed(&harness.client.control.replies[&2]), Error::Denied);
            assert!(harness.server.control_work().is_empty());
        } else {
            let work = harness.server.control_work().pop().unwrap();
            assert_eq!(work.grants(), &grants);
            work.complete(Ok(Disposition::Dispatched));
            harness.sync();
            assert!(matches!(harness.client.control.replies[&2], Response::Complete { outcome: Disposition::Dispatched }));
        }
    }
}

#[test]
fn request_admission_reserves_terminal_slots_and_coalesces_a_slow_observers_state_history() {
    let mut harness = controller();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].id;
    harness.server.focus(Some(surface), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.sync();
    harness.client.keys.clear();
    harness.client.control.hold_ack = true;
    let manager = harness.client.control.manager.clone().unwrap();
    manager.subscribe(1);
    for id in 1..=20 { manager.invoke(id, serde_json::to_string(&call()).unwrap()); }
    harness.sync();
    assert_eq!(harness.server.control_status().pending as usize, IN_FLIGHT_LIMIT);
    for id in 17..=20 { assert_eq!(failed(&harness.client.control.replies[&id]), Error::Busy); }
    for index in 0..EVENT_LIMIT + 10 {
        harness.server.workspaces(WorkspaceSnapshot { owner: 11, entries: vec![WorkspaceEntry { id: 1, name: format!("Workspace {index}"), active: true }] });
        harness.server.dispatch().unwrap();
    }
    harness.sync();
    assert_eq!(harness.client.control.updates.len(), 1);
    harness.server.keyboard(30, true, 1, |_| false).unwrap();
    harness.server.keyboard(30, false, 2, |_| false).unwrap();
    harness.sync();
    assert_eq!(harness.client.keys, vec![(30, true), (30, false)]);
    let mut work = harness.server.control_work();
    work.extend(harness.server.control_work());
    assert_eq!(work.len(), IN_FLIGHT_LIMIT);
    work.remove(0).complete(Ok(Disposition::Applied));
    harness.sync();
    assert!(matches!(harness.client.control.replies[&1], Response::Complete { outcome: Disposition::Applied }));
    let (serial, token) = harness.client.control.acknowledgement.take().unwrap();
    manager.acknowledge(serial, token);
    harness.sync();
    assert_eq!(harness.client.control.updates.len(), 2);
    assert_eq!(harness.client.control.updates.last().unwrap().1, CHANGE_RESYNC);
    drop(work);
    harness.sync();
    assert_eq!(harness.server.control_status().pending, 0);
}

#[test]
fn malformed_and_oversized_invocations_do_not_reserve_or_execute_work() {
    let mut harness = controller();
    let manager = harness.client.control.manager.clone().unwrap();
    manager.invoke(1, "{".into());
    manager.invoke(2, "x".repeat(2048));
    let mut value = serde_json::to_value(call()).unwrap();
    value["capabilities"] = serde_json::json!(["capture", "output-control"]);
    manager.invoke(3, serde_json::to_string(&value).unwrap());
    harness.sync();
    for id in 1..=3 { assert_eq!(failed(&harness.client.control.replies[&id]), Error::InvalidArguments); }
    assert_eq!(harness.server.control_status().pending, 0);
    assert!(harness.server.control_work().is_empty());
    manager.invoke(4, "x".repeat(lince_interface::host::control::ARGUMENT_LIMIT + 1));
    assert!(synchronize(&mut harness.server, &harness.connection, &mut harness.queue, &mut harness.client).is_err());
    assert_eq!(harness.server.control_status().pending, 0);
    assert!(harness.server.control_work().is_empty());
    let (server_stream, client_stream) = UnixStream::pair().unwrap();
    harness.server.insert_authorized_client(server_stream, vec![crate::launch::Capability::WindowList, crate::launch::Capability::DesktopControl], false).unwrap();
    let connection = Connection::from_socket(client_stream).unwrap();
    let mut queue = connection.new_event_queue();
    connection.display().get_registry(&queue.handle(), ());
    let mut client = Client::default();
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    client.control.manager.as_ref().unwrap().invoke(1, serde_json::to_string(&call()).unwrap());
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    let work = harness.server.control_work().pop().unwrap();
    work.complete(Ok(Disposition::Dispatched));
    synchronize(&mut harness.server, &connection, &mut queue, &mut client).unwrap();
    assert!(matches!(client.control.replies[&1], Response::Complete { outcome: Disposition::Dispatched }));
}

#[test]
fn locking_and_unlocking_cannot_revive_an_earlier_queued_request() {
    let mut harness = controller();
    let manager = harness.client.control.manager.clone().unwrap();
    manager.invoke(1, serde_json::to_string(&call()).unwrap());
    harness.sync();
    let work = harness.server.control_work().pop().unwrap();
    let lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    assert!(matches!(work.valid().unwrap_err().code, Error::Locked | Error::Cancelled));
    assert_eq!(failed(&harness.client.control.replies[&1]), Error::Locked);
    harness.server.lock_presented();
    harness.sync();
    lock.unlock_and_destroy();
    harness.sync();
    assert!(!harness.server.locked());
    assert!(work.valid().is_err());
    manager.invoke(2, serde_json::to_string(&call()).unwrap());
    harness.sync();
    let fresh = harness.server.control_work().pop().unwrap();
    assert!(fresh.valid().is_ok());
    fresh.complete(Ok(Disposition::Applied));
    harness.sync();
    assert!(matches!(harness.client.control.replies[&2], Response::Complete { .. }));
}

#[test]
fn owner_revocation_and_resource_destruction_retire_unexecuted_work() {
    for revoke in [false, true] {
        let mut harness = controller();
        let manager = harness.client.control.manager.clone().unwrap();
        manager.invoke(1, serde_json::to_string(&call()).unwrap());
        harness.sync();
        let work = harness.server.control_work().pop().unwrap();
        if revoke { harness.server.revoke_control(); }
        else { manager.destroy(); harness.sync(); }
        assert_eq!(work.valid().unwrap_err().code, Error::Cancelled);
        assert_eq!(harness.server.control_status().pending, 0);
        assert!(harness.server.control_work().is_empty());
    }
}

#[test]
fn snapshot_pages_keep_one_revision_and_report_stale_or_invalid_cursors() {
    let mut harness = controller();
    harness.client.workspace_manager.as_ref().unwrap().stop();
    harness.sync();
    harness.server.control_catalog(Catalog { context: Some(call().context), commands: Vec::new() });
    harness.server.workspaces(WorkspaceSnapshot { owner: 11, entries: (1..=200).map(|id| WorkspaceEntry { id, name: "界".repeat(300), active: id == 1 }).collect() });
    harness.sync();
    let manager = harness.client.control.manager.clone().unwrap();
    manager.snapshot(1, "0".into(), 0);
    harness.sync();
    let Response::Snapshot { revision, next: Some(cursor), entries } = &harness.client.control.replies[&1] else { panic!("The bounded snapshot needs multiple pages"); };
    assert!(entries.iter().any(|entry| matches!(entry, Entry::Context { value: Some(_) })));
    let (revision, cursor) = (*revision, *cursor);
    manager.snapshot(2, revision.to_string(), cursor);
    harness.sync();
    assert!(matches!(harness.client.control.replies[&2], Response::Snapshot { revision: current, .. } if current == revision));
    harness.server.workspaces(WorkspaceSnapshot { owner: 11, entries: vec![WorkspaceEntry { id: 99, name: "New revision".into(), active: true }] });
    harness.sync();
    manager.snapshot(3, revision.to_string(), cursor);
    manager.snapshot(4, "0".into(), 1);
    harness.sync();
    assert_eq!(failed(&harness.client.control.replies[&3]), Error::StaleTarget);
    assert_eq!(failed(&harness.client.control.replies[&4]), Error::InvalidArguments);
}

#[test]
fn duplicate_in_flight_identifiers_disconnect_without_a_second_dispatch() {
    let mut harness = controller();
    let manager = harness.client.control.manager.clone().unwrap();
    let body = serde_json::to_string(&call()).unwrap();
    manager.invoke(1, body.clone());
    manager.invoke(1, body);
    let result = synchronize(&mut harness.server, &harness.connection, &mut harness.queue, &mut harness.client);
    assert!(result.is_err());
    assert_eq!(harness.server.control_status().pending, 0);
    assert!(harness.server.control_work().is_empty());
}

#[test]
fn a_closed_wayland_connection_invalidates_an_already_delivered_work_item() {
    let mut harness = controller();
    harness.client.control.manager.as_ref().unwrap().invoke(1, serde_json::to_string(&call()).unwrap());
    harness.sync();
    let work = harness.server.control_work().pop().unwrap();
    let Harness { mut server, connection, queue, client } = harness;
    drop(client);
    drop(queue);
    drop(connection);
    server.dispatch().unwrap();
    assert_eq!(work.valid().unwrap_err().code, Error::Cancelled);
    assert_eq!(server.control_status().pending, 0);
}

#[test]
fn inactive_seat_round_trips_do_not_revive_a_previously_queued_request() {
    let mut harness = controller();
    let manager = harness.client.control.manager.clone().unwrap();
    manager.invoke(1, serde_json::to_string(&call()).unwrap());
    harness.sync();
    let work = harness.server.control_work().pop().unwrap();
    harness.server.control_active(false);
    harness.server.control_active(true);
    harness.sync();
    assert!(work.valid().is_err());
    assert_eq!(failed(&harness.client.control.replies[&1]), Error::Unavailable);
    manager.invoke(2, serde_json::to_string(&call()).unwrap());
    harness.sync();
    assert!(harness.server.control_work().pop().unwrap().valid().is_ok());
}

#[test]
fn an_unfinished_worker_reply_has_a_finite_deadline_and_retires_its_reservation() {
    let mut harness = controller();
    harness.client.control.manager.as_ref().unwrap().invoke(1, serde_json::to_string(&call()).unwrap());
    harness.sync();
    let work = harness.server.control_work().pop().unwrap();
    let remaining = harness.server.control_timeout().unwrap();
    assert!(remaining <= Duration::from_secs(10));
    std::thread::sleep(remaining + Duration::from_millis(20));
    harness.sync();
    assert_eq!(failed(&harness.client.control.replies[&1]), Error::Cancelled);
    assert_eq!(harness.server.control_status().pending, 0);
    assert!(harness.server.control_timeout().is_none());
    assert!(work.valid().is_err());
}
