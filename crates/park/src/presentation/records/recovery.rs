use super::tests::{pump, window};
use super::*;

fn runtime(engine: Arc<engine::Engine>) -> cell::CellRuntime {
    cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        commands: Default::default(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        speech: None,
        fiote: None,
    }
}

fn scene(runtime: &cell::CellRuntime) -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Messages<native::cell_bridge::CellMessage>>();
    world.insert_resource(WakeSignal::new(|| {}));
    world.insert_non_send(native::cell_bridge::connect(
        runtime.clone(),
        WakeSignal::new(|| {}),
    ));
    let root = world
        .spawn((
            native::workspace::Workspaces::default(),
            native::canvas::CanvasView::default(),
        ))
        .id();
    (world, root)
}

async fn hold_creation_replies(
    world: &mut World,
    expected: BTreeSet<String>,
) -> Vec<cell::ServerMessage> {
    let mut replies = Vec::new();
    let mut created = BTreeSet::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let response = world
            .non_send_mut::<native::cell_bridge::CellBridge>()
            .incoming
            .try_recv();
        match response {
            Ok(
                message @ cell::ServerMessage::ActionOk {
                    created: Some(_), ..
                },
            ) => {
                let cell::ServerMessage::ActionOk {
                    created: Some(uid), ..
                } = &message
                else {
                    unreachable!()
                };
                assert!(expected.contains(uid));
                created.insert(uid.clone());
                replies.push(message);
                if created == expected {
                    return replies;
                }
            }
            Ok(cell::ServerMessage::Error { message, .. }) => {
                panic!("Window creation failed: {message}")
            }
            Ok(_) => {}
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(2)).await
            }
            Err(error) => panic!("Window creation stopped: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "The actual creation responses did not arrive"
        );
    }
}

async fn owner_head(engine: &engine::Engine, uid: &str, head: &str) {
    engine
        .act(
            engine::actions::Action::EditRecordText {
                target: uid.into(),
                head: Some(head.into()),
                body: None,
            },
            None,
        )
        .await
        .unwrap();
}

