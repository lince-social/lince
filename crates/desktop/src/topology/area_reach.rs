use crate::{
    area::{AreaShape, InfluenceArea, ReachMode, ReachShape},
    area_panel::AreaEditor,
    edit_mode::EditMode,
};
use bevy::{asset::RenderAssetUsages, math::DVec2, mesh::PrimitiveTopology, prelude::*};

#[derive(Component)]
struct Preview {
    area: InfluenceArea,
    visuals: Vec<Entity>,
}

#[derive(Default)]
struct Geometry {
    fill: Vec<[f32; 3]>,
    border: Vec<[f32; 3]>,
}

impl Geometry {
    fn triangle(&mut self, area: &InfluenceArea, points: [DVec2; 3]) {
        let distances = points.map(|point| area.signed_distance(point) - area.reach.radius);
        let mut polygon = Vec::with_capacity(4);
        let mut crossings = Vec::with_capacity(2);
        for i in 0..3 {
            let next = (i + 1) % 3;
            if distances[i] <= 0.0 {
                polygon.push(points[i]);
            }
            if (distances[i] <= 0.0) != (distances[next] <= 0.0) {
                let point = points[i].lerp(
                    points[next],
                    distances[i] / (distances[i] - distances[next]),
                );
                polygon.push(point);
                crossings.push(point);
            }
        }
        for i in 1..polygon.len().saturating_sub(1) {
            for point in [polygon[0], polygon[i], polygon[i + 1]] {
                self.fill.push([point.x as f32, 0.0, point.y as f32]);
            }
        }
        if let [a, b] = crossings.as_slice() {
            for y in [0.0, -area.depth as f32] {
                self.border
                    .extend([[a.x as f32, y, a.y as f32], [b.x as f32, y, b.y as f32]]);
            }
        }
    }

    fn tile(&mut self, area: &InfluenceArea, min: DVec2, max: DVec2, depth: u8) {
        let center = (min + max) * 0.5;
        let radius = (max - min).length() * 0.5;
        let distance = area.signed_distance(center) - area.reach.radius;
        if distance > radius {
            return;
        }
        let corners = [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)];
        if distance.abs() < radius && depth < 9 {
            for corner in corners {
                self.tile(area, corner.min(center), corner.max(center), depth + 1);
            }
        } else {
            self.triangle(area, [corners[0], corners[1], corners[2]]);
            self.triangle(area, [corners[0], corners[2], corners[3]]);
        }
    }
}

fn mesh(topology: PrimitiveTopology, points: Vec<[f32; 3]>) -> Mesh {
    let normals = vec![[0.0, 1.0, 0.0]; points.len()];
    Mesh::new(topology, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
}

fn meshes(area: &InfluenceArea) -> (Mesh, Mesh) {
    let mut shape = area.clone();
    if area.reach.shape == ReachShape::Square {
        shape.shape = AreaShape::Square;
        shape.size = [area.size[0].max(area.size[1]) + 2.0 * area.reach.radius; 2];
    } else if area.shape == AreaShape::Circle {
        shape.size = [area.size[0] + 2.0 * area.reach.radius; 2];
    } else if area.reach.radius > 0.0 {
        let half = DVec2::from_array(area.size) * 0.5 + DVec2::splat(area.reach.radius);
        let mut geometry = Geometry::default();
        geometry.tile(area, -half, half, 0);
        return (
            mesh(PrimitiveTopology::TriangleList, geometry.fill),
            mesh(PrimitiveTopology::LineList, geometry.border),
        );
    }
    (super::areas::fill_mesh(&shape), super::areas::mesh(&shape))
}

fn clear(world: &mut World, root: Entity) {
    if let Some(preview) = world.entity_mut(root).take::<Preview>() {
        for entity in preview.visuals {
            if let Ok(entity) = world.get_entity_mut(entity) {
                entity.despawn();
            }
        }
    }
}

fn spawn(world: &mut World, root: Entity, owner: Entity, area: &InfluenceArea) -> Vec<Entity> {
    if area.reach.mode == ReachMode::Unlimited {
        let font = world
            .get_resource::<crate::theme::Typography>()
            .map_or_else(TextFont::default, |typography| typography.text(14.0));
        return vec![
            world
                .spawn((
                    Text::new("Force range · Unlimited"),
                    font,
                    crate::token_style::text(crate::tokens::Token::Ink),
                    crate::token_style::background(crate::tokens::Token::Surface),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(12),
                        bottom: px(12),
                        padding: UiRect::axes(px(10), px(6)),
                        ..default()
                    },
                    ZIndex(2),
                    Pickable::IGNORE,
                    ChildOf(root),
                ))
                .id(),
        ];
    }
    let (fill, border) = meshes(area);
    [
        (fill, Color::srgba(0.65, 0.5, 1.0, 0.10)),
        (border, Color::srgba(0.65, 0.5, 1.0, 0.85)),
    ]
    .into_iter()
    .map(|(mesh, color)| {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: color,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                cull_mode: None,
                double_sided: true,
                depth_bias: 1.0,
                ..default()
            });
        world
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::default(),
                super::presentation::VisualOwner(owner),
                Pickable::IGNORE,
                bevy::light::NotShadowCaster,
                bevy::light::NotShadowReceiver,
            ))
            .id()
    })
    .collect()
}

pub(super) fn update(world: &mut World) {
    if !super::presentation::ready(world) {
        return;
    }
    let roots: Vec<_> = world
        .query_filtered::<Entity, Or<(With<AreaEditor>, With<Preview>)>>()
        .iter(world)
        .collect();
    for root in roots {
        let selected = world
            .get::<EditMode>(root)
            .filter(|mode| mode.enabled)
            .and_then(|_| world.get::<AreaEditor>(root))
            .and_then(|editor| editor.selected)
            .filter(|entity| crate::area_panel::owns(world, root, *entity))
            .filter(|entity| {
                world
                    .get::<crate::inspection::Inspection>(root)
                    .is_none_or(|inspection| inspection.selected == Some(*entity))
            });
        let Some((entity, area)) = selected.and_then(|entity| {
            world
                .get::<InfluenceArea>(entity)
                .filter(|area| area.validate())
                .cloned()
                .map(|area| (entity, area))
        }) else {
            clear(world, root);
            continue;
        };
        let placement = super::spatial(world, entity);
        let transform = Transform {
            translation: (placement.position(DVec2::from_array(area.center))
                - super::presentation::origin(world, root))
            .as_vec3(),
            rotation: placement.rotation().as_quat(),
            ..default()
        };
        let mut area = area;
        area.center = [0.0; 2];
        if world.get::<Preview>(root).is_none_or(|preview| {
            preview.area.shape != area.shape
                || preview.area.size != area.size
                || preview.area.depth != area.depth
                || preview.area.reach != area.reach
                || preview
                    .visuals
                    .iter()
                    .any(|entity| world.get_entity(*entity).is_err())
        }) {
            clear(world, root);
            let visuals = spawn(world, root, entity, &area);
            world.entity_mut(root).insert(Preview { area, visuals });
        }
        let visuals = world.get::<Preview>(root).unwrap().visuals.clone();
        for visual in visuals {
            if world.get::<Mesh3d>(visual).is_some() {
                world
                    .entity_mut(visual)
                    .insert((transform, super::presentation::VisualOwner(entity)));
            }
        }
    }
}

#[cfg(test)]
mod tests;
