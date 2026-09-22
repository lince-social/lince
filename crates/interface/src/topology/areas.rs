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

pub(super) fn mesh(area: &crate::area::InfluenceArea) -> Mesh {
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

pub(super) fn fill_mesh(area: &crate::area::InfluenceArea) -> Mesh {
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

fn workspace_area(world: &World, entity: Entity) -> bool {
    world.get::<crate::area::InfluenceArea>(entity).is_some()
        && world.get::<crate::canvas::CanvasItem>(entity).is_some()
        && world.get::<ChildOf>(entity).is_some_and(|parent| {
            world
                .get::<crate::workspace::Workspaces>(parent.parent())
                .is_some()
        })
}

pub fn update(world: &mut World) {
    if !super::presentation::ready(world) {
        return;
    }
    let stale: Vec<_> = world
        .query::<(Entity, &Volume)>()
        .iter(world)
        .filter(|(entity, volume)| {
            !workspace_area(world, *entity)
                || world.get_entity(volume.entity).is_err()
                || world.get_entity(volume.fill).is_err()
        })
        .map(|(entity, volume)| (entity, volume.entity, volume.fill))
        .collect();
    for (entity, outline, fill) in stale {
        world.entity_mut(entity).remove::<Volume>();
        for visual in [outline, fill] {
            if let Ok(visual) = world.get_entity_mut(visual) {
                visual.despawn();
            }
        }
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
        .filter(|(entity, _, _, _)| workspace_area(world, *entity))
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
    fn shader_castle_feed_does_not_create_workspace_volumes() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        world.spawn(Window::default());
        let root = world
            .spawn((
                crate::workspace::Workspaces::default(),
                crate::canvas::CanvasView::default(),
            ))
            .id();
        let castle = crate::shader_castle::spawn(&mut world, root, 1, DVec2::ZERO, "");
        let feed = world.get::<crate::castle_feed::Frame>(castle).unwrap().area;
        for _ in 0..3 {
            super::super::presentation::synchronize(&mut world);
            update(&mut world);
        }
        assert!(world.get::<Volume>(feed).is_none());
        assert!(
            !world
                .query::<&super::super::presentation::VisualOwner>()
                .iter(&world)
                .any(|owner| owner.0 == feed)
        );
        world.despawn(castle);
        super::super::presentation::synchronize(&mut world);
        update(&mut world);
        assert_eq!(
            world
                .query::<&super::super::presentation::VisualOwner>()
                .iter(&world)
                .count(),
            0
        );
    }

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
    fn removed_visuals_rebuild_and_nested_areas_release_their_volumes() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let entity = world
            .spawn((
                InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0)),
                crate::canvas::CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                ChildOf(root),
                crate::workspace::WorkspaceMember(1),
            ))
            .id();
        update(&mut world);
        let volume = world.get::<Volume>(entity).unwrap();
        let (outline, fill) = (volume.entity, volume.fill);
        world.despawn(fill);
        update(&mut world);
        assert!(world.get_entity(outline).is_err());
        let volume = world.get::<Volume>(entity).unwrap();
        let (outline, fill) = (volume.entity, volume.fill);
        assert!(world.get_entity(outline).is_ok());
        assert!(world.get_entity(fill).is_ok());
        let container = world.spawn(Node::default()).id();
        world.entity_mut(entity).insert(ChildOf(container));
        update(&mut world);
        assert!(world.get::<Volume>(entity).is_none());
        assert!(world.get_entity(outline).is_err());
        assert!(world.get_entity(fill).is_err());
        world.entity_mut(entity).insert(ChildOf(root));
        update(&mut world);
        assert!(world.get::<Volume>(entity).is_some());
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
                crate::canvas::CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
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
