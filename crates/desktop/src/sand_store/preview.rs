use bevy::{prelude::*, text::EditableText};

pub(crate) fn scene(world: &mut World) -> (World, Entity) {
    let mut scene = World::new();
    crate::description::preview_fonts(world, &mut scene);
    scene.insert_resource(crate::theme::Typography(
        world.resource::<crate::theme::Typography>().0.clone(),
    ));
    scene.insert_resource(world.resource::<crate::tokens::ThemeSettings>().clone());
    let root = scene
        .spawn((
            crate::workspace::Workspaces::default(),
            crate::canvas::CanvasView::default(),
        ))
        .id();
    (scene, root)
}

fn copy<T: Component + Clone>(source: &World, from: Entity, world: &mut World, to: Entity) {
    if let Some(value) = source.get::<T>(from) {
        world.entity_mut(to).insert(value.clone());
    }
}

pub(crate) fn snapshot(source: &World, from: Entity, world: &mut World, parent: Entity) -> Entity {
    let to = world.spawn((ChildOf(parent), Pickable::IGNORE)).id();
    copy::<Node>(source, from, world, to);
    copy::<TextSpan>(source, from, world, to);
    if let Some(input) = source.get::<EditableText>(from) {
        world
            .entity_mut(to)
            .insert(Text::new(input.value().to_string()));
    } else {
        copy::<Text>(source, from, world, to);
    }
    copy::<TextFont>(source, from, world, to);
    copy::<TextColor>(source, from, world, to);
    copy::<TextLayout>(source, from, world, to);
    copy::<BackgroundColor>(source, from, world, to);
    copy::<BorderColor>(source, from, world, to);
    copy::<ImageNode>(source, from, world, to);
    copy::<UiTransform>(source, from, world, to);
    copy::<crate::tokens::TokenOverrides>(source, from, world, to);
    copy::<crate::token_style::BackgroundToken>(source, from, world, to);
    copy::<crate::token_style::BorderToken>(source, from, world, to);
    copy::<crate::token_style::TextToken>(source, from, world, to);
    if let Some(area) = source.get::<crate::sand_text::SandText>(from) {
        world.entity_mut(to).insert(area.tokens.clone());
    }
    if source.get::<crate::actions::ActionButton>(from).is_some()
        || source.get::<EditableText>(from).is_some()
    {
        world.get_mut::<Node>(to).unwrap().border = UiRect::all(px(1));
        world
            .entity_mut(to)
            .insert(crate::token_style::border(crate::tokens::Token::Accent));
    }
    if let Some(icon) = source.get::<crate::icons::IconButton>(from) {
        let style = source
            .get::<crate::icons::IconStyle>(from)
            .copied()
            .unwrap_or_default();
        world.entity_mut(to).insert(Node {
            width: px(style.size + 2.0 * style.padding),
            height: px(style.size + 2.0 * style.padding),
            padding: UiRect::all(px(style.padding)),
            flex_shrink: 0.0,
            ..default()
        });
        if let Some(image) = crate::icons::image(world, icon.icon) {
            world.spawn((
                image,
                Node {
                    width: px(style.size),
                    height: px(style.size),
                    ..default()
                },
                ChildOf(to),
                Pickable::IGNORE,
            ));
        }
    } else if let Some(children) = source.get::<Children>(from) {
        for child in children.iter() {
            if source.get::<Node>(child).is_some() || source.get::<TextSpan>(child).is_some() {
                snapshot(source, child, world, to);
            }
        }
    }
    to
}

pub(crate) fn fit(world: &mut World, entity: Entity, size: Vec2) {
    let scale = (68.0 / size.x).min(60.0 / size.y).min(1.0);
    let mut node = world.get_mut::<Node>(entity).unwrap();
    node.position_type = PositionType::Absolute;
    node.left = px((72.0 - size.x) * 0.5);
    node.top = px((64.0 - size.y) * 0.5);
    node.width = px(size.x);
    node.height = px(size.y);
    node.min_width = px(0);
    node.min_height = px(0);
    node.overflow = Overflow::clip();
    world
        .entity_mut(entity)
        .insert(UiTransform::from_scale(Vec2::splat(scale)));
    let mut pending = vec![entity];
    while let Some(entity) = pending.pop() {
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            let node = &mut *node;
            for edge in [
                &mut node.border.left,
                &mut node.border.right,
                &mut node.border.top,
                &mut node.border.bottom,
            ] {
                if let Val::Px(width) = edge
                    && *width > 0.0
                {
                    *width = width.max(0.6 / scale);
                }
            }
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
    }
}

pub(crate) fn compose(world: &mut World, root: Entity) -> Entity {
    let items: Vec<_> = world
        .get::<Children>(root)
        .into_iter()
        .flatten()
        .filter_map(|entity| {
            world
                .get::<crate::canvas::CanvasItem>(*entity)
                .map(|item| (*entity, *item))
        })
        .collect();
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for (_, item) in &items {
        min = min.min(item.position.as_vec2() - item.size * 0.5);
        max = max.max(item.position.as_vec2() + item.size * 0.5);
    }
    let size = if items.is_empty() {
        Vec2::splat(100.0)
    } else {
        max - min
    };
    let container = world
        .spawn((
            Node::default(),
            crate::canvas::CanvasItem {
                position: bevy::math::DVec2::ZERO,
                size,
            },
        ))
        .id();
    for (entity, item) in items {
        world.entity_mut(entity).insert(ChildOf(container));
        if world.get::<Node>(entity).is_none() {
            world.entity_mut(entity).insert(Node::default());
        }
        let position = item.position.as_vec2() - item.size * 0.5 - min;
        let area = world.get::<crate::area::InfluenceArea>(entity).cloned();
        let mut node = world.get_mut::<Node>(entity).unwrap();
        node.display = Display::Flex;
        node.position_type = PositionType::Absolute;
        node.left = px(position.x);
        node.top = px(position.y);
        node.width = px(item.size.x);
        node.height = px(item.size.y);
        if let Some(area) = area {
            node.border = UiRect::all(px(4));
            world.entity_mut(entity).insert((
                BackgroundColor(
                    crate::canvas_background::color(area.color).with_alpha(area.opacity),
                ),
                crate::token_style::border(crate::tokens::Token::Accent),
            ));
        }
    }
    container
}
