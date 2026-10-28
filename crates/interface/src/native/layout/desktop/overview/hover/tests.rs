use super::*;
use std::sync::mpsc::{Receiver, sync_channel};

fn geometry(world: &mut World, entity: Entity, bounds: Rect) {
    world.entity_mut(entity).insert((
        ComputedNode {
            size: bounds.size(),
            content_size: bounds.size(),
            inverse_scale_factor: 1.0,
            ..default()
        },
        UiGlobalTransform::from(bevy::math::Affine2::from_translation(bounds.center())),
        InheritedVisibility::VISIBLE,
    ));
}

fn fixture(capacity: usize) -> (App, Entity, Entity, Receiver<HostRequest>) {
    let (mut app, root, _, _) = super::super::tests::fixture();
    let mut second = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
    second.id = 2;
    second.name = "File destination".into();
    second.desktop = crate::layout::desktop::Workspace::new(WorkspaceLayout::Free);
    app.world_mut()
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(second);
    let window = app.world_mut().spawn(Window::default()).id();
    let (sender, receiver) = sync_channel(capacity);
    app.world_mut().insert_resource(HostRequests(sender));
    toggle(app.world_mut(), root);
    let session = app.world().resource::<Overview>().0.as_ref().unwrap();
    let parts = (
        session.overlay,
        session.list,
        session
            .cards
            .iter()
            .map(|card| (card.entity, card.preview))
            .collect::<Vec<_>>(),
    );
    geometry(app.world_mut(), parts.0, Rect::new(0.0, 0.0, 1000.0, 700.0));
    geometry(
        app.world_mut(),
        parts.1,
        Rect::new(20.0, 100.0, 980.0, 620.0),
    );
    for ((card, preview), bounds) in parts.2.into_iter().zip([
        Rect::new(20.0, 100.0, 980.0, 300.0),
        Rect::new(20.0, 320.0, 980.0, 520.0),
    ]) {
        geometry(app.world_mut(), card, bounds);
        geometry(app.world_mut(), preview, bounds);
    }
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(400.0, 400.0)));
    file_drag(
        app.world_mut(),
        window,
        Some(FileDragContext {
            owner: root.to_bits(),
            output: None,
            token: 1,
        }),
    );
    (app, root, window, receiver)
}

fn tick(world: &mut World, now: Instant) {
    let mut overview = world.remove_resource::<Overview>().unwrap();
    let activation = update(world, overview.0.as_mut().unwrap(), now);
    world.insert_resource(overview);
    if let Some(request) = activation
        && acknowledge_hover(world, request, true)
    {
        activate(
            world,
            Entity::from_bits(request.drag.owner),
            request.workspace,
            None,
        );
    }
}

fn request(receiver: &Receiver<HostRequest>) -> OverviewHoverRequest {
    let HostRequest::OverviewHover(request) = receiver.try_recv().unwrap() else {
        panic!("Missing hover request");
    };
    request
}

#[test]
fn real_context_dwell_fires_once_and_an_acknowledgement_does_not_activate_by_itself() {
    let (mut app, root, _, receiver) = fixture(8);
    let now = Instant::now();
    tick(app.world_mut(), now);
    tick(app.world_mut(), now + Duration::from_millis(399));
    assert!(receiver.try_recv().is_err());
    tick(app.world_mut(), now + Duration::from_millis(400));
    let request = request(&receiver);
    assert_eq!(request.workspace, 2);
    tick(app.world_mut(), now + Duration::from_millis(900));
    assert!(receiver.try_recv().is_err());
    assert!(acknowledge_hover(app.world_mut(), request, true));
    assert!(!acknowledge_hover(app.world_mut(), request, true));
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
}

#[test]
fn leaving_and_reentering_the_same_target_retires_the_old_sequence_and_restarts_dwell() {
    let (mut app, _, window, receiver) = fixture(8);
    let now = Instant::now();
    tick(app.world_mut(), now);
    tick(app.world_mut(), now + Duration::from_millis(400));
    let old = request(&receiver);
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(0.0, 0.0)));
    tick(app.world_mut(), now + Duration::from_millis(450));
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(400.0, 400.0)));
    tick(app.world_mut(), now + Duration::from_millis(500));
    assert!(!acknowledge_hover(app.world_mut(), old, true));
    tick(app.world_mut(), now + Duration::from_millis(899));
    assert!(receiver.try_recv().is_err());
    tick(app.world_mut(), now + Duration::from_millis(900));
    let new = request(&receiver);
    assert_ne!(old.sequence, new.sequence);
    assert!(!acknowledge_hover(app.world_mut(), old, true));
    assert!(acknowledge_hover(app.world_mut(), new, true));
}

