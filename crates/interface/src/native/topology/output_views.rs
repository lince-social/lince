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

pub fn geometry(
    world: &mut World,
    root: Entity,
    output: &crate::host::OutputViewport,
) -> Vec<crate::host::ItemGeometry> {
    use crate::host::{ContentMapping, ItemGeometry, ItemId, Rectangle};
    let canvas = crate::native::output_views::canvas(world, root, output.workspace);
    let members: Vec<_> = world
        .query::<(
            Entity,
            &CanvasItem,
            &ChildOf,
            &WorkspaceMember,
            &crate::native::layout::LayoutBox,
            &Surface,
        )>()
        .iter(world)
        .filter(|(entity, _, parent, member, _, surface)| {
            parent.parent() == root
                && surface.visible
                && super::presentation::displayed(world, *entity)
                && holes::live_in(world, root, output.workspace, member.0)
        })
        .map(|(entity, item, _, member, layout, _)| {
            (
                entity,
                world
                    .get::<crate::native::canvas::ItemPreview>(entity)
                    .map_or(*item, |preview| preview.0),
                member.0,
                layout.id,
            )
        })
        .collect();
    let mut result = Vec::new();
    for (entity, item, workspace, id) in members {
        let scale = world
            .get::<crate::native::area_effects::AreaScale>(entity)
            .map_or(1.0, |scale| f64::from(scale.0));
        let size = item.size.as_dvec2() * scale;
        let screen = |point: bevy::math::DVec2| {
            (point - canvas.center) * canvas.zoom
                + bevy::math::DVec2::from_array(output.bounds.size()) * 0.5
                + bevy::math::DVec2::from_array(output.bounds.origin())
        };
        let Some(mut rectangle) = Rectangle::new(
            screen(item.position - size * 0.5).to_array(),
            (size * canvas.zoom).to_array(),
        ) else {
            continue;
        };
        let (id, surface) = if let Some(application) =
            world.get::<crate::native::layout::desktop::ApplicationWindow>(entity)
        {
            (
                ItemId::Application(application.surface),
                Rectangle::new([0.0; 2], application.content_size.map(f64::from)),
            )
        } else {
            (
                ItemId::Native(id),
                Rectangle::new([0.0; 2], item.size.as_dvec2().to_array()),
            )
        };
        let Some(surface) = surface else {
            continue;
        };
        let spatial = super::view::spatial(world, root)
            && world
                .get_resource::<OutputViews>()
                .is_none_or(|views| views.focused == output.id);
        let projected = spatial
            || world
                .get_resource::<OutputViews>()
                .is_none_or(|views| views.focused == output.id)
                && crate::native::canvas::presented_spatial(world, entity).rotation()
                    != bevy::math::DQuat::IDENTITY;
        let corners = if projected {
            let Some(mut corners) = super::presentation::projected_corners(world, entity) else {
                continue;
            };
            for point in &mut corners {
                for axis in 0..2 {
                    point[axis] += output.raster.origin[axis];
                }
            }
            let low = std::array::from_fn(|axis| {
                corners
                    .iter()
                    .map(|point| point[axis])
                    .fold(f64::INFINITY, f64::min)
            });
            let high: [f64; 2] = std::array::from_fn(|axis| {
                corners
                    .iter()
                    .map(|point| point[axis])
                    .fold(f64::NEG_INFINITY, f64::max)
            });
            let Some(bounds) =
                Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis]))
            else {
                continue;
            };
            rectangle = bounds;
            Some(corners)
        } else {
            None
        };
        let mapping = ContentMapping {
            canvas: rectangle,
            surface,
            corners,
        };
        let (polygons, center, basis) =
            holes::surface_pieces_in(world, root, output.workspace, entity, &item);
        if projected && basis.determinant().abs() < 1e-10 {
            continue;
        }
        let inverse = if projected {
            basis.inverse()
        } else {
            bevy::math::DMat2::IDENTITY
        };
        let screen = |point: bevy::math::DVec2| {
            if projected {
                let uv = inverse * (point - center) / item.size.as_dvec2()
                    + bevy::math::DVec2::splat(0.5);
                mapping
                    .canvas_point(std::array::from_fn(|axis| {
                        surface.origin()[axis] + uv[axis] * surface.size()[axis]
                    }))
                    .map(bevy::math::DVec2::from_array)
            } else {
                Some(screen(point))
            }
        };
        let extent = bevy::math::DVec2::from_array(output.bounds.size());
        let origin = bevy::math::DVec2::from_array(output.bounds.origin());
        let viewport = [
            origin,
            origin + bevy::math::DVec2::new(extent.x, 0.0),
            origin + extent,
            origin + bevy::math::DVec2::new(0.0, extent.y),
        ];
        let clip: Vec<_> = polygons
            .into_iter()
            .map(|polygon| {
                let polygon: Vec<_> = polygon.into_iter().filter_map(screen).collect();
                let winding: f64 = polygon
                    .iter()
                    .zip(polygon.iter().cycle().skip(1))
                    .take(polygon.len())
                    .map(|(a, b)| a.x * b.y - b.x * a.y)
                    .sum();
                let mut polygon = polygon;
                if winding < 0.0 {
                    polygon.reverse();
                }
                holes::intersect(&polygon, &viewport)
            })
            .filter(|polygon| polygon.len() >= 3)
            .map(|polygon| polygon.into_iter().map(|point| point.to_array()).collect())
            .collect();
        if clip.is_empty() {
            continue;
        }
        let depth = holes::layers_for(world, root, output.workspace)
            .and_then(|layers| layers.0.iter().find(|layer| layer.workspace == workspace))
            .map_or(0, |layer| layer.depth);
        result.push((
            depth,
            crate::native::canvas::presented_spatial(world, entity).elevation,
            super::stacking::key(world, entity),
            ItemGeometry {
                id,
                output: output.id,
                mapping,
                floating: world
                    .get::<crate::native::layout::desktop::Managed>(entity)
                    .is_none_or(|managed| managed.floating),
                clip: Some(clip),
            },
        ));
    }
    result.sort_by(
        |(a_depth, a_height, a_order, a), (b_depth, b_height, b_order, b)| {
            b_depth
                .cmp(a_depth)
                .then_with(|| a_height.total_cmp(b_height))
                .then_with(|| a.floating.cmp(&b.floating))
                .then_with(|| a_order.cmp(b_order))
                .then_with(|| a.id.cmp(&b.id))
        },
    );
    result.into_iter().map(|(_, _, _, item)| item).collect()
}

