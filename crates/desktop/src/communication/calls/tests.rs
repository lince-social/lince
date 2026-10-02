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

async fn automatic_world() -> (World, tokio::sync::mpsc::Receiver<ClientMessage>) {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let cell = cell::CellRuntime {
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        speech: None,
        information: None,
    };
    let mut bridge = crate::cell_bridge::connect(cell, crate::wake::WakeSignal::new(|| {}));
    let (outgoing, requests) = tokio::sync::mpsc::channel(8);
    bridge.outgoing = outgoing;
    let mut world = world();
    world.insert_non_send(bridge);
    world.spawn((
        crate::container::BoxRoot,
        crate::workspace::Workspaces::default(),
    ));
    (world, requests)
}

fn automatic_presentation(
    media: nucleus::component::CallMedia,
) -> nucleus::component::Presentation {
    nucleus::component::Presentation {
        slot: "record:record".into(),
        component: nucleus::component::ComponentState::Record {
            record: "record".into(),
            mode: nucleus::component::RecordMode::Call,
            start_call: Some(nucleus::component::CallStart {
                thread: "thread".into(),
                person: "person".into(),
                media,
            }),
        },
    }
}

#[tokio::test]
async fn automatic_component_starts_once_after_context_with_selected_person() {
    for media in [
        nucleus::component::CallMedia::Audio,
        nucleus::component::CallMedia::Video,
    ] {
        let (mut world, mut requests) = automatic_world().await;
        let state = automatic_presentation(media);
        let entity = crate::component_push::present(&mut world, state.clone()).unwrap();
        let ClientMessage::CallContext { id, thread } = requests.try_recv().unwrap() else {
            panic!()
        };
        assert_eq!(thread, "thread");
        assert_eq!(
            crate::component_push::present(&mut world, state.clone()).unwrap(),
            entity
        );
        assert!(requests.try_recv().is_err());
        receive(
            &mut world,
            &ServerMessage::CallContext {
                id,
                context: Context {
                    root: "record".into(),
                    people: vec![("person".into(), "Person".into())],
                    organs: vec![],
                    current_organs: vec![],
                    admitted: vec![],
                    group: None,
                },
            },
        );
        let ClientMessage::Call {
            id,
            thread,
            person,
            operation: Operation::Start,
        } = requests.try_recv().unwrap()
        else {
            panic!()
        };
        assert_eq!(thread, "thread");
        assert_eq!(person.as_deref(), Some("person"));
        let runtime = world.resource::<Runtime>();
        let Pending::Join(captures) = &runtime.pending[&id].1 else {
            panic!()
        };
        assert!(matches!(captures.first(), Some(Capture::Microphone(None))));
        assert_eq!(
            captures.len(),
            if media == nucleus::component::CallMedia::Video {
                2
            } else {
                1
            }
        );
        if media == nucleus::component::CallMedia::Video {
            assert!(matches!(&captures[1], Capture::Camera(id) if id == "0"));
        }
        assert!(runtime.controller.as_ref().unwrap().media.is_none());
        assert!(runtime.controller.as_ref().unwrap().preview.is_none());
        assert_eq!(
            crate::component_push::present(&mut world, state).unwrap(),
            entity
        );
        assert!(requests.try_recv().is_err());
        receive(
            &mut world,
            &ServerMessage::Error {
                id,
                code: None,
                message: "Call admission refused".into(),
            },
        );
        let controller = world.resource::<Runtime>().controller.as_ref().unwrap();
        assert!(controller.media.is_none());
        assert_eq!(
            world.get::<Text>(controller.status).unwrap().0,
            "Call admission refused"
        );
        let root = controller.root;
        assert_eq!(
            crate::component_push::present(&mut world, automatic_presentation(media)).unwrap(),
            entity
        );
        let ClientMessage::CallContext { id, .. } = requests.try_recv().unwrap() else {
            panic!()
        };
        assert_eq!(
            world
                .resource::<Runtime>()
                .controller
                .as_ref()
                .unwrap()
                .root,
            root
        );
        receive(
            &mut world,
            &ServerMessage::CallContext {
                id,
                context: Context {
                    root: "record".into(),
                    people: vec![("person".into(), "Person".into())],
                    organs: vec![],
                    current_organs: vec![],
                    admitted: vec![],
                    group: None,
                },
            },
        );
        assert!(matches!(
            requests.try_recv().unwrap(),
            ClientMessage::Call {
                operation: Operation::Start,
                ..
            }
        ));
    }
}

