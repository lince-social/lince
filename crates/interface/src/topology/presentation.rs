use super::spatial;
use crate::{
    canvas::{CanvasItem, CanvasView},
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{
    asset::RenderAssetUsages,
    camera::{ImageRenderTarget, RenderTarget, visibility::RenderLayers},
    math::Affine2,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    ui::experimental::GhostNode,
};
use std::collections::HashSet;

#[derive(Component)]
pub struct Surface {
    pub camera: Entity,
    pub image: Handle<Image>,
    pub visual: Entity,
    pub body: Entity,
    pub face: Entity,
    pub size: Vec2,
    pub pixels: UVec2,
    pub density: f32,
    pub material: Handle<StandardMaterial>,
    pub uv: Rect,
}

#[derive(Component)]
pub struct VisualOwner(pub Entity);

#[derive(Component)]
pub struct WorldCamera;

#[derive(Resource)]
pub struct SceneCamera(pub Entity);

#[derive(Resource)]
pub struct BackgroundCamera(pub Entity);

#[derive(Component)]
pub struct SpatialRoot;

#[derive(Resource)]
struct SurfaceAssets {
    cube: Handle<Mesh>,
    side: Handle<StandardMaterial>,
}

pub fn ready(world: &World) -> bool {
    world.contains_resource::<Assets<StandardMaterial>>()
        && world.contains_resource::<Assets<Mesh>>()
}

pub fn origin(world: &World, root: Entity) -> bevy::math::DVec3 {
    world
        .get::<CanvasView>(root)
        .map_or(bevy::math::DVec3::ZERO, |view| {
            bevy::math::DVec3::new(view.center.x, 0.0, view.center.y)
        })
}

fn dimensions(size: Vec2, density: f32) -> (UVec2, f32) {
    let density = 2.0_f32
        .powf((density.max(1.0).log2() * 2.0).ceil() * 0.5)
        .min(4096.0 / size.max_element());
    ((size * density).ceil().max(Vec2::ONE).as_uvec2(), density)
}

fn density(
    world: &World,
    root: Entity,
    entity: Entity,
    item: &CanvasItem,
    physical_height: f32,
    scale_factor: f32,
) -> f32 {
    let scale = world
        .get::<crate::area_effects::AreaScale>(entity)
        .map_or(1.0, |scale| scale.0);
    let view = world
        .get::<super::view::View>(root)
        .copied()
        .unwrap_or_default();
    if !view.spatial {
        return world
            .get::<CanvasView>(root)
            .map_or(1.0, |canvas| canvas.zoom as f32)
            * scale_factor
            * scale;
    }
    let placement = spatial(world, entity);
    let rotation = Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch, 0.0).as_dquat();
    let camera = bevy::math::DVec3::from_array(view.position);
    let mut nearest = f64::INFINITY;
    for x in [-0.5, 0.5] {
        for z in [-0.5, 0.5] {
            let point = placement.position(item.position)
                + placement.rotation()
                    * bevy::math::DVec3::new(
                        f64::from(item.size.x * scale * x),
                        0.0,
                        f64::from(item.size.y * scale * z),
                    );
            nearest = nearest.min(-(rotation.inverse() * (point - camera)).z);
        }
    }
    physical_height * scale
        / (2.0 * (PerspectiveProjection::default().fov * 0.5).tan() * (nearest as f32).max(0.5))
}

fn image(size: UVec2) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    image
}

