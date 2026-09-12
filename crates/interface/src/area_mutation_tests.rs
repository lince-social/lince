use super::*;
use crate::area::{AreaShape, Property, PropertyRule};
use std::sync::Arc;

async fn fixture() -> (App, Arc<engine::Engine>, Entity, Entity, Entity, String) {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Work".into(),
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
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: "working".into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    let runtime = cell::CellRuntime {
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
    app.add_plugins((
        MinimalPlugins,
        crate::cell_bridge::CellBridgePlugin,
        AreaMutationPlugin,
    ))
    .insert_resource(crate::app::CellHandle(runtime))
    .insert_resource(crate::wake::WakeSignal::new(|| {}));
    let root = app.world_mut().spawn(Workspaces::default()).id();
    let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0));
    area.rules = vec![PropertyRule {
        property: Property::Quantity,
        value: "0".into(),
    }];
    area.changes.enter = RecordChanges {
        quantity: Some("-3".into()),
        assert: vec!["working".into()],
        retract: vec![],
    };
    area.changes.leave = RecordChanges {
        quantity: Some("1".into()),
        assert: vec![],
        retract: vec!["working".into()],
    };
    let area = crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    let sand = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::new(100.0, 0.0),
                size: Vec2::splat(20.0),
            },
            RecordProperties(serde_json::json!({"uid": uid, "quantity":0})),
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    app.update();
    (app, engine, root, area, sand, uid)
}

fn enable(app: &mut App, root: Entity, area: Entity) {
    preview(app.world_mut(), root, area);
    assert!(previewed(app.world(), area));
    arm(app.world_mut(), root, area);
    assert!(armed(app.world(), area));
}

fn move_to(app: &mut App, sand: Entity, x: f64) {
    app.world_mut()
        .get_mut::<CanvasItem>(sand)
        .unwrap()
        .position
        .x = x;
}

async fn pump(app: &mut App) {
    for _ in 0..100 {
        app.update();
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        if app.world().resource::<Mutations>().pending.is_empty() {
            app.update();
            if app.world().resource::<Mutations>().pending.is_empty() {
                return;
            }
        }
    }
    panic!("Area change did not complete");
}

async fn quantity(engine: &engine::Engine, uid: &str) -> String {
    engine
        .act(
            Action::PreviewAreaTransition {
                target: uid.into(),
                changes: RecordChanges {
                    quantity: Some("0".into()),
                    ..Default::default()
                },
                constraints: Default::default(),
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap()["expected"]["quantity"]
        .as_str()
        .unwrap()
        .into()
}

#[cfg_attr(test, tokio::test)]
async fn entry_and_exit_change_real_records_and_property_changes_do_not_fake_an_exit() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    enable(&mut app, root, area);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    move_to(&mut app, sand, 100.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "1");
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
    app.world_mut().get_mut::<RecordProperties>(sand).unwrap().0["quantity"] =
        serde_json::json!(-3);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
    move_to(&mut app, sand, 100.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "1");
}

#[cfg_attr(test, tokio::test)]
async fn copies_count_together_and_exit_waits_until_the_last_copy_leaves() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    let copy = app
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::new(100.0, 0.0),
                size: Vec2::splat(20.0),
            },
            RecordProperties(serde_json::json!({"uid":uid,"quantity":0})),
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    enable(&mut app, root, area);
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
    move_to(&mut app, copy, 0.0);
    move_to(&mut app, sand, 100.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
    move_to(&mut app, copy, 100.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "1");
}

