use super::{Member, Room, WorldPlacement};
use crate::native::{
    canvas::CanvasItem, canvas_host::Identity, topology, workspace::WorkspaceMember,
};
use bevy::{math::DVec3, prelude::*};
use nucleus::canvas::{Pose, room as model};
use std::collections::{HashMap, HashSet};

type Scope = (Entity, u64);
type Key = (Scope, String);

#[derive(Resource, Default)]
struct Runtime(
    HashMap<Entity, Previous>,
    HashMap<Entity, Producer>,
    HashMap<Entity, Pose>,
);

struct Producer {
    pose: Pose,
    bound: bool,
    member: Option<model::Member>,
}

pub(crate) fn prepare_placement(world: &mut World, entity: Entity) {
    let Some(pose) = world_pose(world, entity) else {
        return;
    };
    let bound = super::frame(world, entity) == Some(entity);
    let member = world.get::<Member>(entity).map(|member| member.0.clone());
    world.init_resource::<Runtime>();
    world
        .resource_mut::<Runtime>()
        .1
        .entry(entity)
        .or_insert(Producer {
            pose,
            bound,
            member,
        });
}

struct Previous {
    parent: Pose,
    world: Pose,
    member: model::Member,
}

struct Node {
    scope: Scope,
    member: model::Member,
    world: Pose,
    edited: bool,
}

pub(crate) fn world_pose(world: &World, entity: Entity) -> Option<Pose> {
    let spatial = topology::spatial(world, entity);
    let pose = Pose {
        position: topology::position(world, entity)?.to_array(),
        rotation: spatial.rotation,
    };
    model::valid_pose(&pose).then_some(pose)
}

pub(crate) fn compose(parent: &Pose, local: &Pose) -> Pose {
    model::compose(parent, local)
}

pub(crate) fn relative(parent: &Pose, point: &Pose) -> Pose {
    model::relative(parent, point)
}

pub(crate) fn accept_snapshot(world: &mut World, root: Entity) {
    if let Some(mut runtime) = world.remove_resource::<Runtime>() {
        runtime.0.retain(|entity, _| {
            world
                .get::<ChildOf>(*entity)
                .is_some_and(|parent| parent.parent() != root)
        });
        world.insert_resource(runtime);
    }
}

pub(crate) fn set_world_pose(world: &mut World, entity: Entity, pose: &Pose) {
    if !model::valid_pose(pose) || world.get::<CanvasItem>(entity).is_none() {
        return;
    }
    topology::set_position(world, entity, DVec3::from_array(pose.position));
    let mut spatial = topology::spatial(world, entity);
    spatial.rotation = pose.rotation;
    world.entity_mut(entity).insert(spatial);
}

fn resolve(
    entity: Entity,
    nodes: &HashMap<Entity, Node>,
    roots: &HashMap<Key, Pose>,
    anchors: &HashMap<Key, Entity>,
    path: &mut HashSet<Entity>,
    resolved: &mut HashMap<Entity, (Pose, Pose)>,
) -> Option<(Pose, Pose)> {
    if let Some(result) = resolved.get(&entity) {
        return Some(result.clone());
    }
    if path.len() >= model::MAX_DEPTH || !path.insert(entity) {
        return None;
    }
    let node = nodes.get(&entity)?;
    let root = roots.get(&(node.scope, node.member.room.clone()))?;
    let parent = if let Some(parent) = &node.member.parent {
        let parent = anchors.get(&(node.scope, parent.clone()))?;
        let anchor = nodes.get(parent)?;
        if anchor.member.room != node.member.room || anchor.member.role == model::Role::Content {
            return None;
        }
        resolve(*parent, nodes, roots, anchors, path, resolved)?.1
    } else {
        root.clone()
    };
    path.remove(&entity);
    let point = compose(&parent, &node.member.local);
    if !model::valid_pose(&point) {
        return None;
    }
    let result = (parent, point);
    resolved.insert(entity, result.clone());
    Some(result)
}