pub fn synchronize(world: &mut World) {
    if !ready(world) {
        return;
    }
    let viewport = world.query::<&Window>().iter(world).next().map(|window| {
        (
            window.resolution.physical_size(),
            window.resolution.scale_factor(),
        )
    });
    let Some((physical_size, scale_factor)) = viewport else {
        return;
    };
    if !world.contains_resource::<SurfaceAssets>() {
        let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::default());
        let side = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.17, 0.24),
                unlit: true,
                ..default()
            });
        world.insert_resource(SurfaceAssets { cube, side });
    }
    let roots: Vec<_> = world
        .query::<(Entity, &Workspaces)>()
        .iter(world)
        .map(|(e, _)| e)
        .collect();
    for root in &roots {
        if world.get::<SpatialRoot>(*root).is_none() {
            world
                .entity_mut(*root)
                .remove::<(Node, BackgroundColor)>()
                .insert((
                    GhostNode,
                    SpatialRoot,
                    Transform::default(),
                    Visibility::default(),
                ));
        }
        if let Some(mut node) = world.get_mut::<ComputedNode>(*root) {
            if node.size != physical_size.as_vec2() || node.content_size != physical_size.as_vec2()
            {
                node.size = physical_size.as_vec2();
                node.content_size = node.size;
            }
        }
        let target = world.query_filtered::<&ComputedUiRenderTargetInfo, With<crate::canvas_controls::CanvasToolbar>>().iter(world).next().copied();
        if let Some(target) = target {
            world.entity_mut(*root).insert(target);
        }
        if let Some(mut transform) = world.get_mut::<UiGlobalTransform>(*root) {
            transform.set_if_neq(UiGlobalTransform::from(Affine2::from_translation(
                physical_size.as_vec2() * 0.5,
            )));
        }
    }
    let entities: Vec<_> = world
        .query::<(Entity, &CanvasItem, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(_, _, p, _)| roots.contains(&p.parent()))
        .map(|(e, item, p, member)| (e, *item, p.parent(), member.0))
        .collect();
    let living: HashSet<_> = entities.iter().map(|(e, _, _, _)| *e).collect();
    let stale: Vec<_> = world
        .query::<(Entity, &VisualOwner)>()
        .iter(world)
        .filter(|(_, owner)| !living.contains(&owner.0))
        .map(|(e, _)| e)
        .collect();
    for entity in stale {
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
    for (entity, item, root, workspace) in entities {
        if !item.size.is_finite() || item.size.min_element() <= 0.0 {
            continue;
        }
        if world.get::<crate::sand_placement::Pinned>(entity).is_some() {
            if let Some(surface) = world.entity_mut(entity).take::<Surface>() {
                world.despawn(surface.camera);
                world.despawn(surface.visual);
                world
                    .entity_mut(entity)
                    .remove::<UiTargetCamera>()
                    .insert(UiTransform::default());
            }
            continue;
        }
        if world.get::<crate::area::InfluenceArea>(entity).is_some()
            || world.get::<super::assets::ImportedAsset>(entity).is_some()
            || world.get::<crate::sand_placement::Pinned>(entity).is_some()
        {
            continue;
        }
        let (pixels, density) = dimensions(
            item.size,
            density(
                world,
                root,
                entity,
                &item,
                physical_size.y as f32,
                scale_factor,
            ),
        );
        if world.get::<Surface>(entity).is_none() {
            let image = world.resource_mut::<Assets<Image>>().add(image(pixels));
            let camera = world
                .spawn((
                    Camera2d,
                    Camera {
                        order: -1,
                        clear_color: ClearColorConfig::Custom(Color::NONE),
                        ..default()
                    },
                    RenderTarget::Image(ImageRenderTarget {
                        handle: image.clone(),
                        scale_factor: density,
                    }),
                    RenderLayers::layer(31),
                    VisualOwner(entity),
                ))
                .id();
            let material = world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial {
                    base_color_texture: Some(image.clone()),
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    depth_bias: 2.0,
                    ..default()
                });
            let side_material = world.resource::<SurfaceAssets>().side.clone();
            let cube = world.resource::<SurfaceAssets>().cube.clone();
            let rectangle = world
                .resource_mut::<Assets<Mesh>>()
                .add(Rectangle::default());
            let visual = world
                .spawn((
                    Transform::default(),
                    Visibility::default(),
                    VisualOwner(entity),
                ))
                .id();
            let body = world
                .spawn((
                    Mesh3d(cube),
                    Pickable::IGNORE,
                    MeshMaterial3d(side_material),
                    Transform::default(),
                    ChildOf(visual),
                    VisualOwner(entity),
                ))
                .id();
            let face = world
                .spawn((
                    Mesh3d(rectangle),
                    Pickable::IGNORE,
                    MeshMaterial3d(material.clone()),
                    Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                    ChildOf(visual),
                    VisualOwner(entity),
                ))
                .id();
            world.entity_mut(entity).insert((
                UiTargetCamera(camera),
                Surface {
                    camera,
                    image,
                    visual,
                    body,
                    face,
                    size: item.size,
                    pixels,
                    density,
                    material,
                    uv: Rect::from_corners(Vec2::ZERO, Vec2::ONE),
                },
            ));
        }
        let surface = world.get::<Surface>(entity).unwrap();
        let (camera, image_handle, visual, body, face, old_pixels) = (
            surface.camera,
            surface.image.clone(),
            surface.visual,
            surface.body,
            surface.face,
            surface.pixels,
        );
        let material = surface.material.clone();
        if pixels != old_pixels {
            world
                .resource_mut::<Assets<Image>>()
                .insert(image_handle.id(), image(pixels))
                .unwrap();
            world
                .resource_mut::<Assets<StandardMaterial>>()
                .get_mut(&material)
                .unwrap()
                .base_color_texture = Some(image_handle.clone());
        }
        if world.get::<Surface>(entity).unwrap().density != density {
            world
                .entity_mut(camera)
                .insert(RenderTarget::Image(ImageRenderTarget {
                    handle: image_handle.clone(),
                    scale_factor: density,
                }));
        }
        let placement = spatial(world, entity);
        let scale = world
            .get::<crate::area_effects::AreaScale>(entity)
            .map_or(1.0, |s| s.0);
        let depth = placement.depth(item.size) as f32;
        let (offset, clip) = crate::layout::viewport::clip(world, entity);
        let clipped_size = clip.size();
        let clipped_center = clip.center() - item.size * 0.5;
        let uv = Rect::from_corners(clip.min / item.size, clip.max / item.size);
        if world.get::<Surface>(entity).unwrap().uv != uv {
            let handle = world.get::<Mesh3d>(face).unwrap().0.clone();
            if let Some(mut mesh) = world.resource_mut::<Assets<Mesh>>().get_mut(&handle) {
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_UV_0,
                    vec![
                        [uv.max.x, uv.min.y],
                        [uv.min.x, uv.min.y],
                        [uv.min.x, uv.max.y],
                        [uv.max.x, uv.max.y],
                    ],
                );
            }
        }
        let transform = Transform {
            translation: (placement.position(item.position) - origin(world, root)).as_vec3()
                - placement.rotation().as_quat() * Vec3::new(offset.x, 0.0, offset.y),
            rotation: placement.rotation().as_quat(),
            scale: Vec3::splat(scale),
        };
        world
            .get_mut::<Transform>(visual)
            .unwrap()
            .set_if_neq(transform);
        world.get_mut::<Transform>(body).unwrap().set_if_neq(
            Transform::from_xyz(clipped_center.x, -depth * 0.5, clipped_center.y)
                .with_scale(Vec3::new(clipped_size.x, depth, clipped_size.y)),
        );
        world.get_mut::<Transform>(face).unwrap().set_if_neq(
            Transform::from_xyz(clipped_center.x, 0.01, clipped_center.y)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::new(clipped_size.x, clipped_size.y, 1.0)),
        );
        let visible = world
            .get::<Workspaces>(root)
            .is_some_and(|spaces| spaces.active == workspace)
            && clipped_size.min_element() > 0.0;
        world
            .get_mut::<Visibility>(visual)
            .unwrap()
            .set_if_neq(if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            });
        if world.get::<Camera>(camera).unwrap().is_active != visible {
            world.get_mut::<Camera>(camera).unwrap().is_active = visible;
        }
        let mut surface = world.get_mut::<Surface>(entity).unwrap();
        surface.size = item.size;
        surface.pixels = pixels;
        surface.density = density;
        surface.uv = uv;
    }
}

