use super::{
    spatial,
    surface_budget::{self, Request, dimensions},
    surface_render::{SurfaceCapture, SurfacePass},
};
use crate::{
    canvas::{CanvasItem, CanvasView},
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{
    asset::RenderAssetUsages,
    camera::{ImageRenderTarget, RenderTarget, visibility::RenderLayers},
    ecs::{entity_disabling::Disabled, query::Allow},
    math::Affine2,
    prelude::*,
    render::camera::CameraRenderGraph,
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
    pub visible: bool,
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

fn request(
    world: &World,
    root: Entity,
    entity: Entity,
    item: &CanvasItem,
    physical_size: UVec2,
    scale_factor: f32,
) -> Request {
    let (offset, clip) = crate::layout::viewport::clip(world, entity);
    let active = world
        .get::<Workspaces>(root)
        .zip(world.get::<WorkspaceMember>(entity))
        .is_some_and(|(spaces, member)| spaces.active == member.0)
        && world.get::<Disabled>(root).is_none()
        && world.get::<Disabled>(entity).is_none()
        && clip.size().min_element() > 0.0
        && world
            .get::<crate::protein_area::placement::Pending>(entity)
            .is_none();
    let previous = world
        .get::<Surface>(entity)
        .filter(|surface| surface.visible)
        .map_or(0.0, |surface| surface.density);
    let mut request = Request {
        size: item.size,
        density: 1.0,
        previous,
        visible: active,
    };
    if !active || !item.size.is_finite() || item.size.min_element() <= 0.0 {
        request.visible = false;
        return request;
    }
    let scale = world
        .get::<crate::area_effects::AreaScale>(entity)
        .map_or(1.0, |scale| scale.0);
    let view = world
        .get::<super::view::View>(root)
        .copied()
        .unwrap_or_default();
    let placement = spatial(world, entity);
    let depth = if world.get::<crate::area::InfluenceArea>(entity).is_some() {
        0.0
    } else {
        placement.depth(item.size) * f64::from(scale)
    };
    let points: [bevy::math::DVec3; 8] = std::array::from_fn(|index| {
        let point = Vec2::new(
            if index & 1 == 0 {
                clip.min.x
            } else {
                clip.max.x
            },
            if index & 2 == 0 {
                clip.min.y
            } else {
                clip.max.y
            },
        ) - item.size * 0.5;
        placement.position(item.position)
            + placement.rotation()
                * bevy::math::DVec3::new(
                    f64::from(point.x * scale - offset.x),
                    if index & 4 == 0 {
                        0.01 * f64::from(scale)
                    } else {
                        -depth
                    },
                    f64::from(point.y * scale - offset.y),
                )
    });
    if !view.spatial {
        let canvas = world.get::<CanvasView>(root).copied().unwrap_or_default();
        let half = physical_size.as_vec2().as_dvec2() / f64::from(scale_factor) / canvas.zoom * 0.5;
        let min = canvas.center - half;
        let max = canvas.center + half;
        request.visible = !points.iter().all(|point| point.x < min.x)
            && !points.iter().all(|point| point.x > max.x)
            && !points.iter().all(|point| point.z < min.y)
            && !points.iter().all(|point| point.z > max.y);
        request.density = canvas.zoom as f32 * scale_factor * scale;
        return request;
    }
    let rotation = Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch, 0.0).as_dquat();
    let camera = bevy::math::DVec3::from_array(view.position);
    let points = points.map(|point| rotation.inverse() * (point - camera));
    let tangent = (PerspectiveProjection::default().fov * 0.5).tan();
    request.visible = surface_budget::perspective_visible(
        &points,
        f64::from(physical_size.x) / f64::from(physical_size.y.max(1)),
        f64::from(tangent),
    );
    let nearest = points[..4]
        .iter()
        .map(|point| -point.z)
        .fold(f64::INFINITY, f64::min);
    request.density = physical_size.y as f32 * scale / (2.0 * tangent * (nearest as f32).max(0.5));
    request
}

fn image(size: UVec2) -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
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
        .query_filtered::<(Entity, &Workspaces), Allow<Disabled>>()
        .iter(world)
        .map(|(e, _)| e)
        .collect();
    for root in &roots {
        if world.get::<Disabled>(*root).is_some() {
            continue;
        }
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
            if let Some(mut current) = world.get_mut::<ComputedUiRenderTargetInfo>(*root) {
                current.set_if_neq(target);
            } else {
                world.entity_mut(*root).insert(target);
            }
        }
        if let Some(mut transform) = world.get_mut::<UiGlobalTransform>(*root) {
            transform.set_if_neq(UiGlobalTransform::from(Affine2::from_translation(
                physical_size.as_vec2() * 0.5,
            )));
        }
    }
    let entities: Vec<_> = world
        .query_filtered::<(Entity, &CanvasItem, &ChildOf, &WorkspaceMember), Allow<Disabled>>()
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
    let requests: Vec<_> = entities
        .iter()
        .map(|(entity, item, root, _)| {
            let mut request = request(world, *root, *entity, item, physical_size, scale_factor);
            request.visible &= world.get::<super::assets::ImportedAsset>(*entity).is_none()
                && world
                    .get::<crate::sand_placement::Pinned>(*entity)
                    .is_none();
            request
        })
        .collect();
    let resolutions = surface_budget::plan(&requests, surface_budget::PIXEL_BUDGET);
    for (((entity, item, root, _), request), (pixels, density)) in
        entities.into_iter().zip(requests).zip(resolutions)
    {
        if !item.size.is_finite() || item.size.min_element() <= 0.0 {
            continue;
        }
        if world.get::<crate::sand_placement::Pinned>(entity).is_some() {
            if let Some(surface) = world.entity_mut(entity).take::<Surface>() {
                world.despawn(surface.camera);
                world.despawn(surface.visual);
                world
                    .entity_mut(entity)
                    .remove::<(UiTargetCamera, bevy::ui::LayoutConfig)>()
                    .insert(UiTransform::default());
            }
            continue;
        }
        if world.get::<super::assets::ImportedAsset>(entity).is_some()
            || world.get::<crate::sand_placement::Pinned>(entity).is_some()
        {
            continue;
        }
        let visible = request.visible;
        if world.get::<Surface>(entity).is_none() {
            let image = world.resource_mut::<Assets<Image>>().add(image(pixels));
            let camera = world
                .spawn((
                    Camera2d,
                    Camera {
                        order: -1,
                        is_active: visible,
                        clear_color: ClearColorConfig::Custom(Color::NONE),
                        ..default()
                    },
                    Msaa::Off,
                    CameraRenderGraph::new(SurfacePass),
                    SurfaceCapture(image.clone()),
                    RenderTarget::Image(ImageRenderTarget {
                        handle: image.clone(),
                        scale_factor: density,
                    }),
                    RenderLayers::layer(31),
                    VisualOwner(entity),
                ))
                .remove::<bevy::camera::visibility::VisibleEntities>()
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
                    Visibility::Inherited,
                    ChildOf(visual),
                    VisualOwner(entity),
                ))
                .id();
            let face = world
                .spawn((
                    Mesh3d(rectangle),
                    Pickable::IGNORE,
                    MeshMaterial3d(material.clone()),
                    Visibility::Inherited,
                    Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                    ChildOf(visual),
                    VisualOwner(entity),
                ))
                .id();
            world.entity_mut(entity).insert((
                UiTargetCamera(camera),
                bevy::ui::LayoutConfig { use_rounding: false },
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
                    visible,
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
        let target = if visible {
            RenderTarget::Image(ImageRenderTarget {
                handle: image_handle.clone(),
                scale_factor: density,
            })
        } else {
            RenderTarget::None {
                size: dimensions(item.size, 1.0).0,
            }
        };
        let target_changed = match (world.get::<RenderTarget>(camera), &target) {
            (Some(RenderTarget::Image(old)), RenderTarget::Image(new)) => {
                old.handle != new.handle || old.scale_factor != new.scale_factor
            }
            (Some(RenderTarget::None { size: old }), RenderTarget::None { size: new }) => {
                old != new
            }
            _ => true,
        };
        if target_changed {
            world.entity_mut(camera).insert(target);
        }
        let placement = spatial(world, entity);
        let area = world.get::<crate::area::InfluenceArea>(entity).is_some();
        world
            .get_mut::<Visibility>(body)
            .unwrap()
            .set_if_neq(if area {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
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
            Transform::from_xyz(
                clipped_center.x,
                if area { 0.0 } else { 0.01 },
                clipped_center.y,
            )
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::new(clipped_size.x, clipped_size.y, 1.0)),
        );
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
        let surface = world.get::<Surface>(entity).unwrap();
        if surface.size != item.size
            || surface.pixels != pixels
            || surface.density != density
            || surface.uv != uv
            || surface.visible != visible
        {
            let mut surface = world.get_mut::<Surface>(entity).unwrap();
            surface.size = item.size;
            surface.pixels = pixels;
            surface.density = density;
            surface.uv = uv;
            surface.visible = visible;
        }
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
    if !surface.visible {
        return None;
    }
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

    fn scene(count: usize) -> (World, Entity, Vec<Entity>) {
        let mut world = World::new();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.spawn(Window {
            resolution: (1920, 1080).into(),
            ..default()
        });
        let root = world
            .spawn((
                Workspaces::default(),
                CanvasView::default(),
                super::super::view::View::default(),
            ))
            .id();
        let entities = (0..count)
            .map(|index| {
                world
                    .spawn((
                        CanvasItem {
                            position: bevy::math::DVec2::new(
                                (index % 32) as f64 * 360.0,
                                (index / 32) as f64 * 700.0,
                            ),
                            size: if index % 2 == 0 {
                                Vec2::splat(300.0)
                            } else {
                                Vec2::new(340.0, 640.0)
                            },
                        },
                        ChildOf(root),
                        WorkspaceMember(1),
                    ))
                    .id()
            })
            .collect();
        (world, root, entities)
    }

    fn texture_bytes(world: &mut World) -> u64 {
        let surfaces: Vec<_> = world
            .query::<&Surface>()
            .iter(world)
            .map(|surface| {
                (
                    surface.image.clone(),
                    surface.pixels,
                    surface.visible,
                    surface.camera,
                )
            })
            .collect();
        let mut bytes = 0;
        for (handle, pixels, visible, camera) in surfaces {
            let image = world.resource::<Assets<Image>>().get(&handle).unwrap();
            assert!(image.data.is_none());
            assert_eq!(image.size(), pixels);
            assert_eq!(world.get::<Camera>(camera).unwrap().is_active, visible);
            if !visible {
                assert_eq!(pixels, UVec2::ONE);
                assert!(matches!(
                    world.get::<RenderTarget>(camera),
                    Some(RenderTarget::None { .. })
                ));
            }
            bytes += u64::from(pixels.x) * u64::from(pixels.y) * 4;
        }
        assert!(bytes <= surface_budget::PIXEL_BUDGET * 4);
        bytes
    }

    #[test]
    fn camera_motion_and_workspace_switches_keep_a_thousand_captures_bounded() {
        let (mut world, root, entities) = scene(1000);
        synchronize(&mut world);
        let planar = texture_bytes(&mut world);
        assert!(planar > 4000);
        for step in 0..32 {
            *world.get_mut::<super::super::view::View>(root).unwrap() = super::super::view::View {
                spatial: true,
                position: [500.0, 500.0, 500.0],
                yaw: step as f32 * std::f32::consts::TAU / 16.0,
                pitch: -std::f32::consts::FRAC_PI_4,
                ..default()
            };
            synchronize(&mut world);
            texture_bytes(&mut world);
            assert_eq!(world.query::<&Surface>().iter(&world).count(), 1000);
        }
        world.get_mut::<Workspaces>(root).unwrap().active = 2;
        synchronize(&mut world);
        assert_eq!(texture_bytes(&mut world), 4000);
        for entity in entities {
            world.despawn(entity);
        }
        synchronize(&mut world);
        assert_eq!(world.query::<&VisualOwner>().iter(&world).count(), 0);
    }

    #[test]
    fn suspended_surfaces_release_textures_and_resume_with_the_same_camera() {
        let (mut world, root, entities) = scene(1);
        let entity = entities[0];
        synchronize(&mut world);
        let camera = world.get::<Surface>(entity).unwrap().camera;
        assert!(world.get::<Surface>(entity).unwrap().visible);
        world.entity_mut(root).insert(Disabled);
        world.entity_mut(entity).insert(Disabled);
        synchronize(&mut world);
        assert_eq!(world.get::<Surface>(entity).unwrap().pixels, UVec2::ONE);
        assert!(
            world
                .get::<Camera>(camera)
                .is_some_and(|camera| !camera.is_active)
        );
        world.entity_mut(root).remove::<Disabled>();
        world.entity_mut(entity).remove::<Disabled>();
        synchronize(&mut world);
        assert_eq!(world.get::<Surface>(entity).unwrap().camera, camera);
        assert!(world.get::<Surface>(entity).unwrap().visible);
        texture_bytes(&mut world);
    }

    #[test]
    fn stationary_surfaces_do_not_invalidate_assets_or_camera_layout() {
        let (mut world, _, entities) = scene(1);
        let entity = entities[0];
        synchronize(&mut world);
        let camera = world.get::<Surface>(entity).unwrap().camera;
        let surface_tick = world
            .entity(entity)
            .get_ref::<Surface>()
            .unwrap()
            .last_changed();
        let target_tick = world
            .entity(camera)
            .get_ref::<RenderTarget>()
            .unwrap()
            .last_changed();
        let images_tick = world
            .get_resource_ref::<Assets<Image>>()
            .unwrap()
            .last_changed();
        let material_tick = world
            .get_resource_ref::<Assets<StandardMaterial>>()
            .unwrap()
            .last_changed();
        world.increment_change_tick();
        synchronize(&mut world);
        assert_eq!(
            world
                .entity(entity)
                .get_ref::<Surface>()
                .unwrap()
                .last_changed(),
            surface_tick
        );
        assert_eq!(
            world
                .entity(camera)
                .get_ref::<RenderTarget>()
                .unwrap()
                .last_changed(),
            target_tick
        );
        assert_eq!(
            world
                .get_resource_ref::<Assets<Image>>()
                .unwrap()
                .last_changed(),
            images_tick
        );
        assert_eq!(
            world
                .get_resource_ref::<Assets<StandardMaterial>>()
                .unwrap()
                .last_changed(),
            material_tick
        );
    }

    #[test]
    fn rotated_and_clipped_surfaces_are_culled_in_world_coordinates() {
        let (mut world, root, entities) = scene(1);
        let entity = entities[0];
        world.get_mut::<CanvasItem>(entity).unwrap().position.x = 1200.0;
        assert!(
            !request(
                &world,
                root,
                entity,
                world.get::<CanvasItem>(entity).unwrap(),
                UVec2::new(1920, 1080),
                1.0
            )
            .visible
        );
        world.get_mut::<CanvasItem>(entity).unwrap().position.x = 0.0;
        world.entity_mut(entity).insert(super::super::Spatial {
            rotation: bevy::math::DQuat::from_rotation_y(0.7).to_array(),
            ..default()
        });
        assert!(
            request(
                &world,
                root,
                entity,
                world.get::<CanvasItem>(entity).unwrap(),
                UVec2::new(1920, 1080),
                1.0
            )
            .visible
        );
        world
            .get_mut::<super::super::view::View>(root)
            .unwrap()
            .spatial = true;
        world
            .get_mut::<super::super::view::View>(root)
            .unwrap()
            .position = [0.0, 500.0, 500.0];
        world
            .get_mut::<super::super::view::View>(root)
            .unwrap()
            .pitch = -std::f32::consts::FRAC_PI_4;
        assert!(
            request(
                &world,
                root,
                entity,
                world.get::<CanvasItem>(entity).unwrap(),
                UVec2::new(1920, 1080),
                1.0
            )
            .visible
        );
        world.get_mut::<super::super::view::View>(root).unwrap().yaw = std::f32::consts::PI;
        assert!(
            !request(
                &world,
                root,
                entity,
                world.get::<CanvasItem>(entity).unwrap(),
                UVec2::new(1920, 1080),
                1.0
            )
            .visible
        );
    }

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
