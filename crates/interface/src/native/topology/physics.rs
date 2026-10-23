use super::{
    groups::{GroupPose, members},
    spatial,
};
use avian3d::prelude::*;
use bevy::{
    math::{DQuat, DVec3},
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Weak},
};

#[derive(Component)]
pub struct Body {
    pub root: Entity,
    pub workspace: u64,
    pub members: Vec<Entity>,
    pub position: DVec3,
    pub rotation: DQuat,
    pub signature: Vec<(Entity, Vec3, super::Spatial)>,
    pub held: bool,
}

#[derive(Resource, Default)]
pub struct PreparationMeasurements {
    pub calls: u64,
    pub elapsed: std::time::Duration,
}

#[derive(Component)]
pub struct MeshCollider {
    shape: Collider,
    key: (String, String, u32, bool),
}

impl MeshCollider {
    pub fn ray_distance(&self, origin: DVec3, direction: DVec3) -> Option<f64> {
        self.shape.shape_scaled().cast_local_ray(
            &avian3d::parry::query::Ray::new(origin, direction),
            f64::MAX,
            false,
        )
    }
}

#[derive(Resource, Default)]
struct GeometryCache(HashMap<(String, String, u32, bool), Weak<dyn avian3d::parry::shape::Shape>>);

#[derive(Resource, Default)]
pub struct Runtime;

#[derive(Component)]
struct Part;

pub(crate) fn imported_shape(
    world: &mut World,
    entity: Entity,
    scale: f32,
) -> Result<Option<Collider>, &'static str> {
    let asset = world
        .get::<super::assets::ImportedAsset>(entity)
        .ok_or("Imported asset is unavailable")?;
    let centered = world.get::<super::assets::CenteredAsset>(entity).is_some();
    if let Some(collider) = world.get::<MeshCollider>(entity)
        && collider.key.0 == asset.id
        && collider.key.1 == asset.file
        && collider.key.2 == scale.to_bits()
        && collider.key.3 == centered
    {
        return Ok(Some(collider.shape.clone()));
    }
    let key = (
        asset.id.clone(),
        asset.file.clone(),
        scale.to_bits(),
        centered,
    );
    let started = world
        .contains_resource::<PreparationMeasurements>()
        .then(std::time::Instant::now);
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let Some(parts) = super::assets::mesh_parts(world, entity) else {
        return Ok(None);
    };
    world.init_resource::<GeometryCache>();
    let cached = world
        .resource::<GeometryCache>()
        .0
        .get(&key)
        .and_then(Weak::upgrade);
    if let Some(cached) = cached {
        let shape = Collider::from(avian3d::parry::shape::SharedShape(cached));
        world.entity_mut(entity).insert(MeshCollider {
            shape: shape.clone(),
            key,
        });
        return Ok(Some(shape));
    }
    for (_, handle, transform) in parts {
        let mesh = world
            .get_resource::<Assets<Mesh>>()
            .and_then(|meshes| meshes.get(&handle))
            .ok_or("Imported mesh data is unavailable")?;
        let bevy::mesh::VertexAttributeValues::Float32x3(points) = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .ok_or("Imported mesh has no positions")?
        else {
            return Err("Imported mesh contains invalid triangle geometry");
        };
        let base =
            u32::try_from(vertices.len()).map_err(|_| "Imported mesh has too many vertices")?;
        vertices.extend(
            points.iter().map(|point| {
                (transform.transform_point(Vec3::from_array(*point)) * scale).as_dvec3()
            }),
        );
        let indices: Vec<_> = mesh.indices().map_or_else(
            || (0..points.len() as u32).collect(),
            |indices| indices.iter().map(|index| index as u32).collect(),
        );
        if indices.iter().any(|index| *index as usize >= points.len()) {
            return Err("Imported mesh contains invalid triangle geometry");
        }
        match mesh.primitive_topology() {
            bevy::mesh::PrimitiveTopology::TriangleList => {
                if !indices.len().is_multiple_of(3) {
                    return Err("Imported mesh contains incomplete triangles");
                }
                triangles.extend(
                    indices.chunks_exact(3).map(|triangle| {
                        [base + triangle[0], base + triangle[1], base + triangle[2]]
                    }),
                );
            }
            bevy::mesh::PrimitiveTopology::TriangleStrip => {
                triangles.extend(indices.windows(3).enumerate().map(|(index, triangle)| {
                    if index % 2 == 0 {
                        [base + triangle[0], base + triangle[1], base + triangle[2]]
                    } else {
                        [base + triangle[1], base + triangle[0], base + triangle[2]]
                    }
                }))
            }
            _ => return Err("Only triangle meshes are supported for imported collision"),
        }
    }
    if vertices.iter().any(|point| !point.is_finite()) || triangles.is_empty() {
        return Err("Imported mesh contains invalid triangle geometry");
    }
    triangles.retain(|triangle| {
        let [a, b, c] = triangle.map(|index| vertices[index as usize]);
        (b - a).cross(c - a).length_squared() > 0.0
    });
    if triangles.is_empty() {
        return Err("Imported mesh has no usable triangles");
    }
    let shape = Collider::try_trimesh_with_config(
        vertices,
        triangles,
        TrimeshFlags::MERGE_DUPLICATE_VERTICES,
    )
    .map_err(|_| "Could not prepare imported triangle collision")?;
    if let Some(started) = started {
        let mut measurements = world.resource_mut::<PreparationMeasurements>();
        measurements.calls += 1;
        measurements.elapsed += started.elapsed();
    }
    world
        .resource_mut::<GeometryCache>()
        .0
        .insert(key.clone(), Arc::downgrade(&shape.shape().0));
    world.entity_mut(entity).insert(MeshCollider {
        shape: shape.clone(),
        key,
    });
    Ok(Some(shape))
}

