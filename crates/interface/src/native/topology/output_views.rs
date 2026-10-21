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

#[derive(Component)]
struct OutputCamera(OutputId);

#[derive(Component)]
struct Background {
    output: OutputId,
    grid: Entity,
    material: Handle<crate::native::canvas_background::PatternMaterial>,
}

#[derive(Component)]
struct Instance {
    output: OutputId,
    owner: Entity,
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
struct Fill {
    output: OutputId,
    owner: Entity,
    workspace: u64,
}

#[derive(Component)]
struct FillGeometry(Vec<[f32; 3]>);

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
        .map(|(entity, item, _, member, layout, _)| (entity, *item, member.0, layout.id))
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
        let spatial = world.get::<super::view::View>(root).is_some_and(|view| view.spatial)
            && world.get_resource::<OutputViews>().is_none_or(|views| views.focused == output.id);
        let corners = if spatial {
            let Some(corners) = super::presentation::projected_corners(world, entity) else { continue };
            let low = std::array::from_fn(|axis| corners.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min));
            let high: [f64; 2] = std::array::from_fn(|axis| corners.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max));
            let Some(bounds) = Rectangle::new(low, std::array::from_fn(|axis| high[axis] - low[axis])) else { continue };
            rectangle = bounds;
            Some(corners)
        } else { None };
        let mapping = ContentMapping { canvas: rectangle, surface, corners };
        let screen = |point: bevy::math::DVec2| {
            if spatial {
                let uv = (point - item.position) / size + bevy::math::DVec2::splat(0.5);
                mapping.canvas_point(std::array::from_fn(|axis| surface.origin()[axis] + uv[axis] * surface.size()[axis]))
                    .map(bevy::math::DVec2::from_array)
            } else { Some(screen(point)) }
        };
        let polygons = holes::surface_pieces_in(world, root, output.workspace, entity, &item).0;
        let extent = bevy::math::DVec2::from_array(output.bounds.size());
        let viewport = [
            bevy::math::DVec2::ZERO,
            bevy::math::DVec2::new(extent.x, 0.0),
            extent,
            bevy::math::DVec2::new(0.0, extent.y),
        ];
        let clip: Vec<_> = polygons
            .into_iter()
            .map(|polygon| {
                let polygon: Vec<_> = polygon.into_iter().filter_map(screen).collect();
                let winding: f64 = polygon.iter().zip(polygon.iter().cycle().skip(1)).take(polygon.len())
                    .map(|(a, b)| a.x * b.y - b.x * a.y).sum();
                let mut polygon = polygon;
                if winding < 0.0 { polygon.reverse(); }
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
            super::spatial(world, entity).elevation,
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
    result.sort_by(|(a_depth, a_height, a), (b_depth, b_height, b)| {
        b_depth
            .cmp(a_depth)
            .then_with(|| a_height.total_cmp(b_height))
            .then_with(|| a.floating.cmp(&b.floating))
            .then_with(|| a.id.cmp(&b.id))
    });
    result.into_iter().map(|(_, _, item)| item).collect()
}

pub(super) fn synchronize(world: &mut World) {
    let Some(views) = world.get_resource::<OutputViews>().cloned() else {
        return;
    };
    let Some(targets) = world
        .get_resource::<Targets>()
        .map(|targets| targets.0.clone())
    else {
        return;
    };
    let passive: HashSet<_> = views
        .outputs
        .iter()
        .filter(|output| output.id != views.focused)
        .map(|output| output.id)
        .collect();
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
    let roots: Vec<_> = world
        .query::<(Entity, &Workspaces)>()
        .iter(world)
        .filter(|(entity, _)| views.applies_to(*entity))
        .map(|(entity, _)| entity)
        .collect();
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
    for (index, output) in views
        .outputs
        .iter()
        .enumerate()
        .filter(|(_, output)| output.id != views.focused)
    {
        let Some(target) = targets.get(&output.id) else {
            continue;
        };
        let layer = RenderLayers::layer(32 + index);
        let Some(root) = roots.first().copied() else {
            continue;
        };
        let canvas = crate::native::output_views::canvas(world, root, output.workspace);
        let colors = crate::native::canvas_background::for_workspace(world, root, output.workspace);
        let background_color =
            ClearColorConfig::Custom(crate::native::canvas_background::color(colors.background));
        if let Some(pattern) = crate::native::canvas_background::output_pattern(
            world,
            root,
            output.workspace,
            canvas,
            Vec2::from_array(output.physical_size.map(|size| size as f32)),
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
                    .insert(bevy::camera::RenderTarget::Image(target.clone().into()));
                world.get_mut::<Camera>(*camera).unwrap().clear_color = background_color;
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
                        bevy::camera::RenderTarget::Image(target.clone().into()),
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
            }
        }
        let clear_color = ClearColorConfig::None;
        let projection = Projection::Orthographic(OrthographicProjection {
            scale: 1.0 / canvas.zoom as f32,
            near: -1_000_000.0,
            far: 1_000_000.0,
            ..OrthographicProjection::default_3d()
        });
        if let Some((camera, _)) = cameras.iter().find(|(_, id)| *id == output.id) {
            world.entity_mut(*camera).insert((
                projection,
                layer.clone(),
                bevy::camera::RenderTarget::Image(target.clone().into()),
            ));
            world.get_mut::<Camera>(*camera).unwrap().clear_color = clear_color;
        } else {
            world.spawn((
                Camera3d::default(),
                OutputCamera(output.id),
                projection,
                layer.clone(),
                bevy::camera::RenderTarget::Image(target.clone().into()),
                Camera {
                    clear_color,
                    output_mode: bevy::camera::CameraOutputMode::Write {
                        blend_state: Some(
                            bevy::render::render_resource::BlendState::ALPHA_BLENDING,
                        ),
                        clear_color: ClearColorConfig::None,
                    },
                    ..default()
                },
                Transform::from_xyz(0.0, 10000.0, 0.0).looking_at(Vec3::ZERO, -Vec3::Z),
                bevy::core_pipeline::tonemapping::Tonemapping::None,
                bevy::core_pipeline::tonemapping::DebandDither::Disabled,
            ));
        }
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
            let placement = super::spatial(world, entity);
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
                (entity, *item, member.0, surface.material.clone())
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
            let elevation =
                super::spatial(world, entity).elevation as f32 - depth as f32 * 0.2 + 0.01;
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
                    .insert((MeshMaterial3d(material), layer.clone()));
            } else {
                let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh());
                world.spawn((
                    Instance {
                        output: output.id,
                        owner: entity,
                        positions,
                        uvs,
                    },
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    layer.clone(),
                    Pickable::IGNORE,
                    Transform::default(),
                ));
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
        world.entity_mut(entity).insert(layer.clone());
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
            layer.clone(),
            Pickable::IGNORE,
            Transform::default(),
        ));
    }
}