async fn record_uids(provider: &cell::CellRuntime) -> BTreeSet<String> {
    let mut session = provider.local_session();
    let replies = session
        .handle(native::cell_bridge::records_subscription())
        .await;
    let rows = replies
        .iter()
        .find_map(|message| match message {
            cell::ServerMessage::Snapshot { id, rows, .. }
                if id == native::cell_bridge::RECORDS =>
            {
                Some(rows)
            }
            _ => None,
        })
        .unwrap();
    assert!(
        rows.len() < 512,
        "The private provider fixture must not be truncated"
    );
    rows.iter()
        .map(|row| row["uid"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn replacing_the_cell_bridge_refreshes_unchanged_live_record_subscriptions() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let runtime = runtime(engine.clone());
    let directory = tempfile::tempdir().unwrap();
    let mut world = World::new();
    world.init_resource::<Messages<native::cell_bridge::CellMessage>>();
    world.insert_resource(WakeSignal::new(|| {}));
    world.insert_non_send(native::cell_bridge::connect(
        runtime.clone(),
        WakeSignal::new(|| {}),
    ));
    let root = world
        .spawn((
            native::workspace::Workspaces::default(),
            native::canvas::CanvasView::default(),
        ))
        .id();
    let (surface, window, identity) = window(&mut world, root);
    let entity = window.entity;
    let windows = HashMap::from([(surface, window)]);
    let identities = HashMap::from([(surface, identity)]);
    let mut records = Records::load(directory.path().join("park-windows.json")).unwrap();
    pump(&mut world, &mut records, &windows, &identities, |records| {
        records
            .profile(identity)
            .and_then(|profile| profile.uid.as_ref())
            .is_some_and(|uid| records.rows.contains_key(uid))
    })
    .await;
    let uid = records.profile(identity).unwrap().uid.clone().unwrap();
    let mut cursor = MessageCursor::<native::cell_bridge::CellMessage>::default();
    let previous = cursor.read(world.resource::<Messages<native::cell_bridge::CellMessage>>())
        .find(|message| matches!(&message.0, cell::ServerMessage::Snapshot { rows, .. } if rows.iter().any(|row| row["uid"] == uid))).unwrap().clone();
    let profile = records.profile(identity).unwrap().clone();
    world.remove_non_send::<native::cell_bridge::CellBridge>();
    engine
        .act(
            engine::actions::Action::EditRecordText {
                target: uid.clone(),
                head: Some("Owner's retained window".into()),
                body: None,
            },
            None,
        )
        .await
        .unwrap();
    world.insert_non_send(native::cell_bridge::connect(
        runtime,
        WakeSignal::new(|| {}),
    ));
    pump(&mut world, &mut records, &windows, &identities, |records| {
        records
            .rows
            .get(&uid)
            .is_some_and(|row| row["head"] == "Owner's retained window")
    })
    .await;
    world.write_message(previous);
    records.tick(&mut world, &windows, &identities);
    assert_eq!(records.rows[&uid]["head"], "Owner's retained window");
    assert!(records.profile(identity).unwrap() == &profile);
    let properties = &world
        .get::<native::area::RecordProperties>(entity)
        .unwrap()
        .0;
    assert_eq!(properties["head"], "Owner's retained window");
    assert_eq!(properties["window_title"], "Private browser page title");
    assert_eq!(
        world
            .get::<native::protein_area::RecordBinding>(entity)
            .unwrap()
            .uid,
        uid
    );
}

#[tokio::test]
async fn lost_creation_acknowledgements_survive_full_engine_release_and_replay_without_new_records()
{
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("cell.sqlite").display()
    );
    let engine = Arc::new(engine::Engine::open(&url).await.unwrap());
    let provider = runtime(engine.clone());
    let (mut world, root) = scene(&provider);
    let (first, first_window, first_identity) = window(&mut world, root);
    let (second, second_window, second_identity) = window(&mut world, root);
    let first_entity = first_window.entity;
    let second_entity = second_window.entity;
    world
        .get_mut::<native::canvas::CanvasItem>(second_entity)
        .unwrap()
        .position = bevy::math::DVec2::new(400.0, 200.0);
    let draft = world
        .spawn(bevy::text::EditableText::new("Unsent owner note"))
        .id();
    let windows = HashMap::from([(first, first_window), (second, second_window)]);
    let identities = HashMap::from([(first, first_identity), (second, second_identity)]);
    let path = directory.path().join("park-windows.json");
    let mut records = Records::load(path.clone()).unwrap();
    records.tick(&mut world, &windows, &identities);
    assert!(
        path.is_file(),
        "Creation intent must reach disk before the engine receives it"
    );
    let first_uid = records
        .profile(first_identity)
        .unwrap()
        .creation
        .clone()
        .unwrap();
    let second_uid = records
        .profile(second_identity)
        .unwrap()
        .creation
        .clone()
        .unwrap();
    assert_ne!(first_uid, second_uid);
    let replies = hold_creation_replies(
        &mut world,
        BTreeSet::from([first_uid.clone(), second_uid.clone()]),
    )
    .await;
    assert_eq!(replies.len(), 2);
    assert!(records.profile(first_identity).unwrap().uid.is_none());
    assert!(records.profile(second_identity).unwrap().uid.is_none());
    assert!(records.pending());
    let launch: crate::launch::Description = serde_json::from_value(
        serde_json::json!({"program":"test-generic","arguments":["literal argument"]}),
    )
    .unwrap();
    records.launch(first_identity, launch).unwrap();
    owner_head(&engine, &first_uid, "Owner's retained window").await;
    owner_head(&engine, &second_uid, "Other retained window").await;
    let known_records = record_uids(&provider).await;
    assert!(known_records.contains(&first_uid) && known_records.contains(&second_uid));
    let first_saved = records.profile(first_identity).unwrap().clone();
    let second_saved = records.profile(second_identity).unwrap().clone();
    let count = world.entities().len();
    let restored = Records::load(path.clone()).unwrap();
    assert!(restored.profile(first_identity).unwrap() == &first_saved);
    assert!(restored.profile(second_identity).unwrap() == &second_saved);
    assert_eq!(world.entities().len(), count);
    drop(restored);
    let old = Arc::downgrade(&engine);
    world.remove_non_send::<native::cell_bridge::CellBridge>();
    drop(provider);
    drop(engine);
    let deadline = Instant::now() + Duration::from_secs(10);
    while old.strong_count() != 0 {
        assert!(
            Instant::now() < deadline,
            "The old provider must actually stop before reopening its database"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let engine = Arc::new(engine::Engine::open(&url).await.unwrap());
    let provider = runtime(engine.clone());
    world.insert_non_send(native::cell_bridge::connect(
        provider.clone(),
        WakeSignal::new(|| {}),
    ));
    pump(&mut world, &mut records, &windows, &identities, |records| {
        records.profile(first_identity).unwrap().uid.as_deref() == Some(first_uid.as_str())
            && records.profile(second_identity).unwrap().uid.as_deref() == Some(second_uid.as_str())
            && records
                .rows
                .get(&first_uid)
                .is_some_and(|row| row["head"] == "Owner's retained window")
            && records
                .rows
                .get(&second_uid)
                .is_some_and(|row| row["head"] == "Other retained window")
    })
    .await;
    assert!(!records.pending());
    assert_eq!(record_uids(&provider).await, known_records);
    for (identity, expected) in [
        (first_identity, first_saved),
        (second_identity, second_saved),
    ] {
        let current = records.profile(identity).unwrap();
        assert_eq!(current.uid.as_ref(), expected.creation.as_ref());
        assert!(current.creation.is_none());
        assert_eq!(current.layout, expected.layout);
        assert_eq!(current.workspace, expected.workspace);
        assert_eq!(current.position, expected.position);
        assert_eq!(current.size, expected.size);
        assert!(current.tokens == expected.tokens);
        assert!(current.launch == expected.launch);
    }
    assert_eq!(
        world
            .get::<native::protein_area::RecordBinding>(first_entity)
            .unwrap()
            .uid,
        first_uid
    );
    assert_eq!(
        world
            .get::<native::protein_area::RecordBinding>(second_entity)
            .unwrap()
            .uid,
        second_uid
    );
    assert_eq!(
        world
            .get::<bevy::text::EditableText>(draft)
            .unwrap()
            .value()
            .to_string(),
        "Unsent owner note"
    );
    assert_eq!(world.entities().len(), count);
    records.flush().unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(!text.contains("Private browser page title"));
    assert!(!text.contains("untrusted.example.client"));
}

#[tokio::test]
async fn a_failed_intent_write_prevents_creation_until_the_same_uid_can_be_saved() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let provider = runtime(engine.clone());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park-windows.json");
    let (mut world, root) = scene(&provider);
    let (surface, window, identity) = window(&mut world, root);
    let windows = HashMap::from([(surface, window)]);
    let identities = HashMap::from([(surface, identity)]);
    let mut records = Records::load(path.clone()).unwrap();
    std::fs::create_dir(&path).unwrap();
    records.tick(&mut world, &windows, &identities);
    let uid = records.profile(identity).unwrap().creation.clone().unwrap();
    assert!(records.requests.is_empty());
    assert!(records.pending());
    assert!(matches!(records.creation_state(identity.0, &uid), Some(native::full_record::window::CreationState::Failed(message)) if message.contains("profile")));
    let query = native::full_record::config(&uid, native::protein_area::Source::Local)
        .draft
        .compile()
        .unwrap();
    let mut session = provider.local_session();
    let response = session
        .handle(cell::ClientMessage::Subscribe {
            id: "verify-unsent-creation".into(),
            protein: query,
        })
        .await;
    assert!(response.iter().any(
        |message| matches!(message, cell::ServerMessage::Snapshot { rows, .. } if rows.is_empty())
    ));
    std::fs::remove_dir(&path).unwrap();
    records.retry(identity);
    pump(&mut world, &mut records, &windows, &identities, |records| {
        records.profile(identity).unwrap().uid.as_deref() == Some(uid.as_str())
    })
    .await;
    assert!(path.is_file());
    assert!(!records.pending());
    assert!(records.creation_state(identity.0, &uid).is_none());
}

#[tokio::test]
async fn an_actual_creation_reply_after_close_cannot_bind_a_replacement_surface() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let provider = runtime(engine);
    let directory = tempfile::tempdir().unwrap();
    let (mut world, root) = scene(&provider);
    let (surface, first_window, first_identity) = window(&mut world, root);
    let first_entity = first_window.entity;
    let mut windows = HashMap::from([(surface, first_window)]);
    let mut identities = HashMap::from([(surface, first_identity)]);
    let mut records = Records::load(directory.path().join("park-windows.json")).unwrap();
    records.tick(&mut world, &windows, &identities);
    let uid = records
        .profile(first_identity)
        .unwrap()
        .creation
        .clone()
        .unwrap();
    let reply = hold_creation_replies(&mut world, BTreeSet::from([uid.clone()]))
        .await
        .pop()
        .unwrap();
    records.capture(&world, &windows[&surface], first_identity);
    windows.remove(&surface);
    identities.remove(&surface);
    world.despawn(first_entity);
    let (_, replacement, replacement_identity) = window(&mut world, root);
    let entity = replacement.entity;
    assert_ne!(first_identity, replacement_identity);
    windows.insert(surface, replacement);
    identities.insert(surface, replacement_identity);
    world.write_message(native::cell_bridge::CellMessage(reply.clone()));
    pump(&mut world, &mut records, &windows, &identities, |records| {
        records
            .profile(replacement_identity)
            .and_then(|profile| profile.uid.as_ref())
            .is_some()
    })
    .await;
    let replacement_uid = records
        .profile(replacement_identity)
        .unwrap()
        .uid
        .clone()
        .unwrap();
    assert_ne!(replacement_uid, uid);
    assert_eq!(
        records.profile(first_identity).unwrap().uid.as_deref(),
        Some(uid.as_str())
    );
    world.write_message(native::cell_bridge::CellMessage(reply));
    records.tick(&mut world, &windows, &identities);
    assert_eq!(
        world
            .get::<native::protein_area::RecordBinding>(entity)
            .unwrap()
            .uid,
        replacement_uid
    );
    assert_eq!(
        records.profile(first_identity).unwrap().uid.as_deref(),
        Some(uid.as_str())
    );
    assert_eq!(
        records
            .profile(replacement_identity)
            .unwrap()
            .uid
            .as_deref(),
        Some(replacement_uid.as_str())
    );
}

#[tokio::test]
async fn a_burst_of_windows_saves_every_intent_but_bounds_the_creation_queue() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let provider = runtime(engine);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("park-windows.json");
    let (mut world, root) = scene(&provider);
    let mut windows = HashMap::new();
    let mut identities = HashMap::new();
    for _ in 0..128 {
        let (surface, window, identity) = window(&mut world, root);
        windows.insert(surface, window);
        identities.insert(surface, identity);
    }
    let mut records = Records::load(path.clone()).unwrap();
    records.tick(&mut world, &windows, &identities);
    assert_eq!(records.requests.len(), 64);
    assert!(records.pending());
    let saved = Records::load(path).unwrap();
    let uids: BTreeSet<_> = identities
        .values()
        .map(|identity| saved.profile(*identity).unwrap().creation.clone().unwrap())
        .collect();
    assert_eq!(uids.len(), 128);
    assert!(
        identities
            .values()
            .all(|identity| saved.profile(*identity).unwrap().uid.is_none())
    );
    world.remove_non_send::<native::cell_bridge::CellBridge>();
}

