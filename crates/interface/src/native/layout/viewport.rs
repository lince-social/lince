use super::LayoutRuntime;
use bevy::prelude::*;

pub(crate) fn clip(world: &World, entity: Entity) -> (Vec2, Rect) {
    let Some(item) = crate::native::canvas::presented(world, entity) else {
        return (Vec2::ZERO, Rect::default());
    };
    let runtime = world
        .get::<LayoutRuntime>(entity)
        .copied()
        .unwrap_or_default();
    let offset = runtime.visual_offset;
    let placement = crate::native::canvas::presented_spatial(world, entity);
    let position = placement.position(item.position);
    let half_size = item.size * 0.5;
    let mut visible = Rect::from_corners(Vec2::ZERO, item.size);
    if let Some(clip) = super::desktop::compact_extent(world, entity) { visible.max = visible.max.min(clip); }
    let mut parent = runtime.parent;
    for _ in 0..super::MAX_DEPTH {
        let Some(entity) = parent else {
            break;
        };
        let Some(item) = crate::native::canvas::presented(world, entity) else {
            break;
        };
        let runtime = world
            .get::<LayoutRuntime>(entity)
            .copied()
            .unwrap_or_default();
        let parent_position =
            crate::native::canvas::presented_spatial(world, entity).position(item.position);
        let relative = placement.rotation().inverse() * (parent_position - position);
        let min = Vec2::new(relative.x as f32, relative.z as f32)
            - item.size * 0.5
            - runtime.visual_offset
            + half_size
            + offset;
        visible.min = visible.min.max(min);
        let size = super::desktop::compact_extent(world, entity).map_or(item.size, |clip| clip.min(item.size));
        visible.max = visible.max.min(min + size);
        parent = runtime.parent;
    }
    visible.max = visible.max.max(visible.min);
    (offset, visible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::canvas::CanvasItem;

    #[test]
    fn preview_clipping_uses_presented_sizes_without_mutating_authored_items() {
        let mut world = World::new();
        let original = CanvasItem {
            position: DVec2::ZERO,
            size: Vec2::splat(100.0),
        };
        let entity = world.spawn(original).id();
        let preview = CanvasItem {
            size: Vec2::new(180.0, 140.0),
            ..original
        };
        world
            .entity_mut(entity)
            .insert(crate::native::canvas::ItemPreview(preview));
        assert_eq!(clip(&world, entity).1.size(), preview.size);
        assert_eq!(world.get::<CanvasItem>(entity).unwrap().size, original.size);
        world
            .entity_mut(entity)
            .remove::<crate::native::canvas::ItemPreview>();
        assert_eq!(clip(&world, entity).1.size(), original.size);
    }
    use bevy::math::DVec2;

    #[test]
    fn nested_scroll_crops_visible_pixels_without_moving_membership() {
        let mut world = World::new();
        let parent = world
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                LayoutRuntime {
                    scroll: Vec2::new(0.0, 40.0),
                    ..default()
                },
            ))
            .id();
        let child = world
            .spawn((
                CanvasItem {
                    position: DVec2::new(0.0, 50.0),
                    size: Vec2::new(80.0, 120.0),
                },
                LayoutRuntime {
                    parent: Some(parent),
                    visual_offset: Vec2::new(0.0, 40.0),
                    ..default()
                },
            ))
            .id();
        let (offset, rect) = clip(&world, child);
        assert_eq!(offset.y, 40.0);
        assert_eq!(rect, Rect::from_corners(Vec2::ZERO, Vec2::new(80.0, 100.0)));
        assert_eq!(world.get::<CanvasItem>(child).unwrap().position.y, 50.0);
    }
}
