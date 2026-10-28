use super::*;
use bevy::prelude::{IntoScheduleConfigs, MessageReader, Update};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const DRAFT: &str = "unsaved-local-assertion";

type Replies = Arc<Mutex<BTreeMap<String, Vec<String>>>>;

fn observe(app: &mut Toolkit) -> Replies {
    let replies = Replies::default();
    let observed = replies.clone();
    app.inspect(move |canvas| {
        canvas
            .world_mut()
            .resource_mut::<bevy::ecs::schedule::Schedules>()
            .get_mut(Update)
            .unwrap()
            .add_systems(
                (move |mut messages: MessageReader<native::cell_bridge::CellMessage>| {
                    for message in messages.read() {
                        if let cell::ServerMessage::Snapshot { id, rows } = &message.0
                            && id.starts_with("protein-area-")
                        {
                            let mut replies = observed.lock().unwrap();
                            assert!(replies.len() < 128 || replies.contains_key(id));
                            replies.insert(
                                id.clone(),
                                rows.iter()
                                    .filter_map(|row| row["uid"].as_str().map(str::to_string))
                                    .collect(),
                            );
                        }
                    }
                })
                .after(native::cell_bridge::ReceiveCell),
            );
    });
    replies
}

fn query(
    runtime: &tokio::runtime::Runtime,
    provider: &native::app::CellHandle,
    uid: &str,
) -> Vec<cell::ServerMessage> {
    runtime.block_on(
        provider
            .0
            .local_session()
            .handle(cell::ClientMessage::Subscribe {
                id: "verify-unavailable-record".into(),
                protein: native::full_record::config(uid, Source::Local)
                    .query()
                    .unwrap(),
            }),
    )
}

fn status(canvas: &mut crate::presentation::Presentation, uid: &str) -> Option<String> {
    let world = canvas.world_mut();
    world
        .query::<(Entity, &Name, &Text, &ChildOf)>()
        .iter(world)
        .find(|(entity, name, _, parent)| {
            name.as_str() == "record.availability.status"
                && belongs(world, *entity, uid)
                && world
                    .get::<bevy::prelude::Node>(parent.parent())
                    .is_some_and(|node| node.display != bevy::prelude::Display::None)
        })
        .map(|(_, _, text, _)| text.0.clone())
}

