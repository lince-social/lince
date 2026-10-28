use super::*;
use nucleus::demeter::{NutrientBound, Unit};

#[allow(dead_code)]
mod authority {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../engine/tests/support/karma.rs"
    ));
}

fn fixture() -> DemeterCastle {
    let id = nucleus::new_uid("r");
    let mut state = DemeterCastle::default();
    state.config = Config::new("2026-10-25".into());
    state.config.days = 2;
    state.config.horizon_days = 14;
    state.config.manual.bounds.push(NutrientBound {
        nutrient: "energy_kcal".into(),
        unit: Unit::Kcal,
        bounds: Bounds {
            lower: Some(demeter::exact("100").unwrap()),
            upper: None,
        },
        scope: "total".into(),
        source: "Personal target".into(),
    });
    state.config.foods.push(FoodChoice {
        food: id.clone(),
        bounds: Vec::new(),
        acquisition: Acquisition::Buy,
        volume_ml_per_g: None,
    });
    state.selected_food = Some(id.clone());
    state.tab = Tab::Foods;
    state.catalog = json!({"foods":[{"uid":id,"label":"Measured cooked food"}],"offers":[],"templates":[],"references":[],"plans":[],"vocabulary":{}});
    state
}

fn mounted(state: DemeterCastle) -> (App, Entity, Entity) {
    let mut app = crate::native::sand_panel::tests::app();
    let root = app.world_mut().spawn(Node::default()).id();
    let owner = spawn(app.world_mut(), root, 1, DVec2::ZERO, state);
    (app, root, owner)
}

fn edit(world: &mut World, owner: Entity, name: &str, value: &str) {
    let key = editor::key(world.get::<DemeterCastle>(owner).unwrap(), name);
    let field = world.get::<View>(owner).unwrap().fields[&key];
    world
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor
        .set_text(value);
}
fn value(world: &World, owner: Entity, name: &str) -> String {
    let key = editor::key(world.get::<DemeterCastle>(owner).unwrap(), name);
    panel::value(world, world.get::<View>(owner).unwrap().fields[&key]).unwrap()
}

#[test]
fn empty_zero_fixed_and_excluded_food_bounds_survive_native_controls() {
    let state = fixture();
    let food = state.selected_food.clone().unwrap();
    let (mut app, _, owner) = mounted(state);
    edit(
        app.world_mut(),
        owner,
        &format!("food:{food}:day:upper"),
        "0",
    );
    editor::capture(app.world_mut(), owner).unwrap();
    let state = app.world().get::<DemeterCastle>(owner).unwrap();
    assert_eq!(
        state.config.foods[0].bounds[0].bounds.upper,
        Some(demeter::exact("0").unwrap())
    );
    assert!(state.config.foods[0].bounds[0].bounds.lower.is_none());
    edit(
        app.world_mut(),
        owner,
        &format!("food:{food}:day:upper"),
        "",
    );
    editor::capture(app.world_mut(), owner).unwrap();
    assert!(
        app.world()
            .get::<DemeterCastle>(owner)
            .unwrap()
            .config
            .foods[0]
            .bounds
            .is_empty()
    );
    edit(app.world_mut(), owner, &format!("food:{food}:fixed"), "125");
    Operation::Fixed.apply(app.world_mut(), owner);
    let bounds = &app
        .world()
        .get::<DemeterCastle>(owner)
        .unwrap()
        .config
        .foods[0]
        .bounds[0]
        .bounds;
    assert_eq!(bounds.lower, bounds.upper);
    assert_eq!(bounds.lower, Some(demeter::exact("125").unwrap()));
    Operation::Exclude(true).apply(app.world_mut(), owner);
    assert_eq!(
        value(app.world(), owner, &format!("food:{food}:day:upper")),
        "0"
    );
    Operation::Exclude(false).apply(app.world_mut(), owner);
    assert_eq!(
        value(app.world(), owner, &format!("food:{food}:day:upper")),
        ""
    );
}

