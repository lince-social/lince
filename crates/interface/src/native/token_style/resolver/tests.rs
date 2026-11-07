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

#[cfg_attr(test, test)]
fn text_autosave_retains_unrelated_styles_and_unchanged_geometry() {
    use crate::native::{
        edit_mode::EditMode,
        sand_text::{self, SavedText},
        sand_text_editor::{self, TextPanel},
        workspace::{WorkspaceMember, Workspaces},
    };
    use bevy::text::EditableText;

    for editable in [false, true] {
        let (mut app, root) = crate::native::edit_mode::tests::fixture();
        let world = app.world_mut();
        let workspace = world.get::<Workspaces>(root).unwrap().active;
        let sand = world
            .spawn((
                CanvasItem {
                    position: default(),
                    size: Vec2::new(300.0, 220.0),
                },
                StoredSand {
                    kind: if editable {
                        SandKind::EditableText
                    } else {
                        SandKind::Text
                    },
                    content: None,
                },
                WorkspaceMember(workspace),
                ChildOf(root),
            ))
            .id();
        let text = sand_text::spawn(
            world,
            sand,
            SavedText {
                area: SandText::new(editable),
                text: "Initial text".into(),
            },
        );
        world.get_mut::<StoredSand>(sand).unwrap().content = Some(text);
        world.get_mut::<EditMode>(root).unwrap().enabled = true;
        sand_text_editor::open(world, root, sand);
        let panel = world.get::<EditMode>(root).unwrap().panel;
        assert!(sand_text_editor::render(world, root, panel));
        sand_text_editor::autosave(world);
        let fields = world.get::<TextPanel>(panel).unwrap().fields.clone();
        for (entity, label) in fields.iter().zip([
            "Text",
            "Distance from left",
            "Distance from top",
            "Width",
            "Height",
        ]) {
            assert_eq!(
                world
                    .get::<bevy::a11y::AccessibilityNode>(*entity)
                    .unwrap()
                    .label(),
                Some(label)
            );
        }
        let notices = world
            .query::<(Entity, &Text, &ChildOf)>()
            .iter(world)
            .filter(|(_, text, parent)| parent.parent() == panel && text.0.is_empty())
            .map(|(entity, _, _)| entity)
            .collect::<Vec<_>>();
        assert_eq!(notices.len(), 1);
        let notice = notices[0];
        let unrelated = world.spawn((Node::default(), ChildOf(root))).id();
        let mut state = SystemState::<Tokens>::new(world);
        {
            let mut tokens = state.get(world).unwrap();
            tokens.refresh();
            for entity in [text, unrelated] {
                tokens.prepare_kind(entity);
                tokens.prepare(entity, [Token::FontSize, Token::Ink]);
            }
        }
        let (cached, kinds) = {
            let mut tokens = state.get(world).unwrap();
            tokens.refresh();
            assert!(!tokens.cache.values.is_empty());
            (tokens.cache.values.clone(), tokens.cache.kinds.clone())
        };
        let area_tick = world
            .entity(text)
            .get_ref::<SandText>()
            .unwrap()
            .last_changed();
        let node_tick = world.entity(text).get_ref::<Node>().unwrap().last_changed();
        let item_tick = world
            .entity(sand)
            .get_ref::<CanvasItem>()
            .unwrap()
            .last_changed();
        let notice_tick = world
            .entity(notice)
            .get_ref::<Text>()
            .unwrap()
            .last_changed();
        world.increment_change_tick();
        let value = "Saved locally · café 日本語 🦀";
        world
            .get_mut::<EditableText>(fields[0])
            .unwrap()
            .editor
            .set_text(value);
        sand_text_editor::autosave(world);
        assert_eq!(sand_text::value(world, text), value);
        assert_eq!(sand_text::snapshot(world, sand)[0].text, value);
        assert_eq!(
            world
                .entity(text)
                .get_ref::<SandText>()
                .unwrap()
                .last_changed(),
            area_tick
        );
        assert_eq!(
            world.entity(text).get_ref::<Node>().unwrap().last_changed(),
            node_tick
        );
        assert_eq!(
            world
                .entity(sand)
                .get_ref::<CanvasItem>()
                .unwrap()
                .last_changed(),
            item_tick
        );
        assert_eq!(
            world
                .entity(notice)
                .get_ref::<Text>()
                .unwrap()
                .last_changed(),
            notice_tick
        );
        {
            let mut tokens = state.get(world).unwrap();
            tokens.refresh();
            assert_eq!(tokens.cache.values, cached);
            assert_eq!(tokens.cache.kinds, kinds);
        }
        world.increment_change_tick();
        for (index, value) in [(0, "Mixed content and geometry"), (1, "40"), (3, "400")] {
            world
                .get_mut::<EditableText>(fields[index])
                .unwrap()
                .editor
                .set_text(value);
        }
        sand_text_editor::autosave(world);
        let snapshot = sand_text::snapshot(world, sand);
        assert_eq!(snapshot[0].text, "Mixed content and geometry");
        assert_eq!(snapshot[0].area.offset, [40.0, 16.0]);
        assert_eq!(snapshot[0].area.size, [400.0, 152.0]);
        let node = world.get::<Node>(text).unwrap();
        assert_eq!((node.left, node.width), (px(40), px(400)));
        assert_eq!(
            world.get::<CanvasItem>(sand).unwrap().size,
            Vec2::new(456.0, 220.0)
        );
        assert_ne!(
            world
                .entity(text)
                .get_ref::<SandText>()
                .unwrap()
                .last_changed(),
            area_tick
        );
        {
            let mut tokens = state.get(world).unwrap();
            tokens.refresh();
            tokens.prepare(text, [Token::FontSize]);
            assert_ne!(tokens.resolve(text, Token::FontSize).0.number(), 31.0);
        }
        world.increment_change_tick();
        let mut values = TokenOverrides::default();
        values.set(Token::FontSize, TokenValue::Number(31.0));
        super::super::set_overrides(world, text, values);
        let mut tokens = state.get(world).unwrap();
        tokens.refresh();
        assert!(tokens.cache.values.is_empty());
        tokens.prepare(text, [Token::FontSize]);
        assert_eq!(tokens.resolve(text, Token::FontSize).0.number(), 31.0);
    }
}

crate::native::laboratory_cases! {
    typed_resolution_tracks_inheritance_sources_rules_and_removals,
    typed_metric_bindings_follow_authored_replacement_and_removal,
    cached_values_follow_same_frame_edits_and_retire_removed_entities,
    text_autosave_retains_unrelated_styles_and_unchanged_geometry,
}