#[tokio::test]
async fn automatic_call_refuses_missing_person_and_does_not_replace_another_controller() {
    let (mut world, mut requests) = automatic_world().await;
    let entity =
        crate::component_push::present(&mut world, automatic_presentation(Default::default()))
            .unwrap();
    let ClientMessage::CallContext { id, .. } = requests.try_recv().unwrap() else {
        panic!()
    };
    receive(
        &mut world,
        &ServerMessage::CallContext {
            id,
            context: Context {
                root: "record".into(),
                people: vec![],
                organs: vec![],
                current_organs: vec![],
                admitted: vec![],
                group: None,
            },
        },
    );
    assert!(requests.try_recv().is_err());
    let controller = world.resource::<Runtime>().controller.as_ref().unwrap();
    assert!(controller.media.is_none());
    assert!(
        world
            .get::<Text>(controller.status)
            .unwrap()
            .0
            .contains("configured Person")
    );
    let root = controller.root;
    assert!(
        start_automatically(
            &mut world,
            RecordBinding {
                area: entity,
                uid: "other".into(),
                source: crate::protein_area::Source::Local
            },
            &nucleus::component::CallStart {
                thread: "other-thread".into(),
                person: "person".into(),
                media: Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(
        world
            .resource::<Runtime>()
            .controller
            .as_ref()
            .unwrap()
            .root,
        root
    );
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn restoring_an_automatic_component_does_not_start_a_call() {
    let (mut world, mut requests) = automatic_world().await;
    let root = world
        .query_filtered::<Entity, With<crate::container::BoxRoot>>()
        .single(&world)
        .unwrap();
    let state = automatic_presentation(Default::default());
    let entity = crate::component_push::present(&mut world, state.clone()).unwrap();
    let ClientMessage::CallContext { .. } = requests.try_recv().unwrap() else {
        panic!()
    };
    let placement = crate::sand_placement::Placement::capture(&world, entity);
    let area = world
        .get::<crate::area::InfluenceArea>(entity)
        .unwrap()
        .clone();
    let controller = world.resource_mut::<Runtime>().controller.take().unwrap();
    world.despawn(controller.root);
    world.despawn(entity);
    world.resource_mut::<Runtime>().pending.clear();
    let restored = crate::area::spawn_area(&mut world, root, 1, area).unwrap();
    placement.restore(&mut world, restored);
    assert!(world.resource::<Runtime>().controller.is_none());
    assert!(requests.try_recv().is_err());
    assert_eq!(
        crate::component_push::present(&mut world, state).unwrap(),
        restored
    );
    assert!(matches!(
        requests.try_recv().unwrap(),
        ClientMessage::CallContext { .. }
    ));
}

#[tokio::test]
async fn later_firing_reuses_the_component_and_starts_again_after_end() {
    let (mut world, mut requests) = automatic_world().await;
    let state = automatic_presentation(Default::default());
    let entity = crate::component_push::present(&mut world, state.clone()).unwrap();
    let ClientMessage::CallContext { id, .. } = requests.try_recv().unwrap() else {
        panic!()
    };
    world.resource_mut::<Runtime>().pending.remove(&id);
    let mut controller = world.resource_mut::<Runtime>().controller.take().unwrap();
    controller.intent = None;
    controller.snapshot.call = Some("previous-call".into());
    apply(&mut world, &mut controller, &Ui::End).unwrap();
    world.resource_mut::<Runtime>().controller = Some(controller);
    let ClientMessage::Call {
        id,
        operation: Operation::End { call },
        ..
    } = requests.try_recv().unwrap()
    else {
        panic!()
    };
    assert_eq!(call, "previous-call");
    receive(
        &mut world,
        &ServerMessage::Call {
            id,
            device: "device".into(),
            snapshot: Snapshot::default(),
        },
    );
    assert_eq!(
        crate::component_push::present(&mut world, state).unwrap(),
        entity
    );
    let ClientMessage::CallContext { id, .. } = requests.try_recv().unwrap() else {
        panic!()
    };
    receive(
        &mut world,
        &ServerMessage::CallContext {
            id,
            context: Context {
                root: "record".into(),
                people: vec![("person".into(), "Person".into())],
                organs: vec![],
                current_organs: vec![],
                admitted: vec![],
                group: None,
            },
        },
    );
    assert!(matches!(
        requests.try_recv().unwrap(),
        ClientMessage::Call {
            operation: Operation::Start,
            ..
        }
    ));
    assert_eq!(
        world
            .query::<&crate::component_push::Placed>()
            .iter(&world)
            .count(),
        1
    );
}