#[test]
fn drafts_are_specific_to_each_food_and_workspace_recovery_keeps_unfinished_values() {
    let mut state = fixture();
    let first = state.selected_food.clone().unwrap();
    let second = nucleus::new_uid("r");
    let saved_offer = nucleus::new_uid("r");
    let other_offer = nucleus::new_uid("r");
    state
        .drafts
        .insert(format!("{first}|offer:id"), saved_offer.clone());
    state
        .drafts
        .insert(format!("{second}|offer:id"), other_offer.clone());
    state.catalog["foods"]
        .as_array_mut()
        .unwrap()
        .push(json!({"uid":second,"label":"Second food"}));
    let (mut app, root, owner) = mounted(state);
    edit(app.world_mut(), owner, "offer:price", "1.25");
    Operation::Select(second.clone()).apply(app.world_mut(), owner);
    assert_eq!(value(app.world(), owner, "offer:price"), "");
    edit(app.world_mut(), owner, "offer:price", "8");
    let session = app
        .world()
        .get::<DemeterCastle>(owner)
        .unwrap()
        .session
        .clone();
    app.world_mut()
        .resource_mut::<Pending>()
        .0
        .insert("saved-offer".into(), (owner, Reply::Offer, session));
    receive(
        app.world_mut(),
        &Source::Local,
        &ServerMessage::ActionOk {
            id: "saved-offer".into(),
            created: Some(saved_offer.clone()),
            facts: 0,
            warnings: Vec::new(),
            data: Some(
                json!({"offer":saved_offer,"market":{"offer":{"id":saved_offer,"food":first},"vendor":"market"}}),
            ),
        },
    );
    let drafts = &app.world().get::<DemeterCastle>(owner).unwrap().drafts;
    assert!(!drafts.contains_key(&format!("{first}|offer:id")));
    assert_eq!(
        drafts.get(&format!("{second}|offer:id")),
        Some(&other_offer)
    );
    Operation::Select(first.clone()).apply(app.world_mut(), owner);
    assert_eq!(value(app.world(), owner, "offer:price"), "1.25");
    edit(
        app.world_mut(),
        owner,
        &format!("food:{first}:day:upper"),
        "unfinished",
    );
    let saved = snapshot(app.world_mut(), root);
    assert_eq!(saved.len(), 1);
    assert!(saved[0].valid());
    let serialized = serde_json::to_string(&saved[0]).unwrap();
    let restored: SavedDemeterCastle = serde_json::from_str(&serialized).unwrap();
    let other = app.world_mut().spawn(Node::default()).id();
    restored.restore(app.world_mut(), other);
    let restored = app
        .world_mut()
        .query::<(Entity, &ChildOf, &DemeterCastle)>()
        .iter(app.world())
        .find(|(_, p, _)| p.parent() == other)
        .unwrap()
        .0;
    assert_eq!(
        value(app.world(), restored, &format!("food:{first}:day:upper")),
        "unfinished"
    );
    assert_eq!(value(app.world(), restored, "offer:price"), "1.25");
}

#[test]
fn late_save_responses_preserve_edits_and_old_plan_responses_are_ignored() {
    let state = fixture();
    let food = state.selected_food.clone().unwrap();
    let config = state.config.clone();
    let (mut app, _, owner) = mounted(state);
    let session = app
        .world()
        .get::<DemeterCastle>(owner)
        .unwrap()
        .session
        .clone();
    app.world_mut()
        .resource_mut::<Pending>()
        .0
        .insert("save".into(), (owner, Reply::Save(false, 0), session));
    edit(
        app.world_mut(),
        owner,
        &format!("food:{food}:day:upper"),
        "95",
    );
    editor::capture(app.world_mut(), owner).unwrap();
    let plan = nucleus::new_uid("r");
    receive(
        app.world_mut(),
        &Source::Local,
        &ServerMessage::ActionOk {
            id: "save".into(),
            created: Some(plan.clone()),
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"plan":plan,"saved":{"revision":1,"config":config}})),
        },
    );
    assert_eq!(
        value(app.world(), owner, &format!("food:{food}:day:upper")),
        "95"
    );
    assert!(app.world().get::<DemeterCastle>(owner).unwrap().changed);
    let session = app
        .world()
        .get::<DemeterCastle>(owner)
        .unwrap()
        .session
        .clone();
    app.world_mut()
        .resource_mut::<Pending>()
        .0
        .insert("old".into(), (owner, Reply::Save(false, 0), session));
    Operation::New.apply(app.world_mut(), owner);
    receive(
        app.world_mut(),
        &Source::Local,
        &ServerMessage::ActionOk {
            id: "old".into(),
            created: Some(plan.clone()),
            facts: 0,
            warnings: Vec::new(),
            data: Some(json!({"plan":plan,"saved":{"revision":1,"config":config}})),
        },
    );
    assert!(
        app.world()
            .get::<DemeterCastle>(owner)
            .unwrap()
            .plan
            .is_none()
    );
}

#[test]
fn four_views_use_wrapping_controls_and_input_widths_at_narrow_and_wide_sizes() {
    let (mut app, _, owner) = mounted(fixture());
    for width in [360.0, 720.0] {
        app.world_mut()
            .get_mut::<crate::native::canvas::CanvasItem>(owner)
            .unwrap()
            .size
            .x = width;
        for tab in [Tab::Requirements, Tab::Foods, Tab::Results, Tab::Production] {
            let body = app.world().get::<View>(owner).unwrap().body;
            app.world_mut().get_mut::<ScrollPosition>(body).unwrap().0.y = 300.0;
            Operation::Tab(tab).apply(app.world_mut(), owner);
            assert_eq!(
                app.world().get::<ScrollPosition>(body).unwrap().0,
                Vec2::ZERO
            );
            let view = app.world().get::<View>(owner).unwrap();
            for field in view.fields.values() {
                let node = app.world().get::<Node>(*field).unwrap();
                assert_eq!(node.width, percent(100));
                assert_eq!(node.min_width, px(0));
            }
        }
    }
    let wrapping = app
        .world_mut()
        .query::<&Node>()
        .iter(app.world())
        .filter(|n| n.flex_wrap == FlexWrap::Wrap)
        .count();
    assert!(wrapping >= 2);
}

