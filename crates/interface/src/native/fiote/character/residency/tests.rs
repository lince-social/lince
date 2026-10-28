use super::*;
use bevy::ecs::system::RunSystemOnce;

fn playback(player: &AnimationPlayer) -> Vec<(usize, f32, f32)> {
    let mut result: Vec<_> = player
        .playing_animations()
        .map(|(index, animation)| (index.index(), animation.seek_time(), animation.weight()))
        .collect();
    result.sort_by_key(|state| state.0);
    result
}

#[test]
fn hiding_releases_target_demand_and_freezes_playback_and_transitions_until_restored() {
    let mut world = World::new();
    world.init_resource::<Character>();
    world.init_resource::<Assets<Image>>();
    world.init_resource::<Assets<AnimationGraph>>();
    world.init_resource::<Time>();
    let avatar = world.spawn(Node::default()).id();
    let palette = Palette::default();
    let groups = BTreeMap::from([(palette, vec![avatar])]);
    let image = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            128,
            128,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let root = world
        .spawn((Transform::default(), Visibility::Inherited))
        .id();
    let model = world.spawn((Transform::default(), ChildOf(root))).id();
    let camera = world
        .spawn((
            Camera::default(),
            RenderTarget::Image(image.clone().into()),
            Target {
                image: image.id(),
                release: false,
            },
            ChildOf(root),
        ))
        .id();
    let (graph, indices) = AnimationGraph::from_clips([Handle::default(), Handle::default()]);
    let graph = world.resource_mut::<Assets<AnimationGraph>>().add(graph);
    let mut player = AnimationPlayer::default();
    let mut transitions = AnimationTransitions::new();
    transitions
        .play(&mut player, indices[0], Duration::ZERO)
        .repeat()
        .set_seek_time(1.25);
    transitions
        .play(&mut player, indices[1], Duration::from_millis(180))
        .repeat()
        .set_seek_time(0.75);
    let before = playback(&player);
    let entity = world.spawn((player, transitions, ChildOf(model))).id();
    world.resource_mut::<Character>().scenes.push(Scene {
        palette,
        layer: crate::native::render_layers::FIOTE.get(0).unwrap(),
        running: true,
        rendered: Some(Instant::now()),
        materials: Vec::new(),
        root,
        model,
        camera,
        image: image.clone(),
        graph,
        clips: HashMap::from([("sit_idle", (indices[0], 4.0))]),
        players: vec![entity],
        behavior: Behavior::default(),
        restored: Vec::new(),
        paused: HashMap::new(),
        visible: true,
    });
    world.get_mut::<Node>(avatar).unwrap().display = Display::None;
    synchronize(&mut world, &groups, true);
    animate(&mut world, false);
    assert_eq!(
        world
            .resource::<Assets<Image>>()
            .get(&image)
            .unwrap()
            .size(),
        UVec2::ONE
    );
    assert!(world.get::<Target>(camera).unwrap().release);
    assert_eq!(*world.get::<Visibility>(root).unwrap(), Visibility::Hidden);
    assert!(!world.get::<Camera>(camera).unwrap().is_active);
    assert!(world.get::<AnimationPlayer>(entity).unwrap().all_paused());
    assert!(world.get::<AnimationTransitions>(entity).is_none());
    world
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs(2));
    world
        .run_system_once(bevy::animation::transition::advance_transitions)
        .unwrap();
    world
        .run_system_once(bevy::animation::transition::expire_completed_transitions)
        .unwrap();
    assert_eq!(
        playback(world.get::<AnimationPlayer>(entity).unwrap()),
        before
    );
    world.get_mut::<Node>(avatar).unwrap().display = Display::Flex;
    synchronize(&mut world, &groups, true);
    animate(&mut world, true);
    assert_eq!(
        world
            .resource::<Assets<Image>>()
            .get(&image)
            .unwrap()
            .size(),
        UVec2::splat(128)
    );
    assert!(!world.get::<Target>(camera).unwrap().release);
    assert!(world.get::<Camera>(camera).unwrap().is_active);
    assert_eq!(
        playback(world.get::<AnimationPlayer>(entity).unwrap()),
        before
    );
    assert!(world.get::<AnimationTransitions>(entity).is_some());
    let scene = world.resource_mut::<Character>().scenes.remove(0);
    remember(&mut world, &scene);
    let mut saved = world
        .resource_mut::<Character>()
        .saved
        .remove(&palette)
        .unwrap();
    let mut replacement = AnimationPlayer::default();
    let transitions = restore(&mut replacement, saved.players.pop(), indices[0]);
    assert_eq!(playback(&replacement), before);
    assert!(replacement.all_paused());
    assert_eq!(saved.behavior.activity, scene.behavior.activity);
    world.entity_mut(entity).insert((replacement, transitions));
    world
        .resource_mut::<Time>()
        .advance_by(Duration::from_millis(200));
    world
        .run_system_once(bevy::animation::transition::advance_transitions)
        .unwrap();
    world
        .run_system_once(bevy::animation::transition::expire_completed_transitions)
        .unwrap();
    let player = world.get::<AnimationPlayer>(entity).unwrap();
    assert!(player.animation(indices[0]).is_none());
    assert_eq!(player.animation(indices[1]).unwrap().seek_time(), 0.75);
    assert_eq!(player.animation(indices[1]).unwrap().weight(), 1.0);
}
