use super::*;
use bevy::prelude::{IntoScheduleConfigs, PostUpdate, Query};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn block_profile(directory: &Path) {
    let path = directory.join("park-windows.json");
    std::fs::rename(&path, directory.join("retained-window-profiles.json")).unwrap();
    std::fs::create_dir(path).unwrap();
}

fn record_uids(
    runtime: &tokio::runtime::Runtime,
    provider: &native::app::CellHandle,
) -> BTreeSet<String> {
    let replies = runtime.block_on(
        provider
            .0
            .local_session()
            .handle(native::cell_bridge::records_subscription()),
    );
    let rows = replies
        .into_iter()
        .find_map(|message| match message {
            cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
            _ => None,
        })
        .unwrap();
    assert!(
        rows.len() < 512,
        "The creation fixture must not truncate Record identities"
    );
    rows.iter()
        .map(|row| row["uid"].as_str().unwrap().to_owned())
        .collect()
}

fn record(
    runtime: &tokio::runtime::Runtime,
    provider: &native::app::CellHandle,
    uid: &str,
) -> Value {
    let config = native::full_record::config(uid, Source::Local);
    let replies = runtime.block_on(provider.0.local_session().handle(
        cell::ClientMessage::Subscribe {
            id: "verify-creation-unrelated-record".into(),
            protein: config.query().unwrap(),
        },
    ));
    let mut rows = replies
        .into_iter()
        .find_map(|message| match message {
            cell::ServerMessage::Snapshot { rows, .. } => Some(rows),
            _ => None,
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    let row = rows.pop().unwrap();
    assert_eq!(row["uid"], uid);
    row
}

#[test]
#[ignore = "requires hardware Vulkan/EGL and LINCE_PARK_TEST_QML with Qt 6.8+ Wayland"]
fn flat_free_qt_creation_retry_keeps_its_pending_inspector_after_workspace_change_and_close() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut app = Toolkit::prepared(&runtime, directory.path(), false, Some(block_profile));
    app.wait("creation-map", |app| {
        app.window("Park toolkit parent").is_some() && !app.state().is_null()
    });
    let main = app.window("Park toolkit parent").unwrap().id;
    app.fixture
        .desktop
        .worker
        .input(Input::Activate(main))
        .unwrap();
    app.tick("creation-parent");
    app.configured(main);
    let placement = app.inspect(move |canvas| {
        let world = canvas.world_mut();
        world
            .query::<(
                &native::layout::desktop::ApplicationWindow,
                &native::layout::LayoutBox,
            )>()
            .iter(world)
            .find(|(window, _)| window.surface == main)
            .unwrap()
            .1
            .id
    });
    app.chord(&[(125, true), (20, true), (20, false), (125, false)]);
    app.chord(&[(125, true), (14, true), (14, false), (125, false)]);
    let existing = record_uids(&runtime, &app.fixture.cell);
    let other = app.application_record(app.fixture.clients[0].1).unwrap();
    app.chord(&[(125, true), (25, true), (25, false), (125, false)]);
    app.wait("creation-operation", |app| {
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
    let (area, uid) = loop {
        if let Some(found) = app.inspect(move |canvas| {
            let world = canvas.world_mut();
            world
                .query::<(Entity, &native::area::InfluenceArea)>()
                .iter(world)
                .find_map(|(entity, area)| {
                    let window = area.protein.as_ref()?.window_record.as_ref()?;
                    (window.creating && window.placement == placement)
                        .then(|| (entity.to_bits(), window.uid.clone()))
                })
        }) {
            break found;
        }
        assert!(
            Instant::now() < deadline,
            "Properties did not show a pending inspector"
        );
        app.tick("creation-pending");
    };
    app.wait("creation-visible-error", |app| {
        let expected = uid.clone();
        app.inspect(move |canvas| {
            let world = canvas.world_mut();
            world
                .query::<(Entity, &Name, &Text)>()
                .iter(world)
                .any(|(entity, name, text)| {
                    name.as_str() == "record.availability.status"
                        && belongs(world, entity, &expected)
                        && text.0.contains("window profile")
                })
        })
    });
    let pending = app.properties_state(&uid).unwrap();
    assert_eq!(pending.area, area);
    assert_eq!(pending.workspace, 1);
    assert!(pending.live);
    assert!(pending.body.is_none() && pending.assertions.is_none());
    assert!(!existing.contains(&uid));
    assert_eq!(record_uids(&runtime, &app.fixture.cell), existing);
    let other_before = record(&runtime, &app.fixture.cell, &other);
    let activated = Arc::new(AtomicUsize::new(0));
    let retried = Arc::new(AtomicUsize::new(0));
    let requested = retried.clone();
    let observed = activated.clone();
    let expected = uid.clone();
    app.inspect(move |canvas| {
        let world = canvas.world_mut();
        let button = world
            .query::<(
                Entity,
                &native::actions::ActionButton,
                &bevy::prelude::Children,
            )>()
            .iter(world)
            .find(|(entity, _, children)| {
                belongs(world, *entity, &expected)
                    && children.iter().any(|child| {
                        world
                            .get::<Text>(*child)
                            .is_some_and(|text| text.0 == "Retry Record creation")
                    })
            })
            .unwrap()
            .0;
        world.entity_mut(button).observe(
            move |_: bevy::prelude::On<bevy::ui_widgets::Activate>| {
                observed.fetch_add(1, Ordering::SeqCst);
            },
        );
        world
            .resource_mut::<bevy::ecs::schedule::Schedules>()
            .get_mut(PostUpdate)
            .unwrap()
            .add_systems(
                (move |pending: Query<(
                    &native::full_record::window::Creation,
                    &native::area::InfluenceArea,
                )>| {
                    if pending.iter().any(|(creation, area)| {
                        creation.retry_requested()
                            && area
                                .protein
                                .as_ref()
                                .and_then(|config| config.window_record.as_ref())
                                .is_some_and(|window| window.uid == expected)
                    }) {
                        requested.fetch_add(1, Ordering::SeqCst);
                    }
                })
                .after(native::actions::ApplyActions),
            );
    });
    app.property_button(&uid, "Retry Record creation");
    assert_eq!(activated.load(Ordering::SeqCst), 1);
    assert_eq!(retried.load(Ordering::SeqCst), 1);
    assert_eq!(record_uids(&runtime, &app.fixture.cell), existing);
    app.fixture.activate(0);
    app.chord(&[(125, true), (3, true), (3, false), (125, false)]);
    app.allow_exit = true;
    app.fixture.desktop.server.close(main);
    app.wait("creation-source-closed", |app| {
        app.window("Park toolkit parent").is_none()
    });
    let expected = uid.clone();
    app.inspect(move |canvas| {
        let world = canvas.world_mut();
        let entity = Entity::from_bits(area);
        let config = world
            .get::<native::area::InfluenceArea>(entity)
            .unwrap()
            .protein
            .as_ref()
            .unwrap();
        assert_eq!(config.window_record.as_ref().unwrap().uid, expected);
        assert!(config.window_record.as_ref().unwrap().creating);
        assert_eq!(
            world
                .get::<native::workspace::WorkspaceMember>(entity)
                .unwrap()
                .0,
            1
        );
        assert!(
            world
                .query::<&RecordBinding>()
                .iter(world)
                .filter(|binding| binding.area == entity)
                .all(|binding| binding.uid == expected)
        );
    });
    std::fs::remove_dir(directory.path().join("park-windows.json")).unwrap();
    std::fs::rename(
        directory.path().join("retained-window-profiles.json"),
        directory.path().join("park-windows.json"),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let expected = uid.clone();
        let ready = app.inspect(move |canvas| {
            let world = canvas.world_mut();
            let config = world
                .get::<native::area::InfluenceArea>(Entity::from_bits(area))
                .unwrap()
                .protein
                .as_ref()
                .unwrap();
            let association = config.window_record.as_ref().unwrap();
            assert_eq!(association.uid, expected);
            !association.creating
        });
        if ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The captured creation did not recover after the original window closed"
        );
        app.tick("creation-recovery");
    }
    assert_eq!(
        app.fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .geometry
            .as_ref()
            .unwrap()
            .outputs[0]
            .workspace,
        2
    );
    assert_eq!(
        record_uids(&runtime, &app.fixture.cell),
        existing.into_iter().chain([uid.clone()]).collect()
    );
    app.chord(&[(125, true), (2, true), (2, false), (125, false)]);
    let state = app.properties_ready(&uid);
    assert_eq!(state.area, area);
    assert_eq!(state.workspace, 1);
    assert!(!state.live);
    assert!(state.header.contains("closed or unavailable"));
    app.dismiss_toast("Record creation is waiting for its window profile to be saved");
    app.property_button(&uid, "Raw");
    let editable = app.editable_properties(&uid);
    app.click_property(editable.body.as_ref().unwrap());
    app.text("created-after-close");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let rows = records(&runtime, &app.fixture.cell, &uid, &other);
        if rows
            .iter()
            .any(|row| row["uid"] == uid && row["body"] == "created-after-close")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The recovered editor did not save to its captured Record: {rows:?}"
        );
        app.tick("creation-save");
    }
    app.tick("creation-recovered-visible");
    let expected = uid.clone();
    app.inspect(move |canvas| {
        let world = canvas.world_mut();
        assert_eq!(
            world
                .query::<&native::area::InfluenceArea>()
                .iter(world)
                .filter(|area| area
                    .protein
                    .as_ref()
                    .and_then(|config| config.window_record.as_ref())
                    .is_some_and(|window| window.uid == expected))
                .count(),
            1
        );
    });
    let other_after = records(&runtime, &app.fixture.cell, &uid, &other)
        .into_iter()
        .find(|row| row["uid"] == other)
        .unwrap();
    for field in ["head", "body", "assertions"] {
        assert_eq!(other_after[field], other_before[field]);
    }
    app.fixture.desktop.worker.stop().unwrap();
}