pub(crate) fn room_transform(
    world: &mut World,
    member: Entity,
    before: &nucleus::canvas::Pose,
    after: &nucleus::canvas::Pose,
) {
    let delta = DQuat::from_array(after.rotation) * DQuat::from_array(before.rotation).inverse();
    let bodies: Vec<_> = world
        .query::<(Entity, &Body)>()
        .iter(world)
        .filter(|(_, body)| body.members.as_slice() == [member])
        .map(|(entity, _)| entity)
        .collect();
    for entity in bodies {
        let body = world.get::<Body>(entity).unwrap();
        let pose = nucleus::canvas::Pose {
            position: body.position.to_array(),
            rotation: body.rotation.to_array(),
        };
        let target =
            nucleus::canvas::room::compose(after, &nucleus::canvas::room::relative(before, &pose));
        let position = DVec3::from_array(target.position);
        let rotation = DQuat::from_array(target.rotation);
        let placement = super::spatial(world, member);
        {
            let mut body = world.get_mut::<Body>(entity).unwrap();
            body.position = position;
            body.rotation = rotation;
            for (_, _, spatial) in &mut body.signature {
                spatial.rotation = placement.rotation;
            }
        }
        world
            .entity_mut(entity)
            .insert((Position(position), Rotation(rotation)));
        if let Some(mut velocity) = world.get_mut::<LinearVelocity>(entity) {
            velocity.0 = delta * velocity.0;
        }
        if let Some(mut velocity) = world.get_mut::<AngularVelocity>(entity) {
            velocity.0 = delta * velocity.0;
        }
    }
}

