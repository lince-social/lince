use super::{LayoutRuntime, MAX_DEPTH, desktop::compact_extent};
use crate::native::canvas::{presented, presented_spatial};
use bevy::prelude::*;
use std::cell::LazyCell;

fn parent_and_offset(world: &World, entity: Entity) -> (Option<Entity>, Vec2) {
    match world.get::<LayoutRuntime>(entity) {
        Some(runtime) => (runtime.parent, runtime.visual_offset),
        None => (None, Vec2::ZERO),
    }
}

pub(crate) fn visual_offset(world: &World, entity: Entity) -> Vec2 {
    parent_and_offset(world, entity).1
}

pub(crate) fn clip(world: &World, entity: Entity) -> (Vec2, Rect) {
    let Some(item) = presented(world, entity) else {
        return (Vec2::ZERO, Rect::default());
    };
    let (mut parent, offset) = parent_and_offset(world, entity);
    let transform = LazyCell::new(|| {
        let placement = presented_spatial(world, entity);
        let position = placement.position(item.position);
        (position, placement.rotation().inverse())
    });
    let half_size = item.size * 0.5;
    let mut visible = Rect::from_corners(Vec2::ZERO, item.size);
    if let Some(clip) = compact_extent(world, entity) {
        visible.max = visible.max.min(clip);
    }
    for _ in 0..MAX_DEPTH {
        let Some(entity) = parent else {
            break;
        };
        let Some(item) = presented(world, entity) else {
            break;
        };
        let (next_parent, parent_offset) = parent_and_offset(world, entity);
        let parent_position = presented_spatial(world, entity).position(item.position);
        let (position, inverse) = *transform;
        let relative = inverse * (parent_position - position);
        let min = Vec2::new(relative.x as f32, relative.z as f32) - item.size * 0.5 - parent_offset
            + half_size
            + offset;
        visible.min = visible.min.max(min);
        let size = compact_extent(world, entity).map_or(item.size, |clip| clip.min(item.size));
        visible.max = visible.max.min(min + size);
        parent = next_parent;
    }
    visible.max = visible.max.max(visible.min);
    (offset, visible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::canvas::{CanvasItem, ItemPreview};
    use bevy::math::DVec2;

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
        world.entity_mut(entity).insert(ItemPreview(preview));
        assert_eq!(clip(&world, entity).1.size(), preview.size);
        let authored_size = world.get::<CanvasItem>(entity).map(|item| item.size);
        assert_eq!(authored_size, Some(original.size));
        world.entity_mut(entity).remove::<ItemPreview>();
        assert_eq!(clip(&world, entity).1.size(), original.size);
    }

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
        let authored_y = world.get::<CanvasItem>(child).map(|item| item.position.y);
        assert_eq!(authored_y, Some(50.0));
    }
}
