use crate::native::{
    area::InfluenceArea,
    canvas::CanvasItem,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{
    asset::RenderAssetUsages,
    math::{DMat2, DVec2},
    mesh::PrimitiveTopology,
    prelude::*,
};

type Polygon = Vec<DVec2>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layer {
    pub workspace: u64,
    pub depth: usize,
    pub openings: Vec<Polygon>,
    pub cuts: Vec<Polygon>,
}

#[derive(Component, Clone, Default, PartialEq)]
pub(crate) struct Layers(pub Vec<Layer>);

#[derive(Resource, Default)]
struct Contexts(std::collections::HashMap<(Entity, u64), (Inputs, Layers)>);

pub(crate) fn layers_for(world: &World, root: Entity, active: u64) -> Option<&Layers> {
    if world
        .get::<Workspaces>(root)
        .is_some_and(|spaces| spaces.active == active)
    {
        world.get::<Layers>(root)
    } else {
        world
            .get_resource::<Contexts>()?
            .0
            .get(&(root, active))
            .map(|(_, layers)| layers)
    }
}

pub(crate) fn live_in(world: &World, root: Entity, active: u64, workspace: u64) -> bool {
    active == workspace
        || layers_for(world, root, active)
            .is_some_and(|layers| layers.0.iter().any(|layer| layer.workspace == workspace))
}

#[derive(Component, PartialEq)]
struct Inputs {
    active: u64,
    entries: Vec<u64>,
    holes: Vec<(Entity, u64, DVec2, Polygon, super::Spatial)>,
}

pub(crate) fn live(world: &World, root: Entity, workspace: u64) -> bool {
    world.get::<Workspaces>(root).is_some_and(|spaces| {
        spaces.active == workspace
            || world.get::<Layers>(root).is_some_and(|layers| {
                layers
                    .0
                    .first()
                    .is_some_and(|layer| layer.workspace == spaces.active)
                    && layers.0.iter().any(|layer| layer.workspace == workspace)
            })
    })
}

fn signed_area(polygon: &[DVec2]) -> f64 {
    let Some(origin) = polygon.first() else {
        return 0.0;
    };
    polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
        .map(|(a, b)| (*a - *origin).perp_dot(*b - *origin))
        .sum::<f64>()
        * 0.5
}

fn half_plane(polygon: &[DVec2], a: DVec2, b: DVec2, inside: bool) -> Polygon {
    let mut result = Vec::new();
    for (p, q) in polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
    {
        let distance = |point: DVec2| (b - a).perp_dot(point - a) * if inside { 1.0 } else { -1.0 };
        let dp = distance(*p);
        let dq = distance(*q);
        if dp >= 0.0 {
            result.push(*p);
        }
        if (dp >= 0.0) != (dq >= 0.0) {
            result.push(*p + (*q - *p) * (dp / (dp - dq)));
        }
    }
    result
}

pub(super) fn intersect(polygon: &[DVec2], clip: &[DVec2]) -> Polygon {
    let mut result = polygon.to_vec();
    for (a, b) in clip
        .iter()
        .zip(clip.iter().cycle().skip(1))
        .take(clip.len())
    {
        result = half_plane(&result, *a, *b, true);
        if result.len() < 3 {
            break;
        }
    }
    result
}

fn subtract(polygon: &[DVec2], clip: &[DVec2]) -> Vec<Polygon> {
    let bounds = |polygon: &[DVec2]| {
        polygon.iter().fold(
            (DVec2::INFINITY, DVec2::NEG_INFINITY),
            |(min, max), point| (min.min(*point), max.max(*point)),
        )
    };
    let (a, b) = bounds(polygon);
    let (c, d) = bounds(clip);
    if a.x >= d.x || a.y >= d.y || b.x <= c.x || b.y <= c.y {
        return vec![polygon.to_vec()];
    }
    if signed_area(&intersect(polygon, clip)).abs() < 1e-8 {
        return vec![polygon.to_vec()];
    }
    let mut remaining = polygon.to_vec();
    let mut result = Vec::new();
    for (a, b) in clip
        .iter()
        .zip(clip.iter().cycle().skip(1))
        .take(clip.len())
    {
        let outside = half_plane(&remaining, *a, *b, false);
        if signed_area(&outside).abs() > 1e-8 {
            result.push(outside);
        }
        remaining = half_plane(&remaining, *a, *b, true);
        if remaining.len() < 3 {
            break;
        }
    }
    result
}

pub(super) fn remove(polygons: Vec<Polygon>, cuts: &[Polygon]) -> Vec<Polygon> {
    cuts.iter().fold(polygons, |pieces, cut| {
        pieces
            .into_iter()
            .flat_map(|piece| subtract(&piece, cut))
            .collect()
    })
}

fn union(polygons: Vec<Polygon>) -> Vec<Polygon> {
    let mut result = Vec::new();
    for polygon in polygons {
        let fresh = remove(vec![polygon], &result);
        result.extend(fresh);
    }
    result
}

pub(super) fn triangles(area: &InfluenceArea, placement: super::Spatial) -> Vec<Polygon> {
    triangulate(area.outline(), DVec2::from_array(area.center), placement)
}

fn triangulate(outline: Polygon, center: DVec2, placement: super::Spatial) -> Vec<Polygon> {
    let basis = basis(placement);
    let outline: Vec<_> = outline
        .into_iter()
        .map(|point| center + basis * (point - center))
        .collect();
    let mut convex = outline[..outline.len().saturating_sub(1)].to_vec();
    if signed_area(&convex) < 0.0 {
        convex.reverse();
    }
    if convex
        .iter()
        .zip(convex.iter().cycle().skip(1))
        .zip(convex.iter().cycle().skip(2))
        .take(convex.len())
        .all(|((a, b), c)| (*b - *a).perp_dot(*c - *b) >= -1e-8)
    {
        return if signed_area(&convex).abs() > 1e-8 {
            vec![convex]
        } else {
            Vec::new()
        };
    }
    let mut levels: Vec<_> = outline.iter().map(|point| point.y).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let mut result = Vec::new();
    for level in levels.windows(2) {
        let middle = (level[0] + level[1]) * 0.5;
        let mut edges: Vec<_> = outline
            .windows(2)
            .filter_map(|edge| {
                let [a, b] = [edge[0], edge[1]];
                if (a.y > middle) == (b.y > middle) {
                    return None;
                }
                let x = |y| a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y);
                Some([x(level[0]), x(level[1])])
            })
            .collect();
        edges.sort_by(|a, b| (a[0] + a[1]).total_cmp(&(b[0] + b[1])));
        for pair in edges.chunks_exact(2) {
            let polygon = vec![
                DVec2::new(pair[0][0], level[0]),
                DVec2::new(pair[1][0], level[0]),
                DVec2::new(pair[1][1], level[1]),
                DVec2::new(pair[0][1], level[1]),
            ];
            if signed_area(&polygon).abs() > 1e-8 {
                result.push(polygon);
            }
        }
    }
    result
}