#[cfg_attr(test, tokio::test)]
async fn overlap_conflicts_disarm_without_writing_and_equal_changes_coalesce() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    let mut other = app.world().get::<InfluenceArea>(area).unwrap().clone();
    other.id = "11111111111111111111111111111111".into();
    other.changes.enter.quantity = Some("-4".into());
    let other = crate::area::spawn_area(app.world_mut(), root, 1, other).unwrap();
    enable(&mut app, root, area);
    enable(&mut app, root, other);
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert!(!armed(app.world(), area));
    assert!(!armed(app.world(), other));
    assert_eq!(quantity(&engine, &uid).await, "0");
    move_to(&mut app, sand, 100.0);
    app.world_mut()
        .get_mut::<InfluenceArea>(other)
        .unwrap()
        .changes
        .enter
        .quantity = Some("-3".into());
    enable(&mut app, root, area);
    enable(&mut app, root, other);
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
    assert_eq!(
        app.world().resource::<Mutations>().grants[&area].remaining,
        127
    );
    assert_eq!(
        app.world().resource::<Mutations>().grants[&other].remaining,
        127
    );
    move_to(&mut app, sand, 200.0);
    app.update();
    disarm(app.world_mut(), area, "Disarmed shared request");
    assert!(!armed(app.world(), other));
    assert_eq!(
        app.world().get::<MutationStatus>(other).unwrap().0,
        "Disarmed shared request"
    );
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
}

#[cfg_attr(test, tokio::test)]
async fn edits_switches_and_disarming_cancel_unsubmitted_changes() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    enable(&mut app, root, area);
    move_to(&mut app, sand, 0.0);
    app.update();
    assert!(!app.world().resource::<Mutations>().pending.is_empty());
    disarm_all(app.world_mut(), root);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    enable(&mut app, root, area);
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .center[0] = 200.0;
    pump(&mut app).await;
    assert!(!armed(app.world(), area));
    assert_eq!(quantity(&engine, &uid).await, "0");
    enable(&mut app, root, area);
    app.world_mut().get_mut::<Workspaces>(root).unwrap().active = 2;
    pump(&mut app).await;
    assert!(!armed(app.world(), area));
}

#[cfg_attr(test, tokio::test)]
async fn newly_arriving_records_and_pinned_sands_do_not_trigger_changes() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    app.world_mut().entity_mut(sand).insert(Pinned {
        anchor: [0.5; 2],
        scale: 1.0,
    });
    enable(&mut app, root, area);
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    app.world_mut().entity_mut(sand).remove::<Pinned>();
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    let saved = serde_json::to_value(app.world().get::<InfluenceArea>(area).unwrap()).unwrap();
    assert!(saved.get("armed").is_none());
    let restored: InfluenceArea = serde_json::from_value(saved).unwrap();
    let copy = crate::area::spawn_area(app.world_mut(), root, 1, restored).unwrap();
    assert!(!armed(app.world(), copy));
}

#[cfg_attr(test, tokio::test)]
async fn entering_a_conflicting_area_later_keeps_the_first_change() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    let mut other = app.world().get::<InfluenceArea>(area).unwrap().clone();
    other.id = "22222222222222222222222222222222".into();
    other.center[0] = 50.0;
    other.changes.enter.quantity = Some("-4".into());
    let other = crate::area::spawn_area(app.world_mut(), root, 1, other).unwrap();
    move_to(&mut app, sand, 200.0);
    enable(&mut app, root, area);
    enable(&mut app, root, other);
    move_to(&mut app, sand, -25.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
    move_to(&mut app, sand, 25.0);
    pump(&mut app).await;
    assert!(!armed(app.world(), area));
    assert!(!armed(app.world(), other));
    assert_eq!(quantity(&engine, &uid).await, "-3");
}

    crate::laboratory_cases! {
        async entry_and_exit_change_real_records_and_property_changes_do_not_fake_an_exit,
        async copies_count_together_and_exit_waits_until_the_last_copy_leaves,
        async overlap_conflicts_disarm_without_writing_and_equal_changes_coalesce,
        async edits_switches_and_disarming_cancel_unsubmitted_changes,
        async newly_arriving_records_and_pinned_sands_do_not_trigger_changes,
        async entering_a_conflicting_area_later_keeps_the_first_change,
    }
