use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};

#[derive(Component)]
struct Volume {
    entity: Entity,
    shape: crate::area::InfluenceArea,
    mesh: Handle<Mesh>,
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
            let (visual, handle, changed) = (old.entity, old.mesh.clone(), old.shape != area);
            if changed {
                world
                    .resource_mut::<Assets<Mesh>>()
                    .insert(handle.id(), mesh(&area))
                    .unwrap();
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
            world.entity_mut(entity).insert(Volume {
                entity: visual,
                shape: area.clone(),
                mesh,
            });
            visual
        };
        let placement = super::spatial(world, entity);
        let origin = super::presentation::origin(world, root);
        let visible = world
            .get::<crate::workspace::Workspaces>(root)
            .is_some_and(|s| s.active == workspace)
            && world
                .get::<crate::edit_mode::EditMode>(root)
                .is_some_and(|m| m.enabled);
        world.entity_mut(volume).insert((
            Transform {
                translation: (placement.position(bevy::math::DVec2::from_array(area.center))
                    - origin)
                    .as_vec3(),
                rotation: placement.rotation().as_quat(),
                ..default()
            },
            if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
    }
}
