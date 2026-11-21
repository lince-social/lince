// non-lean-yet
// Scope: fn changed, fn image, fn clear_color, fn layers.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 16, 39, 91.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (24, 32, 86).

use super::*;
use bevy::camera::{ImageRenderTarget, RenderTarget, ScalingMode};

fn changed() {
    nucleus::performance::add("native-output-component-writes", 1);
}

pub(super) fn image(world: &mut World, camera: Entity, image: &Handle<Image>, scale: f32) {
    if !world.get::<RenderTarget>(camera).is_some_and(|target| {
        matches!(target, RenderTarget::Image(current)
            if current.handle == *image && current.scale_factor == scale)
    }) {
        world
            .entity_mut(camera)
            .insert(RenderTarget::Image(ImageRenderTarget {
                handle: image.clone(),
                scale_factor: scale,
            }));
        changed();
    }
}

pub(super) fn clear_color(world: &mut World, camera: Entity, color: ClearColorConfig) {
    let current = &world.get::<Camera>(camera).unwrap().clear_color;
    let equal = match (current, &color) {
        (ClearColorConfig::Default, ClearColorConfig::Default)
        | (ClearColorConfig::None, ClearColorConfig::None) => true,
        (ClearColorConfig::Custom(a), ClearColorConfig::Custom(b)) => a == b,
        _ => false,
    };
    if !equal {
        world.get_mut::<Camera>(camera).unwrap().clear_color = color;
        changed();
    }
}

pub(super) fn layers(world: &mut World, entity: Entity, layers: &RenderLayers) {
    if world.get::<RenderLayers>(entity) != Some(layers) {
        world.entity_mut(entity).insert(layers.clone());
        changed();
    }
}

pub(super) fn transform(world: &mut World, entity: Entity, transform: Transform) {
    if world.get::<Transform>(entity) != Some(&transform) {
        world.entity_mut(entity).insert(transform);
        changed();
    }
}

pub(super) fn projection(world: &mut World, camera: Entity, scale: f32) {
    let equal = world.get::<Projection>(camera).is_some_and(|projection| {
        matches!(projection, Projection::Orthographic(current)
            if current.scale == scale
                && current.near == -1_000_000.0
                && current.far == 1_000_000.0
                && current.viewport_origin == Vec2::splat(0.5)
                && matches!(current.scaling_mode, ScalingMode::WindowSize))
    });
    if !equal {
        world
            .entity_mut(camera)
            .insert(Projection::Orthographic(OrthographicProjection {
                scale,
                near: -1_000_000.0,
                far: 1_000_000.0,
                ..OrthographicProjection::default_3d()
            }));
        changed();
    }
}

pub(super) fn owner(world: &mut World, entity: Entity, owner: Entity) {
    if world
        .get::<super::super::presentation::VisualOwner>(entity)
        .is_none_or(|current| current.0 != owner)
    {
        world
            .entity_mut(entity)
            .insert(super::super::presentation::VisualOwner(owner));
        changed();
    }
}

pub(super) fn surface(world: &mut World, entity: Entity, source: Entity, layer: &RenderLayers) {
    let material = &world.get::<Surface>(source).unwrap().material;
    if world
        .get::<MeshMaterial3d<StandardMaterial>>(entity)
        .is_none_or(|current| current.0 != *material)
    {
        let material = material.clone();
        world.entity_mut(entity).insert(MeshMaterial3d(material));
        changed();
    }
    layers(world, entity, layer);
    owner(world, entity, source);
    if world
        .get::<super::super::compositor::SurfaceFace>(entity)
        .is_none()
    {
        world
            .entity_mut(entity)
            .insert(super::super::compositor::SurfaceFace);
        changed();
    }
}
