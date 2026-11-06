use super::*;

pub(super) fn setup() -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<Assets<Shader>>();
    world.init_resource::<Assets<ShaderMaterial>>();
    world.init_resource::<Assets<Image>>();
    world.init_resource::<crate::native::theme::Typography>();
    let parent = world.spawn(Node::default()).id();
    let source = effect(&mut world, parent, Effect::default(), Some("1".into()));
    (world, source)
}

#[cfg_attr(test, test)]
fn empty_sources_preserve_consumer_expiry_and_first_material_activation() {
    let (mut world, source) = setup();
    let surface = world.spawn_empty().id();
    let queued = world.spawn_empty().id();
    let owned = world
        .resource_mut::<EffectAssets>()
        .owned
        .remove(&source)
        .unwrap();
    let material = world
        .spawn((
            SurfaceConsumer {
                image: owned.2.clone(),
                surface,
            },
            Visibility::Visible,
        ))
        .id();
    let native = HashMap::from([(
        surface,
        (
            Request {
                size: Vec2::new(300.0, 150.0),
                density: 2.0,
                previous: 0.125,
                visible: true,
                text: false,
                exact: true,
            },
            5,
        ),
    )]);
    request_extent(&mut world, source, queued, UVec2::new(320, 180));
    let epoch = world.resource::<Consumers>().epoch;
    assert!(requests(&mut world, &native).is_empty());
    assert_eq!(world.resource::<Consumers>().epoch, epoch + 1);
    assert!(
        world
            .resource::<Consumers>()
            .entries
            .contains_key(&(source, queued))
    );
    assert!(
        !world
            .resource::<Consumers>()
            .entries
            .contains_key(&(source, material))
    );
    world
        .resource_mut::<EffectAssets>()
        .owned
        .insert(source, owned);
    let active = requests(&mut world, &native);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].0, source);
    assert_eq!(active[0].1.request.size, Vec2::new(600.0, 300.0));
    assert!(active[0].1.request.visible);
    assert_eq!(active[0].1.priority, 5);
    assert!(
        !world
            .resource::<Consumers>()
            .entries
            .contains_key(&(source, queued))
    );
    assert!(
        world
            .resource::<Consumers>()
            .entries
            .contains_key(&(source, material))
    );
    world.entity_mut(material).insert(Visibility::Hidden);
    assert!(!requests(&mut world, &native)[0].1.request.visible);
    assert!(world.resource::<Consumers>().entries.is_empty());
    let owned = world
        .resource_mut::<EffectAssets>()
        .owned
        .remove(&source)
        .unwrap();
    request_extent(&mut world, source, queued, UVec2::new(640, 360));
    let epoch = world.resource::<Consumers>().epoch;
    assert!(requests(&mut world, &native).is_empty());
    assert_eq!(world.resource::<Consumers>().epoch, epoch + 1);
    assert!(
        world
            .resource::<Consumers>()
            .entries
            .contains_key(&(source, queued))
    );
    assert!(requests(&mut world, &native).is_empty());
    assert_eq!(world.resource::<Consumers>().epoch, epoch + 2);
    assert!(world.resource::<Consumers>().entries.is_empty());
    world
        .resource_mut::<EffectAssets>()
        .owned
        .insert(source, owned);
    world.entity_mut(material).insert(Visibility::Visible);
    assert_eq!(
        requests(&mut world, &native)[0].1.request.size,
        Vec2::new(600.0, 300.0)
    );
    world.despawn(material);
    assert!(!requests(&mut world, &native)[0].1.request.visible);
    assert!(world.resource::<Consumers>().entries.is_empty());
}

crate::native::laboratory_cases! {
    empty_sources_preserve_consumer_expiry_and_first_material_activation,
}
