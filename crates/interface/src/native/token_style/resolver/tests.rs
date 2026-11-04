use super::*;
use bevy::ecs::system::SystemState;

#[cfg_attr(test, test)]
fn typed_resolution_tracks_inheritance_sources_rules_and_removals() {
    let mut world = World::new();
    world.init_resource::<ThemeSettings>();
    let mut root_values = TokenOverrides::default();
    root_values.set(Token::FontSize, TokenValue::Number(24.0));
    root_values.set(Token::Padding, TokenValue::Number(16.0));
    let root = world.spawn((Node::default(), root_values)).id();
    let text = world
        .spawn((Node::default(), SandText::new(true), ChildOf(root)))
        .id();
    let leaf = world.spawn((Node::default(), ChildOf(text))).id();
    let mut source_text = SandText::new(false);
    source_text
        .tokens
        .set(Token::FontSize, TokenValue::Number(18.0));
    let source = world.spawn((Node::default(), source_text, Disabled)).id();
    let preview = world
        .spawn((
            Node::default(),
            PreviewSource(source),
            PreviewKind(SandStyleKind::Text),
            ChildOf(root),
        ))
        .id();
    let mut entities = vec![root, text, leaf, source, preview];
    for kind in SandKind::ALL {
        entities.push(
            world
                .spawn((
                    Node::default(),
                    StoredSand {
                        kind,
                        content: None,
                    },
                    ChildOf(root),
                ))
                .id(),
        );
    }
    for kind in SandStyleKind::ALL {
        entities.push(world.spawn((Node::default(), PreviewKind(kind))).id());
    }
    entities.push(
        world
            .spawn((Node::default(), RecordPlacement("record".into())))
            .id(),
    );
    let mut state = SystemState::<Tokens>::new(&mut world);
    let mut verify = |world: &World| {
        let mut typed = state.get(world).unwrap();
        typed.refresh();
        for entity in &entities {
            if world.get_entity(*entity).is_ok() {
                typed.prepare_kind(*entity);
                typed.prepare(
                    *entity,
                    crate::native::tokens::TOKENS
                        .iter()
                        .map(|definition| definition.token),
                );
            }
        }
        typed.finish_preparing();
        for entity in &entities {
            assert_eq!(
                typed.inherited_kind(*entity),
                super::super::inherited_kind(world, *entity)
            );
            for definition in crate::native::tokens::TOKENS {
                assert_eq!(
                    typed.resolve(*entity, definition.token),
                    super::super::resolve(world, *entity, definition.token),
                    "{entity:?}: {:?}",
                    definition.token
                );
            }
        }
    };
    verify(&world);
    verify(&world);
    world.entity_mut(root).insert(Colors {
        background: Some(RuleColor::Specific([10, 20, 30, 255])),
        text: Some(RuleColor::Token(Token::Accent)),
        border: Some(RuleColor::Token(Token::Warning)),
        link: Some(RuleColor::Specific([40, 50, 60, 255])),
    });
    verify(&world);
    world.resource_mut::<ThemeSettings>().scheme = crate::native::tokens::ColorScheme::Light;
    world
        .resource_mut::<ThemeSettings>()
        .global
        .set(Token::Spacing, TokenValue::Number(12.0));
    world
        .resource_mut::<ThemeSettings>()
        .kinds
        .entry(SandStyleKind::Text)
        .or_default()
        .set(Token::Width, TokenValue::Number(320.0));
    verify(&world);
    let mut own = TokenOverrides::default();
    own.set(Token::Padding, TokenValue::Number(4.0));
    world.entity_mut(preview).insert(own);
    world
        .get_mut::<SandText>(source)
        .unwrap()
        .tokens
        .set(Token::FontSize, TokenValue::Number(30.0));
    world.entity_mut(leaf).insert(ChildOf(preview));
    verify(&world);
    world.entity_mut(root).remove::<Colors>();
    world.entity_mut(preview).remove::<TokenOverrides>();
    world.entity_mut(text).remove::<SandText>();
    verify(&world);
    world.entity_mut(preview).remove::<PreviewSource>();
    world
        .entity_mut(preview)
        .insert(PreviewKind(SandStyleKind::Fiote));
    world.entity_mut(root).insert(Colors {
        border: Some(RuleColor::Specific([1, 2, 3, 255])),
        ..default()
    });
    world.entity_mut(leaf).remove::<ChildOf>();
    verify(&world);
    world.entity_mut(preview).insert(PreviewSource(source));
    world.despawn(source);
    verify(&world);
}