#[tokio::test]
async fn mismatched_actual_creation_replies_cannot_bind_windows_and_old_attempts_cannot_satisfy_retry()
 {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let provider = runtime(engine);
    let directory = tempfile::tempdir().unwrap();
    let (mut world, root) = scene(&provider);
    let (first, first_window, first_identity) = window(&mut world, root);
    let (second, second_window, second_identity) = window(&mut world, root);
    let first_entity = first_window.entity;
    let second_entity = second_window.entity;
    let windows = HashMap::from([(first, first_window), (second, second_window)]);
    let identities = HashMap::from([(first, first_identity), (second, second_identity)]);
    let mut records = Records::load(directory.path().join("park-windows.json")).unwrap();
    records.tick(&mut world, &windows, &identities);
    let first_uid = records
        .profile(first_identity)
        .unwrap()
        .creation
        .clone()
        .unwrap();
    let second_uid = records
        .profile(second_identity)
        .unwrap()
        .creation
        .clone()
        .unwrap();
    let replies = hold_creation_replies(
        &mut world,
        BTreeSet::from([first_uid.clone(), second_uid.clone()]),
    )
    .await;
    let known_records = record_uids(&provider).await;
    let old_ids: Vec<_> = replies
        .iter()
        .map(|message| match message {
            cell::ServerMessage::ActionOk { id, .. } => id.clone(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(replies.len(), 2);
    for (index, mut message) in replies.iter().cloned().enumerate() {
        let cell::ServerMessage::ActionOk { id, .. } = &mut message else {
            unreachable!()
        };
        *id = old_ids[1 - index].clone();
        world.write_message(native::cell_bridge::CellMessage(message));
    }
    records.tick(&mut world, &windows, &identities);
    for (identity, entity) in [
        (first_identity, first_entity),
        (second_identity, second_entity),
    ] {
        assert!(records.profile(identity).unwrap().uid.is_none());
        assert!(records.errors[&identity].contains("unexpected identity"));
        assert!(
            world
                .get::<native::protein_area::RecordBinding>(entity)
                .is_none()
        );
        records.retry(identity);
    }
    records.tick(&mut world, &windows, &identities);
    assert!(records.requests.keys().all(|id| !old_ids.contains(id)));
    world.write_message_batch(replies.into_iter().map(native::cell_bridge::CellMessage));
    records.tick(&mut world, &windows, &identities);
    assert!(records.profile(first_identity).unwrap().uid.is_none());
    assert!(records.profile(second_identity).unwrap().uid.is_none());
    pump(&mut world, &mut records, &windows, &identities, |records| {
        records.profile(first_identity).unwrap().uid.as_deref() == Some(first_uid.as_str())
            && records.profile(second_identity).unwrap().uid.as_deref() == Some(second_uid.as_str())
    })
    .await;
    assert_eq!(
        world
            .get::<native::protein_area::RecordBinding>(first_entity)
            .unwrap()
            .uid,
        first_uid
    );
    assert_eq!(
        world
            .get::<native::protein_area::RecordBinding>(second_entity)
            .unwrap()
            .uid,
        second_uid
    );
    assert_eq!(record_uids(&provider).await, known_records);
}