pub fn bounds(world: &World, entity: Entity) -> Option<Rect> {
    let Some(item) = world.get::<CanvasItem>(entity) else {
        return content_bounds(world, entity);
    };
    let root = world.get::<ChildOf>(entity)?.parent();
    if world.get::<SpatialRoot>(root).is_none()
        || world.get::<crate::sand_placement::Pinned>(entity).is_some()
    {
        return None;
    }
    let camera = world
        .get_entity(world.get_resource::<SceneCamera>()?.0)
        .ok()?;
    let (camera, transform) = (camera.get::<Camera>()?, camera.get::<GlobalTransform>()?);
    let placement = spatial(world, entity);
    let origin = origin(world, root);
    let (offset, clip) = crate::layout::viewport::clip(world, entity);
    if clip.size().min_element() <= 0.0 {
        return None;
    }
    let scale = world
        .get::<crate::area_effects::AreaScale>(entity)
        .map_or(1.0, |scale| f64::from(scale.0));
    let (low, high) = if let Some(bounds) = world.get::<super::assets::Bounds>(entity) {
        let scale = f64::from(super::assets::effective_scale(
            world,
            entity,
            world.get::<super::assets::ImportedAsset>(entity)?,
        ));
        (bounds.min.as_dvec3() * scale, bounds.max.as_dvec3() * scale)
    } else {
        let depth = world
            .get::<crate::area::InfluenceArea>(entity)
            .map_or_else(|| placement.depth(item.size), |area| area.depth);
        (
            bevy::math::DVec3::new(
                f64::from(clip.min.x - item.size.x * 0.5),
                -depth,
                f64::from(clip.min.y - item.size.y * 0.5),
            ) * scale,
            bevy::math::DVec3::new(
                f64::from(clip.max.x - item.size.x * 0.5),
                0.0,
                f64::from(clip.max.y - item.size.y * 0.5),
            ) * scale,
        )
    };
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for x in [low.x, high.x] {
        for z in [low.z, high.z] {
            for y in [low.y, high.y] {
                let point = placement.position(item.position)
                    - placement.rotation()
                        * bevy::math::DVec3::new(f64::from(offset.x), 0.0, f64::from(offset.y))
                    + placement.rotation() * bevy::math::DVec3::new(x, y, z);
                if let Ok(point) = camera.world_to_viewport(transform, (point - origin).as_vec3()) {
                    min = min.min(point);
                    max = max.max(point);
                }
            }
        }
    }
    min.is_finite().then(|| Rect::from_corners(min, max))
}

