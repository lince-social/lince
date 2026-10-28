use super::*;

#[test]
fn exclusion_follows_layout_ecs_and_visual_ownership_without_changing_public_siblings() {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    let mut layout = LayoutBox::new(Vec2::splat(100.0));
    layout.capture_excluded = true;
    let private = world.spawn((layout, ChildOf(root))).id();
    let child = world.spawn(ChildOf(private)).id();
    let mut linked = LayoutBox::new(Vec2::splat(100.0));
    linked.parent = Some(layout.id);
    let linked = world.spawn((linked, LayoutRuntime { parent: Some(private), ..default() }, ChildOf(root))).id();
    let mirror = world.spawn((crate::native::topology::presentation::VisualOwner(child), ChildOf(root))).id();
    let public = world.spawn((LayoutBox::new(Vec2::splat(100.0)), ChildOf(root))).id();
    for entity in [private, child, linked, mirror] { assert!(excluded(&world, entity)); }
    assert!(!excluded(&world, public));
    world.get_mut::<LayoutBox>(private).unwrap().capture_excluded = false;
    for entity in [private, child, linked, mirror, public] { assert!(!excluded(&world, entity)); }
}

#[test]
fn unresolved_or_cyclic_ownership_is_private() {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    let a = world.spawn((LayoutBox::new(Vec2::splat(100.0)), ChildOf(root))).id();
    let b = world.spawn((LayoutBox::new(Vec2::splat(100.0)), ChildOf(root))).id();
    let b_id = world.get::<LayoutBox>(b).unwrap().id;
    let a_id = world.get::<LayoutBox>(a).unwrap().id;
    world.get_mut::<LayoutBox>(a).unwrap().parent = Some(b_id);
    assert!(excluded(&world, a));
    world.entity_mut(a).insert(LayoutRuntime { parent: Some(b), ..default() });
    assert!(!excluded(&world, a));
    world.get_mut::<LayoutBox>(b).unwrap().parent = Some(a_id);
    world.entity_mut(b).insert(LayoutRuntime { parent: Some(a), ..default() });
    assert!(excluded(&world, a));
    assert!(excluded(&world, b));
    world.despawn(b);
    assert!(excluded(&world, a));
}

#[test]
fn saved_native_placement_retains_identity_and_exclusion_without_a_second_store() {
    let mut world = World::new();
    let mut layout = LayoutBox::new(Vec2::splat(100.0));
    layout.capture_excluded = true;
    let original = world.spawn((layout, crate::native::canvas::CanvasItem { position: bevy::math::DVec2::ZERO, size: Vec2::splat(100.0) })).id();
    let saved = crate::native::sand_placement::Placement::capture(&world, original);
    let bytes = serde_json::to_vec(&saved).unwrap();
    let restored: crate::native::sand_placement::Placement = serde_json::from_slice(&bytes).unwrap();
    world.despawn(original);
    let replacement = world.spawn_empty().id();
    restored.restore(&mut world, replacement);
    assert_eq!(world.get::<LayoutBox>(replacement), Some(&layout));
    assert!(excluded(&world, replacement));
}

#[test]
fn derived_window_labels_follow_the_specific_source_and_fail_closed_after_its_lifetime() {
    let mut world = World::new();
    let private = crate::host::SurfaceId(7);
    let public = crate::host::SurfaceId(8);
    let private_label = world.spawn(WindowSources(vec![private])).id();
    let public_label = world.spawn(WindowSources(vec![public])).id();
    let mut policy = crate::host::capture::Policy { windows: BTreeMap::from([(private, true), (public, false)]), ..default() };
    assert_eq!(private_window_sources(&mut world, &policy), BTreeSet::from([private_label]));
    policy.windows.remove(&public);
    assert_eq!(private_window_sources(&mut world, &policy), BTreeSet::from([private_label, public_label]));
}

#[test]
fn temporary_private_render_instances_do_not_change_semantic_security_revisions() {
    let mut world = World::new();
    let mut layout = LayoutBox::new(Vec2::splat(100.0));
    layout.capture_excluded = true;
    let owner = world.spawn(layout).id();
    let gate = crate::host::capture::PolicyGate::default();
    let mut revision = None;
    for _ in 0..120 {
        let visual = world.spawn(crate::native::topology::presentation::VisualOwner(owner)).id();
        let glyph = world.spawn(ChildOf(owner)).id();
        assert_eq!(semantic_sources(&world, visual), BTreeSet::from([owner]));
        assert_eq!(semantic_sources(&world, glyph), BTreeSet::from([owner]));
        let entities: Vec<_> = world.query::<Entity>().iter(&world).collect();
        let private_entities = entities.into_iter().filter(|entity| excluded(&world, *entity))
            .flat_map(|entity| semantic_sources(&world, entity)).map(Entity::to_bits).collect();
        gate.publish(crate::host::capture::Policy { private_entities, ..default() });
        let current = gate.read().unwrap().revision();
        assert_eq!(*revision.get_or_insert(current), current);
        world.despawn(visual);
        world.despawn(glyph);
    }
}

#[test]
fn an_authored_source_keeps_its_own_identity_as_well_as_its_explicit_input_owner() {
    let mut world = World::new();
    let input = world.spawn(LayoutBox::new(Vec2::splat(100.0))).id();
    let mut layout = LayoutBox::new(Vec2::splat(100.0));
    layout.capture_excluded = true;
    let source = world.spawn((layout, render::TargetSource(input))).id();
    let child = world.spawn(ChildOf(source)).id();
    assert_eq!(semantic_sources(&world, child), BTreeSet::from([source, input]));
    world.despawn(source);
    assert_eq!(semantic_sources(&world, source), BTreeSet::from([source]));
}
