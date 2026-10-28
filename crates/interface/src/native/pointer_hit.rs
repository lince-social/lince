use bevy::{camera::RenderTarget, prelude::*};

pub(crate) fn nodes(world: &World, window: Entity, position: Vec2) -> Vec<Entity> {
    let Some(target) = world
        .get_resource::<crate::native::app::OutputTarget>()
        .map_or_else(
            || RenderTarget::Window(bevy::window::WindowRef::Entity(window)),
            |target| target.0.clone(),
        )
        .normalize(Some(window))
    else {
        return Vec::new();
    };
    world
        .get_resource::<bevy::ui::UiStack>()
        .into_iter()
        .flat_map(|stack| stack.uinodes.iter().rev().copied())
        .filter(|entity| {
            let Some(node) = world.get::<ComputedNode>(*entity) else {
                return false;
            };
            if node.size().min_element() <= 0.0
                || world
                    .get::<InheritedVisibility>(*entity)
                    .is_none_or(|visible| !visible.get())
                || world
                    .get::<Pickable>(*entity)
                    .is_some_and(|pickable| !pickable.should_block_lower && !pickable.is_hoverable)
            {
                return false;
            }
            let Some(camera) = world
                .get::<bevy::ui::ComputedUiTargetCamera>(*entity)
                .and_then(|camera| camera.get())
            else {
                return false;
            };
            if world
                .get::<RenderTarget>(camera)
                .and_then(|target| target.normalize(Some(window)))
                .as_ref()
                != Some(&target)
            {
                return false;
            }
            let Some(camera) = world.get::<Camera>(camera) else {
                return false;
            };
            let point = position * camera.target_scaling_factor().unwrap_or(1.0)
                - camera
                    .physical_viewport_rect()
                    .map_or(Vec2::ZERO, |viewport| viewport.min.as_vec2());
            world
                .get::<UiGlobalTransform>(*entity)
                .is_some_and(|transform| node.contains_point(*transform, point))
                && world
                    .get::<bevy::ui::CalculatedClip>(*entity)
                    .is_none_or(|clip| clip.clip.contains(point))
        })
        .collect()
}
