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
        fiote: None,
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
    area.changes_enabled = false;
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
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .changes_enabled = true;
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

#[cfg(test)]
#[tokio::test]
async fn appearance_changes_keep_record_transitions_armed() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    enable(&mut app, root, area);
    {
        let mut area = app.world_mut().get_mut::<InfluenceArea>(area).unwrap();
        area.color = [255, 128, 0];
        area.opacity = 0.4;
    }
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert!(armed(app.world(), area));
    assert_eq!(quantity(&engine, &uid).await, "-3");
}

#[cfg(test)]
#[tokio::test]
async fn grant_survives_more_than_128_successful_crossings() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    enable(&mut app, root, area);
    for _ in 0..70 {
        move_to(&mut app, sand, 0.0);
        pump(&mut app).await;
        assert_eq!(quantity(&engine, &uid).await, "-3");
        move_to(&mut app, sand, 100.0);
        pump(&mut app).await;
        assert_eq!(quantity(&engine, &uid).await, "1");
        assert!(armed(app.world(), area));
    }
}

#[cfg(test)]
#[tokio::test]
async fn bulk_crossings_wait_for_capacity_without_losing_changes_or_disarming() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    let mut records = vec![(sand, uid)];
    for _ in 0..69 {
        let uid = engine
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "Bulk task".into(),
                    body: String::new(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let sand = app
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
        records.push((sand, uid));
    }
    enable(&mut app, root, area);
    let sender = app
        .world()
        .get_non_send::<CellBridge>()
        .unwrap()
        .outgoing
        .clone();
    let mut reserved = Vec::new();
    while let Ok(permit) = sender.clone().try_reserve_owned() {
        reserved.push(permit);
    }
    for (sand, _) in &records {
        move_to(&mut app, *sand, 0.0);
    }
    app.update();
    assert!(armed(app.world(), area));
    drop(reserved);
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            app.update();
            assert!(armed(app.world(), area));
            let state = app.world().resource::<Mutations>();
            if state.pending.is_empty()
                && state.grants[&area]
                    .visits
                    .values()
                    .all(|visit| visit.inside)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    for (_, uid) in records {
        assert_eq!(quantity(&engine, &uid).await, "-3");
    }
}

#[cfg_attr(test, tokio::test)]
async fn new_areas_and_force_configuration_cannot_change_records() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    let configured = app.world().get::<InfluenceArea>(area).unwrap().clone();
    *app.world_mut().get_mut::<InfluenceArea>(area).unwrap() =
        InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0));
    for configured_force in [false, true] {
        if configured_force {
            let mut current = app.world_mut().get_mut::<InfluenceArea>(area).unwrap();
            current.strength = 400.0;
            current.rules = configured.rules.clone();
        }
        preview(app.world_mut(), root, area);
        arm(app.world_mut(), root, area);
        assert!(!previewed(app.world(), area));
        assert!(!armed(app.world(), area));
        for x in [0.0, 100.0, 0.0, 100.0] {
            move_to(&mut app, sand, x);
            pump(&mut app).await;
            assert_eq!(quantity(&engine, &uid).await, "0");
            assert!(app.world().resource::<Mutations>().pending.is_empty());
        }
    }
}

#[cfg_attr(test, tokio::test)]
async fn movement_reach_does_not_expand_record_change_boundaries() {
    for mode in [
        crate::area::ReachMode::Limited,
        crate::area::ReachMode::Unlimited,
    ] {
        let (mut app, engine, root, area, sand, uid) = fixture().await;
        {
            let mut area = app.world_mut().get_mut::<InfluenceArea>(area).unwrap();
            area.reach.mode = mode;
            area.reach.radius = 500.0;
            area.strength = 100.0;
            area.target = crate::area::AttractionTarget::Point([400.0, 0.0]);
        }
        enable(&mut app, root, area);
        for x in [400.0, 200.0, 75.0] {
            move_to(&mut app, sand, x);
            pump(&mut app).await;
            assert_eq!(quantity(&engine, &uid).await, "0");
        }
        move_to(&mut app, sand, 0.0);
        pump(&mut app).await;
        assert_eq!(quantity(&engine, &uid).await, "-3");
        move_to(&mut app, sand, 75.0);
        pump(&mut app).await;
        assert_eq!(quantity(&engine, &uid).await, "1");
    }
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
    assert!(armed(app.world(), area));
    assert!(armed(app.world(), other));
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
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .changes_enabled = false;
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    enable(&mut app, root, area);
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .center[0] = 200.0;
    pump(&mut app).await;
    assert!(armed(app.world(), area));
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
    async depth_only_crossings_change_records_in_a_rotated_area,
    async immunity_suppresses_record_transitions_and_cancels_pending_previews,
    async protein_filters_control_crossings_and_exit_restores_after_the_record_stops_matching,
    async movement_reach_does_not_expand_record_change_boundaries,
    async new_areas_and_force_configuration_cannot_change_records,
    async entry_and_exit_change_real_records_and_property_changes_do_not_fake_an_exit,
    async copies_count_together_and_exit_waits_until_the_last_copy_leaves,
    async overlap_conflicts_disarm_without_writing_and_equal_changes_coalesce,
    async edits_switches_and_disarming_cancel_unsubmitted_changes,
    async newly_arriving_records_and_pinned_sands_do_not_trigger_changes,
    async entering_a_conflicting_area_later_keeps_the_first_change,
}

