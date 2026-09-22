use super::*;
use serde_json::json;
use std::{sync::Arc, time::Duration};

fn selection() -> Selection {
    Selection {
        predicate: nucleus::new_uid("c"),
        name: "my-order".into(),
    }
}

fn row(selection: &Selection, quantity: Option<&str>) -> serde_json::Value {
    json!({"uid":nucleus::new_uid("r"), "head":"Record", "assertions":[{"predicate_uid":selection.predicate,"predicate":selection.name,"quantity":quantity,"object":null,"unit":null}]})
}

#[test]
fn quantities_sort_exactly_and_ties_keep_protein_order() {
    let selection = selection();
    let rows: Vec<_> = [
        Some("10"),
        Some("3"),
        Some("4"),
        None,
        Some("-2.5"),
        Some("3.0"),
        Some("9007199254740993"),
        Some("9007199254740992"),
    ]
    .into_iter()
    .map(|value| row(&selection, value))
    .collect();
    let sorted = model::ordered(&rows, Some(&selection)).unwrap();
    assert_eq!(
        sorted
            .iter()
            .map(|item| item.uid.as_str())
            .collect::<Vec<_>>(),
        [4, 1, 5, 2, 0, 7, 6, 3].map(|index| rows[index]["uid"].as_str().unwrap())
    );
    assert_eq!(
        model::ordered(&rows, None)
            .unwrap()
            .iter()
            .map(|item| item.uid.as_str())
            .collect::<Vec<_>>(),
        rows.iter()
            .map(|row| row["uid"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
}

#[test]
fn units_duplicates_and_invalid_quantities_cannot_be_numbered() {
    let selection = selection();
    let mut value = row(&selection, Some("3"));
    value["assertions"][0]["unit"] = json!(nucleus::new_uid("c"));
    assert!(model::choices(&[value.clone()]).is_empty());
    assert!(model::ordered(&[value.clone()], Some(&selection)).is_err());
    value["assertions"][0]["unit"] = serde_json::Value::Null;
    value["assertions"][0]["quantity"] = json!("not a number");
    assert!(model::ordered(&[value.clone()], Some(&selection)).is_err());
    value["assertions"][0]["quantity"] = json!("3");
    let assertion = value["assertions"][0].clone();
    value["assertions"].as_array_mut().unwrap().push(assertion);
    assert!(model::ordered(&[value], Some(&selection)).is_err());
    let value = row(&selection, Some("3"));
    assert!(model::ordered(&[value.clone(), value], Some(&selection)).is_err());
}

#[test]
fn large_lists_are_sorted_without_losing_records() {
    let selection = selection();
    let rows: Vec<_> = (1..=10_000)
        .rev()
        .map(|value| row(&selection, Some(&value.to_string())))
        .collect();
    let sorted = model::ordered(&rows, Some(&selection)).unwrap();
    assert_eq!(sorted.len(), rows.len());
    assert!(sorted.windows(2).all(|pair| {
        pair[0]
            .quantity
            .unwrap()
            .exact_numeric_cmp(pair[1].quantity.unwrap())
            .is_lt()
    }));
}

async fn until(app: &mut App, condition: impl Fn(&mut World) -> bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            app.update();
            if condition(app.world_mut()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn protein_selection_renumbers_in_visible_order_and_persists() {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let predicate = store_concept(&engine, "my-assertion").await;
    let other = store_concept(&engine, "untouched").await;
    let mut ids = Vec::new();
    for (index, quantity) in [Some("10"), Some("3"), Some("4"), None]
        .into_iter()
        .enumerate()
    {
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: format!("Task {index}"),
                    body: String::new(),
                    quantity: -1.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        for (predicate, quantity) in [(&predicate, quantity), (&other, Some("77"))] {
            if let Some(quantity) = quantity {
                engine
                    .act(
                        engine::actions::Action::AssertRecord {
                            subject: uid.clone(),
                            predicate: predicate.clone(),
                            object: None,
                            quantity: Some(quantity.into()),
                            unit: None,
                        },
                        None,
                    )
                    .await
                    .unwrap();
            }
        }
        ids.push(uid);
    }
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<crate::tokens::ThemeSettings>()
        .init_resource::<bevy::input_focus::InputFocus>()
        .init_resource::<bevy::picking::hover::HoverMap>()
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .insert_resource(crate::app::CellHandle(cell::CellRuntime {
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: default(),
            fiote: None,
            information: None,
        }))
        .add_plugins((
            crate::cell_bridge::CellBridgePlugin,
            crate::protein_area::ProteinAreaPlugin,
            crate::record_binding::RecordBindingPlugin,
            AssertionCastlePlugin,
        ));
    let root = app
        .world_mut()
        .spawn((
            crate::workspace::Workspaces::default(),
            crate::canvas::CanvasView::default(),
        ))
        .id();
    let owner = spawn(
        app.world_mut(),
        root,
        1,
        DVec2::new(30.0, 40.0),
        AssertionCastle::default(),
    );
    let area = app.world().get::<Frame>(owner).unwrap().area;
    app.world_mut()
        .get_mut::<crate::area::InfluenceArea>(area)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .draft
        .query = json!({"source":"record","where":[{"quantity_lt":"0"}],"order":[{"asc":"head"}],"limit":null});
    until(&mut app, |world| {
        world.get::<View>(owner).unwrap().rows.len() == 4
    })
    .await;
    assert!(castle_feed::cards(app.world_mut(), area).is_empty());
    Command::Select(Selection {
        predicate: predicate.clone(),
        name: "my-assertion".into(),
    })
    .apply(app.world_mut(), owner);
    let mut expected = [
        ids[1].clone(),
        ids[2].clone(),
        ids[0].clone(),
        ids[3].clone(),
    ];
    assert_eq!(
        app.world()
            .get::<View>(owner)
            .unwrap()
            .rows
            .iter()
            .map(|item| item.uid.clone())
            .collect::<Vec<_>>(),
        expected
    );
    input::move_row(app.world_mut(), owner, &ids[3], &ids[1], false);
    expected = [
        ids[3].clone(),
        ids[1].clone(),
        ids[2].clone(),
        ids[0].clone(),
    ];
    app.world_mut().get_mut::<View>(owner).unwrap().feed.clear();
    app.update();
    assert_eq!(
        app.world()
            .get::<View>(owner)
            .unwrap()
            .rows
            .iter()
            .map(|row| row.uid.clone())
            .collect::<Vec<_>>(),
        expected
    );
    Command::Renumber.apply(app.world_mut(), owner);
    assert!(renumber(app.world_mut(), owner).is_err());
    until(&mut app, |world| {
        world.get::<View>(owner).unwrap().batch.is_none()
    })
    .await;
    assert_eq!(
        app.world().get::<View>(owner).unwrap().message,
        "Renumbered 4 Records"
    );
    let query: protein::Protein = serde_json::from_value(json!({"source":"record","where":[{"quantity_lt":"0"}],"fields":["uid","assertions"],"limit":null})).unwrap();
    let records = protein::execute(&engine.store, &query).await.unwrap();
    for (index, uid) in expected.iter().enumerate() {
        let row = records
            .iter()
            .find(|row| row["uid"].as_str() == Some(uid))
            .unwrap();
        let assertions = row["assertions"].as_array().unwrap();
        assert_eq!(
            assertions
                .iter()
                .find(|value| value["predicate_uid"] == predicate)
                .unwrap()["quantity"],
            (index + 1).to_string()
        );
        assert_eq!(
            assertions
                .iter()
                .find(|value| value["predicate_uid"] == other)
                .unwrap()["quantity"],
            "77"
        );
    }
    let saved = snapshot(app.world_mut(), root).pop().unwrap();
    assert!(saved.valid());
    let json = serde_json::to_string(&saved).unwrap();
    app.world_mut().despawn(owner);
    serde_json::from_str::<SavedAssertion>(&json)
        .unwrap()
        .restore(app.world_mut(), root);
    let (restored, castle) = app
        .world_mut()
        .query::<(Entity, &AssertionCastle)>()
        .iter(app.world())
        .next()
        .unwrap();
    assert_eq!(castle.selection.as_ref().unwrap().predicate, predicate);
    assert_eq!(
        app.world()
            .get::<crate::canvas::CanvasItem>(restored)
            .unwrap()
            .position,
        DVec2::new(30.0, 40.0)
    );
    assert_eq!(
        config(app.world(), restored).unwrap().draft.query["where"][0]["quantity_lt"],
        "0"
    );
}

#[test]
fn click_drag_reorders_rows_and_cancel_leaves_them_unchanged() {
    use bevy::picking::{
        backend::HitData,
        hover::HoverMap,
        pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
    };
    for pointer in [PointerId::Mouse, crate::topology::input::CONTENT_POINTER] {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>();
        input::install(&mut app);
        app.add_plugins(crate::canvas_pan::CanvasPanPlugin);
        let root = app
            .world_mut()
            .spawn((
                crate::workspace::Workspaces::default(),
                crate::canvas::CanvasView::default(),
            ))
            .id();
        let owner = spawn(
            app.world_mut(),
            root,
            1,
            DVec2::ZERO,
            AssertionCastle::default(),
        );
        let selection = selection();
        let data: Vec<_> = ["3", "4", "10"]
            .map(|quantity| row(&selection, Some(quantity)))
            .into();
        let rows = model::ordered(&data, Some(&selection)).unwrap();
        let ids: Vec<_> = rows.iter().map(|row| row.uid.clone()).collect();
        app.world_mut().get_mut::<View>(owner).unwrap().rows = rows;
        ui::render(app.world_mut(), owner);
        let send = |app: &mut App, uid: &str, action: PointerAction, y: f32| {
            let entry = app
                .world_mut()
                .query::<(Entity, &input::ListRow)>()
                .iter(app.world())
                .find(|(_, row)| row.uid == uid)
                .unwrap()
                .0;
            let title = app.world().get::<Children>(entry).unwrap()[2];
            let text = app.world().get::<Children>(title).unwrap()[0];
            let mut hover = app.world_mut().resource_mut::<HoverMap>();
            hover.clear();
            hover
                .entry(pointer)
                .or_default()
                .insert(text, HitData::new(root, 0.0, None, None));
            let location = Location {
                target: bevy::camera::NormalizedRenderTarget::None {
                    width: 900,
                    height: 680,
                },
                position: Vec2::new(30.0, y),
            };
            if pointer != PointerId::Mouse {
                app.world_mut().write_message(PointerInput::new(
                    PointerId::Mouse,
                    location.clone(),
                    action.clone(),
                ));
            }
            app.world_mut()
                .write_message(PointerInput::new(pointer, location, action));
            app.update();
        };
        send(
            &mut app,
            &ids[0],
            PointerAction::Press(PointerButton::Primary),
            0.0,
        );
        send(
            &mut app,
            &ids[2],
            PointerAction::Move {
                delta: Vec2::new(0.0, 20.0),
            },
            20.0,
        );
        assert_eq!(app.world().get::<View>(owner).unwrap().rows[0].uid, ids[0]);
        send(
            &mut app,
            &ids[2],
            PointerAction::Release(PointerButton::Primary),
            20.0,
        );
        assert_eq!(
            app.world().get::<View>(owner).unwrap().manual_order,
            [ids[1].clone(), ids[2].clone(), ids[0].clone()]
        );
        assert_eq!(
            app.world()
                .get::<crate::canvas::CanvasItem>(owner)
                .unwrap()
                .position,
            DVec2::ZERO
        );
        assert_eq!(
            app.world_mut()
                .query::<&crate::area::InfluenceArea>()
                .iter(app.world())
                .count(),
            1
        );
        send(
            &mut app,
            &ids[0],
            PointerAction::Press(PointerButton::Primary),
            0.0,
        );
        send(
            &mut app,
            &ids[1],
            PointerAction::Move {
                delta: Vec2::new(0.0, -20.0),
            },
            -20.0,
        );
        send(&mut app, &ids[1], PointerAction::Cancel, -20.0);
        assert_eq!(
            app.world().get::<View>(owner).unwrap().manual_order,
            [ids[1].clone(), ids[2].clone(), ids[0].clone()]
        );
        assert!(!app.world().get::<View>(owner).unwrap().dragging);
    }
}

async fn store_concept(engine: &engine::Engine, name: &str) -> String {
    let lingua = engine
        .act(
            engine::actions::Action::CreateLingua {
                name: format!("{name}-language"),
                visibility: "private".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            engine::actions::Action::CreateConcept {
                lingua,
                name: name.into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

#[test]
fn failed_saves_stop_the_batch_and_late_replies_cannot_advance_another_run() {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<bevy::input_focus::InputFocus>();
    let root = app
        .world_mut()
        .spawn(crate::workspace::Workspaces::default())
        .id();
    let world = app.world_mut();
    let selection = selection();
    let owner = spawn(
        world,
        root,
        1,
        DVec2::ZERO,
        AssertionCastle {
            selection: Some(selection.clone()),
        },
    );
    let rows = model::ordered(
        &[row(&selection, Some("3")), row(&selection, Some("4"))],
        Some(&selection),
    )
    .unwrap();
    let first = world.spawn((Submission(owner), ChildOf(owner))).id();
    let next = world.spawn((Submission(owner), ChildOf(owner))).id();
    let config = config(world, owner).unwrap();
    world.get_mut::<View>(owner).unwrap().batch = Some(Batch {
        rows: rows.clone(),
        config: config.clone(),
        selection: selection.clone(),
        next: 0,
        waiting: Some(first),
    });
    assert!(finished(world, first, None));
    assert_eq!(
        world
            .get::<View>(owner)
            .unwrap()
            .batch
            .as_ref()
            .unwrap()
            .next,
        1
    );
    world
        .get_mut::<View>(owner)
        .unwrap()
        .batch
        .as_mut()
        .unwrap()
        .waiting = Some(next);
    assert!(finished(world, next, Some("Permission denied".into())));
    let view = world.get::<View>(owner).unwrap();
    assert!(view.batch.is_none());
    assert_eq!(
        view.message,
        "Stopped with 1 of 2 Records confirmed: Permission denied"
    );
    let late = world.spawn((Submission(owner), ChildOf(owner))).id();
    let current = world.spawn((Submission(owner), ChildOf(owner))).id();
    world.get_mut::<View>(owner).unwrap().batch = Some(Batch {
        rows,
        config,
        selection,
        next: 0,
        waiting: Some(current),
    });
    assert!(finished(world, late, None));
    assert_eq!(
        world
            .get::<View>(owner)
            .unwrap()
            .batch
            .as_ref()
            .unwrap()
            .next,
        0
    );
    assert_eq!(
        world
            .get::<View>(owner)
            .unwrap()
            .batch
            .as_ref()
            .unwrap()
            .waiting,
        Some(current)
    );
}
