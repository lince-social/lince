use super::*;
use bevy::ecs::system::SystemState;

fn equivalent(world: &World, state: &mut SystemState<Sources>, entities: &[Entity]) {
    let sources = state.get(world).unwrap();
    for entity in entities {
        assert_eq!(
            sources.resolve(*entity),
            semantic_sources(world, *entity),
            "Source changed for {entity:?}"
        );
    }
}

#[test]
fn typed_sources_follow_all_boundaries_disabled_nodes_and_live_graph_replacements() {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    let owner = world
        .spawn((LayoutBox::new(Vec2::splat(100.0)), ChildOf(root)))
        .id();
    let child = world.spawn(ChildOf(owner)).id();
    let public = world.spawn(LayoutBox::new(Vec2::splat(100.0))).id();
    let visual = world
        .spawn(crate::native::topology::presentation::VisualOwner(child))
        .id();
    let mesh = world
        .spawn((
            crate::native::shader_effects::RecordMesh(owner),
            render::TargetSource(public),
        ))
        .id();
    let sphere = world
        .spawn((
            crate::native::shader_effects::RecordSphere(owner),
            ChildOf(public),
        ))
        .id();
    let windows = world
        .spawn((WindowSources(vec![]), render::TargetSource(owner)))
        .id();
    let metadata = world
        .spawn(crate::native::full_record::window::Metadata {
            placement: [0; 16],
            uid: "fixture".into(),
            title: String::new(),
            app_id: String::new(),
            live: false,
            state: String::new(),
        })
        .id();
    let application = world
        .spawn(crate::native::layout::desktop::ApplicationWindow {
            surface: crate::host::SurfaceId(1),
            content_size: [10; 2],
        })
        .id();
    let disabled = world.spawn((Disabled, ChildOf(owner))).id();
    let orphan = world.spawn_empty().id();
    let mut entities = vec![
        root,
        owner,
        child,
        public,
        visual,
        mesh,
        sphere,
        windows,
        metadata,
        application,
        disabled,
        orphan,
    ];
    let live = world.query_filtered::<Entity, Allow<Disabled>>().iter(&world).collect::<BTreeSet<_>>();
    let mut state = SystemState::<Sources>::new(&mut world);
    equivalent(&world, &mut state, &entities);
    let sources = state.get(&world).unwrap();
    assert_eq!(
        sources.entities().collect::<BTreeSet<_>>(),
        live
    );
    assert_eq!(sources.resolve(visual), BTreeSet::from([owner]));
    assert_eq!(sources.resolve(mesh), BTreeSet::from([owner, public]));
    assert_eq!(sources.resolve(sphere), BTreeSet::from([owner, public]));
    assert_eq!(sources.resolve(windows), BTreeSet::from([windows, owner]));
    for entity in [root, metadata, application, orphan] {
        assert_eq!(sources.resolve(entity), BTreeSet::from([entity]));
    }
    assert_eq!(sources.resolve(disabled), BTreeSet::from([owner]));
    world.entity_mut(child).insert(ChildOf(public));
    world
        .entity_mut(visual)
        .remove::<crate::native::topology::presentation::VisualOwner>()
        .insert((
            LayoutBox::new(Vec2::splat(50.0)),
            render::TargetSource(owner),
        ));
    world.entity_mut(disabled).remove::<Disabled>();
    let added = world.spawn((Disabled, render::TargetSource(visual))).id();
    entities.push(added);
    equivalent(&world, &mut state, &entities);
    assert_eq!(state.get(&world).unwrap().resolve(child), BTreeSet::from([public]));
    assert_eq!(
        state.get(&world).unwrap().resolve(added),
        BTreeSet::from([visual, owner])
    );
    world.despawn(public);
    equivalent(&world, &mut state, &entities);
    assert_eq!(state.get(&world).unwrap().resolve(public), BTreeSet::from([public]));
}

#[test]
fn typed_sources_preserve_cycle_depth_and_visit_budget_fallbacks() {
    let mut world = World::new();
    let a = world.spawn_empty().id();
    let b = world.spawn(render::TargetSource(a)).id();
    world
        .entity_mut(a)
        .insert(crate::native::topology::presentation::VisualOwner(b));
    let chain: Vec<_> = (0..MAX_DEPTH + 2)
        .map(|_| world.spawn_empty().id())
        .collect();
    for pair in chain.windows(2) {
        world
            .entity_mut(pair[0])
            .insert(render::TargetSource(pair[1]));
    }
    world
        .entity_mut(*chain.last().unwrap())
        .insert(LayoutBox::new(Vec2::ONE));
    let tree: Vec<_> = (0..2047).map(|_| world.spawn_empty().id()).collect();
    for (index, entity) in tree.iter().copied().enumerate() {
        if index < 1023 {
            world.entity_mut(entity).insert((
                crate::native::topology::presentation::VisualOwner(tree[index * 2 + 1]),
                render::TargetSource(tree[index * 2 + 2]),
            ));
        } else {
            world.entity_mut(entity).insert(LayoutBox::new(Vec2::ONE));
        }
    }
    let mut state = SystemState::<Sources>::new(&mut world);
    equivalent(&world, &mut state, &[a, b, chain[0], tree[0]]);
    let sources = state.get(&world).unwrap();
    assert_eq!(sources.resolve(a), BTreeSet::from([a]));
    assert_eq!(sources.resolve(b), BTreeSet::from([b]));
    assert_eq!(
        sources.resolve(chain[0]),
        BTreeSet::from([chain[MAX_DEPTH]])
    );
    assert!(
        sources
            .resolve(tree[0])
            .iter()
            .any(|entity| world.get::<LayoutBox>(*entity).is_none())
    );
}
