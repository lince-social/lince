use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};

#[derive(Component)]
struct Volume {
    entity: Entity,
    fill: Entity,
    shape: crate::area::InfluenceArea,
    mesh: Handle<Mesh>,
    fill_mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Resource)]
struct SelectionVolume(Entity);

fn selection(world: &mut World) {
    let bounds = crate::canvas_selection::active_volume(world);
    let previous = world
        .get_resource::<SelectionVolume>()
        .map(|volume| volume.0);
    let Some((root, min, max)) = bounds else {
        if let Some(entity) = previous {
            world.entity_mut(entity).insert(Visibility::Hidden);
        }
        return;
    };
    let entity = if let Some(entity) = previous {
        entity
    } else {
        let mut points = Vec::new();
        for axis in 0..3 {
            for a in [-0.5, 0.5] {
                for b in [-0.5, 0.5] {
                    let mut first = Vec3::ZERO;
                    first[axis] = -0.5;
                    first[(axis + 1) % 3] = a;
                    first[(axis + 2) % 3] = b;
                    let mut second = first;
                    second[axis] = 0.5;
                    points.extend([first.to_array(), second.to_array()]);
                }
            }
        }
        let mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; points.len()])
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points);
        let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.65, 1.0),
                unlit: true,
                ..default()
            });
        let entity = world
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Pickable::IGNORE,
                Transform::default(),
            ))
            .id();
        world.insert_resource(SelectionVolume(entity));
        entity
    };
    let transform = Transform::from_translation(
        ((min + max) * 0.5 - super::presentation::origin(world, root)).as_vec3(),
    )
    .with_scale((max - min).as_vec3());
    world
        .entity_mut(entity)
        .insert((Visibility::Visible, transform));
}

fn mesh(area: &crate::area::InfluenceArea) -> Mesh {
    let outline = area.outline();
    let origin = bevy::math::DVec2::from_array(area.center);
    let mut points = Vec::new();
    for index in 0..outline.len() {
        let a = outline[index] - origin;
        let b = outline[(index + 1) % outline.len()] - origin;
        for y in [0.0, -area.depth as f32] {
            points.extend([[a.x as f32, y, a.y as f32], [b.x as f32, y, b.y as f32]]);
        }
        points.extend([
            [a.x as f32, 0.0, a.y as f32],
            [a.x as f32, -area.depth as f32, a.y as f32],
        ]);
    }
    let normals = vec![[0.0, 1.0, 0.0]; points.len()];
    Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
}