#[test]
fn source_mapping_modal_and_destination_changes_reject_a_queued_activation() {
    for change in 0..5 {
        let (mut app, root, window, receiver) = fixture(8);
        let now = Instant::now();
        tick(app.world_mut(), now);
        tick(app.world_mut(), now + Duration::from_millis(400));
        let request = request(&receiver);
        match change {
            0 => {
                file_drag(app.world_mut(), window, None);
                file_drag(app.world_mut(), window, Some(request.drag));
            }
            1 => {
                app.world_mut()
                    .get_mut::<ComputedNode>(root)
                    .unwrap()
                    .size
                    .x += 1.0
            }
            2 => app
                .world_mut()
                .get_mut::<Workspaces>(root)
                .unwrap()
                .entries
                .retain(|entry| entry.id != 2),
            3 => {
                close(app.world_mut(), true);
                toggle(app.world_mut(), root);
            }
            _ => app.world_mut().get_mut::<Window>(window).unwrap().focused = false,
        }
        assert!(!acknowledge_hover(app.world_mut(), request, true));
        assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
    }
}

#[test]
fn backpressure_preserves_one_candidate_and_disconnect_cancels_instead_of_rearming() {
    let (mut app, _, window, receiver) = fixture(1);
    app.world()
        .resource::<HostRequests>()
        .0
        .try_send(HostRequest::Exit)
        .unwrap();
    let now = Instant::now();
    tick(app.world_mut(), now);
    for elapsed in [400, 416, 432] {
        tick(app.world_mut(), now + Duration::from_millis(elapsed));
    }
    assert_eq!(receiver.try_recv(), Ok(HostRequest::Exit));
    tick(app.world_mut(), now + Duration::from_millis(448));
    let first = request(&receiver);
    assert_eq!(first.sequence, 1);
    assert!(receiver.try_recv().is_err());
    drop(receiver);
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(400.0, 200.0)));
    tick(app.world_mut(), now + Duration::from_millis(500));
    tick(app.world_mut(), now + Duration::from_millis(900));
    assert!(
        app.world()
            .resource::<Overview>()
            .0
            .as_ref()
            .unwrap()
            .hover
            .is_none()
    );
}

#[test]
fn sandbox_multi_file_hover_has_one_lifetime_and_another_window_cannot_end_it() {
    use bevy::window::FileDragAndDrop;
    let mut world = World::new();
    let first = world.spawn(Window::default()).id();
    let other = world.spawn(Window::default()).id();
    file_event(
        &mut world,
        &FileDragAndDrop::HoveredFile {
            window: first,
            path_buf: "a.glb".into(),
        },
    );
    let source = world.resource::<Incoming>().source;
    file_event(
        &mut world,
        &FileDragAndDrop::HoveredFile {
            window: first,
            path_buf: "b.png".into(),
        },
    );
    assert!(world.resource::<Incoming>().source == source);
    file_event(
        &mut world,
        &FileDragAndDrop::HoveredFileCanceled { window: other },
    );
    assert!(world.resource::<Incoming>().source == source);
    file_event(
        &mut world,
        &FileDragAndDrop::DroppedFile {
            window: first,
            path_buf: "a.glb".into(),
        },
    );
    assert!(world.resource::<Incoming>().source.is_none());
    file_event(
        &mut world,
        &FileDragAndDrop::HoveredFile {
            window: first,
            path_buf: "a.glb".into(),
        },
    );
    assert!(world.resource::<Incoming>().source != source);
}

#[test]
fn ordinary_pointer_motion_and_a_wrong_owner_or_output_cannot_arm_file_hover() {
    let (mut app, root, window, receiver) = fixture(8);
    let now = Instant::now();
    for context in [
        None,
        Some(FileDragContext {
            owner: root.to_bits().wrapping_add(1),
            output: None,
            token: 2,
        }),
        Some(FileDragContext {
            owner: root.to_bits(),
            output: Some(OutputId(9)),
            token: 3,
        }),
    ] {
        file_drag(app.world_mut(), window, context);
        tick(app.world_mut(), now);
        tick(app.world_mut(), now + Duration::from_secs(2));
        assert!(receiver.try_recv().is_err());
        assert!(
            app.world()
                .resource::<Overview>()
                .0
                .as_ref()
                .unwrap()
                .hover
                .is_none()
        );
    }
}

#[test]
fn host_hover_never_self_authorizes_when_its_response_route_is_missing() {
    let (mut app, root, _, _receiver) = fixture(1);
    app.world_mut().remove_resource::<HostRequests>();
    let now = Instant::now();
    tick(app.world_mut(), now);
    tick(app.world_mut(), now + Duration::from_millis(400));
    assert!(
        app.world()
            .resource::<Overview>()
            .0
            .as_ref()
            .unwrap()
            .hover
            .is_none()
    );
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
}

