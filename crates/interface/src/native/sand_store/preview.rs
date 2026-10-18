use bevy::{prelude::*, text::EditableText};

#[derive(Component, Clone, Copy)]
pub(crate) struct PreviewBorder(pub UiRect);

pub(crate) fn scene(world: &mut World) -> (World, Entity) {
    let mut scene = World::new();
    world.init_resource::<super::StoreFont>();
    scene.insert_resource(world.resource::<super::StoreFont>().clone());
    crate::native::description::preview_fonts(world, &mut scene);
    crate::native::ide::preview_font(world, &mut scene);
    crate::native::terminal::preview_fonts(world, &mut scene);
    scene.insert_resource(crate::native::theme::Typography(
        world
            .resource::<crate::native::theme::Typography>()
            .0
            .clone(),
    ));
    scene.insert_resource(
        world
            .resource::<crate::native::tokens::ThemeSettings>()
            .clone(),
    );
    let root = scene
        .spawn((
            crate::native::workspace::Workspaces::default(),
            crate::native::canvas::CanvasView::default(),
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
    if let Some(border) = source.get::<PreviewBorder>(from) {
        world.get_mut::<Node>(to).unwrap().border = border.0;
    }
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
    if let Some(mut image) = source.get::<ImageNode>(from).cloned() {
        if let Some(asset) = source
            .get_resource::<Assets<Image>>()
            .and_then(|images| images.get(&image.image))
        {
            world.init_resource::<Assets<Image>>();
            image.image = world.resource_mut::<Assets<Image>>().add(asset.clone());
        }
        world.entity_mut(to).insert(image);
    }
    copy::<UiTransform>(source, from, world, to);
    copy::<crate::native::tokens::TokenOverrides>(source, from, world, to);
    copy::<crate::native::token_style::BackgroundToken>(source, from, world, to);
    copy::<crate::native::token_style::BorderToken>(source, from, world, to);
    copy::<crate::native::token_style::TextToken>(source, from, world, to);
    if let Some(area) = source.get::<crate::native::sand_text::SandText>(from) {
        world.entity_mut(to).insert(area.tokens.clone());
    }
    if source
        .get::<crate::native::actions::ActionButton>(from)
        .is_some()
        || source.get::<EditableText>(from).is_some()
    {
        world.get_mut::<Node>(to).unwrap().border = UiRect::all(px(1));
        world
            .entity_mut(to)
            .insert(crate::native::token_style::border(
                crate::native::tokens::Token::Accent,
            ));
    }
    if let Some(icon) = source.get::<crate::native::icons::IconButton>(from) {
        let style = source
            .get::<crate::native::icons::IconStyle>(from)
            .copied()
            .unwrap_or_default();
        world.entity_mut(to).insert(Node {
            width: px(style.size + 2.0 * style.padding),
            height: px(style.size + 2.0 * style.padding),
            padding: UiRect::all(px(style.padding)),
            flex_shrink: 0.0,
            ..default()
        });
        if let Some(image) = crate::native::icons::image(world, icon.icon) {
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
        if let Some(node) = world.get::<Node>(entity) {
            let border = node.border;
            world.entity_mut(entity).insert(PreviewBorder(border));
        }
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
                .get::<crate::native::canvas::CanvasItem>(*entity)
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
            crate::native::canvas::CanvasItem {
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
        let area = world
            .get::<crate::native::area::InfluenceArea>(entity)
            .cloned();
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
                    crate::native::canvas_background::color(area.color).with_alpha(area.opacity),
                ),
                crate::native::token_style::border(crate::native::tokens::Token::Accent),
            ));
        }
    }
    container
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::RenderAssetUsages,
        render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    };

    #[test]
    fn preview_images_keep_source_pixels_when_asset_ids_collide_with_main_world_textures() {
        let mut source = World::new();
        source.init_resource::<Assets<Image>>();
        let pixels = [64, 128, 192, 255];
        let size = Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };
        let source_image = source.resource_mut::<Assets<Image>>().add(Image::new_fill(
            size,
            TextureDimension::D2,
            &pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ));
        let from = source.spawn(ImageNode::new(source_image.clone())).id();
        let mut world = World::new();
        world.init_resource::<Assets<Image>>();
        let unrelated = world.resource_mut::<Assets<Image>>().add(Image::new_fill(
            size,
            TextureDimension::D3,
            &[255; 4],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ));
        assert_eq!(source_image.id(), unrelated.id());
        let parent = world.spawn_empty().id();
        let preview = snapshot(&source, from, &mut world, parent);
        drop(source);
        let handle = &world.get::<ImageNode>(preview).unwrap().image;
        assert_ne!(handle.id(), unrelated.id());
        let images = world.resource::<Assets<Image>>();
        let copied = images.get(handle).unwrap();
        assert_eq!(copied.texture_descriptor.dimension, TextureDimension::D2);
        assert_eq!(copied.data.as_deref(), Some(pixels.as_slice()));
        assert_eq!(
            images.get(&unrelated).unwrap().texture_descriptor.dimension,
            TextureDimension::D3,
        );
    }
}