pub fn synchronize(world: &mut World) -> bool {
    if world
        .query_filtered::<Entity, With<super::rooms::Member>>()
        .iter(world)
        .any(|entity| super::rooms::collider(world, entity))
    {
        super::rooms::synchronize(world);
    }
    super::holes::refresh(world);
    if let Some(mut cache) = world.get_resource_mut::<GeometryCache>() {
        cache.0.retain(|_, shape| shape.strong_count() > 0);
    }
    let existing_bodies: HashMap<_, _> = world
        .query::<(Entity, &Body)>()
        .iter(world)
        .filter_map(|(entity, body)| body.members.first().map(|member| (*member, entity)))
        .collect();
    let entities: Vec<_> = world
        .query::<(
            Entity,
            &crate::native::canvas::CanvasItem,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
        )>()
        .iter(world)
        .filter(|(e, _, root, member)| {
            !crate::native::layout::desktop::tiled(world, *e)
                && world
                    .get::<crate::native::layout::desktop::Suppressed>(*e)
                    .is_none()
                && world
                    .get::<crate::native::external_drop::Preview>(*e)
                    .is_none()
                && super::holes::live(world, root.parent(), member.0)
                && world
                    .get::<crate::native::sand_placement::Pinned>(*e)
                    .is_none()
                && (!super::rooms::fixed(world, *e) || super::rooms::collider(world, *e))
                && world
                    .get::<crate::native::arrow_sand::ArrowSand>(*e)
                    .is_none()
                && world.get::<super::terrain::Effect>(*e).is_none()
                && world
                    .get::<crate::native::time_castle::AttachedCard>(*e)
                    .is_none()
                && world
                    .get::<crate::native::protein_area::placement::Pending>(*e)
                    .is_none()
                && world
                    .get::<crate::native::layout::LayoutBox>(*e)
                    .is_none_or(|layout| layout.parent.is_none())
        })
        .map(|(e, _, _, _)| e)
        .collect();
    let mut covered = HashSet::new();
    let mut retained = HashSet::new();
    let mut changed_static_scopes = HashSet::new();
    let mut changed = false;
    for entity in entities {
        if covered.contains(&entity) {
            continue;
        }
        let members = members(world, entity);
        let room_proxy = members
            .iter()
            .any(|member| super::rooms::collider(world, *member));
        if !room_proxy
            && members
                .iter()
                .any(|member| super::rooms::fixed(world, *member))
        {
            covered.extend(members);
            continue;
        }
        if members.len() == 1 {
            world
                .entity_mut(entity)
                .remove::<(GroupPose, super::Attachment)>();
        }
        covered.extend(members.iter().copied());
        let solid: Vec<_> = members
            .iter()
            .copied()
            .filter(|e| {
                (!super::rooms::fixed(world, *e) || super::rooms::collider(world, *e))
                    && world
                        .get::<crate::native::area::InfluenceArea>(*e)
                        .is_none()
                    && world.get::<super::terrain::Effect>(*e).is_none()
                    && world
                        .get::<crate::native::arrow_sand::ArrowSand>(*e)
                        .is_none()
                    && world
                        .get::<crate::native::time_castle::AttachedCard>(*e)
                        .is_none()
                    && !world
                        .get::<super::assets::ImportedAsset>(*e)
                        .is_some_and(super::splats::is_splat)
            })
            .collect();
        if solid.is_empty() {
            continue;
        }
        if members.len() > 1 && world.get::<GroupPose>(entity).is_none() {
            super::groups::attach(world, &members);
        }
        let root = world.get::<ChildOf>(entity).unwrap().parent();
        let workspace = world
            .get::<crate::native::workspace::WorkspaceMember>(entity)
            .unwrap()
            .0;
        let pose = world.get::<GroupPose>(entity).copied();
        let position = pose.map_or_else(
            || super::position(world, entity).unwrap(),
            |p| DVec3::from_array(p.position),
        );
        let rotation = pose.map_or_else(
            || spatial(world, entity).rotation(),
            |p| DQuat::from_array(p.rotation),
        );
        let enabled = crate::native::workspace_config::enabled(world, root, workspace);
        let held = room_proxy
            || !enabled
            || members.iter().any(|e| {
                spatial(world, *e).world_pinned
                    || world.get::<super::terrain::Unsupported>(*e).is_some()
                    || crate::native::area_mutation::pending(world, *e)
                    || crate::native::physics::held(world, *e)
                    || world
                        .get_resource::<super::input::PointerState>()
                        .is_some_and(|p| p.drag.is_some_and(|(drag, _)| drag == *e))
            });
        let signature: Vec<_> = solid
            .iter()
            .filter_map(|e| {
                let item = world.get::<crate::native::canvas::CanvasItem>(*e)?;
                let mut placement = spatial(world, *e);
                placement.elevation = 0.0;
                placement.world_pinned = false;
                let scale = world
                    .get::<crate::native::area_effects::AreaScale>(*e)
                    .map_or(1.0, |s| s.0);
                Some((
                    *e,
                    Vec3::new(item.size.x, placement.depth(item.size) as f32, item.size.y) * scale,
                    placement,
                ))
            })
            .collect();
        let existing = members
            .first()
            .and_then(|member| existing_bodies.get(member))
            .copied()
            .filter(|entity| {
                world.get::<Body>(*entity).is_some_and(|body| {
                    body.root == root && body.workspace == workspace && body.members == members
                })
            });
        let rebuild = existing.is_none_or(|e| world.get::<Body>(e).unwrap().signature != signature);
        let mut shape = None;
        if rebuild {
            let mut parts = Vec::new();
            let mut complete = true;
            for (e, size, placement) in &signature {
                let relative =
                    rotation.inverse() * (super::position(world, *e).unwrap() - position);
                let local_rotation = rotation.inverse() * placement.rotation();
                let collider = if let Some(asset) = world.get::<super::assets::ImportedAsset>(*e) {
                    let scale = super::assets::effective_scale(world, *e, asset);
                    imported_shape(world, *e, scale).ok().flatten()
                } else {
                    Some(Collider::cuboid(
                        f64::from(size.x),
                        f64::from(size.y),
                        f64::from(size.z),
                    ))
                };
                if let Some(collider) = collider {
                    let offset = if world.get::<super::assets::ImportedAsset>(*e).is_some() {
                        DVec3::ZERO
                    } else {
                        DVec3::new(0.0, -f64::from(size.y) * 0.5, 0.0)
                    };
                    parts.push((relative + local_rotation * offset, local_rotation, collider));
                } else {
                    complete = false;
                }
            }
            if !complete || parts.is_empty() {
                continue;
            }
            shape = Some(parts);
        }
        let kind = if held {
            RigidBody::Static
        } else {
            RigidBody::Dynamic
        };
        let body = if let Some(existing) = existing {
            existing
        } else {
            changed = true;
            world
                .spawn((
                    Position(position),
                    Rotation(rotation),
                    Transform::default(),
                    kind,
                    Mass(solid.len() as f32),
                    AngularInertia::new(Vec3::ONE),
                    ColliderDensity(0.0),
                    LockedAxes::ROTATION_LOCKED,
                    LinearDamping(4.0),
                    Friction::new(0.05),
                    MaxLinearSpeed(1000.0),
                    ConstantForce::default(),
                    ActiveCollisionHooks::FILTER_PAIRS,
                    Body {
                        root,
                        workspace,
                        members: members.clone(),
                        position,
                        rotation,
                        signature: signature.clone(),
                        held,
                    },
                ))
                .id()
        };
        if world.get::<RigidBody>(body) != Some(&kind) {
            world.entity_mut(body).insert(kind);
        }
        if let Some(parts) = shape {
            if held {
                changed_static_scopes.insert((root, workspace));
            }
            let previous: Vec<_> = world
                .query_filtered::<(Entity, &ChildOf), With<Part>>()
                .iter(world)
                .filter(|(_, parent)| parent.parent() == body)
                .map(|(entity, _)| entity)
                .collect();
            for entity in previous {
                world.entity_mut(entity).remove::<Collider>();
                world.despawn(entity);
            }
            for (local, local_rotation, shape) in parts {
                world
                    .spawn((
                        Part,
                        ChildOf(body),
                        Transform::from_translation(local.as_vec3())
                            .with_rotation(local_rotation.as_quat()),
                        Position(position + rotation * local),
                        Rotation(rotation * local_rotation),
                        ColliderDensity(0.0),
                        ActiveCollisionHooks::FILTER_PAIRS,
                    ))
                    .insert((shape, ColliderOf { body }));
            }
            world.entity_mut(body).remove::<Sleeping>();
            changed = true;
        }
        let previous = world.get::<Body>(body).unwrap();
        if previous.position != position || previous.rotation != rotation || previous.held != held {
            if previous.held || held {
                changed_static_scopes.insert((root, workspace));
            }
            world
                .entity_mut(body)
                .insert((Position(position), Rotation(rotation), LinearVelocity::ZERO))
                .remove::<Sleeping>();
            changed = true;
        }
        let force = if held {
            DVec3::ZERO
        } else {
            members
                .iter()
                .map(|e| {
                    world
                        .resource::<super::influence::Forces>()
                        .totals
                        .get(e)
                        .copied()
                        .unwrap_or_default()
                        + world
                            .get_resource::<crate::native::protein_motion::Motion>()
                            .and_then(|motion| motion.forces.get(e))
                            .copied()
                            .unwrap_or_default()
                })
                .sum::<DVec3>()
                + if world.contains_resource::<super::terrain::Terrain>() {
                    DVec3::new(0.0, -981.0 * solid.len() as f64, 0.0)
                } else {
                    DVec3::ZERO
                }
        };
        if world.get::<ConstantForce>(body).unwrap().0 != force {
            let threshold = crate::native::physics::sleep_threshold(
                force,
                f64::from(world.get::<Mass>(body).unwrap().0),
            );
            world
                .entity_mut(body)
                .insert((ConstantForce(force), SleepTimer(0.0), threshold))
                .remove::<Sleeping>();
            changed = true;
        }
        let mut link = world.get_mut::<Body>(body).unwrap();
        link.position = position;
        link.rotation = rotation;
        link.signature = signature;
        link.held = held;
        retained.insert(body);
    }
    let stale: Vec<_> = world
        .query_filtered::<Entity, With<Body>>()
        .iter(world)
        .filter(|e| !retained.contains(e))
        .collect();
    for entity in stale {
        let body = world.get::<Body>(entity).unwrap();
        if body.held {
            changed_static_scopes.insert((body.root, body.workspace));
        }
        let colliders: Vec<_> = world
            .query::<(Entity, &ColliderOf)>()
            .iter(world)
            .filter(|(_, owner)| owner.body == entity)
            .map(|(collider, _)| collider)
            .collect();
        for collider in colliders {
            world.entity_mut(collider).remove::<Collider>();
        }
        world.despawn(entity);
        changed = true;
    }
    let wake: Vec<_> = world
        .query::<(Entity, &Body)>()
        .iter(world)
        .filter(|(_, body)| {
            !body.held && changed_static_scopes.contains(&(body.root, body.workspace))
        })
        .map(|(entity, _)| entity)
        .collect();
    for entity in wake {
        world
            .entity_mut(entity)
            .insert(SleepTimer(0.0))
            .remove::<Sleeping>();
    }
    changed
}

