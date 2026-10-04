use super::*;
use crate::{
    actions::Action,
    area::{AreaForces, AreaShape, Property, PropertyRule, RecordProperties},
    canvas::CanvasItem,
};
use bevy::math::DVec2;

fn fixture() -> (World, Entity, Entity, Entity) {
    let mut world = World::new();
    let root = world
        .spawn((
            Workspaces::default(),
            crate::canvas_selection::SandSelection::default(),
        ))
        .id();
    let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0));
    area.name = "Pull".into();
    area.strength = 100.0;
    area.reach.mode = crate::area::ReachMode::Unlimited;
    area.rules.push(PropertyRule {
        property: Property::Quantity,
        value: "-1".into(),
    });
    let area = crate::area::spawn_area(&mut world, root, 1, area).unwrap();
    let sand = world
        .spawn((
            CanvasItem {
                position: DVec2::new(100.0, 0.0),
                size: Vec2::splat(20.0),
            },
            ChildOf(root),
            WorkspaceMember(1),
            RecordProperties(serde_json::json!({"uid":"r_test", "quantity":-1})),
        ))
        .id();
    world
        .get_mut::<crate::canvas_selection::SandSelection>(root)
        .unwrap()
        .0 = vec![sand];
    track(&mut world);
    (world, root, area, sand)
}

#[cfg_attr(test, test)]
fn explanation_matches_forces_and_names_suppressed_rules() {
    let (mut world, root, area, sand) = fixture();
    crate::area_effects::update(&mut world);
    let report = world.get::<Report>(sand).unwrap();
    assert_eq!(
        report.total.x,
        world.get::<AreaForces>(sand).unwrap().total().x
    );
    assert!(ui::describe(&world, root, sand, None).contains("Pull"));
    let mut shield = InfluenceArea::new(
        AreaShape::Square,
        DVec2::new(100.0, 0.0),
        DVec2::splat(50.0),
    );
    shield.name = "Shield".into();
    shield.immunity = crate::area_effects::Immunity::External;
    let shield = crate::area::spawn_area(&mut world, root, 1, shield).unwrap();
    crate::area_effects::update(&mut world);
    assert_eq!(
        world
            .get::<Report>(sand)
            .unwrap()
            .entries
            .iter()
            .find(|e| e.area == area)
            .unwrap()
            .motion,
        Outcome::Immune(shield)
    );
    assert!(ui::describe(&world, root, sand, None).contains("blocked by immunity from Shield"));
    assert_eq!(world.get::<AreaForces>(sand).unwrap().total(), DVec2::ZERO);
    Control::Area(shield).apply(&mut world, root);
    crate::area_effects::update(&mut world);
    assert!(world.get::<Report>(sand).unwrap().total.x < 0.0);
    assert!(world.get::<InfluenceArea>(shield).unwrap().paused);
    world.get_mut::<RecordProperties>(sand).unwrap().0["quantity"] = serde_json::json!(1);
    crate::area_effects::update(&mut world);
    assert_eq!(
        world
            .get::<Report>(sand)
            .unwrap()
            .entries
            .iter()
            .find(|e| e.area == area)
            .unwrap()
            .motion,
        Outcome::Filter
    );
}

