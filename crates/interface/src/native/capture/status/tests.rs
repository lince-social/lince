use super::*;

#[test]
fn stop_targets_only_the_captured_live_session_and_uses_the_existing_host_queue() {
    let mut world = World::new();
    let session = Session { id: 37, connection: 12, target: Target::Output(crate::host::OutputId(2)), label: "Display B".into(), cursor: false, paused: false };
    world.insert_resource(Status { available: true, paused: false, sessions: vec![session] });
    let (sender, receiver) = std::sync::mpsc::sync_channel(2);
    world.insert_resource(crate::native::layout::desktop::HostRequests(sender));
    let button = world.spawn_empty().id();
    Stop { id: 37, connection: 11 }.apply(&mut world, button);
    assert!(receiver.try_recv().is_err());
    Stop { id: 37, connection: 12 }.apply(&mut world, button);
    assert_eq!(receiver.try_recv().unwrap(), crate::host::HostRequest::StopCapture(37));
    world.resource_mut::<Status>().paused = true;
    Stop { id: 37, connection: 12 }.apply(&mut world, button);
    assert!(receiver.try_recv().is_err());
    world.resource_mut::<Status>().paused = false;
    world.resource_mut::<Status>().sessions.clear();
    Stop { id: 37, connection: 12 }.apply(&mut world, button);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn status_does_not_claim_recording_and_controls_track_session_teardown() {
    let mut world = World::new();
    world.insert_resource(crate::native::theme::Typography(Handle::default()));
    world.insert_resource(Status::default());
    let root = world.spawn((Workspaces::default(), Node::default(), crate::native::canvas::CanvasView::default())).id();
    panel(&mut world, root);
    refresh(&mut world);
    assert_eq!(world.query::<&Indicator>().iter(&world).count(), 0);
    world.insert_resource(Status { available: true, paused: false, sessions: vec![Session { id: 37, connection: 12, target: Target::Output(crate::host::OutputId(2)), label: "Display B".into(), cursor: false, paused: false }] });
    refresh(&mut world);
    let label = world.query::<&Indicator>().single(&world).unwrap().label;
    let text = &world.get::<Text>(label).unwrap().0;
    assert!(text.contains("Sharing"));
    assert!(!text.contains("Recording"));
    let page = world.query::<&Page>().single(&world).unwrap();
    assert_eq!(page.rows.len(), 1);
    let summary = &world.get::<Text>(page.summary).unwrap().0;
    assert!(summary.contains("not whether another program is recording"));
    world.resource_mut::<Status>().sessions.clear();
    refresh(&mut world);
    assert_eq!(world.query::<&Indicator>().iter(&world).count(), 0);
    assert!(world.query::<&Page>().single(&world).unwrap().rows.is_empty());
}
