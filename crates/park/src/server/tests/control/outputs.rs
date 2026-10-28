use super::*;
use crate::{launch::Capability, server::output_control::OutputMode};

fn snapshot(harness: &mut Harness, id: u32) -> (u64, Vec<Entry>) {
    harness.client.control.manager.as_ref().unwrap().snapshot(id, "0".into(), 0);
    harness.sync();
    let Response::Snapshot { revision, next, entries } = harness.client.control.replies.remove(&id).unwrap() else { panic!("The output observer must receive a snapshot"); };
    assert_eq!(next, None);
    (revision, entries)
}

#[test]
fn management_observation_tracks_real_mode_refresh_disable_reenable_hotplug_and_stale_output_pages() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![Capability::WindowList, Capability::DesktopControl, Capability::OutputControl]));
    let monitors = [
        Monitor { id: OutputId(1), name: "Park-1".into(), origin: [0, 0], size: [320, 480] },
        Monitor { id: OutputId(2), name: "Park-2".into(), origin: [320, 0], size: [320, 480] },
    ];
    harness.server.configure_outputs(&monitors).unwrap();
    harness.server.registered_output_catalog().unwrap();
    let mut catalog = harness.server.state.output_control.snapshot_heads().to_vec();
    catalog[0].modes.push(OutputMode { size: [320, 480], refresh_mhz: 120_000, preferred: false });
    harness.server.output_catalog(catalog).unwrap();
    harness.sync();
    let manager = harness.client.control.manager.as_ref().unwrap().clone();
    manager.subscribe(1);
    harness.sync();
    let (before, entries) = snapshot(&mut harness, 1);
    assert_eq!(entries.iter().filter(|entry| matches!(entry, Entry::Output { .. })).count(), 2);
    assert!(entries.iter().any(|entry| matches!(entry, Entry::Output { id: 1, modes, .. } if modes.iter().any(|mode| mode.refresh_mhz == 120_000))));
    let output_manager = harness.client.output_control.manager.as_ref().unwrap().clone();
    let heads = harness.client.output_control.heads.clone();
    let high = harness.client.output_control.modes.iter().rev().find(|(head, _)| head == &heads[0]).unwrap().1.clone();
    let configuration = output_manager.create_configuration(harness.client.output_control.serial, &harness.queue.handle(), ());
    let first = configuration.enable_head(&heads[0], &harness.queue.handle(), ());
    first.set_mode(&high);
    configuration.enable_head(&heads[1], &harness.queue.handle(), ());
    configuration.apply();
    harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, true);
    harness.sync();
    assert_eq!(harness.client.output_control.outcomes.last(), Some(&"succeeded"));
    let (changed, entries) = snapshot(&mut harness, 2);
    assert!(changed > before);
    assert!(entries.iter().any(|entry| matches!(entry, Entry::Output { id: 1, enabled: true, refresh_mhz: Some(120_000), .. })));
    manager.snapshot(3, before.to_string(), 1);
    harness.sync();
    assert_eq!(failed(&harness.client.control.replies[&3]), Error::StaleTarget);
    configuration.destroy();
    let configuration = output_manager.create_configuration(harness.client.output_control.serial, &harness.queue.handle(), ());
    configuration.enable_head(&heads[0], &harness.queue.handle(), ());
    configuration.disable_head(&heads[1]);
    configuration.apply();
    harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, true);
    harness.sync();
    assert!(harness.server.output(OutputId(2)).is_none());
    let (disabled, entries) = snapshot(&mut harness, 4);
    assert!(disabled > changed);
    assert!(entries.iter().any(|entry| matches!(entry, Entry::Output { id: 2, enabled: false, refresh_mhz: None, modes, .. } if !modes.is_empty())));
    configuration.destroy();
    let configuration = output_manager.create_configuration(harness.client.output_control.serial, &harness.queue.handle(), ());
    for head in &heads { configuration.enable_head(head, &harness.queue.handle(), ()); }
    configuration.apply();
    harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, true);
    harness.sync();
    assert!(harness.server.output(OutputId(2)).is_some());
    let (enabled, entries) = snapshot(&mut harness, 5);
    assert!(enabled > disabled);
    assert!(entries.iter().any(|entry| matches!(entry, Entry::Output { id: 2, enabled: true, .. })));
    configuration.destroy();
    harness.server.configure_outputs(&monitors[..1]).unwrap();
    harness.server.registered_output_catalog().unwrap();
    harness.sync();
    let (removed, entries) = snapshot(&mut harness, 6);
    assert!(removed > enabled);
    assert!(!entries.iter().any(|entry| matches!(entry, Entry::Output { id: 2, .. })));
    harness.server.configure_outputs(&monitors).unwrap();
    harness.server.registered_output_catalog().unwrap();
    harness.sync();
    let (returned, entries) = snapshot(&mut harness, 7);
    assert!(returned > removed);
    assert!(entries.iter().any(|entry| matches!(entry, Entry::Output { id: 2, name, enabled: true, .. } if name == "Park-2")));
    assert!(harness.client.control.updates.iter().any(|(_, scope)| scope & lince_interface::host::control::CHANGE_OUTPUT != 0));
    assert_eq!(harness.server.control_status().pending, 0);
    assert!(harness.server.control_work().is_empty());
}