fn fill_mesh(area: &crate::area::InfluenceArea) -> Mesh {
    let origin = bevy::math::DVec2::from_array(area.center);
    let outline: Vec<_> = area.outline().into_iter().map(|p| p - origin).collect();
    let mut levels: Vec<_> = outline.iter().map(|p| p.y).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let mut points = Vec::new();
    for level in levels.windows(2) {
        let middle = (level[0] + level[1]) * 0.5;
        let mut edges: Vec<_> = outline
            .windows(2)
            .filter_map(|edge| {
                let [a, b] = [edge[0], edge[1]];
                if (a.y > middle) == (b.y > middle) {
                    return None;
                }
                let x = |y| a.x + (b.x - a.x) * ((y - a.y) / (b.y - a.y));
                Some([x(level[0]), x(level[1])])
            })
            .collect();
        edges.sort_by(|a, b| (a[0] + a[1]).total_cmp(&(b[0] + b[1])));
        for pair in edges.chunks_exact(2) {
            let a = [pair[0][0] as f32, -0.1, level[0] as f32];
            let b = [pair[1][0] as f32, -0.1, level[0] as f32];
            let c = [pair[1][1] as f32, -0.1, level[1] as f32];
            let d = [pair[0][1] as f32, -0.1, level[1] as f32];
            points.extend([a, c, b, a, d, c]);
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; points.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
}

fn fill_color(area: &crate::area::InfluenceArea) -> Color {
    crate::canvas_background::color(area.color).with_alpha(area.opacity)
}

pub fn update(world: &mut World) {
    if !super::presentation::ready(world) {
        return;
    }
    selection(world);
    let areas: Vec<_> = world
        .query::<(
            Entity,
            &crate::area::InfluenceArea,
            &ChildOf,
            &crate::workspace::WorkspaceMember,
        )>()
        .iter(world)
        .map(|(e, a, p, m)| (e, a.clone(), p.parent(), m.0))
        .collect();
    for (entity, area, root, workspace) in areas {
        let old = world.get::<Volume>(entity);
        let volume = if let Some(old) = old {
            let (visual, handle, fill_handle, material, changed) = (
                old.entity,
                old.mesh.clone(),
                old.fill_mesh.clone(),
                old.material.clone(),
                old.shape != area,
            );
            if changed {
                world
                    .resource_mut::<Assets<Mesh>>()
                    .insert(handle.id(), mesh(&area))
                    .unwrap();
                world
                    .resource_mut::<Assets<Mesh>>()
                    .insert(fill_handle.id(), fill_mesh(&area))
                    .unwrap();
                world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .get_mut(&material)
                    .unwrap()
                    .base_color = fill_color(&area);
                world.get_mut::<Volume>(entity).unwrap().shape = area.clone();
            }
            visual
        } else {
            let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh(&area));
            let material = world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial {
                    base_color: Color::srgb(0.5, 0.4, 0.85),
                    unlit: true,
                    ..default()
                });
            let visual = world
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material),
                    Transform::default(),
                    super::presentation::VisualOwner(entity),
                    Pickable::IGNORE,
                ))
                .id();
            let fill_mesh = world.resource_mut::<Assets<Mesh>>().add(fill_mesh(&area));
            let material = world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial {
                    base_color: fill_color(&area),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                });
            let fill = world
                .spawn((
                    Mesh3d(fill_mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    super::presentation::VisualOwner(entity),
                    Pickable::IGNORE,
                    bevy::light::NotShadowCaster,
                    bevy::light::NotShadowReceiver,
                ))
                .id();
            world.entity_mut(entity).insert(Volume {
                entity: visual,
                fill,
                shape: area.clone(),
                mesh,
                fill_mesh,
                material,
            });
            visual
        };
        let placement = super::spatial(world, entity);
        let origin = super::presentation::origin(world, root);
        let visible = world
            .get::<crate::workspace::Workspaces>(root)
            .is_some_and(|s| s.active == workspace);
        let editing = world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|m| m.enabled);
        let transform = Transform {
            translation: (placement.position(bevy::math::DVec2::from_array(area.center)) - origin)
                .as_vec3(),
            rotation: placement.rotation().as_quat(),
            ..default()
        };
        let fill = world.get::<Volume>(entity).unwrap().fill;
        world.entity_mut(fill).insert((
            transform,
            if visible && area.opacity > 0.0 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
        world.entity_mut(volume).insert((
            transform,
            if visible && editing {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::area::{AreaShape, InfluenceArea};
    use bevy::{math::DVec2, mesh::VertexAttributeValues};

    #[test]
    fn fills_concave_shapes_without_covering_the_notch() {
        let area = InfluenceArea::drawn(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(100.0, 0.0),
            DVec2::new(100.0, 40.0),
            DVec2::new(40.0, 40.0),
            DVec2::new(40.0, 100.0),
            DVec2::new(0.0, 100.0),
        ])
        .unwrap();
        let mesh = fill_mesh(&area);
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing positions")
        };
        let mut surface = 0.0;
        for triangle in points.chunks_exact(3) {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
            let [a, b, c] = [
                Vec3::from_array(a),
                Vec3::from_array(b),
                Vec3::from_array(c),
            ];
            surface += (b - a).cross(c - a).length() * 0.5;
            let middle = (a + b + c) / 3.0;
            assert!(area.contains(
                DVec2::from_array(area.center)
                    + DVec2::new(f64::from(middle.x), f64::from(middle.z))
            ));
        }
        assert!((surface - 6400.0).abs() < 0.01);
    }

    #[test]
    fn fills_remain_visible_in_normal_mode_and_follow_workspace_and_color() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let workspace = world
            .get::<crate::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0));
        let entity = world
            .spawn((
                area,
                ChildOf(root),
                crate::workspace::WorkspaceMember(workspace),
            ))
            .id();
        update(&mut world);
        let volume = world.get::<Volume>(entity).unwrap();
        let (fill, outline, material) = (volume.fill, volume.entity, volume.material.clone());
        assert_eq!(world.get::<Visibility>(fill), Some(&Visibility::Visible));
        assert_eq!(world.get::<Visibility>(outline), Some(&Visibility::Hidden));
        world
            .get_mut::<crate::area::InfluenceArea>(entity)
            .unwrap()
            .color = [255, 0, 0];
        update(&mut world);
        assert_eq!(
            world
                .resource::<Assets<StandardMaterial>>()
                .get(&material)
                .unwrap()
                .base_color,
            Color::srgba(1.0, 0.0, 0.0, 0.18)
        );
        world
            .get_mut::<crate::workspace::WorkspaceMember>(entity)
            .unwrap()
            .0 = workspace + 1;
        update(&mut world);
        assert_eq!(world.get::<Visibility>(fill), Some(&Visibility::Hidden));
    }
}
