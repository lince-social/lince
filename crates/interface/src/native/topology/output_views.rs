// non-lean-yet
// Scope: struct Targets, mod tests, enum ViewKey, struct RenderView.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 211, 234, 330,
//   342, 375, 377, 425, 559, 564, 570, 583, 596, 647, 651, 697.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   419, 538, 632.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (312, 419, 461, 568, 570, 574, 575, 583, 650, 651, 655, 657, 664, 675).
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 151.

use super::{holes, presentation::Surface};
use crate::{
    host::OutputId,
    native::{
        canvas::CanvasItem,
        output_views::OutputViews,
        workspace::{WorkspaceMember, Workspaces},
    },
};
use bevy::{
    asset::RenderAssetUsages, camera::visibility::RenderLayers, mesh::PrimitiveTopology, prelude::*,
};
use std::collections::{HashMap, HashSet};

#[derive(Resource, Default)]
pub struct Targets(pub HashMap<OutputId, Handle<Image>>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{OutputViewport, Rectangle, raster::RasterMapping};

    #[test]
    fn flat_item_geometry_and_clipping_use_desktop_coordinates_at_mixed_output_origins_and_scales() {
        for origin in [[0.0; 2], [120.0, 40.0], [920.0, 80.0], [-800.0, -120.0]] {
            for scale in [1.0, 1.25, 2.0] {
                let mut world = World::new();
                let root = world.spawn((Workspaces::default(), crate::native::canvas::CanvasView::default())).id();
                let size = Vec2::new(80.0, 60.0);
                world.spawn((
                    CanvasItem { position: bevy::math::DVec2::ZERO, size },
                    crate::native::layout::LayoutBox::new(size),
                    WorkspaceMember(1),
                    ChildOf(root),
                    super::super::presentation::DisplayedImage(Handle::default()),
                    Surface { camera: root, image: Handle::default(), visual: root, body: root, face: root, size,
                        pixels: UVec2::new(80, 60), density: 1.0, material: Handle::default(), uv: Rect::from_corners(Vec2::ZERO, Vec2::ONE), visible: true },
                ));
                let bounds = Rectangle::new(origin, [800.0 / scale, 600.0 / scale]).unwrap();
                let output = OutputViewport { id: OutputId(1), bounds, usable: Some(bounds), physical_size: [800, 600],
                    raster: RasterMapping { origin, scale: [scale; 2] }, workspace: 1, rotation: Default::default() };
                let items = geometry(&mut world, root, &output);
                assert_eq!(items.len(), 1, "Missing output geometry at {origin:?}, scale {scale}");
                let center = output.raster.logical_point([400.0, 300.0]);
                let actual = items[0].mapping.canvas_point([40.0, 30.0]).unwrap();
                for axis in 0..2 { assert!((actual[axis] - center[axis]).abs() < 1e-9); }
                let local = items[0].surface_point(center).unwrap();
                for axis in 0..2 { assert!((local[axis] - [40.0, 30.0][axis]).abs() < 1e-9); }
                assert!(items[0].clip.as_ref().unwrap().iter().flatten().all(|point| bounds.contains(*point)));
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ViewKey {
    Output(OutputId),
    Preview(Entity),
}

struct RenderView {
    id: ViewKey,
    root: Entity,
    workspace: u64,
    physical_size: [u32; 2],
    logical_size: Vec2,
    scale: f32,
    offset: Vec2,
    canvas: crate::native::canvas::CanvasView,
    center: bevy::math::DVec2,
    image: Handle<Image>,
    layer: usize,
}

#[derive(Component)]
struct OutputCamera(ViewKey);

#[derive(Component)]
pub(crate) struct WorldView;

#[derive(Component)]
struct Background {
    output: ViewKey,
    grid: Entity,
    material: Handle<crate::native::canvas_background::PatternMaterial>,
}

#[derive(Component)]
struct Instance {
    output: ViewKey,
    owner: Entity,
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
struct Fill {
    output: ViewKey,
    owner: Entity,
    workspace: u64,
}

#[derive(Component)]
struct FillGeometry(Vec<[f32; 3]>);

pub(crate) fn asset_layers(world: &World, root: Entity, workspace: u64) -> RenderLayers {
    let mut layers = if world
        .get::<Workspaces>(root)
        .is_some_and(|spaces| spaces.active == workspace)
    {
        RenderLayers::layer(0)
    } else {
        RenderLayers::none()
    };
    if let Some(views) = world
        .get_resource::<OutputViews>()
        .filter(|views| views.applies_to(root))
    {
        for (index, output) in views.outputs.iter().enumerate() {
            if output.id != views.focused
                && output.workspace == workspace
                && let Some(layer) = crate::native::render_layers::OUTPUTS.get(index)
            {
                layers = layers.with(layer);
            }
        }
    }
    if let Some(previews) = world.get_resource::<super::previews::Previews>() {
        for (index, preview) in previews.0.iter().enumerate() {
            if preview.root == root
                && preview.workspace == workspace
                && let Some(layer) = crate::native::render_layers::OVERVIEW.get(index)
            {
                layers = layers.with(layer);
            }
        }
    }
    layers
}

mod geometry;
pub(crate) use geometry::matching_geometry;
pub use geometry::{geometry, item_geometry};

mod preparation;
mod publication;

pub(super) fn synchronize(world: &mut World) {
    let _synchronization = nucleus::performance::Span::new("native-output-synchronization");
    world.init_resource::<preparation::Preparation>();
    world.resource_scope(|world, mut state: Mut<preparation::Preparation>| {
        synchronize_prepared(world, &mut state);
        state.clear_batch();
    });
}

fn synchronize_prepared(world: &mut World, state: &mut preparation::Preparation) {
    let preparation::Preparation {
        queries,
        views: render_views,
        passive,
        backgrounds,
        cameras,
        instances: old,
        living,
        fills,
        live_fills,
        depths,
        areas,
        members,
        positions,
        uvs,
    } = state;
    let view_preparation = nucleus::performance::Span::new("native-output-view-preparation");
    if world.get_resource::<OutputViews>().is_some_and(|views| {
        views
            .outputs
            .iter()
            .any(|output| output.id != views.focused)
    }) {
        let roots = preparation::query(world, &mut queries.roots);
        let views = world.resource::<OutputViews>();
        let root = roots.iter(world).find(|root| views.applies_to(*root));
        let targets = world.get_resource::<Targets>();
        for (index, output) in views
            .outputs
            .iter()
            .enumerate()
            .filter(|(_, output)| output.id != views.focused)
        {
            let Some(root) = root else {
                continue;
            };
            let Some(image) = targets.and_then(|targets| targets.0.get(&output.id)) else {
                continue;
            };
            let Some(layer) = crate::native::render_layers::OUTPUTS.get(index) else {
                continue;
            };
            let canvas = crate::native::output_views::canvas(world, root, output.workspace);
            render_views.push(RenderView {
                id: ViewKey::Output(output.id),
                root,
                workspace: output.workspace,
                physical_size: output.physical_size,
                logical_size: Vec2::from_array(output.bounds.size().map(|value| value as f32)),
                scale: output.raster.scale[0] as f32,
                offset: Vec2::from_array(output.raster.origin.map(|value| value as f32)),
                canvas,
                center: canvas.center,
                image: image.clone(),
                layer,
            });
        }
    }
    if let Some(previews) = world.get_resource::<super::previews::Previews>() {
        for (index, preview) in previews.0.iter().enumerate() {
            let Some(layer) = crate::native::render_layers::OVERVIEW.get(index) else {
                continue;
            };
            let mut canvas =
                crate::native::output_views::canvas(world, preview.root, preview.workspace);
            canvas.zoom = preview.view.zoom;
            render_views.push(RenderView {
                id: ViewKey::Preview(preview.id),
                root: preview.root,
                workspace: preview.workspace,
                physical_size: preview.pixels.to_array(),
                logical_size: preview.pixels.as_vec2(),
                scale: 1.0,
                offset: Vec2::ZERO,
                canvas,
                center: preview.view.center,
                image: preview.image.clone(),
                layer,
            });
        }
    }
    passive.extend(render_views.iter().map(|view| view.id));
    backgrounds.extend(
        preparation::query(world, &mut queries.backgrounds)
            .iter(world)
            .map(|(entity, background)| {
                (
                    background.output,
                    (entity, background.grid, background.material.id()),
                )
            }),
    );
    for (output, (camera, grid, _)) in backgrounds.iter() {
        if !passive.contains(output) {
            world.despawn(*grid);
            world.despawn(*camera);
        }
    }
    cameras.extend(
        preparation::query(world, &mut queries.cameras)
            .iter(world)
            .map(|(entity, camera)| (entity, camera.0)),
    );
    for (entity, output) in cameras.iter() {
        if !passive.contains(output) {
            world.despawn(*entity);
        }
    }
    old.extend(
        preparation::query(world, &mut queries.instances)
            .iter(world)
            .map(|(entity, instance)| ((instance.output, instance.owner), entity)),
    );
    fills.extend(
        preparation::query(world, &mut queries.fills)
            .iter(world)
            .map(|(entity, fill)| (*fill, entity)),
    );
    nucleus::performance::add("native-output-views-prepared", render_views.len() as u64);
    drop(view_preparation);
    for output in render_views.iter() {
        let target = &output.image;
        let layer = RenderLayers::none().with(output.layer);
        let root = output.root;
        let canvas = output.canvas;
        let native_size =
            Vec2::from_array(output.physical_size.map(|value| value as f32)) / output.scale;
        let inset =
            ((native_size - output.logical_size) * 0.5 + output.offset) / canvas.zoom as f32;
        let delta = (output.center - canvas.center).as_vec2() + inset;
        let center = Vec3::new(delta.x, 0.0, delta.y);
        let transform =
            Transform::from_translation(center + Vec3::Y * 10000.0).looking_at(center, -Vec3::Z);
        let colors = crate::native::canvas_background::for_workspace(world, root, output.workspace);
        let background_color =
            ClearColorConfig::Custom(crate::native::canvas_background::color(colors.background));
        if let Some(pattern) = crate::native::canvas_background::output_pattern(
            world,
            colors.grid,
            crate::native::canvas::CanvasView {
                center: output.center + inset.as_dvec2(),
                ..canvas
            },
            native_size,
        ) {
            if let Some((camera, _, handle)) = backgrounds.get(&output.id) {
                if world
                    .resource::<Assets<crate::native::canvas_background::PatternMaterial>>()
                    .get(*handle)
                    != Some(&pattern)
                {
                    *world
                        .resource_mut::<Assets<crate::native::canvas_background::PatternMaterial>>()
                        .get_mut(*handle)
                        .unwrap() = pattern;
                }
                publication::image(world, *camera, target, output.scale);
                publication::clear_color(world, *camera, background_color);
                crate::native::app::configure_output_color(world, *camera, target);
            } else {
                let material = world
                    .resource_mut::<Assets<crate::native::canvas_background::PatternMaterial>>()
                    .add(pattern);
                let camera = world
                    .spawn((
                        Camera2d,
                        Camera {
                            order: -2,
                            clear_color: background_color,
                            ..default()
                        },
                        bevy::camera::RenderTarget::Image(bevy::camera::ImageRenderTarget {
                            handle: target.clone(),
                            scale_factor: output.scale,
                        }),
                    ))
                    .id();
                let grid = world
                    .spawn((
                        Node {
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                        MaterialNode(material.clone()),
                        UiTargetCamera(camera),
                        Pickable::IGNORE,
                    ))
                    .id();
                world.entity_mut(camera).insert(Background {
                    output: output.id,
                    grid,
                    material,
                });
                crate::native::app::configure_output_color(world, camera, target);
            }
        }
        let clear_color = ClearColorConfig::None;
        let camera = if let Some((camera, _)) = cameras.iter().find(|(_, id)| *id == output.id) {
            publication::projection(world, *camera, 1.0 / canvas.zoom as f32);
            publication::transform(world, *camera, transform);
            publication::layers(world, *camera, &layer);
            publication::image(world, *camera, target, output.scale);
            publication::clear_color(world, *camera, clear_color);
            *camera
        } else {
            world
                .spawn((
                    Camera3d::default(),
                    OutputCamera(output.id),
                    WorldView,
                    Projection::Orthographic(OrthographicProjection {
                        scale: 1.0 / canvas.zoom as f32,
                        near: -1_000_000.0,
                        far: 1_000_000.0,
                        ..OrthographicProjection::default_3d()
                    }),
                    layer.clone(),
                    bevy::camera::RenderTarget::Image(bevy::camera::ImageRenderTarget {
                        handle: target.clone(),
                        scale_factor: output.scale,
                    }),
                    Camera {
                        order: if matches!(output.id, ViewKey::Preview(_)) {
                            -1
                        } else {
                            0
                        },
                        clear_color,
                        output_mode: bevy::camera::CameraOutputMode::Write {
                            blend_state: Some(
                                bevy::render::render_resource::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
                            ),
                            clear_color: ClearColorConfig::None,
                        },
                        ..default()
                    },
                    transform,
                    bevy::core_pipeline::tonemapping::Tonemapping::None,
                    bevy::core_pipeline::tonemapping::DebandDither::Disabled,
                ))
                .id()
        };
        crate::native::app::configure_output_color(world, camera, target);
        super::stacking::camera(world, camera);
        let fill_preparation = nucleus::performance::Span::new("native-output-fill-preparation");
        depths.clear();
        let layer_count = if let Some(layers) = holes::layers_for(world, root, output.workspace) {
            for hole_layer in &layers.0 {
                depths
                    .entry(hole_layer.workspace)
                    .or_insert(hole_layer.depth);
            }
            layers.0.len()
        } else {
            0
        };
        drop(fill_preparation);
        for index in 0..layer_count {
            let fill_preparation =
                nucleus::performance::Span::new("native-output-fill-preparation");
            let hole_layer = &holes::layers_for(world, root, output.workspace).unwrap().0[index];
            if hole_layer.depth == 0 {
                continue;
            }
            let workspace = hole_layer.workspace;
            let depth = hole_layer.depth;
            let polygons = holes::remove(hole_layer.openings.clone(), &hole_layer.cuts);
            let color =
                crate::native::canvas_background::for_workspace(world, root, workspace).background;
            drop(fill_preparation);
            let key = Fill {
                output: output.id,
                owner: root,
                workspace,
            };
            live_fills.insert(key);
            fill(
                world,
                key,
                fills.get(&key).copied(),
                &polygons,
                canvas.center,
                -(depth as f32) * 0.2 - 0.15,
                crate::native::canvas_background::color(color),
                &layer,
                positions,
            );
        }
        areas.extend(
            preparation::query(world, &mut queries.areas)
                .iter(world)
                .filter(|(_, _, parent, member)| {
                    parent.parent() == root
                        && (member.0 == output.workspace || depths.contains_key(&member.0))
                })
                .map(|(entity, _, _, member)| (entity, member.0)),
        );
        for (entity, workspace) in areas.drain(..) {
            let fill_preparation =
                nucleus::performance::Span::new("native-output-fill-preparation");
            let area = world
                .get::<crate::native::area::InfluenceArea>(entity)
                .unwrap();
            let placement = crate::native::canvas::presented_spatial(world, entity);
            let polygons: Vec<_> = holes::triangles(area, placement)
                .into_iter()
                .flat_map(|polygon| {
                    holes::pieces_in(world, root, output.workspace, workspace, polygon)
                })
                .collect();
            let depth = depths.get(&workspace).copied().unwrap_or(0);
            let color =
                crate::native::canvas_background::color(area.color).with_alpha(area.opacity);
            nucleus::performance::add("native-output-areas-prepared", 1);
            drop(fill_preparation);
            let key = Fill {
                output: output.id,
                owner: entity,
                workspace,
            };
            live_fills.insert(key);
            fill(
                world,
                key,
                fills.get(&key).copied(),
                &polygons,
                canvas.center,
                placement.elevation as f32 - depth as f32 * 0.2 - 0.1,
                color,
                &layer,
                positions,
            );
        }
        {
            let query = preparation::query(world, &mut queries.members);
            let mut ordering = super::stacking::Keys::new(world);
            members.extend(
                query
                    .iter(world)
                    .filter(|(_, _, parent, member, surface)| {
                        parent.parent() == root
                            && surface.visible
                            && (member.0 == output.workspace || depths.contains_key(&member.0))
                    })
                    .filter(|(entity, ..)| super::presentation::displayed(world, *entity))
                    .map(|(entity, item, _, member, _)| {
                        (
                            entity,
                            world
                                .get::<crate::native::canvas::ItemPreview>(entity)
                                .map_or(*item, |preview| preview.0),
                            member.0,
                            ordering.key(entity),
                        )
                    }),
            );
        }
        for (entity, item, workspace, order) in members.drain(..) {
            let surface_preparation =
                nucleus::performance::Span::new("native-output-surface-preparation");
            let (pieces, center, basis) =
                holes::surface_pieces_in(world, root, output.workspace, entity, &item);
            if basis.determinant().abs() < 1e-10 {
                continue;
            }
            let inverse = basis.inverse();
            let depth = depths.get(&workspace).copied().unwrap_or(0);
            let elevation = crate::native::canvas::presented_spatial(world, entity).elevation
                as f32
                - depth as f32 * 0.2
                + 0.01;
            let plane = super::stacking::Plane::new(
                bevy::math::DVec3::Y,
                bevy::math::DVec3::new(0.0, f64::from(elevation), 0.0),
            );
            positions.clear();
            uvs.clear();
            for piece in pieces {
                for triangle in 1..piece.len().saturating_sub(1) {
                    for point in [piece[0], piece[triangle + 1], piece[triangle]] {
                        positions.push([
                            (point.x - canvas.center.x) as f32,
                            elevation,
                            (point.y - canvas.center.y) as f32,
                        ]);
                        let uv =
                            (inverse * (point - center)).as_vec2() / item.size + Vec2::splat(0.5);
                        uvs.push(uv.to_array());
                    }
                }
            }
            nucleus::performance::add("native-output-surfaces-prepared", 1);
            drop(surface_preparation);
            let key = (output.id, entity);
            living.insert(key);
            let mesh = || {
                Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                )
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone())
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_NORMAL,
                    vec![[0.0, 1.0, 0.0]; positions.len()],
                )
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs.clone())
            };
            if let Some(instance) = old.get(&key).copied() {
                super::stacking::synchronize(world, instance, order, plane);
                let previous = world.get::<Instance>(instance).unwrap();
                if previous.positions != *positions || previous.uvs != *uvs {
                    let handle = world.get::<Mesh3d>(instance).unwrap().0.clone();
                    world
                        .resource_mut::<Assets<Mesh>>()
                        .insert(handle.id(), mesh())
                        .unwrap();
                    let mut previous = world.get_mut::<Instance>(instance).unwrap();
                    std::mem::swap(&mut previous.positions, positions);
                    std::mem::swap(&mut previous.uvs, uvs);
                    nucleus::performance::add("native-output-mesh-updates", 1);
                }
                publication::surface(world, instance, entity, &layer);
            } else {
                let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh());
                let material = world.get::<Surface>(entity).unwrap().material.clone();
                let instance = world
                    .spawn((
                        Instance {
                            output: output.id,
                            owner: entity,
                            positions: std::mem::take(positions),
                            uvs: std::mem::take(uvs),
                        },
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        super::compositor::SurfaceFace,
                        super::presentation::VisualOwner(entity),
                        layer.clone(),
                        Pickable::IGNORE,
                        Transform::default(),
                    ))
                    .id();
                super::stacking::synchronize(world, instance, order, plane);
            }
        }
    }
    for (key, entity) in old.drain() {
        if !living.contains(&key) {
            world.despawn(entity);
        }
    }
    for (key, entity) in fills.drain() {
        if !live_fills.contains(&key) {
            world.despawn(entity);
        }
    }
}

fn fill(
    world: &mut World,
    key: Fill,
    previous: Option<Entity>,
    polygons: &[Vec<bevy::math::DVec2>],
    center: bevy::math::DVec2,
    elevation: f32,
    color: Color,
    layer: &RenderLayers,
    points: &mut Vec<[f32; 3]>,
) {
    let _publication = nucleus::performance::Span::new("native-output-fill-publication");
    points.clear();
    for polygon in polygons {
        for index in 1..polygon.len().saturating_sub(1) {
            for point in [polygon[0], polygon[index + 1], polygon[index]] {
                points.push([
                    (point.x - center.x) as f32,
                    elevation,
                    (point.y - center.y) as f32,
                ]);
            }
        }
    }
    let mesh = || {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; points.len()])
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points.clone())
    };
    if let Some(entity) = previous {
        if world.get::<FillGeometry>(entity).unwrap().0 != *points {
            let handle = world.get::<Mesh3d>(entity).unwrap().0.clone();
            world
                .resource_mut::<Assets<Mesh>>()
                .insert(handle.id(), mesh())
                .unwrap();
            std::mem::swap(
                &mut world.get_mut::<FillGeometry>(entity).unwrap().0,
                points,
            );
            nucleus::performance::add("native-output-mesh-updates", 1);
        }
        let material = world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .unwrap()
            .0
            .id();
        let changed = world
            .resource::<Assets<StandardMaterial>>()
            .get(material)
            .is_some_and(|material| material.base_color != color);
        if changed {
            world
                .resource_mut::<Assets<StandardMaterial>>()
                .get_mut(material)
                .unwrap()
                .base_color = color;
            nucleus::performance::add("native-output-fill-material-writes", 1);
        }
        publication::layers(world, entity, layer);
        publication::owner(world, entity, key.owner);
    } else {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh());
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: color,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
        world.spawn((
            key,
            FillGeometry(std::mem::take(points)),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            super::presentation::VisualOwner(key.owner),
            layer.clone(),
            Pickable::IGNORE,
            Transform::default(),
        ));
    }
}
