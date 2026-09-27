use super::*;

fn world() -> World {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<Assets<Image>>();
    world.init_resource::<crate::theme::Typography>();
    world.init_resource::<Runtime>();
    world
}

#[test]
fn opening_a_thread_or_group_never_starts_capture() {
    let mut world = world();
    let parent = world.spawn_empty().id();
    let binding = RecordBinding {
        area: parent,
        uid: "record".into(),
        source: crate::protein_area::Source::Local,
    };
    populate(&mut world, parent, &binding, "thread");
    assert!(world.resource::<Runtime>().controller.is_none());
    open(
        &mut world,
        binding,
        "thread".into(),
        Snapshot::default(),
        None,
    );
    let controller = world.resource::<Runtime>().controller.as_ref().unwrap();
    assert!(controller.media.is_none());
    assert!(controller.preview.is_none());
    assert_eq!(controller.tracks, Tracks::default());
    assert!(controller.person.is_none());
    let root = controller.root;
    let body = controller.body;
    Ui::Collapse.apply(&mut world, root);
    assert_eq!(world.get::<Node>(body).unwrap().display, Display::None);
    assert!(
        world
            .resource::<Runtime>()
            .controller
            .as_ref()
            .unwrap()
            .media
            .is_none()
    );
    Ui::Collapse.apply(&mut world, root);
    assert_eq!(world.get::<Node>(body).unwrap().display, Display::Flex);
}

#[test]
fn closed_controls_discard_old_admission_responses() {
    let mut world = world();
    let parent = world.spawn_empty().id();
    let binding = RecordBinding {
        area: parent,
        uid: "record".into(),
        source: crate::protein_area::Source::Local,
    };
    open(
        &mut world,
        binding,
        "thread".into(),
        Snapshot::default(),
        None,
    );
    let root = world
        .resource::<Runtime>()
        .controller
        .as_ref()
        .unwrap()
        .root;
    world
        .resource_mut::<Runtime>()
        .pending
        .insert("old-context".into(), (Instant::now(), Pending::Context));
    Ui::Close.apply(&mut world, root);
    assert!(world.resource::<Runtime>().controller.is_none());
    assert!(
        !world
            .resource::<Runtime>()
            .pending
            .contains_key("old-context")
    );
}

#[test]
fn video_uploads_reuse_the_image_asset_and_keep_the_latest_pixels() {
    let mut world = world();
    let entity = world.spawn_empty().id();
    let mut image = Handle::default();
    for value in 0..60 {
        let frame =
            lince_media::video::VideoFrame::new(32, 18, [value, 10, 20, 255].repeat(32 * 18))
                .unwrap();
        crate::communication::texture(&mut world, entity, &mut image, frame);
    }
    let images = world.resource::<Assets<Image>>();
    assert_eq!(images.len(), 1);
    assert_eq!(images.get(&image).unwrap().data.as_ref().unwrap()[0], 59);
    assert_eq!(world.get::<ImageNode>(entity).unwrap().image, image);
}

#[test]
fn old_control_replies_do_not_restore_attendance_or_renew_the_call() {
    let mut world = world();
    let parent = world.spawn_empty().id();
    open(
        &mut world,
        RecordBinding {
            area: parent,
            uid: "record".into(),
            source: crate::protein_area::Source::Local,
        },
        "thread".into(),
        Snapshot::default(),
        None,
    );
    let mut controller = world.resource_mut::<Runtime>().controller.take().unwrap();
    controller.heard = Instant::now() - Duration::from_secs(31);
    let heard = controller.heard;
    let reply = ServerMessage::Call {
        id: "old".into(),
        device: "device".into(),
        snapshot: Snapshot {
            call: Some("old-call".into()),
            participants: vec![engine::calls::Participant {
                identity: Identity {
                    organ: "organ".into(),
                    person: "person".into(),
                    device: "device".into(),
                },
                name: "Departed person".into(),
                organ_name: "Organ".into(),
                tracks: Tracks::default(),
            }],
            ..Default::default()
        },
    };
    for pending in [Pending::Signal, Pending::Tracks(None), Pending::Poll] {
        received(&mut world, &mut controller, pending, &reply).unwrap();
        assert!(controller.snapshot.call.is_none());
        assert!(controller.snapshot.participants.is_empty());
        assert_eq!(controller.heard, heard);
        assert!(controller.media.is_none());
    }
    world.resource_mut::<Runtime>().pending.insert(
        "screen-reservation".into(),
        (Instant::now(), Pending::Tracks(Some(Capture::Screen(None)))),
    );
    assert!(
        apply(
            &mut world,
            &mut controller,
            &Ui::Capture(Capture::StopScreen)
        )
        .is_err()
    );
    assert!(
        !world
            .resource::<Runtime>()
            .pending
            .contains_key("screen-reservation")
    );
}

#[tokio::test]
async fn call_connection_survives_closing_the_record_view() {
    let mut world = world();
    let area = world.spawn_empty().id();
    open(
        &mut world,
        RecordBinding {
            area,
            uid: "record".into(),
            source: crate::protein_area::Source::Local,
        },
        "thread".into(),
        Snapshot::default(),
        None,
    );
    let mut controller = world.resource_mut::<Runtime>().controller.take().unwrap();
    let (outgoing, mut requests) = tokio::sync::mpsc::channel(8);
    let (_responses, incoming) = tokio::sync::mpsc::channel(8);
    let task = tokio::spawn(std::future::pending());
    let task_lifetime = task.abort_handle();
    controller.binding.source = crate::protein_area::Source::Organ("remote".into());
    controller.remote = Some(crate::protein_area::Remote {
        outgoing,
        incoming,
        task,
    });
    world.despawn(area);
    request(
        &mut world,
        &controller,
        Operation::Inspect,
        Pending::InspectController,
    )
    .unwrap();
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::Call { thread, .. } if thread == "thread")
    );
    assert!(!task_lifetime.is_finished());
    drop(controller);
    tokio::task::yield_now().await;
    assert!(task_lifetime.is_finished());
}
