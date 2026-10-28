use super::*;

pub(super) fn synchronize(world: &mut World, root: Entity, physical_size: UVec2) {
    if world.get::<Disabled>(root).is_some() {
        return;
    }
    if world.get::<SpatialRoot>(root).is_none() {
        world
            .entity_mut(root)
            .remove::<(Node, BackgroundColor)>()
            .insert((
                GhostNode,
                SpatialRoot,
                Transform::default(),
                Visibility::default(),
            ));
    }
    let target = world
        .query_filtered::<(&ChildOf, &ComputedUiRenderTargetInfo, &ComputedNode), With<crate::native::canvas_controls::CanvasToolbar>>()
        .iter(world)
        .find(|(parent, _, _)| parent.parent() == root)
        .map(|(_, target, node)| (*target, node.inverse_scale_factor()));
    if let Some(mut node) = world.get_mut::<ComputedNode>(root) {
        let size = physical_size.as_vec2();
        let inverse = target.map_or(node.inverse_scale_factor(), |(_, inverse)| inverse);
        if node.size != size
            || node.content_size != size
            || node.unrounded_size != size
            || node.inverse_scale_factor != inverse
        {
            node.size = size;
            node.content_size = size;
            node.unrounded_size = size;
            node.inverse_scale_factor = inverse;
        }
    }
    if let Some((target, _)) = target {
        if let Some(mut current) = world.get_mut::<ComputedUiRenderTargetInfo>(root) {
            current.set_if_neq(target);
        } else {
            world.entity_mut(root).insert(target);
        }
    }
    if let Some(mut transform) = world.get_mut::<UiGlobalTransform>(root) {
        transform.set_if_neq(UiGlobalTransform::from(Affine2::from_translation(
            physical_size.as_vec2() * 0.5,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_canvas_tracks_its_own_toolbar_density_and_complete_physical_bounds() {
        let mut world = World::new();
        let root = world
            .spawn((Node::default(), UiGlobalTransform::default()))
            .id();
        let other = world
            .spawn((Node::default(), UiGlobalTransform::default()))
            .id();
        let other_bar = crate::native::canvas_controls::toolbar(&mut world, other);
        world
            .get_mut::<ComputedNode>(other_bar)
            .unwrap()
            .inverse_scale_factor = 0.25;
        let bar = crate::native::canvas_controls::toolbar(&mut world, root);
        for (inverse, size) in [
            (1.0, UVec2::new(1000, 700)),
            (0.8, UVec2::new(1000, 700)),
            (2.0 / 3.0, UVec2::new(360, 700)),
            (0.5, UVec2::new(720, 1280)),
            (1.0, UVec2::new(800, 600)),
        ] {
            world
                .get_mut::<ComputedNode>(bar)
                .unwrap()
                .inverse_scale_factor = inverse;
            synchronize(&mut world, root, size);
            let node = world.get::<ComputedNode>(root).unwrap();
            assert_eq!(node.size(), size.as_vec2());
            assert_eq!(node.content_size(), size.as_vec2());
            assert_eq!(node.unrounded_size(), size.as_vec2());
            assert_eq!(node.inverse_scale_factor(), inverse);
            assert_eq!(
                world
                    .get::<UiGlobalTransform>(root)
                    .unwrap()
                    .transform_point2(Vec2::ZERO),
                size.as_vec2() * 0.5
            );
            assert!(world.get::<Node>(root).is_none());
            assert!(world.get::<GhostNode>(root).is_some());
            assert_eq!(
                world
                    .get::<ComputedNode>(other)
                    .unwrap()
                    .inverse_scale_factor(),
                1.0
            );
        }
    }
}