#[cfg_attr(test, test)]
fn pins_and_workspace_pause_stop_effects_and_cannot_modify_other_workspaces() {
    let (mut world, root, area, sand) = fixture();
    world.get_mut::<InfluenceArea>(area).unwrap().sorting =
        Some(crate::area_effects::Sorting::default());
    world.get_mut::<InfluenceArea>(area).unwrap().scale = 2.0;
    world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::new(20.0, 0.0);
    crate::area_effects::update(&mut world);
    assert!(world.get::<Report>(sand).unwrap().entries[0].slot.is_some());
    assert_eq!(world.get::<Report>(sand).unwrap().scale, 2.0);
    world
        .entity_mut(sand)
        .insert(crate::sand_placement::Pinned {
            anchor: [0.5, 0.5],
            scale: 1.0,
        });
    crate::area_effects::update(&mut world);
    assert_eq!(world.get::<Report>(sand).unwrap().total, DVec3::ZERO);
    assert_eq!(
        world.get::<Report>(sand).unwrap().entries[0].motion,
        Outcome::Pinned
    );
    world
        .entity_mut(sand)
        .remove::<crate::sand_placement::Pinned>();
    Control::Workspace.apply(&mut world, root);
    crate::area_effects::update(&mut world);
    assert_eq!(world.get::<Report>(sand).unwrap().total, DVec3::ZERO);
    assert_eq!(world.get::<Report>(sand).unwrap().scale, 1.0);
    assert_eq!(
        world.get::<Report>(sand).unwrap().entries[0].motion,
        Outcome::Paused
    );
    Control::Workspace.apply(&mut world, root);
    crate::area_effects::update(&mut world);
    assert!(world.get::<Report>(sand).unwrap().total.length() > 0.0);
    world.get_mut::<WorkspaceMember>(area).unwrap().0 = 2;
    Control::Area(area).apply(&mut world, root);
    assert!(!world.get::<InfluenceArea>(area).unwrap().paused);
    track(&mut world);
    assert!(tracked(&world, sand));
    world.get_mut::<Workspaces>(root).unwrap().active = 2;
    track(&mut world);
    assert!(!tracked(&world, sand));
    assert!(world.get::<Report>(sand).is_none());
}

#[cfg_attr(test, test)]
fn spatial_explanation_keeps_full_force_vectors_and_paused_immunity() {
    let (mut world, root, area, sand) = fixture();
    world
        .entity_mut(root)
        .insert(crate::topology::presentation::SpatialRoot);
    world.entity_mut(area).insert(crate::topology::Spatial {
        elevation: 50.0,
        ..default()
    });
    crate::area_effects::refresh(&mut world);
    crate::topology::influence::update(&mut world);
    assert_eq!(
        world.get::<Report>(sand).unwrap().total,
        world
            .resource::<crate::topology::influence::Forces>()
            .totals[&sand]
    );
    assert!(world.get::<Report>(sand).unwrap().total.y > 0.0);
    Control::Area(area).apply(&mut world, root);
    crate::area_effects::refresh(&mut world);
    crate::topology::influence::update(&mut world);
    assert_eq!(world.get::<Report>(sand).unwrap().total, DVec3::ZERO);
    assert_eq!(
        world.get::<Report>(sand).unwrap().entries[0].motion,
        Outcome::Paused
    );
}

#[cfg_attr(test, test)]
fn live_panels_reuse_evaluation_and_preserve_controls_while_sands_move() {
    let (mut world, root, area, sand) = fixture();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    let parent = world.spawn_empty().id();
    ui::panel(&mut world, root, parent, None);
    crate::area_effects::update(&mut world);
    ui::update(&mut world);
    let buttons: Vec<_> = world
        .query_filtered::<Entity, With<crate::actions::ActionButton>>()
        .iter(&world)
        .collect();
    world.get_mut::<CanvasItem>(sand).unwrap().position.x += 2.0;
    crate::area_effects::update(&mut world);
    ui::update(&mut world);
    for button in buttons {
        assert!(world.get_entity(button).is_ok());
    }
    let filtered = ui::describe(&world, root, sand, Some(area));
    assert!(filtered.contains("Pull"));
    assert!(
        world
            .query::<&Text>()
            .iter(&world)
            .any(|text| text.0 == ui::describe(&world, root, sand, None))
    );
}

crate::laboratory_cases! {
    explanation_matches_forces_and_names_suppressed_rules,
    pins_and_workspace_pause_stop_effects_and_cannot_modify_other_workspaces,
    spatial_explanation_keeps_full_force_vectors_and_paused_immunity,
    live_panels_reuse_evaluation_and_preserve_controls_while_sands_move,
}