pub fn awake(world: &mut World) -> bool {
    world
        .query::<(&Body, Has<Sleeping>)>()
        .iter(world)
        .any(|(body, sleeping)| !body.held && !sleeping)
}

pub fn apply(world: &mut World) {
    let bodies: Vec<_> = world
        .query::<(Entity, &Body, &Position)>()
        .iter(world)
        .filter(|(_, body, position)| {
            !body.held && body.position != position.0 && position.0.is_finite()
        })
        .map(|(e, body, position)| (e, body.members.clone(), position.0, body.rotation))
        .collect();
    for (body, members, position, rotation) in bodies {
        if members.len() > 1 {
            super::groups::apply(
                world,
                &members,
                GroupPose {
                    position: position.to_array(),
                    rotation: rotation.to_array(),
                },
            );
        } else {
            super::set_position(world, members[0], position);
        }
        world.get_mut::<Body>(body).unwrap().position = position;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{
        canvas::CanvasItem,
        workspace::{WorkspaceMember, Workspaces},
    };
    use bevy::math::DVec2;

    #[test]
    fn room_body_rebase_preserves_velocity_and_collider_signature() {
        use nucleus::canvas::{
            Pose,
            room::{self as model, compose},
        };
        let mut world = World::new();
        let point = DVec3::new(100.0, 20.0, 40.0);
        let member = world
            .spawn((
                CanvasItem {
                    position: DVec2::new(point.x, point.z),
                    size: Vec2::splat(100.0),
                },
                super::super::Spatial {
                    elevation: point.y,
                    ..default()
                },
            ))
            .id();
        let body = world
            .spawn((
                Body {
                    root: member,
                    workspace: 1,
                    members: vec![member],
                    position: point,
                    rotation: DQuat::IDENTITY,
                    signature: vec![(member, Vec3::splat(100.0), super::super::Spatial::default())],
                    held: false,
                },
                Position(point),
                Rotation(DQuat::IDENTITY),
                LinearVelocity(DVec3::X * 10.0),
                AngularVelocity(DVec3::Y),
            ))
            .id();
        let before = model::identity_pose();
        let after = Pose {
            position: [1e9, 50.0, -1e9],
            rotation: DQuat::from_euler(EulerRot::XYZ, 0.2, 0.7, 0.3).to_array(),
        };
        let target = compose(
            &after,
            &Pose {
                position: point.to_array(),
                ..model::identity_pose()
            },
        );
        super::super::set_position(&mut world, member, DVec3::from_array(target.position));
        world.entity_mut(member).insert(super::super::Spatial {
            rotation: target.rotation,
            elevation: target.position[1],
            ..default()
        });
        room_transform(&mut world, member, &before, &after);
        let rotation = DQuat::from_array(after.rotation);
        assert_eq!(
            world.get::<Body>(body).unwrap().position,
            DVec3::from_array(target.position)
        );
        assert_eq!(
            world.get::<Body>(body).unwrap().signature[0].2.rotation,
            target.rotation
        );
        assert!(
            (world.get::<LinearVelocity>(body).unwrap().0 - rotation * DVec3::X * 10.0).length()
                < 1e-8
        );
        assert!(
            (world.get::<AngularVelocity>(body).unwrap().0 - rotation * DVec3::Y).length() < 1e-8
        );
    }

    #[test]
    fn shared_geometry_is_released_after_the_last_instance() {
        let mut world = World::new();
        let shape = Collider::cuboid(10.0, 20.0, 30.0);
        let key = (
            "a".repeat(32),
            "source.gltf".into(),
            1.0_f32.to_bits(),
            false,
        );
        let weak = Arc::downgrade(&shape.shape().0);
        world.insert_resource(GeometryCache(HashMap::from([(key.clone(), weak.clone())])));
        let first = world
            .spawn(MeshCollider {
                shape: shape.clone(),
                key: key.clone(),
            })
            .id();
        let second = world.spawn(MeshCollider { shape, key }).id();
        world.despawn(first);
        synchronize(&mut world);
        assert!(weak.upgrade().is_some());
        assert_eq!(world.resource::<GeometryCache>().0.len(), 1);
        world.despawn(second);
        synchronize(&mut world);
        assert!(weak.upgrade().is_none());
        assert!(world.resource::<GeometryCache>().0.is_empty());
    }

    #[test]
    fn indexed_picking_preserves_openings_and_both_sides() {
        let shape = Collider::trimesh(
            vec![
                DVec3::new(-10.0, -10.0, 0.0),
                DVec3::new(-2.0, -10.0, 0.0),
                DVec3::new(-2.0, 10.0, 0.0),
                DVec3::new(-10.0, 10.0, 0.0),
                DVec3::new(2.0, -10.0, 0.0),
                DVec3::new(10.0, -10.0, 0.0),
                DVec3::new(10.0, 10.0, 0.0),
                DVec3::new(2.0, 10.0, 0.0),
            ],
            vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
        );
        let mesh = MeshCollider {
            shape,
            key: (
                "asset".into(),
                "source.gltf".into(),
                1.0_f32.to_bits(),
                false,
            ),
        };
        for side in [-1.0, 1.0] {
            assert_eq!(
                mesh.ray_distance(DVec3::Z * 30.0 * side, -DVec3::Z * side),
                None
            );
            assert_eq!(
                mesh.ray_distance(DVec3::new(5.0, 0.0, 30.0 * side), -DVec3::Z * side),
                Some(30.0)
            );
        }
    }

    #[test]
    fn child_colliders_stop_unpinned_sands_at_pinned_solids() {
        for pinned in [false, true] {
            let mut app = App::new();
            crate::native::laboratory::isolate(app.world_mut());
            app.add_plugins((
                MinimalPlugins,
                crate::native::physics::WorkspacePhysicsPlugin,
            ))
            .init_resource::<Runtime>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f64(1.0 / 120.0),
            ));
            app.finish();
            let root = app.world_mut().spawn(Workspaces::default()).id();
            crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
            let moving = app
                .world_mut()
                .spawn((
                    CanvasItem {
                        position: DVec2::ZERO,
                        size: Vec2::splat(20.0),
                    },
                    WorkspaceMember(1),
                    ChildOf(root),
                    super::super::Spatial {
                        elevation: 20.0,
                        world_pinned: pinned,
                        ..default()
                    },
                    crate::native::area::RecordProperties(serde_json::json!({"quantity": -1})),
                ))
                .id();
            app.world_mut().spawn((
                CanvasItem {
                    position: DVec2::new(0.0, -100.0),
                    size: Vec2::new(200.0, 20.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
                super::super::Spatial {
                    elevation: 20.0,
                    depth: Some(20.0),
                    world_pinned: true,
                    ..default()
                },
            ));
            let mut area = crate::native::area::InfluenceArea::new(
                crate::native::area::AreaShape::Square,
                DVec2::new(0.0, -500.0),
                DVec2::splat(1000.0),
            );
            area.strength = 2000.0;
            area.reach.mode = crate::native::area::ReachMode::Unlimited;
            area.rules.push(crate::native::area::PropertyRule {
                property: crate::native::area::Property::Quantity,
                value: "-1".into(),
            });
            let owner = crate::native::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
            app.world_mut()
                .get_mut::<super::super::Spatial>(owner)
                .unwrap()
                .elevation = 20.0;
            for _ in 0..10 {
                app.update();
            }
            app.world_mut()
                .get_mut::<super::super::Spatial>(moving)
                .unwrap()
                .world_pinned = false;
            for _ in 0..240 {
                app.update();
            }
            for width in 21..26 {
                app.world_mut()
                    .get_mut::<CanvasItem>(moving)
                    .unwrap()
                    .size
                    .x = width as f32;
                app.update();
            }
            let trees = app
                .world()
                .resource::<avian3d::collider_tree::ColliderTrees>();
            for tree in trees.iter_trees() {
                for (_, proxy) in tree.proxies.iter() {
                    assert!(
                        app.world().get::<Collider>(proxy.collider).is_some(),
                        "Removed collider remains in collision tree"
                    );
                    assert_eq!(
                        proxy.body,
                        app.world()
                            .get::<ColliderOf>(proxy.collider)
                            .map(|owner| owner.body)
                    );
                }
            }
            let position = super::super::position(app.world(), moving).unwrap();
            assert!(
                position.z < -20.0 && position.z > -82.0,
                "Started pinned {pinned}: {position:?}"
            );
        }
    }

    #[test]
    fn pinned_group_stays_fixed_and_unpinning_enables_spatial_forces() {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins((
            MinimalPlugins,
            crate::native::physics::WorkspacePhysicsPlugin,
        ))
        .init_resource::<Runtime>()
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(1.0 / 60.0),
        ));
        app.finish();
        let root = app.world_mut().spawn(Workspaces::default()).id();
        assert!(crate::native::workspace_config::set_physics(
            app.world_mut(),
            root,
            1,
            true
        ));
        let sand = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(20.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
                super::super::Spatial {
                    world_pinned: true,
                    ..default()
                },
                crate::native::area::RecordProperties(serde_json::json!({"quantity": -1})),
                crate::native::canvas_selection::SandGroup([4; 16]),
            ))
            .id();
        let companion = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::new(-60.0, 0.0),
                    size: Vec2::splat(20.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
                crate::native::canvas_selection::SandGroup([4; 16]),
            ))
            .id();
        let mut area = crate::native::area::InfluenceArea::new(
            crate::native::area::AreaShape::Square,
            DVec2::new(100.0, 0.0),
            DVec2::splat(1000.0),
        );
        area.strength = 400.0;
        area.reach.mode = crate::native::area::ReachMode::Unlimited;
        area.rules.push(crate::native::area::PropertyRule {
            property: crate::native::area::Property::Quantity,
            value: "-1".into(),
        });
        let influence = crate::native::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
        app.world_mut()
            .entity_mut(influence)
            .insert(super::super::Spatial {
                elevation: 100.0,
                ..default()
            });
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(
            super::super::position(app.world(), sand).unwrap(),
            DVec3::ZERO
        );
        assert_eq!(
            app.world_mut().query::<&Body>().iter(app.world()).count(),
            1
        );
        app.world_mut()
            .get_mut::<super::super::Spatial>(sand)
            .unwrap()
            .world_pinned = false;
        for _ in 0..100 {
            app.update();
        }
        let point = super::super::position(app.world(), sand).unwrap();
        assert!(point.x > 0.0 && point.y > 0.0, "{point:?}");
        crate::native::actions::Action::apply(
            &crate::native::influence_report::Control::Workspace,
            app.world_mut(),
            root,
        );
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(super::super::position(app.world(), sand).unwrap(), point);
        crate::native::actions::Action::apply(
            &crate::native::influence_report::Control::Workspace,
            app.world_mut(),
            root,
        );
        for _ in 0..20 {
            app.update();
        }
        let resumed = super::super::position(app.world(), sand).unwrap();
        assert!(resumed.x > point.x && resumed.y > point.y, "{resumed:?}");
        let point = resumed;
        let offset = super::super::position(app.world(), companion).unwrap() - point;
        assert!((offset - DVec3::new(-60.0, 0.0, 0.0)).length() < 1e-9);
    }
}
