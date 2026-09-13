use super::*;
use crate::area::{AreaShape, Property, PropertyRule, spawn_area};
use serde_json::json;

fn fixture() -> (World, Entity, Entity, Entity) {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0));
    area.strength = 100.0;
    area.reach.mode = ReachMode::Unlimited;
    area.rules.push(PropertyRule {
        property: Property::Quantity,
        value: "-3".into(),
    });
    let owner = spawn_area(&mut world, root, 1, area).unwrap();
    let sand = world
        .spawn((
            CanvasItem {
                position: DVec2::new(100.0, 0.0),
                size: Vec2::splat(20.0),
            },
            RecordProperties(json!({"uid":"r_one", "quantity":-3})),
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    (world, root, owner, sand)
}

#[cfg_attr(test, test)]
fn simple_forces_reuse_destinations_during_motion_and_invalidate_for_changes() {
    let (mut world, root, owner, sand) = fixture();
    update(&mut world);
    let record = world.get::<RecordProperties>(sand).unwrap().clone();
    let mut runtime = world.resource_mut::<Influences>();
    let evaluated = runtime.evaluations;
    for i in 0..1000 {
        let point = DVec2::new(-100.0, i as f64);
        let (_, total) = runtime.forces(sand, root, 1, point, Some(&record), None);
        assert!((total - -point.normalize() * 100.0).length() < 1e-10);
        assert_eq!(runtime.cache[&sand].simple[&owner].point, DVec2::ZERO);
    }
    assert_eq!(runtime.evaluations, evaluated);
    assert_eq!(
        runtime.total(sand, root, 1, DVec2::ZERO, Some(&record), None),
        DVec2::ZERO
    );
    assert_eq!(
        runtime.total(sand, root, 1, DVec2::X, Some(&record), None),
        -DVec2::X * 100.0
    );
    world.get_mut::<InfluenceArea>(owner).unwrap().strength = 250.0;
    update(&mut world);
    assert_eq!(
        world.get::<AreaForces>(sand).unwrap().total(),
        DVec2::new(-250.0, 0.0)
    );
    world.get_mut::<RecordProperties>(sand).unwrap().0["quantity"] = json!(4);
    update(&mut world);
    assert_eq!(world.get::<AreaForces>(sand).unwrap().total(), DVec2::ZERO);
    world.despawn(sand);
    refresh(&mut world);
    assert!(world.resource::<Influences>().cache.is_empty());
}

#[cfg_attr(test, test)]
fn combined_simple_destinations_keep_their_individual_strengths() {
    let (mut world, root, owner, sand) = fixture();
    world.get_mut::<InfluenceArea>(owner).unwrap().center = [-100.0, 0.0];
    let mut other = InfluenceArea::new(
        AreaShape::Square,
        DVec2::new(100.0, 0.0),
        DVec2::splat(100.0),
    );
    other.strength = 100.0;
    other.reach.mode = ReachMode::Unlimited;
    other.rules = world.get::<InfluenceArea>(owner).unwrap().rules.clone();
    spawn_area(&mut world, root, 1, other).unwrap();
    world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::ZERO;
    update(&mut world);
    let record = world.get::<RecordProperties>(sand).unwrap().clone();
    let mut runtime = world.resource_mut::<Influences>();
    let evaluations = runtime.evaluations;
    assert_eq!(
        runtime.total(sand, root, 1, DVec2::ZERO, Some(&record), None),
        DVec2::ZERO
    );
    let total = runtime.total(sand, root, 1, DVec2::new(0.0, 100.0), Some(&record), None);
    assert!(total.x.abs() < 1e-10);
    assert!((total.y + 100.0 * 2.0_f64.sqrt()).abs() < 1e-10);
    assert_eq!(runtime.evaluations, evaluations);
    assert_eq!(runtime.cache[&sand].simple.len(), 2);
}

#[cfg_attr(test, test)]
fn newtonian_force_uses_distance_while_simple_reach_still_stops_at_the_boundary() {
    let (mut world, root, owner, sand) = fixture();
    world.get_mut::<InfluenceArea>(owner).unwrap().force_mode = ForceMode::Newtonian;
    update(&mut world);
    assert_eq!(
        world.get::<AreaForces>(sand).unwrap().total(),
        DVec2::new(-25.0, 0.0)
    );
    world.get_mut::<CanvasItem>(sand).unwrap().position.x = 200.0;
    update(&mut world);
    assert_eq!(
        world.get::<AreaForces>(sand).unwrap().total(),
        DVec2::new(-6.25, 0.0)
    );
    {
        let mut area = world.get_mut::<InfluenceArea>(owner).unwrap();
        area.force_mode = ForceMode::Simple;
        area.reach.mode = ReachMode::Limited;
    }
    world.get_mut::<CanvasItem>(sand).unwrap().position.x = 20.0;
    update(&mut world);
    let record = world.get::<RecordProperties>(sand).unwrap().clone();
    assert_eq!(
        world
            .resource_mut::<Influences>()
            .forces(sand, root, 1, DVec2::new(60.0, 0.0), Some(&record), None)
            .1,
        DVec2::ZERO
    );
    assert_eq!(
        world
            .resource_mut::<Influences>()
            .forces(sand, root, 1, DVec2::new(-20.0, 0.0), Some(&record), None)
            .1,
        DVec2::new(100.0, 0.0)
    );
}

#[cfg_attr(test, test)]
fn moving_targets_invalidates_destinations_and_newtonian_distance_uses_the_target() {
    let (mut world, _, owner, sand) = fixture();
    update(&mut world);
    world.get_mut::<InfluenceArea>(owner).unwrap().target =
        crate::area::AttractionTarget::Point([200.0, 0.0]);
    update(&mut world);
    assert_eq!(
        world.get::<AreaForces>(sand).unwrap().total(),
        DVec2::new(100.0, 0.0)
    );
    world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::new(300.0, 0.0);
    update(&mut world);
    assert_eq!(
        world.get::<AreaForces>(sand).unwrap().total(),
        DVec2::new(-100.0, 0.0)
    );
    world.get_mut::<InfluenceArea>(owner).unwrap().force_mode = ForceMode::Newtonian;
    update(&mut world);
    assert_eq!(
        world.get::<AreaForces>(sand).unwrap().total(),
        DVec2::new(-25.0, 0.0)
    );
    world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::new(200.0, 100.0);
    update(&mut world);
    assert_eq!(
        world.get::<AreaForces>(sand).unwrap().total(),
        DVec2::new(0.0, -25.0)
    );
    world.get_mut::<InfluenceArea>(owner).unwrap().reach.mode = ReachMode::Limited;
    update(&mut world);
    assert_eq!(world.get::<AreaForces>(sand).unwrap().total(), DVec2::ZERO);
    world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::ZERO;
    update(&mut world);
    assert_eq!(
        world.get::<AreaForces>(sand).unwrap().total(),
        DVec2::new(6.25, 0.0)
    );
}

#[cfg_attr(test, test)]
fn immunity_scopes_use_centers_and_do_not_cross_workspaces_or_disable_other_shields() {
    let (mut world, root, owner, sand) = fixture();
    let mut shield = InfluenceArea::new(
        AreaShape::Circle,
        DVec2::new(100.0, 0.0),
        DVec2::splat(80.0),
    );
    shield.immunity = Immunity::External;
    let shield = spawn_area(&mut world, root, 1, shield).unwrap();
    for (mode, expected) in [
        (Immunity::External, 0.0),
        (Immunity::Internal, -100.0),
        (Immunity::All, 0.0),
        (Immunity::None, -100.0),
    ] {
        world.get_mut::<InfluenceArea>(shield).unwrap().immunity = mode;
        update(&mut world);
        assert_eq!(world.get::<AreaForces>(sand).unwrap().total().x, expected);
    }
    world.get_mut::<InfluenceArea>(shield).unwrap().immunity = Immunity::All;
    world.get_mut::<WorkspaceMember>(shield).unwrap().0 = 2;
    update(&mut world);
    assert_eq!(world.get::<AreaForces>(sand).unwrap().total().x, -100.0);
    world.get_mut::<WorkspaceMember>(shield).unwrap().0 = 1;
    world.get_mut::<InfluenceArea>(owner).unwrap().immunity = Immunity::All;
    update(&mut world);
    assert_eq!(world.get::<AreaForces>(sand).unwrap().total(), DVec2::ZERO);
    let record = world.get::<RecordProperties>(sand).unwrap().clone();
    assert!(blocked(
        &mut world,
        root,
        1,
        owner,
        DVec2::new(100.0, 0.0),
        Some(&record),
        None
    ));
    world.get_mut::<InfluenceArea>(shield).unwrap().filter = Some(Default::default());
    world.entity_mut(shield).insert(Matches {
        source: Source::Organ("remote".into()),
        uids: ["r_one".into()].into(),
        current: true,
    });
    update(&mut world);
    assert_eq!(world.get::<AreaForces>(sand).unwrap().total().x, -100.0);
}

#[cfg_attr(test, test)]
fn size_effects_combine_restore_and_leave_authored_sizes_untouched() {
    let (mut world, root, owner, sand) = fixture();
    world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::ZERO;
    world.get_mut::<InfluenceArea>(owner).unwrap().scale = 0.5;
    let mut second = world.get::<InfluenceArea>(owner).unwrap().clone();
    second.id = "f".repeat(32);
    second.scale = 3.0;
    let second = spawn_area(&mut world, root, 1, second).unwrap();
    update(&mut world);
    assert_eq!(world.get::<AreaScale>(sand), Some(&AreaScale(1.5)));
    for _ in 0..100 {
        update(&mut world);
    }
    assert_eq!(
        world.get::<CanvasItem>(sand).unwrap().size,
        Vec2::splat(20.0)
    );
    assert_eq!(world.get::<AreaScale>(sand), Some(&AreaScale(1.5)));
    world.get_mut::<CanvasItem>(sand).unwrap().size = Vec2::splat(40.0);
    world.despawn(second);
    update(&mut world);
    assert_eq!(world.get::<AreaScale>(sand), Some(&AreaScale(0.5)));
    world.get_mut::<CanvasItem>(sand).unwrap().position.x = 100.0;
    update(&mut world);
    assert!(world.get::<AreaScale>(sand).is_none());
    assert_eq!(
        world.get::<CanvasItem>(sand).unwrap().size,
        Vec2::splat(40.0)
    );
    world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::ZERO;
    world.entity_mut(sand).insert(Pinned {
        anchor: [0.5; 2],
        scale: 1.0,
    });
    update(&mut world);
    assert!(world.get::<AreaScale>(sand).is_none());
    world.entity_mut(sand).remove::<Pinned>();
    world.get_mut::<InfluenceArea>(owner).unwrap().scale = 20.0;
    update(&mut world);
    assert_eq!(world.get::<AreaScale>(sand), Some(&AreaScale(20.0)));
    world.despawn(owner);
    update(&mut world);
    assert!(world.get::<AreaScale>(sand).is_none());
}

#[cfg_attr(test, test)]
fn independent_sorting_steers_existing_sands_and_immunity_stops_it() {
    let (mut world, root, owner, sand) = fixture();
    {
        let mut area = world.get_mut::<InfluenceArea>(owner).unwrap();
        area.strength = 0.0;
        area.sorting = Some(Sorting::default());
    }
    world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::ZERO;
    let other = world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(20.0),
            },
            RecordProperties(json!({"uid":"r_two", "quantity":-3})),
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    update(&mut world);
    assert_eq!(world.query::<&CanvasItem>().iter(&world).count(), 3);
    assert!(world.get::<AreaForces>(sand).unwrap().total().y < 0.0);
    assert!(world.get::<AreaForces>(other).unwrap().total().y > 0.0);
    let before = world.resource::<Influences>().fields[0].targets[&sand];
    world
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .sorting
        .as_mut()
        .unwrap()
        .reverse = true;
    update(&mut world);
    assert!(world.get::<AreaForces>(sand).unwrap().total().y > 0.0);
    assert_ne!(
        world.resource::<Influences>().fields[0].targets[&sand],
        before
    );
    let mut shield = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0));
    shield.immunity = Immunity::All;
    shield.scale = 0.5;
    spawn_area(&mut world, root, 1, shield).unwrap();
    update(&mut world);
    assert_eq!(world.get::<AreaForces>(sand).unwrap().total(), DVec2::ZERO);
    assert_eq!(world.get::<AreaScale>(sand), Some(&AreaScale(0.5)));
}

crate::laboratory_cases! {
    combined_simple_destinations_keep_their_individual_strengths,
    moving_targets_invalidates_destinations_and_newtonian_distance_uses_the_target,
    simple_forces_reuse_destinations_during_motion_and_invalidate_for_changes,
    newtonian_force_uses_distance_while_simple_reach_still_stops_at_the_boundary,
    immunity_scopes_use_centers_and_do_not_cross_workspaces_or_disable_other_shields,
    size_effects_combine_restore_and_leave_authored_sizes_untouched,
    independent_sorting_steers_existing_sands_and_immunity_stops_it,
}
