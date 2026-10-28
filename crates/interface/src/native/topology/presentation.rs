use super::{
    surface_budget::{self, Request, dimensions},
    surface_render::{SurfaceCapture, SurfacePass},
};
use crate::native::{
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
use std::collections::{BTreeSet, HashMap, HashSet};

mod root;

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
pub(super) struct DisplayedImage(pub(super) Handle<Image>);

#[derive(Component, Default)]
struct RetainedImages(BTreeSet<bevy::asset::AssetId<Image>>);

pub(crate) fn capture_images(world: &mut World) -> Vec<bevy::asset::AssetId<Image>> {
    world.query_filtered::<(&Surface, Option<&DisplayedImage>, Option<&RetainedImages>), Allow<Disabled>>()
        .iter(world)
        .flat_map(|(surface, displayed, retained)| std::iter::once(surface.image.id())
            .chain(displayed.map(|image| image.0.id()))
            .chain(retained.into_iter().flat_map(|retained| retained.0.iter().copied())))
        .collect()
}

pub(crate) fn capture_aliases(world: &mut World) -> Vec<(Entity, bevy::asset::AssetId<Image>, bevy::asset::AssetId<Image>)> {
    world.query_filtered::<(Entity, &Surface, Option<&DisplayedImage>, Option<&RetainedImages>), Allow<Disabled>>()
        .iter(world)
        .filter(|(owner, surface, _, _)| world.get::<VisualOwner>(surface.camera).is_some_and(|source| source.0 == *owner)
            && world.get::<SurfaceCapture>(surface.camera).is_some_and(|capture| capture.0 == surface.image)
            && world.get::<RenderTarget>(surface.camera).is_some_and(|target| matches!(target,
                RenderTarget::Image(image) if image.handle == surface.image)))
        .flat_map(|(_, surface, displayed, retained)| displayed.into_iter().map(|displayed| displayed.0.id())
            .chain(retained.into_iter().flat_map(|retained| retained.0.iter().copied()))
            .filter(move |image| *image != surface.image.id())
            .map(move |image| (surface.camera, surface.image.id(), image)))
        .collect()
}

#[derive(Component)]
pub struct VisualOwner(pub Entity);

#[derive(Component, Clone, Copy)]
pub struct SurfaceOpacity(pub f32);

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

fn has_text(world: &World, entity: Entity) -> bool {
    if world
        .get::<Node>(entity)
        .is_some_and(|node| node.display == Display::None)
    {
        return false;
    }
    world.get::<Text>(entity).is_some()
        || world
            .get::<crate::native::time_castle::TimeSettings>(entity)
            .is_some()
        || world
            .get::<Children>(entity)
            .is_some_and(|children| children.iter().any(|child| has_text(world, child)))
}

fn request(
    world: &World,
    root: Entity,
    entity: Entity,
    item: &CanvasItem,
    physical_size: UVec2,
    scale_factor: f32,
) -> Request {
    let active = world
        .get::<Workspaces>(root)
        .map_or(0, |spaces| spaces.active);
    let mut result = request_in(
        world,
        root,
        active,
        entity,
        item,
        physical_size,
        scale_factor,
    );
    if let Some(views) = world
        .get_resource::<crate::native::output_views::OutputViews>()
        .filter(|views| views.applies_to(root))
    {
        for output in views
            .outputs
            .iter()
            .filter(|output| output.workspace != active)
        {
            let other = request_in(
                world,
                root,
                output.workspace,
                entity,
                item,
                UVec2::from_array(output.physical_size),
                output.raster.scale[0] as f32,
            );
            if other.visible {
                result.density = if result.visible {
                    result.density.max(other.density)
                } else {
                    other.density
                };
                result.visible = true;
            }
        }
    }
    if let Some(density) = super::previews::demand(world, root, entity, item) {
        let density = density * if result.text { 2.0 } else { 1.0 };
        result.density = if result.visible {
            result.density.max(density)
        } else {
            density
        };
        result.visible = true;
    }
    result
}

fn request_in(
    world: &World,
    root: Entity,
    active_workspace: u64,
    entity: Entity,
    item: &CanvasItem,
    physical_size: UVec2,
    scale_factor: f32,
) -> Request {
    let (offset, clip) = crate::native::layout::viewport::clip(world, entity);
    let active = world
        .get::<WorkspaceMember>(entity)
        .is_some_and(|member| super::holes::live_in(world, root, active_workspace, member.0))
        && world.get::<Disabled>(root).is_none()
        && world.get::<Disabled>(entity).is_none()
        && world
            .get::<crate::native::layout::desktop::Suppressed>(entity)
            .is_none()
        && clip.size().min_element() > 0.0
        && world
            .get::<crate::native::protein_area::placement::Pending>(entity)
            .is_none()
        && !super::holes::surface_pieces_in(world, root, active_workspace, entity, item)
            .0
            .is_empty();
    let previous = world
        .get::<Surface>(entity)
        .filter(|surface| surface.visible)
        .map_or(0.0, |surface| surface.density);
    let mut request = Request {
        size: item.size,
        density: 1.0,
        previous,
        visible: active,
        text: has_text(world, entity),
        exact: world
            .get::<crate::native::layout::desktop::ApplicationWindow>(entity)
            .is_some(),
    };
    if !active || !item.size.is_finite() || item.size.min_element() <= 0.0 {
        request.visible = false;
        return request;
    }
    let scale = world
        .get::<crate::native::area_effects::AreaScale>(entity)
        .map_or(1.0, |scale| scale.0);
    let mut view = world
        .get::<super::view::View>(root)
        .copied()
        .unwrap_or_default();
    view.spatial = super::view::spatial(world, root);
    let placement = crate::native::canvas::presented_spatial(world, entity);
    let depth =
        if let Some(settings) = world.get::<crate::native::time_castle::TimeSettings>(entity) {
            (f64::from(item.size.min_element()) * 0.18 * settings.0.horizon_ms as f64
                / settings.0.aperture_ms.max(1000) as f64)
                .min(100_000.0)
                * f64::from(scale)
        } else if world
            .get::<crate::native::area::InfluenceArea>(entity)
            .is_some()
        {
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
        let canvas = crate::native::output_views::canvas(world, root, active_workspace);
        let half = physical_size.as_vec2().as_dvec2() / f64::from(scale_factor) / canvas.zoom * 0.5;
        let min = canvas.center - half;
        let max = canvas.center + half;
        request.visible = !points.iter().all(|point| point.x < min.x)
            && !points.iter().all(|point| point.x > max.x)
            && !points.iter().all(|point| point.z < min.y)
            && !points.iter().all(|point| point.z > max.y);
        request.density =
            canvas.zoom as f32 * scale_factor * scale * if request.text { 2.0 } else { 1.0 };
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
    request.density = physical_size.y as f32 * scale / (2.0 * tangent * (nearest as f32).max(0.5))
        * if request.text { 2.0 } else { 1.0 };
    request
}

fn image(size: UVec2, format: TextureFormat) -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST
        | TextureUsages::COPY_SRC
        | TextureUsages::RENDER_ATTACHMENT;
    image
}

struct NativePlan {
    physical_size: UVec2,
    roots: Vec<Entity>,
    entities: Vec<(Entity, CanvasItem, Entity, u64)>,
    requests: Vec<Request>,
    priorities: Vec<u8>,
}

fn collect(world: &mut World) -> Option<NativePlan> {
    if !ready(world) {
        return None;
    }
    super::holes::refresh(world);
    let viewport = world.query::<&Window>().iter(world).next().map(|window| {
        (
            window.resolution.physical_size(),
            window.resolution.scale_factor(),
        )
    });
    let (physical_size, scale_factor) = viewport?;
    let roots: Vec<_> = world
        .query_filtered::<Entity, (With<Workspaces>, Allow<Disabled>)>()
        .iter(world)
        .collect();
    let entities: Vec<_> = world
        .query_filtered::<(Entity, &CanvasItem, &ChildOf, &WorkspaceMember), Allow<Disabled>>()
        .iter(world)
        .filter(|(_, _, parent, _)| roots.contains(&parent.parent()))
        .map(|(entity, item, parent, member)| {
            (
                entity,
                world
                    .get::<crate::native::canvas::ItemPreview>(entity)
                    .map_or(*item, |preview| preview.0),
                parent.parent(),
                member.0,
            )
        })
        .collect();
    let mut requests = Vec::with_capacity(entities.len());
    let mut priorities = Vec::with_capacity(entities.len());
    for (entity, item, root, workspace) in &entities {
        let mut demand = request(world, *root, *entity, item, physical_size, scale_factor);
        demand.visible &= world.get::<super::assets::ImportedAsset>(*entity).is_none()
            && (world.get::<super::terrain::Effect>(*entity).is_none()
                || world
                    .get::<crate::native::edit_mode::EditMode>(*root)
                    .is_some_and(|mode| mode.enabled))
            && world
                .get::<crate::native::sand_placement::Pinned>(*entity)
                .is_none();
        let spaces = world.get::<Workspaces>(*root).unwrap();
        let focused = spaces.active == *workspace
            && world
                .get::<crate::native::layout::desktop::Managed>(*entity)
                .is_some_and(|managed| {
                    spaces
                        .entries
                        .iter()
                        .find(|entry| entry.id == *workspace)
                        .is_some_and(|entry| entry.desktop.focused() == Some(managed.id))
                });
        let presented = spaces.active == *workspace
            || world
                .get_resource::<crate::native::output_views::OutputViews>()
                .is_some_and(|views| {
                    views.applies_to(*root)
                        && views
                            .outputs
                            .iter()
                            .any(|output| output.workspace == *workspace)
                });
        priorities.push(if focused {
            5
        } else if !presented {
            0
        } else if demand.text {
            2
        } else {
            1
        });
        requests.push(demand);
    }
    Some(NativePlan {
        physical_size,
        roots,
        entities,
        requests,
        priorities,
    })
}

pub fn synchronize(world: &mut World) {
    let native = collect(world);
    let demands: HashMap<_, _> = native
        .iter()
        .flat_map(|plan| {
            plan.entities
                .iter()
                .zip(&plan.requests)
                .zip(&plan.priorities)
                .map(|(((entity, _, _, _), request), priority)| (*entity, (*request, *priority)))
        })
        .collect();
    let effects = crate::native::description::shader::extent::requests(world, &demands);
    let native_count = native.as_ref().map_or(0, |plan| plan.entities.len());
    let entries: Vec<_> = native
        .iter()
        .flat_map(|plan| {
            plan.requests
                .iter()
                .zip(&plan.priorities)
                .map(|(request, priority)| surface_budget::Entry {
                    request: *request,
                    priority: *priority,
                    bytes_per_pixel: 8,
                })
        })
        .chain(effects.iter().map(|(_, entry)| *entry))
        .collect();
    let maximum = world
        .get_resource::<bevy::render::renderer::RenderDevice>()
        .map_or(4096, |device| device.limits().max_texture_dimension_2d);
    let grants = surface_budget::allocate(&entries, surface_budget::BYTE_BUDGET, maximum);
    world.insert_resource(surface_budget::Diagnostics {
        granted_bytes: entries
            .iter()
            .zip(&grants)
            .map(|(entry, grant)| surface_budget::bytes(*entry, grant.pixels))
            .sum(),
        limited_targets: grants
            .iter()
            .filter(|grant| grant.device_limited || grant.budget_limited)
            .count(),
        maximum_extent: maximum,
    });
    crate::native::description::shader::extent::apply(world, &effects, &grants[native_count..]);
    let Some(NativePlan {
        physical_size,
        roots,
        entities,
        requests,
        priorities: _,
    }) = native
    else {
        crate::native::description::shader::update_effects(world);
        return;
    };
    let format = if world.contains_resource::<crate::native::app::LinearOutput>() {
        TextureFormat::Rgba16Float
    } else {
        TextureFormat::Rgba8UnormSrgb
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
    for root in &roots {
        root::synchronize(world, *root, physical_size);
    }
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
    for (((entity, item, root, _), request), grant) in entities
        .into_iter()
        .zip(requests)
        .zip(&grants[..native_count])
    {
        let pixels = grant.pixels.max(UVec2::ONE);
        let density = if grant.pixels == UVec2::ZERO {
            1.0
        } else {
            grant.density
        };
        if !item.size.is_finite() || item.size.min_element() <= 0.0 {
            continue;
        }
        if world
            .get::<crate::native::sand_placement::Pinned>(entity)
            .is_some()
        {
            world
                .entity_mut(entity)
                .remove::<surface_budget::NativeRaster>();
            if let Some(surface) = world.entity_mut(entity).take::<Surface>() {
                world.despawn(surface.camera);
                world.despawn(surface.visual);
                world
                    .entity_mut(entity)
                    .remove::<(UiTargetCamera, bevy::ui::LayoutConfig, DisplayedImage)>()
                    .insert(UiTransform::default());
            }
            continue;
        }
        if world.get::<super::assets::ImportedAsset>(entity).is_some()
            || world
                .get::<crate::native::sand_placement::Pinned>(entity)
                .is_some()
        {
            continue;
        }
        let visible = request.visible && grant.pixels != UVec2::ZERO;
        if world.get::<Surface>(entity).is_none() {
            let image = world.resource_mut::<Assets<Image>>().add(image(pixels, format));
            let camera = world
                .spawn((
                    Camera2d,
                    Camera {
                        order: -3,
                        is_active: visible,
                        clear_color: ClearColorConfig::Custom(Color::NONE),
                        ..default()
                    },
                    Msaa::Off,
                    CameraRenderGraph::new(SurfacePass),
                    SurfaceCapture(image.clone()),
                    surface_budget::residency::Target {
                        image: image.id(),
                        release: !visible,
                    },
                    RenderTarget::Image(ImageRenderTarget {
                        handle: image.clone(),
                        scale_factor: density,
                    }),
                    RenderLayers::layer(31),
                    VisualOwner(entity),
                ))
                .remove::<bevy::camera::visibility::VisibleEntities>()
                .id();
            crate::native::app::configure_output_color(world, camera, &image);
            let material = world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial {
                    base_color_texture: Some(image.clone()),
                    unlit: true,
                    alpha_mode: AlphaMode::Premultiplied,
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
                    super::compositor::SurfaceFace,
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
                bevy::ui::LayoutConfig {
                    use_rounding: false,
                },
                DisplayedImage(image.clone()),
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
        let (camera, mut image_handle, visual, body, face, old_pixels) = (
            surface.camera,
            surface.image.clone(),
            surface.visual,
            surface.body,
            surface.face,
            surface.pixels,
        );
        let material = surface.material.clone();
        if pixels != old_pixels || world.resource::<Assets<Image>>().get(&image_handle)
            .is_none_or(|image| image.texture_descriptor.format != format)
        {
            let previous = image_handle.id();
            if let Some(mut retained) = world.get_mut::<RetainedImages>(entity) { retained.0.insert(previous); }
            else { world.entity_mut(entity).insert(RetainedImages(BTreeSet::from([previous]))); }
            image_handle = world.resource_mut::<Assets<Image>>().add(image(pixels, format));
            world.get_mut::<Surface>(entity).unwrap().image = image_handle.clone();
            world.get_mut::<SurfaceCapture>(camera).unwrap().0 = image_handle.clone();
        }
        crate::native::app::configure_output_color(world, camera, &image_handle);
        if let Some(retained) = world.get::<RetainedImages>(entity) {
            let live: BTreeSet<_> = retained.0.iter().copied().filter(|image| world.resource::<Assets<Image>>().get(*image).is_some()).collect();
            if live != retained.0 { world.get_mut::<RetainedImages>(entity).unwrap().0 = live; }
        }
        if world
            .get::<DisplayedImage>(entity)
            .is_some_and(|displayed| displayed.0 != image_handle)
            && (!visible
                || world
                    .get_resource::<super::surface_render::CompletedCaptures>()
                    .is_some_and(|completed| completed.contains(&image_handle)))
        {
            world
                .resource_mut::<Assets<StandardMaterial>>()
                .get_mut(&material)
                .unwrap()
                .base_color_texture = Some(image_handle.clone());
            world
                .entity_mut(entity)
                .insert(DisplayedImage(image_handle.clone()));
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
        if let Some(mut monitor) = world.get_mut::<surface_budget::residency::Target>(camera) {
            if monitor.image != image_handle.id() {
                monitor.image = image_handle.id();
            }
            if monitor.release == visible {
                monitor.release = !visible;
            }
        }
        let raster = surface_budget::NativeRaster {
            pixels,
            requested: grant.requested,
            device_limited: grant.device_limited,
            budget_limited: grant.budget_limited,
        };
        if world.get::<surface_budget::NativeRaster>(entity) != Some(&raster) {
            world.entity_mut(entity).insert(raster);
        }
        let placement = crate::native::canvas::presented_spatial(world, entity);
        let area = world
            .get::<crate::native::area::InfluenceArea>(entity)
            .is_some();
        let opacity = if area {
            super::areas::opacity(world, entity)
        } else {
            1.0
        } * world
            .get::<SurfaceOpacity>(entity)
            .map_or(1.0, |opacity| opacity.0.clamp(0.0, 1.0))
            * world.get::<crate::native::shader_effects::compositor::Opacity>(entity).map_or(1.0, |opacity| opacity.0);
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        if materials.get(&material).unwrap().base_color.alpha() != opacity {
            materials.get_mut(&material).unwrap().base_color =
                Color::linear_rgba(opacity, opacity, opacity, opacity);
        }
        let arrow = world
            .get::<crate::native::arrow_sand::ArrowSand>(entity)
            .is_some();
        let flat = world
            .get::<crate::native::time_castle::TimeSettings>(entity)
            .is_some()
            || area
            || arrow
            || world
                .get::<crate::native::layout::desktop::manipulation::Outline>(entity)
                .is_some()
            || !super::view::spatial(world, root);
        let boundary = world.get::<super::rooms::Room>(entity).is_some()
            && world.get::<super::appearance::Material>(entity).is_none()
            && crate::native::token_style::overrides(world, entity)
                .0
                .get(&crate::native::tokens::Token::SandBackground)
                .is_some_and(|value| {
                    matches!(
                        value,
                        crate::native::tokens::TokenValue::Color([_, _, _, 0])
                    )
                });
        world
            .get_mut::<Visibility>(body)
            .unwrap()
            .set_if_neq(if flat || boundary {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
        let scale = world
            .get::<crate::native::area_effects::AreaScale>(entity)
            .map_or(1.0, |s| s.0);
        let depth = placement.depth(item.size) as f32;
        let (offset, clip) = crate::native::layout::viewport::clip(world, entity);
        let clipped_size = clip.size();
        let clipped_center = clip.center() - item.size * 0.5;
        let uv = Rect::from_corners(clip.min / item.size, clip.max / item.size);
        if world.get::<Surface>(entity).unwrap().uv != uv
            && !super::holes::clipped(world, face)
            && world
                .get::<crate::native::time_castle::TimeSettings>(entity)
                .is_none()
        {
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
                + if world.get::<super::terrain::Effect>(entity).is_some() {
                    Vec3::Y
                        * (super::terrain::height(
                            world,
                            root,
                            world.get::<WorkspaceMember>(entity).unwrap().0,
                            item.position,
                        ) as f32
                            + 24.0
                            - placement.elevation as f32)
                } else {
                    Vec3::ZERO
                }
                - placement.rotation().as_quat() * Vec3::new(offset.x, 0.0, offset.y)
                - Vec3::Y
                    * super::holes::depth(
                        world,
                        root,
                        world.get::<WorkspaceMember>(entity).unwrap().0,
                    ) as f32
                    * 0.2,
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
                if arrow {
                    -0.05
                } else if area {
                    0.0
                } else {
                    0.01
                },
                clipped_center.y,
            )
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::new(clipped_size.x, clipped_size.y, 1.0)),
        );
        let plane = super::stacking::Plane::new(
            placement.rotation() * bevy::math::DVec3::Y,
            transform
                .compute_affine()
                .transform_point3(world.get::<Transform>(face).unwrap().translation)
                .as_dvec3(),
        );
        super::stacking::synchronize(world, entity, face, plane);
        let displayed_ready = world
            .get::<DisplayedImage>(entity)
            .is_some_and(|displayed| {
                world
                    .get_resource::<super::surface_render::CompletedCaptures>()
                    .is_none_or(|completed| completed.contains(&displayed.0))
            });
        let focused_visible =
            super::holes::live(world, root, world.get::<WorkspaceMember>(entity).unwrap().0);
        world.get_mut::<Visibility>(visual).unwrap().set_if_neq(
            if visible && displayed_ready && focused_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        );
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
        super::holes::clip_surface(world, root, entity, &item);
    }
    super::holes::backgrounds(world);
    super::output_views::synchronize(world);
    crate::native::description::shader::update_effects(world);
}

pub fn displayed(world: &World, entity: Entity) -> bool {
    world
        .get::<DisplayedImage>(entity)
        .is_some_and(|displayed| {
            world
                .get_resource::<super::surface_render::CompletedCaptures>()
                .is_none_or(|completed| completed.contains(&displayed.0))
        })
}

pub fn projected_corners(world: &World, entity: Entity) -> Option<[[f64; 2]; 4]> {
    let surface = world.get::<Surface>(entity)?;
    let camera = world
        .get_entity(world.get_resource::<SceneCamera>()?.0)
        .ok()?;
    let transform = super::view::current_transform(world, camera.id())?;
    let camera = camera.get::<Camera>()?;
    let face = super::view::current_transform(world, surface.face)?;
    let mut corners = [[0.0; 2]; 4];
    for (index, uv) in [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]
        .into_iter()
        .enumerate()
    {
        let uv = (uv - surface.uv.min) / surface.uv.size();
        let point = face.transform_point(Vec3::new(uv.x - 0.5, 0.5 - uv.y, 0.0));
        corners[index] = camera
            .world_to_viewport(&transform, point)
            .ok()?
            .as_dvec2()
            .to_array();
    }
    Some(corners)
}

pub fn bounds(world: &World, entity: Entity) -> Option<Rect> {
    let Some(item) = crate::native::canvas::presented(world, entity) else {
        return content_bounds(world, entity);
    };
    let root = world.get::<ChildOf>(entity)?.parent();
    if world.get::<SpatialRoot>(root).is_none()
        || world
            .get::<crate::native::sand_placement::Pinned>(entity)
            .is_some()
    {
        return None;
    }
    let camera = world
        .get_entity(world.get_resource::<SceneCamera>()?.0)
        .ok()?;
    let transform = super::view::current_transform(world, camera.id())?;
    let camera = camera.get::<Camera>()?;
    let placement = crate::native::canvas::presented_spatial(world, entity);
    let origin = origin(world, root);
    let (offset, clip) = crate::native::layout::viewport::clip(world, entity);
    if clip.size().min_element() <= 0.0 {
        return None;
    }
    let scale = world
        .get::<crate::native::area_effects::AreaScale>(entity)
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
            .get::<crate::native::area::InfluenceArea>(entity)
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
                if let Ok(point) = camera.world_to_viewport(&transform, (point - origin).as_vec3())
                {
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
    let camera_transform = super::view::current_transform(world, camera.id())?;
    let camera = camera.get::<Camera>()?;
    let face = super::view::current_transform(world, surface.face)?;
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
        if let Ok(point) = camera.world_to_viewport(&camera_transform, point) {
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
    fn room_boundaries_keep_the_outline_surface_without_an_opaque_body() {
        let (mut world, root, entities) = scene(1);
        let room = entities[0];
        world
            .get_mut::<super::super::view::View>(root)
            .unwrap()
            .spatial = true;
        world
            .entity_mut(room)
            .insert(super::super::rooms::Room(default()));
        crate::native::token_style::set_overrides(
            &mut world,
            room,
            super::super::rooms::boundary_style(),
        );
        synchronize(&mut world);
        let surface = world.get::<Surface>(room).unwrap();
        assert!(surface.visible);
        assert_eq!(
            world.get::<Visibility>(surface.body),
            Some(&Visibility::Hidden)
        );
        world
            .entity_mut(room)
            .insert(super::super::appearance::Material(default()));
        synchronize(&mut world);
        let surface = world.get::<Surface>(room).unwrap();
        assert_eq!(
            world.get::<Visibility>(surface.body),
            Some(&Visibility::Inherited)
        );
    }

    #[test]
    fn text_surfaces_supersample_and_fade_without_darkening_transparent_edges() {
        let (mut world, _, entities) = scene(2);
        let text = entities[0];
        world.spawn((Text::new("Clear titles and times"), ChildOf(text)));
        synchronize(&mut world);
        let surface = world.get::<Surface>(text).unwrap();
        let (image, material) = (surface.image.clone(), surface.material.clone());
        assert_eq!(surface.density, 2.0);
        assert_eq!(world.get::<Surface>(entities[1]).unwrap().density, 1.0);
        assert_eq!(
            world
                .resource::<Assets<StandardMaterial>>()
                .get(&material)
                .unwrap()
                .alpha_mode,
            AlphaMode::Premultiplied
        );
        world.entity_mut(text).insert(SurfaceOpacity(0.35));
        synchronize(&mut world);
        assert_eq!(world.get::<Surface>(text).unwrap().image.id(), image.id());
        let tint = world
            .resource::<Assets<StandardMaterial>>()
            .get(&material)
            .unwrap()
            .base_color
            .to_linear();
        assert_eq!([tint.red, tint.green, tint.blue, tint.alpha], [0.35; 4]);
    }

    #[test]
    fn planar_sands_keep_transparent_corners_and_restore_bodies_in_spatial_view() {
        let (mut world, root, entities) = scene(1);
        let entity = entities[0];
        world.entity_mut(entity).insert(Node {
            border_radius: BorderRadius::all(px(24)),
            ..default()
        });
        for spatial in [false, true, false] {
            world
                .get_mut::<super::super::view::View>(root)
                .unwrap()
                .spatial = spatial;
            synchronize(&mut world);
            let surface = world.get::<Surface>(entity).unwrap();
            assert_eq!(
                *world.get::<Visibility>(surface.body).unwrap(),
                if spatial {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            );
            assert_eq!(
                *world.get::<Visibility>(surface.face).unwrap(),
                Visibility::Inherited
            );
        }
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
        let surface = world.get::<Surface>(entity).unwrap();
        assert_eq!(
            world
                .resource::<Assets<StandardMaterial>>()
                .get(&surface.material)
                .unwrap()
                .base_color_texture
                .as_ref(),
            Some(&surface.image)
        );
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
    fn a_displayed_capture_alias_requires_its_current_producer_target_and_visual_owner() {
        let mut world = World::new();
        let mut images = Assets::<Image>::default();
        let current = images.add(Image::default());
        let retained = images.add(Image::default());
        let owner = world.spawn_empty().id();
        let unrelated = world.spawn_empty().id();
        let camera = world.spawn((Camera { is_active: false, ..default() }, RenderTarget::Image(current.clone().into()),
            SurfaceCapture(current.clone()), VisualOwner(owner))).id();
        world.entity_mut(owner).insert((Surface {
            camera, image: current.clone(), visual: owner, body: owner, face: owner,
            size: Vec2::splat(100.0), pixels: UVec2::splat(100), density: 1.0,
            material: Handle::default(), uv: Rect::from_corners(Vec2::ZERO, Vec2::ONE), visible: true,
        }, DisplayedImage(retained.clone())));
        assert_eq!(capture_aliases(&mut world), [(camera, current.id(), retained.id())]);
        world.get_mut::<VisualOwner>(camera).unwrap().0 = unrelated;
        assert!(capture_aliases(&mut world).is_empty());
        world.get_mut::<VisualOwner>(camera).unwrap().0 = owner;
        world.get_mut::<SurfaceCapture>(camera).unwrap().0 = retained.clone();
        assert!(capture_aliases(&mut world).is_empty());
        world.get_mut::<SurfaceCapture>(camera).unwrap().0 = current.clone();
        world.entity_mut(camera).insert(RenderTarget::None { size: UVec2::splat(100) });
        assert!(capture_aliases(&mut world).is_empty());
        world.entity_mut(camera).insert(RenderTarget::Image(current.clone().into()));
        assert_eq!(capture_aliases(&mut world), [(camera, current.id(), retained.id())]);
        world.get_mut::<DisplayedImage>(owner).unwrap().0 = current;
        assert!(capture_aliases(&mut world).is_empty());
        world.entity_mut(owner).insert(RetainedImages(BTreeSet::from([retained.id()])));
        assert_eq!(capture_aliases(&mut world), [(camera, world.get::<Surface>(owner).unwrap().image.id(), retained.id())]);
        world.entity_mut(owner).remove::<RetainedImages>();
        assert!(capture_aliases(&mut world).is_empty());
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