#[cfg_attr(test, tokio::test)]
async fn depth_only_crossings_change_records_in_a_rotated_area() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .depth = 20.0;
    let placement = crate::topology::Spatial {
        elevation: 50.0,
        depth: Some(20.0),
        rotation: bevy::math::DQuat::from_rotation_z(0.7).to_array(),
        ..default()
    };
    app.world_mut().entity_mut(area).insert(placement);
    let point =
        |y| placement.position(DVec2::ZERO) + placement.rotation() * DVec3::new(0.0, y, 0.0);
    crate::topology::set_position(app.world_mut(), sand, point(-21.0));
    enable(&mut app, root, area);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    crate::topology::set_position(app.world_mut(), sand, point(-10.0));
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
    crate::topology::set_position(app.world_mut(), sand, point(1.0));
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "1");
}

#[cfg_attr(test, tokio::test)]
async fn protein_filters_control_crossings_and_exit_restores_after_the_record_stops_matching() {
    use crate::protein_area::{Config, ProteinAreaPlugin, filter::Matches};
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    app.add_plugins(ProteinAreaPlugin);
    let mut config = Config {
        enabled: true,
        bindings: vec![],
        ..default()
    };
    config.draft.query["where"] = serde_json::json!([{"all":[{"uid_eq":uid},{"quantity_eq":"0"}]}]);
    {
        let mut current = app.world_mut().get_mut::<InfluenceArea>(area).unwrap();
        current.rules.clear();
        current.filter = Some(config);
    }
    for _ in 0..200 {
        app.update();
        if app
            .world()
            .get::<Matches>(area)
            .is_some_and(|m| m.uids.contains(&uid))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        app.world()
            .get::<Matches>(area)
            .unwrap()
            .uids
            .contains(&uid)
    );
    enable(&mut app, root, area);
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
    for _ in 0..200 {
        app.update();
        if app
            .world()
            .get::<Matches>(area)
            .is_some_and(|m| m.uids.is_empty())
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(armed(app.world(), area));
    assert!(app.world().get::<Matches>(area).unwrap().uids.is_empty());
    move_to(&mut app, sand, 100.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "1");
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .filter
        .as_mut()
        .unwrap()
        .enabled = false;
    app.update();
    assert!(!armed(app.world(), area));
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "1");
}

#[cfg_attr(test, tokio::test)]
async fn immunity_suppresses_record_transitions_and_cancels_pending_previews() {
    let (mut app, engine, root, area, sand, uid) = fixture().await;
    let mut shield = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(300.0));
    shield.immunity = crate::area_effects::Immunity::All;
    let shield = crate::area::spawn_area(app.world_mut(), root, 1, shield).unwrap();
    enable(&mut app, root, area);
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    move_to(&mut app, sand, 100.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    app.world_mut()
        .get_mut::<InfluenceArea>(shield)
        .unwrap()
        .immunity = crate::area_effects::Immunity::None;
    move_to(&mut app, sand, 0.0);
    app.update();
    assert!(!app.world().resource::<Mutations>().pending.is_empty());
    app.world_mut()
        .get_mut::<InfluenceArea>(shield)
        .unwrap()
        .immunity = crate::area_effects::Immunity::All;
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "0");
    assert!(!armed(app.world(), area));
    app.world_mut().despawn(shield);
    move_to(&mut app, sand, 100.0);
    app.update();
    enable(&mut app, root, area);
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
}

#[cfg(test)]
#[tokio::test]
async fn configured_changes_start_automatically_and_area_switch_stops_writes() {
    let (mut app, engine, _, area, sand, uid) = fixture().await;
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .changes_enabled = true;
    pump(&mut app).await;
    assert!(armed(app.world(), area));
    move_to(&mut app, sand, 0.0);
    pump(&mut app).await;
    assert_eq!(quantity(&engine, &uid).await, "-3");
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .enabled = false;
    pump(&mut app).await;
    move_to(&mut app, sand, 100.0);
    pump(&mut app).await;
    assert!(!armed(app.world(), area));
    assert_eq!(quantity(&engine, &uid).await, "-3");
    app.world_mut()
        .get_mut::<InfluenceArea>(area)
        .unwrap()
        .enabled = true;
    pump(&mut app).await;
    assert!(armed(app.world(), area));
    assert_eq!(quantity(&engine, &uid).await, "-3");
}