fn basis(placement: super::Spatial) -> DMat2 {
    let x = placement.rotation() * bevy::math::DVec3::X;
    let y = placement.rotation() * bevy::math::DVec3::Z;
    DMat2::from_cols(DVec2::new(x.x, x.z), DVec2::new(y.x, y.z))
}

pub(crate) fn refresh(world: &mut World) {
    world.init_resource::<Contexts>();
    let roots: Vec<_> = world
        .query::<(Entity, &Workspaces)>()
        .iter(world)
        .flat_map(|(entity, spaces)| {
            let mut active = vec![spaces.active];
            if let Some(views) = world
                .get_resource::<crate::native::output_views::OutputViews>()
                .filter(|views| views.applies_to(entity))
            {
                active.extend(views.outputs.iter().map(|output| output.workspace));
            }
            active.sort_unstable();
            active.dedup();
            active
                .into_iter()
                .map(|active| {
                    (
                        entity,
                        active,
                        spaces
                            .entries
                            .iter()
                            .map(|space| space.id)
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let areas: Vec<_> = world
        .query::<(Entity, &InfluenceArea, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(entity, area, parent, member)| {
            area.hole
                && area.enabled
                && !area.paused
                && area.validate()
                && !area.backend_components
                && world
                    .get::<crate::native::component_push::composition::Generated>(*entity)
                    .is_none()
                && world
                    .get::<crate::native::canvas_host::composition::Generated>(*entity)
                    .is_none()
                && crate::native::workspace_config::rules_enabled(world, parent.parent(), member.0)
        })
        .map(|(entity, area, parent, member)| {
            (
                parent.parent(),
                (
                    entity,
                    member.0,
                    DVec2::from_array(area.center),
                    area.outline(),
                    super::spatial(world, entity),
                ),
            )
        })
        .collect();
    let living: std::collections::HashSet<_> = roots
        .iter()
        .map(|(root, active, _)| (*root, *active))
        .collect();
    world
        .resource_mut::<Contexts>()
        .0
        .retain(|key, _| living.contains(key));
    for (root, active, entries) in roots {
        let focused = world
            .get::<Workspaces>(root)
            .is_some_and(|spaces| spaces.active == active);
        let Some(mut index) = entries.iter().position(|workspace| *workspace == active) else {
            if focused {
                world.entity_mut(root).remove::<(Layers, Inputs)>();
            }
            continue;
        };
        let mut holes: Vec<_> = areas
            .iter()
            .filter(|(owner, _)| *owner == root)
            .map(|(_, source)| source.clone())
            .collect();
        holes.sort_by_key(|source| source.0);
        let inputs = Inputs {
            active,
            entries: entries.clone(),
            holes,
        };
        if focused && world.get::<Inputs>(root) == Some(&inputs)
            || !focused
                && world
                    .resource::<Contexts>()
                    .0
                    .get(&(root, active))
                    .is_some_and(|(old, _)| old == &inputs)
        {
            continue;
        }
        let shapes: Vec<_> = inputs
            .holes
            .iter()
            .map(|(_, workspace, center, outline, placement)| {
                (
                    *workspace,
                    triangulate(outline.clone(), *center, *placement),
                )
            })
            .collect();
        let mut layers = Vec::new();
        let mut openings = Vec::new();
        loop {
            let workspace = entries[index];
            let depth = layers.len();
            let cuts = if index > 0 {
                union(
                    shapes
                        .iter()
                        .filter(|(member, _)| *member == workspace)
                        .flat_map(|(_, polygons)| polygons.clone())
                        .collect(),
                )
            } else {
                Vec::new()
            };
            let next = if depth == 0 {
                cuts.clone()
            } else {
                openings
                    .iter()
                    .flat_map(|opening: &Polygon| cuts.iter().map(|cut| intersect(opening, cut)))
                    .filter(|polygon| signed_area(polygon).abs() > 1e-8)
                    .collect()
            };
            layers.push(Layer {
                workspace,
                depth,
                openings,
                cuts,
            });
            if next.is_empty() || index == 0 {
                break;
            }
            openings = next;
            index -= 1;
        }
        let layers = Layers(layers);
        if !focused {
            world
                .resource_mut::<Contexts>()
                .0
                .insert((root, active), (inputs, layers));
            continue;
        }
        if world.get::<Layers>(root) != Some(&layers) {
            world.entity_mut(root).insert(layers);
        }
        world.entity_mut(root).insert(inputs);
    }
}

pub(crate) fn pieces(
    world: &World,
    root: Entity,
    workspace: u64,
    polygon: Polygon,
) -> Vec<Polygon> {
    let active = world
        .get::<Workspaces>(root)
        .map_or(workspace, |spaces| spaces.active);
    pieces_in(world, root, active, workspace, polygon)
}

pub(crate) fn pieces_in(
    world: &World,
    root: Entity,
    active: u64,
    workspace: u64,
    polygon: Polygon,
) -> Vec<Polygon> {
    let Some(layers) = layers_for(world, root, active) else {
        return if active == workspace {
            vec![polygon]
        } else {
            Vec::new()
        };
    };
    let Some(layer) = layers.0.iter().find(|layer| layer.workspace == workspace) else {
        return Vec::new();
    };
    let pieces = if layer.depth == 0 {
        vec![polygon]
    } else {
        layer
            .openings
            .iter()
            .map(|opening| intersect(&polygon, opening))
            .filter(|piece| signed_area(piece).abs() > 1e-8)
            .collect()
    };
    remove(pieces, &layer.cuts)
}

pub(crate) fn contains(layers: Option<&Layers>, workspace: u64, point: DVec2) -> bool {
    let Some(layer) =
        layers.and_then(|layers| layers.0.iter().find(|layer| layer.workspace == workspace))
    else {
        return layers.is_none();
    };
    let inside = |polygon: &Polygon| {
        polygon
            .iter()
            .zip(polygon.iter().cycle().skip(1))
            .take(polygon.len())
            .all(|(a, b)| (*b - *a).perp_dot(point - *a) >= -1e-6)
    };
    (layer.depth == 0 || layer.openings.iter().any(inside)) && !layer.cuts.iter().any(inside)
}

pub(crate) fn depth(world: &World, root: Entity, workspace: u64) -> usize {
    world
        .get::<Layers>(root)
        .and_then(|layers| layers.0.iter().find(|layer| layer.workspace == workspace))
        .map_or(0, |layer| layer.depth)
}

pub(crate) fn surface_pieces(
    world: &World,
    root: Entity,
    entity: Entity,
    item: &CanvasItem,
) -> (Vec<Polygon>, DVec2, DMat2) {
    let active = world
        .get::<Workspaces>(root)
        .map_or(0, |spaces| spaces.active);
    surface_pieces_in(world, root, active, entity, item)
}

pub(crate) fn surface_pieces_in(
    world: &World,
    root: Entity,
    active: u64,
    entity: Entity,
    item: &CanvasItem,
) -> (Vec<Polygon>, DVec2, DMat2) {
    let (offset, clip) = crate::native::layout::viewport::clip(world, entity);
    let placement = crate::native::canvas::presented_spatial(world, entity);
    let scale = f64::from(
        world
            .get::<crate::native::area_effects::AreaScale>(entity)
            .map_or(1.0, |scale| scale.0),
    );
    let basis = basis(placement) * scale;
    let center = item.position - basis * offset.as_dvec2() / scale;
    let mut polygon = vec![
        clip.min,
        Vec2::new(clip.max.x, clip.min.y),
        clip.max,
        Vec2::new(clip.min.x, clip.max.y),
    ]
    .into_iter()
    .map(|point| center + basis * (point - item.size * 0.5).as_dvec2())
    .collect::<Polygon>();
    if signed_area(&polygon) < 0.0 {
        polygon.reverse();
    }
    let workspace = world
        .get::<WorkspaceMember>(entity)
        .map_or(0, |member| member.0);
    (
        pieces_in(world, root, active, workspace, polygon),
        center,
        basis,
    )
}

#[derive(Component)]
struct ClippedFace {
    vertices: Vec<[f32; 2]>,
    uv: Rect,
}

pub(crate) fn clipped(world: &World, face: Entity) -> bool {
    world.get::<ClippedFace>(face).is_some()
}

pub(crate) fn layer(world: &World, root: Entity, workspace: u64) -> Option<Layer> {
    world
        .get::<Layers>(root)?
        .0
        .iter()
        .find(|layer| layer.workspace == workspace)
        .cloned()
}

pub(crate) fn area_mesh(
    world: &World,
    root: Entity,
    workspace: u64,
    entity: Entity,
    area: &InfluenceArea,
) -> Mesh {
    let placement = super::spatial(world, entity);
    let basis = basis(placement);
    let center = DVec2::from_array(area.center);
    let mut points = Vec::new();
    if basis.determinant().abs() > 1e-10 {
        let inverse = basis.inverse();
        for polygon in triangles(area, placement)
            .into_iter()
            .flat_map(|polygon| pieces(world, root, workspace, polygon))
        {
            for index in 1..polygon.len().saturating_sub(1) {
                for point in [polygon[0], polygon[index + 1], polygon[index]] {
                    let local = inverse * (point - center);
                    points.push([local.x as f32, -0.1, local.y as f32]);
                }
            }
        }
    } else {
        for polygon in triangles(area, super::Spatial::default()) {
            for index in 1..polygon.len().saturating_sub(1) {
                for point in [polygon[0], polygon[index + 1], polygon[index]] {
                    let local = point - center;
                    points.push([local.x as f32, -0.1, local.y as f32]);
                }
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; points.len()])
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
}

pub(crate) fn clip_surface(world: &mut World, root: Entity, entity: Entity, item: &CanvasItem) {
    let Some(surface) = world.get::<super::presentation::Surface>(entity) else {
        return;
    };
    let face = surface.face;
    let body = surface.body;
    let workspace = world
        .get::<WorkspaceMember>(entity)
        .map_or(0, |member| member.0);
    let masked = world.get::<Layers>(root).is_some_and(|layers| {
        layers
            .0
            .iter()
            .find(|layer| layer.workspace == workspace)
            .is_some_and(|layer| layer.depth > 0 || !layer.cuts.is_empty())
    });
    if !masked && world.get::<ClippedFace>(face).is_none() {
        return;
    }
    let (pieces, center, basis) = surface_pieces(world, root, entity, item);
    let (_, clip) = crate::native::layout::viewport::clip(world, entity);
    if basis.determinant().abs() < 1e-10 || clip.size().min_element() <= 0.0 {
        return;
    }
    let inverse = basis.inverse();
    let mut vertices = Vec::new();
    for piece in pieces {
        for index in 1..piece.len().saturating_sub(1) {
            for point in [piece[0], piece[index + 1], piece[index]] {
                let local = (inverse * (point - center)).as_vec2() + item.size * 0.5;
                vertices.push(((local - clip.min) / clip.size()).to_array());
            }
        }
    }
    let uv = Rect::from_corners(clip.min / item.size, clip.max / item.size);
    if world
        .get::<ClippedFace>(face)
        .is_none_or(|old| old.vertices != vertices || old.uv != uv)
    {
        let positions: Vec<_> = vertices
            .iter()
            .map(|point| [point[0] - 0.5, 0.5 - point[1], 0.0])
            .collect();
        let uvs: Vec<_> = vertices
            .iter()
            .map(|point| {
                ((clip.min + Vec2::from_array(*point) * clip.size()) / item.size).to_array()
            })
            .collect();
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            vec![[0.0, 0.0, 1.0]; positions.len()],
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        let handle = world.get::<Mesh3d>(face).unwrap().0.clone();
        world
            .resource_mut::<Assets<Mesh>>()
            .insert(handle.id(), mesh)
            .unwrap();
        world.entity_mut(face).insert(ClippedFace { vertices, uv });
    }
    if masked {
        world
            .get_mut::<Visibility>(body)
            .unwrap()
            .set_if_neq(Visibility::Hidden);
    }
}

#[derive(Component)]
struct Background {
    root: Entity,
    workspace: u64,
    points: Vec<[f32; 3]>,
}

pub(crate) fn backgrounds(world: &mut World) {
    let layers: Vec<_> = world
        .query::<(Entity, &Layers)>()
        .iter(world)
        .flat_map(|(root, layers)| {
            layers
                .0
                .iter()
                .filter(|layer| layer.depth > 0)
                .map(move |layer| (root, layer.clone()))
        })
        .collect();
    let old: Vec<_> = world
        .query::<(Entity, &Background)>()
        .iter(world)
        .map(|(entity, background)| (entity, background.root, background.workspace))
        .collect();
    for (entity, root, workspace) in &old {
        if !layers
            .iter()
            .any(|(owner, layer)| owner == root && layer.workspace == *workspace)
        {
            world.despawn(*entity);
        }
    }
    for (root, layer) in layers {
        let color = crate::native::canvas_background::color(
            crate::native::canvas_background::for_workspace(world, root, layer.workspace)
                .background,
        );
        let origin = super::presentation::origin(world, root);
        let pieces = remove(layer.openings, &layer.cuts);
        let mut points = Vec::new();
        for piece in pieces {
            for index in 1..piece.len().saturating_sub(1) {
                for point in [piece[0], piece[index + 1], piece[index]] {
                    points.push([
                        (point.x - origin.x) as f32,
                        -(layer.depth as f32) * 0.2 - 0.15,
                        (point.y - origin.z) as f32,
                    ]);
                }
            }
        }
        let entity = old
            .iter()
            .find(|(_, owner, workspace)| *owner == root && *workspace == layer.workspace)
            .map(|(entity, _, _)| *entity);
        let mesh = || {
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; points.len()])
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points.clone())
        };
        if let Some(entity) = entity {
            if world.get::<Background>(entity).unwrap().points != points {
                let handle = world.get::<Mesh3d>(entity).unwrap().0.clone();
                world
                    .resource_mut::<Assets<Mesh>>()
                    .insert(handle.id(), mesh())
                    .unwrap();
                world.get_mut::<Background>(entity).unwrap().points = points;
            }
            let material = world
                .get::<MeshMaterial3d<StandardMaterial>>(entity)
                .unwrap()
                .0
                .clone();
            if world
                .resource::<Assets<StandardMaterial>>()
                .get(&material)
                .unwrap()
                .base_color
                != color
            {
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .get_mut(&material)
                    .unwrap()
                    .base_color = color;
            }
        } else {
            let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh());
            let material = world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial {
                    base_color: color,
                    unlit: true,
                    ..default()
                });
            world.spawn((
                Background {
                    root,
                    workspace: layer.workspace,
                    points,
                },
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::default(),
                Pickable::IGNORE,
            ));
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod render_tests;