fn exercise(spatial: bool) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut app = Toolkit::new(&runtime, directory.path(), spatial);
    app.wait("unavailable-map", |app| {
        app.window("Park toolkit parent").is_some() && !app.state().is_null()
    });
    let main = app.window("Park toolkit parent").unwrap().id;
    app.fixture
        .desktop
        .worker
        .input(Input::Activate(main))
        .unwrap();
    app.tick("unavailable-parent");
    app.configured(main);
    if !spatial {
        app.chord(&[(125, true), (20, true), (20, false), (125, false)]);
        app.chord(&[(125, true), (14, true), (14, false), (125, false)]);
        app.configured(main);
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    let uid = loop {
        if let Some(uid) = app.application_record(main) {
            break uid;
        }
        assert!(Instant::now() < deadline);
    };
    let other = app.application_record(app.fixture.clients[0].1).unwrap();
    for (uid, body) in [(&uid, BODY), (&other, OTHER)] {
        runtime
            .block_on(app.fixture.cell.0.engine.act(
                engine::actions::Action::EditRecordText {
                    target: uid.clone(),
                    head: None,
                    body: Some(body.into()),
                },
                None,
            ))
            .unwrap();
    }
    let replies = observe(&mut app);
    app.chord(&[(125, true), (25, true), (25, false), (125, false)]);
    app.wait("unavailable-operation", |app| {
        app.fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .editor
            .is_some()
    });
    app.text("/window/properties");
    app.type_key(28);
    let deadline = Instant::now() + Duration::from_secs(15);
    let initial = loop {
        let state = app.properties_ready(&uid);
        if state
            .assertions
            .as_ref()
            .is_some_and(|field| field.point.is_some())
        {
            break state;
        }
        assert!(Instant::now() < deadline);
        app.tick("unavailable-presented");
    };
    let field = initial.assertions.as_ref().unwrap();
    app.click_property(field);
    app.text(DRAFT);
    let edited = app.properties_ready(&uid);
    assert_eq!(edited.assertions.as_ref().unwrap().entity, field.entity);
    assert_eq!(edited.assertions.as_ref().unwrap().value, DRAFT);
    let subscription = replies
        .lock()
        .unwrap()
        .iter()
        .find(|(_, rows)| rows.as_slice() == [uid.clone()])
        .map(|(id, _)| id.clone())
        .expect("The original inspector never received its actual Record snapshot");
    runtime
        .block_on(app.fixture.cell.0.engine.act(
            engine::actions::Action::DeleteRecord {
                target: uid.clone(),
            },
            None,
        ))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let requested = uid.clone();
        let message = app.inspect(move |canvas| status(canvas, &requested));
        if message.is_some_and(|message| message.contains("Your draft is kept")) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The deleted Record never produced visible availability feedback"
        );
        app.tick("unavailable-wait");
    }
    let missing = app.properties_ready(&uid);
    assert_eq!(missing.area, initial.area);
    assert_eq!(missing.workspace, initial.workspace);
    assert!(missing.live);
    assert_eq!(missing.assertions.as_ref().unwrap().entity, field.entity);
    assert_eq!(missing.assertions.as_ref().unwrap().value, DRAFT);
    assert!(missing.assertions.as_ref().unwrap().point.is_none());
    app.tick("unavailable-visible");
    app.wait("unavailable-editor-release", |app| {
        app.fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .editor
            .is_none()
    });
    let before = replies.lock().unwrap().clone();
    app.property_button(&uid, "Refresh Record");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let requested = uid.clone();
        let message = app.inspect(move |canvas| status(canvas, &requested));
        let completed = {
            let replies = replies.lock().unwrap();
            let fresh: Vec<_> = replies
                .iter()
                .filter(|(id, _)| !before.contains_key(*id))
                .collect();
            assert!(
                fresh.len() <= 1,
                "One Refresh issued multiple fresh Record queries: {replies:?}"
            );
            fresh
                .first()
                .is_some_and(|(id, rows)| *id != &subscription && rows.is_empty())
        };
        if completed
            && message.is_some_and(|message| {
                message.contains("Your draft is kept") && !message.contains("Loading Record")
            })
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Refreshing a deleted Record never received its fresh empty query response"
        );
        app.tick("unavailable-refresh-wait");
    }
    let retried = app.properties_ready(&uid);
    assert_eq!(retried.area, initial.area);
    assert_eq!(retried.assertions.as_ref().unwrap().entity, field.entity);
    assert_eq!(retried.assertions.as_ref().unwrap().value, DRAFT);
    assert_eq!(app.application_record(main).as_deref(), Some(uid.as_str()));
    let missing = query(&runtime, &app.fixture.cell, &uid);
    assert!(
        missing.iter().any(|reply| match reply {
            cell::ServerMessage::Snapshot { rows, .. } => rows.is_empty(),
            cell::ServerMessage::Error { id, .. } => id == "verify-unavailable-record",
            _ => false,
        }),
        "The deleted Record was recreated or returned as available: {missing:?}"
    );
    let unaffected = query(&runtime, &app.fixture.cell, &other)
        .into_iter()
        .find_map(|reply| match reply {
            cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
            _ => None,
        })
        .unwrap();
    assert_eq!(unaffected.len(), 1);
    assert_eq!(unaffected[0]["uid"], other);
    assert_eq!(unaffected[0]["body"], OTHER);
    app.tick("unavailable-refresh-visible");
    app.fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and LINCE_PARK_TEST_QML with Qt 6.8+ Wayland"]
fn flat_free_qt_deleted_record_keeps_its_draft_and_refresh_target() {
    exercise(false);
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and LINCE_PARK_TEST_QML with Qt 6.8+ Wayland"]
fn spatial_tiled_qt_deleted_record_keeps_its_draft_and_refresh_target() {
    exercise(true);
}