#[test]
fn sandbox_hover_uses_its_camera_window_even_without_keyboard_focus() {
    use bevy::window::FileDragAndDrop;
    let (mut app, root, window, _) = fixture(8);
    app.world_mut().remove_resource::<HostRequests>();
    file_drag(app.world_mut(), window, None);
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    let camera = app
        .world_mut()
        .spawn((
            Camera::default(),
            bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
        ))
        .id();
    app.world_mut()
        .entity_mut(root)
        .insert(bevy::ui::UiTargetCamera(camera));
    let other = app.world_mut().spawn(Window::default()).id();
    app.world_mut()
        .get_mut::<Window>(other)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(400.0, 400.0)));
    file_event(
        app.world_mut(),
        &FileDragAndDrop::HoveredFile {
            window: other,
            path_buf: "wrong.txt".into(),
        },
    );
    let now = Instant::now();
    tick(app.world_mut(), now);
    tick(app.world_mut(), now + Duration::from_millis(400));
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
    file_event(
        app.world_mut(),
        &FileDragAndDrop::HoveredFile {
            window,
            path_buf: "right.txt".into(),
        },
    );
    tick(app.world_mut(), now + Duration::from_millis(500));
    tick(app.world_mut(), now + Duration::from_millis(899));
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
    tick(app.world_mut(), now + Duration::from_millis(900));
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 2);
    assert!(!active(app.world()));
    assert!(drop_allowed(app.world_mut(), window));
}

#[test]
fn a_delayed_sandbox_switch_blocks_the_entire_drop_batch_until_the_intended_workspace_is_active() {
    use bevy::window::FileDragAndDrop;
    let (mut app, root, window, _) = fixture(8);
    file_drag(app.world_mut(), window, None);
    file_event(
        app.world_mut(),
        &FileDragAndDrop::HoveredFile {
            window,
            path_buf: "one.txt".into(),
        },
    );
    app.world_mut().resource_mut::<Incoming>().drop = Some(DropGuard {
        window,
        root,
        workspace: 2,
        reported: false,
    });
    assert!(!drop_allowed(app.world_mut(), window));
    file_event(
        app.world_mut(),
        &FileDragAndDrop::DroppedFile {
            window,
            path_buf: "one.txt".into(),
        },
    );
    file_event(
        app.world_mut(),
        &FileDragAndDrop::HoveredFileCanceled { window },
    );
    assert!(!drop_allowed(app.world_mut(), window));
    file_event(
        app.world_mut(),
        &FileDragAndDrop::DroppedFile {
            window,
            path_buf: "two.txt".into(),
        },
    );
    assert!(!drop_allowed(app.world_mut(), window));
    app.world_mut().get_mut::<Workspaces>(root).unwrap().active = 2;
    assert!(drop_allowed(app.world_mut(), window));
    app.world_mut().get_mut::<Workspaces>(root).unwrap().active = 1;
    file_event(
        app.world_mut(),
        &FileDragAndDrop::HoveredFile {
            window,
            path_buf: "new.txt".into(),
        },
    );
    assert!(drop_allowed(app.world_mut(), window));
}

#[test]
fn replacing_targets_and_cancelling_aborts_every_superseded_timer_without_a_late_wake() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let (mut app, _, window, receiver) = fixture(8);
    let wakes = Arc::new(AtomicUsize::new(0));
    let observed = wakes.clone();
    app.world_mut().insert_resource(WakeSignal::new(move || {
        observed.fetch_add(1, Ordering::AcqRel);
    }));
    let mut timers = Vec::new();
    for y in [400.0, 200.0, 400.0, 200.0, 400.0] {
        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .set_cursor_position(Some(Vec2::new(400.0, y)));
        tick(app.world_mut(), Instant::now());
        let timer = app
            .world()
            .resource::<Overview>()
            .0
            .as_ref()
            .unwrap()
            .hover
            .as_ref()
            .unwrap()
            .timer
            .as_ref()
            .unwrap();
        timers.push(timer.task.clone());
        runtime.block_on(async {
            tokio::time::sleep(Duration::from_millis(5)).await;
        });
        assert!(!timers.last().unwrap().is_finished());
        assert!(
            timers[..timers.len() - 1]
                .iter()
                .all(tokio::task::AbortHandle::is_finished)
        );
    }
    file_drag(app.world_mut(), window, None);
    runtime.block_on(async {
        tokio::time::sleep(Duration::from_millis(450)).await;
    });
    assert!(timers.iter().all(tokio::task::AbortHandle::is_finished));
    assert_eq!(wakes.load(Ordering::Acquire), 0);
    assert!(receiver.try_recv().is_err());
}
