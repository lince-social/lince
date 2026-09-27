use super::LayoutRuntime;
use crate::canvas::CanvasItem;
use bevy::prelude::*;

pub(crate) fn clip(world: &World, entity: Entity) -> (Vec2, Rect) {
    let Some(item) = world.get::<CanvasItem>(entity) else {
        return (Vec2::ZERO, Rect::default());
    };
    let runtime = world
        .get::<LayoutRuntime>(entity)
        .copied()
        .unwrap_or_default();
    let offset = runtime.visual_offset;
    let placement = crate::topology::spatial(world, entity);
    let position = placement.position(item.position);
    let half_size = item.size * 0.5;
    let mut visible = Rect::from_corners(Vec2::ZERO, item.size);
    let mut parent = runtime.parent;
    for _ in 0..super::MAX_DEPTH {
        let Some(entity) = parent else {
            break;
        };
        let Some(item) = world.get::<CanvasItem>(entity) else {
            break;
        };
        let runtime = world
            .get::<LayoutRuntime>(entity)
            .copied()
            .unwrap_or_default();
        let parent_position = crate::topology::spatial(world, entity).position(item.position);
        let relative = placement.rotation().inverse() * (parent_position - position);
        let min = Vec2::new(relative.x as f32, relative.z as f32)
            - item.size * 0.5
            - runtime.visual_offset
            + half_size
            + offset;
        visible.min = visible.min.max(min);
        visible.max = visible.max.min(min + item.size);
        parent = runtime.parent;
    }
    visible.max = visible.max.max(visible.min);
    (offset, visible)
}

#[cfg(test)]
mod tests {
    use super::*;
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