#[cfg_attr(test, test)]
fn cached_values_follow_same_frame_edits_and_retire_removed_entities() {
    let mut world = World::new();
    world.init_resource::<ThemeSettings>();
    let root = world
        .spawn((Node::default(), PreviewKind(SandStyleKind::Text)))
        .id();
    let leaf = world.spawn((Node::default(), ChildOf(root))).id();
    let mut first = SystemState::<Tokens>::new(&mut world);
    let mut second = SystemState::<Tokens>::new(&mut world);
    let prepare = |state: &mut SystemState<Tokens>, world: &World| {
        let mut tokens = state.get(world).unwrap();
        tokens.refresh();
        tokens.prepare_kind(leaf);
        tokens.prepare_metric(leaf, "font-size", Token::FontSize);
        (
            tokens.inherited_kind(leaf),
            tokens.metric(leaf, "font-size", Token::FontSize),
        )
    };
    assert_eq!(prepare(&mut first, &world).1.number(), 16.0);
    assert_eq!(prepare(&mut second, &world).1.number(), 16.0);
    world
        .entity_mut(leaf)
        .insert(RecordPlacement("record".into()));
    assert_eq!(prepare(&mut first, &world).0, Some(SandStyleKind::Record));
    world.entity_mut(leaf).remove::<RecordPlacement>();
    assert_eq!(prepare(&mut first, &world).0, Some(SandStyleKind::Text));
    let mut values = TokenOverrides::default();
    values.set(Token::FontSize, TokenValue::Number(25.0));
    world.entity_mut(root).insert(values);
    assert_eq!(prepare(&mut first, &world).1.number(), 25.0);
    world
        .entity_mut(leaf)
        .insert(AuthoredTokenUses(std::collections::BTreeMap::from([(
            "font-size".into(),
            Token::Padding,
        )])));
    assert_eq!(prepare(&mut second, &world).1.number(), 8.0);
    world.entity_mut(leaf).remove::<AuthoredTokenUses>();
    world.entity_mut(root).remove::<TokenOverrides>();
    world.entity_mut(root).remove::<PreviewKind>();
    assert_eq!(
        prepare(&mut first, &world),
        (None, TokenValue::Number(16.0))
    );
    let many = (0..1024)
        .map(|_| world.spawn(Node::default()).id())
        .collect::<Vec<_>>();
    {
        let mut tokens = first.get(&world).unwrap();
        tokens.refresh();
        for entity in &many {
            tokens.prepare(*entity, [Token::Padding]);
        }
        assert!(tokens.cache.values.len() >= 1024);
    }
    for entity in many {
        world.despawn(entity);
    }
    world.despawn(leaf);
    let mut tokens = first.get(&world).unwrap();
    tokens.refresh();
    tokens.finish_preparing();
    assert!(tokens.cache.values.is_empty());
    assert!(tokens.cache.kinds.is_empty());
    assert!(tokens.cache.values.capacity() <= 256);
}

#[cfg_attr(test, test)]
fn typed_metric_bindings_follow_authored_replacement_and_removal() {
    let mut world = World::new();
    world.init_resource::<ThemeSettings>();
    let entity = world.spawn(Node::default()).id();
    let mut state = SystemState::<Tokens>::new(&mut world);
    assert_eq!(
        state
            .get(&world)
            .unwrap()
            .metric(entity, "font-size", Token::FontSize)
            .number(),
        16.0
    );
    world
        .entity_mut(entity)
        .insert(AuthoredTokenUses(std::collections::BTreeMap::from([(
            "font-size".into(),
            Token::Spacing,
        )])));
    assert_eq!(
        state
            .get(&world)
            .unwrap()
            .metric(entity, "font-size", Token::FontSize)
            .number(),
        8.0
    );
    world
        .resource_mut::<ThemeSettings>()
        .global
        .set(Token::Spacing, TokenValue::Number(12.0));
    assert_eq!(
        state
            .get(&world)
            .unwrap()
            .metric(entity, "font-size", Token::FontSize)
            .number(),
        12.0
    );
    world.entity_mut(entity).remove::<AuthoredTokenUses>();
    assert_eq!(
        state
            .get(&world)
            .unwrap()
            .metric(entity, "font-size", Token::FontSize)
            .number(),
        16.0
    );
}

crate::native::laboratory_cases! {
    typed_resolution_tracks_inheritance_sources_rules_and_removals,
    typed_metric_bindings_follow_authored_replacement_and_removal,
    cached_values_follow_same_frame_edits_and_retire_removed_entities,
}