pub(super) fn synchronize(world: &mut World) {
    let views = world.get_resource::<OutputViews>().cloned();
    let targets = world
        .get_resource::<Targets>()
        .map(|targets| targets.0.clone())
        .unwrap_or_default();
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Workspaces>>()
        .iter(world)
        .collect();
    let mut render_views = Vec::new();
    if let Some(views) = &views {
        for (index, output) in views
            .outputs
            .iter()
            .enumerate()
            .filter(|(_, output)| output.id != views.focused)
        {
            let Some(root) = roots.iter().copied().find(|root| views.applies_to(*root)) else {
                continue;
            };
            let Some(image) = targets.get(&output.id).cloned() else {
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
                image,
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
    let passive: HashSet<_> = render_views.iter().map(|view| view.id).collect();
    let backgrounds: HashMap<_, _> = world
        .query::<(Entity, &Background)>()
        .iter(world)
        .map(|(entity, background)| {
            (
                background.output,
                (entity, background.grid, background.material.clone()),
            )
        })
        .collect();
    for (output, (camera, grid, _)) in &backgrounds {
        if !passive.contains(output) {
            world.despawn(*grid);
            world.despawn(*camera);
        }
    }
    let cameras: Vec<_> = world
        .query::<(Entity, &OutputCamera)>()
        .iter(world)
        .map(|(entity, camera)| (entity, camera.0))
        .collect();
    for (entity, output) in &cameras {
        if !passive.contains(output) {
            world.despawn(*entity);
        }
    }
    let old: HashMap<_, _> = world
        .query::<(Entity, &Instance)>()
        .iter(world)
        .map(|(entity, instance)| ((instance.output, instance.owner), entity))
        .collect();
    let mut living = HashSet::new();
    let fills: HashMap<_, _> = world
        .query::<(Entity, &Fill)>()
        .iter(world)
        .map(|(entity, fill)| (*fill, entity))
        .collect();
    let mut live_fills = HashSet::new();
    for output in &render_views {
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
            root,
            output.workspace,
            crate::native::canvas::CanvasView {
                center: output.center + inset.as_dvec2(),
                ..canvas
            },
            native_size,
        ) {
            if let Some((camera, _, handle)) = backgrounds.get(&output.id) {
                if world
                    .resource::<Assets<crate::native::canvas_background::PatternMaterial>>()
                    .get(handle)
                    != Some(&pattern)
                {
                    *world
                        .resource_mut::<Assets<crate::native::canvas_background::PatternMaterial>>()
                        .get_mut(handle)
                        .unwrap() = pattern;
                }
                world
                    .entity_mut(*camera)
                    .insert(bevy::camera::RenderTarget::Image(
                        bevy::camera::ImageRenderTarget {
                            handle: target.clone(),
                            scale_factor: output.scale,
                        },
                    ));
                world.get_mut::<Camera>(*camera).unwrap().clear_color = background_color;
                crate::native::app::configure_output_color(world, *camera, &target);
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
                crate::native::app::configure_output_color(world, camera, &target);
            }
        }
        let clear_color = ClearColorConfig::None;
        let projection = Projection::Orthographic(OrthographicProjection {
            scale: 1.0 / canvas.zoom as f32,
            near: -1_000_000.0,
            far: 1_000_000.0,
            ..OrthographicProjection::default_3d()
        });
        let camera = if let Some((camera, _)) = cameras.iter().find(|(_, id)| *id == output.id) {
            world.entity_mut(*camera).insert((
                projection,
                transform,
                layer.clone(),
                bevy::camera::RenderTarget::Image(bevy::camera::ImageRenderTarget {
                    handle: target.clone(),
                    scale_factor: output.scale,
                }),
            ));
            world.get_mut::<Camera>(*camera).unwrap().clear_color = clear_color;
            *camera
        } else {
            world
                .spawn((
                    Camera3d::default(),
                    OutputCamera(output.id),
                    WorldView,
                    projection,
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
        crate::native::app::configure_output_color(world, camera, &target);
        super::stacking::camera(world, camera);
        let layers = holes::layers_for(world, root, output.workspace)
            .cloned()
            .unwrap_or_default();
        for hole_layer in layers.0.iter().filter(|layer| layer.depth > 0) {
            let polygons = holes::remove(hole_layer.openings.clone(), &hole_layer.cuts);
            let color =
                crate::native::canvas_background::for_workspace(world, root, hole_layer.workspace)
                    .background;
            let key = Fill {
                output: output.id,
                owner: root,
                workspace: hole_layer.workspace,
            };
            live_fills.insert(key);
            fill(
                world,
                key,
                fills.get(&key).copied(),
                &polygons,
                canvas.center,
                -(hole_layer.depth as f32) * 0.2 - 0.15,
                crate::native::canvas_background::color(color),
                &layer,
            );
        }
        let areas: Vec<_> = world
            .query::<(
                Entity,
                &crate::native::area::InfluenceArea,
                &ChildOf,
                &WorkspaceMember,
            )>()
            .iter(world)
            .filter(|(_, _, parent, member)| {
                parent.parent() == root && holes::live_in(world, root, output.workspace, member.0)
            })
            .map(|(entity, area, _, member)| (entity, area.clone(), member.0))
            .collect();
        for (entity, area, workspace) in areas {
            let placement = crate::native::canvas::presented_spatial(world, entity);
            let polygons: Vec<_> = holes::triangles(&area, placement)
                .into_iter()
                .flat_map(|polygon| {
                    holes::pieces_in(world, root, output.workspace, workspace, polygon)
                })
                .collect();
            let depth = layers
                .0
                .iter()
                .find(|layer| layer.workspace == workspace)
                .map_or(0, |layer| layer.depth);
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
                crate::native::canvas_background::color(area.color).with_alpha(area.opacity),
                &layer,
            );
        }
        let members: Vec<_> = world
            .query::<(Entity, &CanvasItem, &ChildOf, &WorkspaceMember, &Surface)>()
            .iter(world)
            .filter(|(_, _, parent, member, surface)| {
                parent.parent() == root
                    && surface.visible
                    && holes::live_in(world, root, output.workspace, member.0)
            })
            .map(|(entity, item, _, member, surface)| {
                (
                    entity,
                    world
                        .get::<crate::native::canvas::ItemPreview>(entity)
                        .map_or(*item, |preview| preview.0),
                    member.0,
                    surface.material.clone(),
                )
            })
            .collect();
        for (entity, item, workspace, material) in members {
            if !super::presentation::displayed(world, entity) {
                continue;
            }
            let (pieces, center, basis) =
                holes::surface_pieces_in(world, root, output.workspace, entity, &item);
            if basis.determinant().abs() < 1e-10 {
                continue;
            }
            let inverse = basis.inverse();
            let depth = holes::layers_for(world, root, output.workspace)
                .and_then(|layers| layers.0.iter().find(|layer| layer.workspace == workspace))
                .map_or(0, |layer| layer.depth);
            let elevation = crate::native::canvas::presented_spatial(world, entity).elevation
                as f32
                - depth as f32 * 0.2
                + 0.01;
            let plane = super::stacking::Plane::new(
                bevy::math::DVec3::Y,
                bevy::math::DVec3::new(0.0, f64::from(elevation), 0.0),
            );
            let mut positions = Vec::new();
            let mut uvs = Vec::new();
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
                super::stacking::synchronize(world, entity, instance, plane);
                let previous = world.get::<Instance>(instance).unwrap();
                if previous.positions != positions || previous.uvs != uvs {
                    let handle = world.get::<Mesh3d>(instance).unwrap().0.clone();
                    world
                        .resource_mut::<Assets<Mesh>>()
                        .insert(handle.id(), mesh())
                        .unwrap();
                    let mut previous = world.get_mut::<Instance>(instance).unwrap();
                    previous.positions = positions;
                    previous.uvs = uvs;
                }
                world
                    .entity_mut(instance)
                    .insert((MeshMaterial3d(material), layer.clone(), super::presentation::VisualOwner(entity), super::compositor::SurfaceFace));
            } else {
                let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh());
                let instance = world
                    .spawn((
                        Instance {
                            output: output.id,
                            owner: entity,
                            positions,
                            uvs,
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
                super::stacking::synchronize(world, entity, instance, plane);
            }
        }
    }
    for (key, entity) in old {
        if !living.contains(&key) {
            world.despawn(entity);
        }
    }
    for (key, entity) in fills {
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
) {
    let mut points = Vec::new();
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
        if world.get::<FillGeometry>(entity).unwrap().0 != points {
            let handle = world.get::<Mesh3d>(entity).unwrap().0.clone();
            world
                .resource_mut::<Assets<Mesh>>()
                .insert(handle.id(), mesh())
                .unwrap();
            world.get_mut::<FillGeometry>(entity).unwrap().0 = points;
        }
        let material = world
            .get::<MeshMaterial3d<StandardMaterial>>(entity)
            .unwrap()
            .0
            .clone();
        if let Some(mut material) = world
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&material)
        {
            if material.base_color != color {
                material.base_color = color;
            }
        }
        world.entity_mut(entity).insert((layer.clone(), super::presentation::VisualOwner(key.owner)));
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
            FillGeometry(points),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            super::presentation::VisualOwner(key.owner),
            layer.clone(),
            Pickable::IGNORE,
            Transform::default(),
        ));
    }
}