#[test]
fn failed_requests_leave_editor_values_available_for_retry() {
    let (mut app, _, owner) = mounted(fixture());
    edit(app.world_mut(), owner, "offer:price", "7.50");
    let session = app
        .world()
        .get::<DemeterCastle>(owner)
        .unwrap()
        .session
        .clone();
    app.world_mut()
        .resource_mut::<Pending>()
        .0
        .insert("failed".into(), (owner, Reply::Offer, session));
    receive(
        app.world_mut(),
        &Source::Local,
        &ServerMessage::Error {
            id: "failed".into(),
            message: "Permission changed".into(),
            code: Some("forbidden".into()),
        },
    );
    assert_eq!(value(app.world(), owner, "offer:price"), "7.50");
    assert!(!app.world().resource::<Pending>().0.contains_key("failed"));
}

#[tokio::test]
async fn native_workflow_saves_prices_solves_previews_and_accepts_through_the_cell() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    authority::authorize(&engine).await;
    let food = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("native-meal-food".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Measured native food".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            engine::actions::Action::SetExtension {
                target: food.clone(),
                namespace: "nutrition".into(),
                fds: json!({"basis_g":100,"nutrients":{"energy_kcal":100}}),
            },
            None,
        )
        .await
        .unwrap();
    let mut app = crate::native::sand_panel::tests::app();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .add_plugins(DemeterCastlePlugin);
    crate::native::sand_panel::tests::connect(&mut app, engine.clone());
    let root = app
        .world_mut()
        .spawn((
            Node::default(),
            crate::native::workspace::Workspaces::default(),
        ))
        .id();
    let mut state = fixture();
    state.selected_food = Some(food.clone());
    state.config.foods[0].food = food;
    state.catalog = Value::Null;
    let owner = spawn(app.world_mut(), root, 1, DVec2::ZERO, state);
    crate::native::sand_panel::tests::settle(&mut app, |world| {
        world.get::<DemeterCastle>(owner).unwrap().catalog["foods"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    })
    .await;
    Operation::Vocabulary.apply(app.world_mut(), owner);
    crate::native::sand_panel::tests::settle(&mut app, |world| {
        world.get::<DemeterCastle>(owner).unwrap().catalog["vocabulary"]["price"].is_string()
    })
    .await;
    edit(app.world_mut(), owner, "offer:vendor", "Local test market");
    edit(app.world_mut(), owner, "offer:price", "2");
    edit(app.world_mut(), owner, "offer:package", "100");
    Operation::SaveOffer.apply(app.world_mut(), owner);
    crate::native::sand_panel::tests::settle(&mut app, |world| {
        world
            .get::<DemeterCastle>(owner)
            .unwrap()
            .config
            .offers
            .len()
            == 1
    })
    .await;
    Operation::Save(true).apply(app.world_mut(), owner);
    crate::native::sand_panel::tests::settle(&mut app, |world| {
        world.get::<DemeterCastle>(owner).unwrap().result.is_some()
    })
    .await;
    let state = app.world().get::<DemeterCastle>(owner).unwrap();
    assert_eq!(
        state.result.as_ref().unwrap().solution.status,
        nucleus::optimization::Status::Optimal
    );
    assert_eq!(state.result.as_ref().unwrap().rounded_bill, 4.0);
    Operation::Preview.apply(app.world_mut(), owner);
    crate::native::sand_panel::tests::settle(&mut app, |world| {
        world
            .get::<DemeterCastle>(owner)
            .unwrap()
            .projection
            .is_some()
    })
    .await;
    assert!(
        store::records::all_extensions(&engine.store.pool, engine::demeter::TASK)
            .await
            .unwrap()
            .is_empty()
    );
    Operation::Accept.apply(app.world_mut(), owner);
    crate::native::sand_panel::tests::settle(&mut app, |world| {
        world
            .get::<DemeterCastle>(owner)
            .unwrap()
            .managed_tasks
            .len()
            == 2
    })
    .await;
    let task = app
        .world()
        .get::<DemeterCastle>(owner)
        .unwrap()
        .managed_tasks[0]["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    engine
        .act(
            engine::actions::Action::SetQuantityExact {
                target: task,
                amount: "0".into(),
            },
            None,
        )
        .await
        .unwrap();
    Operation::Refresh.apply(app.world_mut(), owner);
    crate::native::sand_panel::tests::settle(&mut app, |world| {
        world
            .get::<DemeterCastle>(owner)
            .unwrap()
            .managed_tasks
            .iter()
            .any(|t| t["state"] == "completed")
    })
    .await;
}
