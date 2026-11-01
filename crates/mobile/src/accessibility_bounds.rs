use bevy::prelude::*;

pub(super) fn modal(world: &mut World) -> Option<Entity> {
    world
        .query_filtered::<(Entity, Option<&GlobalZIndex>), With<crate::app::Modal>>()
        .iter(world)
        .max_by_key(|(entity, z)| (z.map_or(0, |z| z.0), entity.to_bits()))
        .map(|(entity, _)| entity)
}

pub(super) fn clipped(world: &World, entity: Entity, screen: Vec2) -> Option<Rect> {
    let node = world.get::<ComputedNode>(entity)?;
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let mut bounds = Rect::from_center_size(transform.translation, node.size())
        .intersect(Rect::from_corners(Vec2::ZERO, screen));
    let mut parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
    while let Some(current) = parent {
        if let (Some(node), Some(computed), Some(transform)) = (
            world.get::<Node>(current),
            world.get::<ComputedNode>(current),
            world.get::<UiGlobalTransform>(current),
        ) {
            if !node.overflow.is_visible() {
                let mut clip = computed.resolve_clip_rect(node.overflow, node.overflow_clip_margin);
                clip.min += transform.translation;
                clip.max += transform.translation;
                bounds = bounds.intersect(clip);
            }
        }
        parent = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    (bounds.width() > 0.0 && bounds.height() > 0.0).then_some(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_modal_keeps_accessibility_scope_above_the_retained_map() {
        let mut world = World::new();
        let panel = world.spawn((crate::app::Modal, GlobalZIndex(100))).id();
        let map = world.spawn((crate::app::Modal, GlobalZIndex(50))).id();
        assert_eq!(modal(&mut world), Some(panel));
        world.despawn(panel);
        assert_eq!(modal(&mut world), Some(map));
        world.despawn(map);
        assert_eq!(modal(&mut world), None);
    }

    fn node(world: &mut World, center: Vec2, size: Vec2, overflow: Overflow) -> Entity {
        world
            .spawn((
                Node {
                    overflow,
                    ..default()
                },
                ComputedNode {
                    size,
                    inverse_scale_factor: 0.5,
                    ..default()
                },
                UiGlobalTransform::from_translation(center),
            ))
            .id()
    }

    #[test]
    fn nested_inset_scroll_exports_only_visible_physical_control_bounds() {
        let mut world = World::new();
        let modal = node(
            &mut world,
            Vec2::new(200.0, 400.0),
            Vec2::new(360.0, 600.0),
            Overflow::clip(),
        );
        let viewport = node(
            &mut world,
            Vec2::new(200.0, 400.0),
            Vec2::new(300.0, 400.0),
            Overflow::scroll_y(),
        );
        world.entity_mut(viewport).insert(ChildOf(modal));
        let mut controls = Vec::new();
        for (center, size) in [
            (Vec2::new(200.0, 130.0), Vec2::new(100.0, 60.0)),
            (Vec2::new(20.0, 180.0), Vec2::new(100.0, 80.0)),
            (Vec2::new(200.0, 630.0), Vec2::new(100.0, 80.0)),
        ] {
            let control = node(&mut world, center, size, Overflow::visible());
            world.entity_mut(control).insert(ChildOf(viewport));
            controls.push(control);
        }
        let exported = controls
            .into_iter()
            .filter_map(|entity| clipped(&world, entity, Vec2::new(400.0, 800.0)))
            .collect::<Vec<_>>();
        assert_eq!(
            exported,
            vec![
                Rect::from_corners(Vec2::new(20.0, 200.0), Vec2::new(70.0, 220.0)),
                Rect::from_corners(Vec2::new(150.0, 590.0), Vec2::new(250.0, 600.0)),
            ]
        );
    }

    #[test]
    fn clipping_axes_preserve_visible_overflow_for_hidden_scroll_and_clip() {
        let mut world = World::new();
        let viewport = node(
            &mut world,
            Vec2::splat(200.0),
            Vec2::splat(100.0),
            Overflow::visible(),
        );
        let control = node(
            &mut world,
            Vec2::splat(140.0),
            Vec2::splat(100.0),
            Overflow::visible(),
        );
        world.entity_mut(control).insert(ChildOf(viewport));
        for axis in [
            OverflowAxis::Hidden,
            OverflowAxis::Scroll,
            OverflowAxis::Clip,
        ] {
            world.get_mut::<Node>(viewport).unwrap().overflow = Overflow {
                x: axis,
                y: OverflowAxis::Visible,
            };
            assert_eq!(
                clipped(&world, control, Vec2::splat(400.0)),
                Some(Rect::from_corners(
                    Vec2::new(150.0, 90.0),
                    Vec2::new(190.0, 190.0)
                ))
            );
        }
    }
}