pub fn synchronize(world: &mut World) {
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    let overrides: Vec<_> = world
        .query::<(Entity, &WorldPlacement)>()
        .iter(world)
        .map(|(entity, pose)| (entity, pose.0.clone()))
        .collect();
    let mut world_poses = HashMap::new();
    for (entity, saved) in overrides {
        if !model::valid_pose(&saved) || world.get::<Member>(entity).is_some() {
            world.entity_mut(entity).remove::<WorldPlacement>();
            continue;
        }
        let Some(current) = world_pose(world, entity) else {
            continue;
        };
        let pose = if runtime.2.get(&entity) == Some(&saved) && !super::hosted(world, entity) {
            current
        } else {
            set_world_pose(world, entity, &saved);
            saved.clone()
        };
        if pose != saved {
            world
                .entity_mut(entity)
                .insert(WorldPlacement(pose.clone()));
            crate::native::record_presentation::save(world, entity);
            crate::native::workspace::request_save(world);
        }
        world_poses.insert(entity, pose);
    }
    runtime.2 = world_poses;
    let mut roots: Vec<_> = world
        .query::<(Entity, &Room, &Identity, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(_, room, _, _, _)| room.0.valid())
        .filter_map(|(entity, _, id, parent, workspace)| {
            Some((
                (parent.parent(), workspace.0),
                id.0.clone(),
                world_pose(world, entity)?,
            ))
        })
        .collect();
    roots.sort_by(|a, b| a.1.cmp(&b.1));
    let mut root_counts = HashMap::<Scope, usize>::new();
    let roots: HashMap<_, _> = roots
        .into_iter()
        .filter_map(|(scope, id, pose)| {
            let count = root_counts.entry(scope).or_default();
            *count += 1;
            (*count <= model::MAX_ROOMS).then_some(((scope, id), pose))
        })
        .collect();
    let mut entries: Vec<_> = world
        .query::<(Entity, &Member, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .map(|(entity, member, parent, workspace)| {
            (entity, member.0.clone(), (parent.parent(), workspace.0))
        })
        .collect();
    entries.sort_by_key(|(entity, _, _)| *entity);
    let mut nodes = HashMap::new();
    let mut anchors = HashMap::new();
    let mut counts = HashMap::<Scope, usize>::new();
    let mut detached = Vec::new();
    for (entity, mut member, scope) in entries {
        let count = counts.entry(scope).or_default();
        *count += 1;
        if *count > model::MAX_MEMBERS
            || !member.valid()
            || world.get::<Room>(entity).is_some()
            || world
                .get::<crate::native::sand_placement::Pinned>(entity)
                .is_some()
            || world.get::<topology::terrain::Bound>(entity).is_some()
            || world
                .get::<topology::terrain::FollowSlope>(entity)
                .is_some()
        {
            detached.push(entity);
            continue;
        }
        let Some(current) = world_pose(world, entity) else {
            continue;
        };
        let mut edited = false;
        if let Some(previous) = runtime.0.get(&entity)
            && previous.member == member
            && previous.world != current
            && !super::hosted(world, entity)
        {
            member.local = relative(&previous.parent, &current);
            edited = true;
        }
        if let Some(id) = world.get::<Identity>(entity) {
            anchors.insert((scope, id.0.clone()), entity);
        }
        nodes.insert(
            entity,
            Node {
                scope,
                member,
                world: current,
                edited,
            },
        );
    }
    let mut resolved = HashMap::new();
    let mut entities: Vec<_> = nodes.keys().copied().collect();
    entities.sort();
    for entity in &entities {
        if resolve(
            *entity,
            &nodes,
            &roots,
            &anchors,
            &mut HashSet::new(),
            &mut resolved,
        )
        .is_none()
        {
            detached.push(*entity);
        }
    }
    let mut next = HashMap::new();
    let mut changed = false;
    for entity in entities {
        let Some((parent, point)) = resolved.get(&entity) else {
            continue;
        };
        let node = &nodes[&entity];
        if node.world != *point {
            if let Some(pose) = world.get::<topology::groups::GroupPose>(entity).copied() {
                let group = Pose {
                    position: pose.position,
                    rotation: pose.rotation,
                };
                let transformed = compose(point, &relative(&node.world, &group));
                world
                    .entity_mut(entity)
                    .insert(topology::groups::GroupPose {
                        position: transformed.position,
                        rotation: transformed.rotation,
                    });
            }
            set_world_pose(world, entity, point);
        }
        if node.edited {
            world.entity_mut(entity).insert(Member(node.member.clone()));
            crate::native::record_presentation::save(world, entity);
            changed = true;
        }
        next.insert(
            entity,
            Previous {
                parent: parent.clone(),
                world: world_pose(world, entity).unwrap_or_else(|| point.clone()),
                member: node.member.clone(),
            },
        );
    }
    for entity in detached {
        if world
            .get::<crate::native::protein_area::RecordBinding>(entity)
            .is_some()
            && let Some(pose) = world_pose(world, entity)
        {
            world.entity_mut(entity).insert(WorldPlacement(pose));
        }
        world.entity_mut(entity).remove::<Member>();
        crate::native::record_presentation::save(world, entity);
        changed = true;
    }
    runtime.0 = next;
    let producers: Vec<_> = world
        .query::<(Entity, &CanvasItem)>()
        .iter(world)
        .filter(|(entity, _)| {
            super::frame(world, *entity) == Some(*entity)
                || world
                    .get::<crate::native::area::InfluenceArea>(*entity)
                    .is_some()
                || world
                    .get::<crate::native::kanban::Kanban>(*entity)
                    .is_some()
                || runtime.1.get(entity).is_some_and(|previous| previous.bound)
        })
        .filter_map(|(entity, _)| {
            Some((
                entity,
                world_pose(world, entity)?,
                super::frame(world, entity) == Some(entity),
                world.get::<Member>(entity).map(|member| member.0.clone()),
            ))
        })
        .collect();
    let mut producer_poses = HashMap::new();
    for (entity, pose, bound, member) in producers {
        if let Some(before) = runtime.1.get(&entity)
            && (bound || before.bound)
            && (before.pose != pose || before.bound != bound || before.member != member)
        {
            if before.pose != pose {
                crate::native::protein_area::room_transform(world, entity, &before.pose, &pose);
            }
            crate::native::kanban::room_transform(world, entity, &before.pose, &pose);
        }
        producer_poses.insert(
            entity,
            Producer {
                pose,
                bound,
                member,
            },
        );
    }
    for (entity, producer) in &mut producer_poses {
        if let Some(current) = world_pose(world, *entity) {
            producer.pose = current;
            producer.bound = super::frame(world, *entity) == Some(*entity);
            producer.member = world.get::<Member>(*entity).map(|member| member.0.clone());
        }
    }
    runtime.1 = producer_poses;
    world.insert_resource(runtime);
    if changed {
        crate::native::workspace::request_save(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::{DQuat, DVec2};

    fn fixture() -> (World, Entity, Entity) {
        let mut world = World::new();
        let canvas = world.spawn_empty().id();
        let root = world
            .spawn((
                CanvasItem {
                    position: DVec2::new(1e9, -1e9),
                    size: Vec2::splat(500.0),
                },
                topology::Spatial::default(),
                ChildOf(canvas),
                WorkspaceMember(1),
                Identity(nucleus::new_uid("placement")),
                Room(model::Room::default()),
            ))
            .id();
        (world, canvas, root)
    }

    fn member(
        world: &mut World,
        canvas: Entity,
        root: Entity,
        parent: Option<Entity>,
        local: [f64; 3],
        role: model::Role,
    ) -> Entity {
        let room = world.get::<Identity>(root).unwrap().0.clone();
        let parent = parent.map(|parent| world.get::<Identity>(parent).unwrap().0.clone());
        world
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(50.0),
                },
                topology::Spatial::default(),
                ChildOf(canvas),
                WorkspaceMember(1),
                Identity(nucleus::new_uid("placement")),
                Member(model::Member {
                    room,
                    parent,
                    local: Pose {
                        position: local,
                        ..model::identity_pose()
                    },
                    role,
                }),
            ))
            .id()
    }

    fn near(actual: &Pose, expected: &Pose) {
        assert!(
            (DVec3::from_array(actual.position) - DVec3::from_array(expected.position)).length()
                < 1e-6,
            "{actual:?} != {expected:?}"
        );
        assert!(
            DQuat::from_array(actual.rotation)
                .dot(DQuat::from_array(expected.rotation))
                .abs()
                > 1.0 - 1e-10
        );
    }

    #[test]
    fn room_transforms_preserve_nested_locals_at_large_coordinates_without_rotation_drift() {
        let (mut world, canvas, root) = fixture();
        let desk = member(
            &mut world,
            canvas,
            root,
            None,
            [100.0, 20.0, 50.0],
            model::Role::Desk,
        );
        let computer = member(
            &mut world,
            canvas,
            root,
            Some(desk),
            [10.0, 30.0, 5.0],
            model::Role::Computer,
        );
        let task = member(
            &mut world,
            canvas,
            root,
            Some(computer),
            [-5.0, 40.0, 3.0],
            model::Role::Content,
        );
        synchronize(&mut world);
        let locals: Vec<_> = [desk, computer, task]
            .into_iter()
            .map(|entity| world.get::<Member>(entity).unwrap().0.clone())
            .collect();
        for index in 0..100 {
            let pose = Pose {
                position: [1e9 + index as f64, index as f64, -1e9 + 2.0 * index as f64],
                rotation: DQuat::from_rotation_y(index as f64 * 0.37).to_array(),
            };
            set_world_pose(&mut world, root, &pose);
            synchronize(&mut world);
            synchronize(&mut world);
            let expected = compose(
                &compose(&compose(&pose, &locals[0].local), &locals[1].local),
                &locals[2].local,
            );
            near(&world_pose(&world, task).unwrap(), &expected);
            for (entity, saved) in [desk, computer, task].into_iter().zip(&locals) {
                assert_eq!(&world.get::<Member>(entity).unwrap().0, saved);
            }
        }
    }

    #[test]
    fn child_drag_updates_local_pose_and_combines_with_same_frame_parent_motion() {
        let (mut world, canvas, root) = fixture();
        let child = member(
            &mut world,
            canvas,
            root,
            None,
            [10.0, 20.0, 30.0],
            model::Role::Content,
        );
        synchronize(&mut world);
        let old_root = world_pose(&world, root).unwrap();
        let mut dragged = world_pose(&world, child).unwrap();
        dragged.position[0] += 25.0;
        dragged.position[1] += 5.0;
        set_world_pose(&mut world, child, &dragged);
        let new_root = Pose {
            position: [1e9 + 100.0, 10.0, -1e9 + 150.0],
            rotation: DQuat::from_rotation_x(0.3).to_array(),
        };
        set_world_pose(&mut world, root, &new_root);
        synchronize(&mut world);
        let local = relative(&old_root, &dragged);
        assert_eq!(world.get::<Member>(child).unwrap().0.local, local);
        near(
            &world_pose(&world, child).unwrap(),
            &compose(&new_root, &local),
        );
        synchronize(&mut world);
        assert_eq!(world.get::<Member>(child).unwrap().0.local, local);
    }

    #[test]
    fn deleted_anchor_detaches_descendants_at_last_world_pose_and_cleans_runtime() {
        let (mut world, canvas, root) = fixture();
        let desk = member(
            &mut world,
            canvas,
            root,
            None,
            [20.0, 30.0, 40.0],
            model::Role::Desk,
        );
        let child = member(
            &mut world,
            canvas,
            root,
            Some(desk),
            [5.0, 6.0, 7.0],
            model::Role::Content,
        );
        synchronize(&mut world);
        let before = world_pose(&world, child).unwrap();
        world.despawn(desk);
        synchronize(&mut world);
        assert!(world.get::<Member>(child).is_none());
        assert_eq!(world_pose(&world, child).unwrap(), before);
        assert!(!world.resource::<Runtime>().0.contains_key(&child));
    }

    #[test]
    fn invalid_room_cycles_workspace_links_and_hosted_edits_preserve_safe_world_poses() {
        let (mut world, canvas, root) = fixture();
        let desk = member(
            &mut world,
            canvas,
            root,
            None,
            [20.0, 30.0, 40.0],
            model::Role::Desk,
        );
        let child = member(
            &mut world,
            canvas,
            root,
            Some(desk),
            [5.0, 6.0, 7.0],
            model::Role::Computer,
        );
        synchronize(&mut world);
        let expected = world_pose(&world, child).unwrap();
        let local = world.get::<Member>(child).unwrap().0.local.clone();
        world
            .entity_mut(child)
            .insert(crate::native::workspace_sync::HostedReplica);
        set_world_pose(&mut world, child, &model::identity_pose());
        synchronize(&mut world);
        assert_eq!(world.get::<Member>(child).unwrap().0.local, local);
        near(&world_pose(&world, child).unwrap(), &expected);
        world.get_mut::<WorkspaceMember>(child).unwrap().0 = 2;
        synchronize(&mut world);
        assert!(world.get::<Member>(child).is_none());
        world.get_mut::<Member>(desk).unwrap().0.parent =
            Some(world.get::<Identity>(desk).unwrap().0.clone());
        let before = world_pose(&world, desk).unwrap();
        synchronize(&mut world);
        assert!(world.get::<Member>(desk).is_none());
        assert_eq!(world_pose(&world, desk).unwrap(), before);
    }
}