fn content_bounds(world: &World, entity: Entity) -> Option<Rect> {
    let node = world.get::<ComputedNode>(entity)?;
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let mut owner = entity;
    let surface = loop {
        if let Some(surface) = world.get::<Surface>(owner) {
            break surface;
        }
        owner = world.get::<ChildOf>(owner)?.parent();
    };
    let camera = world
        .get_entity(world.get_resource::<SceneCamera>()?.0)
        .ok()?;
    let (camera, camera_transform) = (camera.get::<Camera>()?, camera.get::<GlobalTransform>()?);
    let face = world.get::<GlobalTransform>(surface.face)?;
    let a = transform
        .transform_point2(-node.size() * 0.5)
        .clamp(Vec2::ZERO, surface.pixels.as_vec2());
    let b = transform
        .transform_point2(node.size() * 0.5)
        .clamp(Vec2::ZERO, surface.pixels.as_vec2());
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for point in [a, b, Vec2::new(a.x, b.y), Vec2::new(b.x, a.y)] {
        let uv = (point / surface.pixels.as_vec2()).clamp(surface.uv.min, surface.uv.max);
        let uv = (uv - surface.uv.min) / surface.uv.size();
        let point = face.transform_point(Vec3::new(uv.x - 0.5, 0.5 - uv.y, 0.0));
        if let Ok(point) = camera.world_to_viewport(camera_transform, point) {
            min = min.min(point);
            max = max.max(point);
        }
    }
    min.is_finite().then(|| Rect::from_corners(min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surfaces_rerasterize_at_zoom_and_display_density() {
        let size = Vec2::new(300.0, 120.0);
        assert_eq!(dimensions(size, 1.0), (UVec2::new(300, 120), 1.0));
        assert_eq!(dimensions(size, 4.0), (UVec2::new(1200, 480), 4.0));
        assert_eq!(dimensions(size, 8.0), (UVec2::new(2400, 960), 8.0));
        let (pixels, density) = dimensions(size, 1000.0);
        assert!(pixels.max_element() <= 4096);
        assert!((pixels.as_vec2() / density - size).abs().max_element() < 1.0);
        assert_eq!(dimensions(size, 1.1), dimensions(size, 1.2));
    }
}
