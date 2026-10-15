use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room(pub nucleus::canvas::room::Room);

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member(pub nucleus::canvas::room::Member);

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldPlacement(pub nucleus::canvas::Pose);

mod runtime;
pub(crate) use runtime::{accept_snapshot, prepare_placement, world_pose};

pub(crate) fn hosted(world: &World, owner: Entity) -> bool {
    world
        .get::<crate::native::workspace_sync::HostedReplica>(owner)
        .is_some()
        || world
            .get::<crate::native::protein_area::RecordBinding>(owner)
            .is_some_and(|binding| {
                world
                    .get::<crate::native::workspace_sync::HostedReplica>(binding.area)
                    .is_some()
            })
}

pub(crate) fn frame(world: &World, entity: Entity) -> Option<Entity> {
    if world.get::<WorldPlacement>(entity).is_some() {
        return None;
    }
    if world.get::<Room>(entity).is_some() || world.get::<Member>(entity).is_some() {
        return Some(entity);
    }
    crate::native::protein_area::room_frame(world, entity)
}

pub(crate) fn group_moved(world: &World, owner: Entity, entity: Entity) -> bool {
    let Some(group) = world.get::<crate::native::canvas_selection::SandGroup>(owner) else {
        return false;
    };
    if world.get::<crate::native::canvas_selection::SandGroup>(entity) != Some(group) {
        return false;
    }
    let Some(pose) = world.get::<super::groups::GroupPose>(owner) else {
        return false;
    };
    let Some(other) = world.get::<super::groups::GroupPose>(entity) else {
        return false;
    };
    if (bevy::math::DVec3::from_array(pose.position)
        - bevy::math::DVec3::from_array(other.position))
    .length()
        >= 1e-6
        || bevy::math::DQuat::from_array(pose.rotation)
            .dot(bevy::math::DQuat::from_array(other.rotation))
            .abs()
            <= 1.0 - 1e-10
    {
        return false;
    }
    let Some(local) = world.get::<super::Attachment>(owner) else {
        return false;
    };
    let expected = nucleus::canvas::room::compose(
        &nucleus::canvas::Pose {
            position: pose.position,
            rotation: pose.rotation,
        },
        &nucleus::canvas::Pose {
            position: local.position,
            rotation: local.rotation,
        },
    );
    let Some(actual) = runtime::world_pose(world, owner) else {
        return false;
    };
    (bevy::math::DVec3::from_array(expected.position)
        - bevy::math::DVec3::from_array(actual.position))
    .length()
        < 1e-6
        && bevy::math::DQuat::from_array(expected.rotation)
            .dot(bevy::math::DQuat::from_array(actual.rotation))
            .abs()
            > 1.0 - 1e-10
}

pub(crate) fn fixed(world: &World, entity: Entity) -> bool {
    owned(world, entity) || crate::native::protein_area::room_fixed(world, entity)
}

pub(crate) fn owned(world: &World, entity: Entity) -> bool {
    world.get::<WorldPlacement>(entity).is_some()
        || world.get::<Room>(entity).is_some()
        || world.get::<Member>(entity).is_some()
}

pub(crate) fn ambient_enabled(world: &World, entity: Entity) -> bool {
    let Some(member) = world.get::<Member>(entity) else {
        return false;
    };
    let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
        return false;
    };
    let Some(workspace) = world.get::<crate::native::workspace::WorkspaceMember>(entity) else {
        return false;
    };
    world.get::<Children>(root).is_some_and(|children| {
        children.iter().any(|candidate| {
            world
                .get::<crate::native::canvas_host::Identity>(candidate)
                .is_some_and(|id| id.0 == member.0.room)
                && world.get::<crate::native::workspace::WorkspaceMember>(candidate)
                    == Some(workspace)
                && world
                    .get::<Room>(candidate)
                    .is_some_and(|room| room.0.ambient_enabled)
        })
    })
}
pub use runtime::synchronize;
mod ui;
pub use ui::{boundary_style, create_proxy, mount};
pub(crate) use ui::{controls, store_controls, update_controls};

pub(crate) fn collider(world: &World, entity: Entity) -> bool {
    world.get::<Member>(entity).is_some_and(|member| {
        matches!(
            member.0.role,
            nucleus::canvas::room::Role::Floor | nucleus::canvas::room::Role::Wall
        )
    }) && world
        .get::<crate::native::protein_area::RecordBinding>(entity)
        .is_none()
        && world.get::<super::assets::ImportedAsset>(entity).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{canvas::CanvasItem, sand_placement::Placement};
    use bevy::math::DVec2;

    #[test]
    fn room_placement_snapshots_roundtrip_and_clear_membership_without_changing_world_pose() {
        let mut world = World::new();
        let root_id = nucleus::new_uid("placement");
        let settings = nucleus::canvas::room::Room {
            home: nucleus::canvas::Pose {
                position: [1e9, 20.0, -1e9],
                ..nucleus::canvas::room::identity_pose()
            },
            ..default()
        };
        let root = world
            .spawn((
                CanvasItem {
                    position: DVec2::new(1e9, -1e9),
                    size: Vec2::splat(500.0),
                },
                Room(settings.clone()),
                crate::native::canvas_host::Identity(root_id.clone()),
            ))
            .id();
        let member = nucleus::canvas::room::Member {
            room: root_id,
            parent: None,
            local: nucleus::canvas::Pose {
                position: [15.0, 25.0, 35.0],
                ..nucleus::canvas::room::identity_pose()
            },
            role: nucleus::canvas::room::Role::Desk,
        };
        let child = world
            .spawn((
                CanvasItem {
                    position: DVec2::new(1e9 + 15.0, -1e9 + 35.0),
                    size: Vec2::splat(100.0),
                },
                super::super::Spatial {
                    elevation: 45.0,
                    ..default()
                },
                Member(member.clone()),
            ))
            .id();
        let saved = [
            Placement::capture(&world, root),
            Placement::capture(&world, child),
        ];
        assert!(saved.iter().all(Placement::valid));
        let restored: [Placement; 2] =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        world.entity_mut(root).remove::<Room>();
        world.entity_mut(child).remove::<Member>();
        for (settings, entity) in restored.into_iter().zip([root, child]) {
            settings.restore(&mut world, entity);
        }
        assert_eq!(world.get::<Room>(root).unwrap().0, settings);
        assert_eq!(world.get::<Member>(child).unwrap().0, member);
        let position = super::super::position(&world, child).unwrap();
        let mut cleared = saved[1].clone();
        cleared.room_member = None;
        cleared.restore(&mut world, child);
        assert!(world.get::<Member>(child).is_none());
        assert_eq!(super::super::position(&world, child), Some(position));
        let mut invalid = saved[1].clone();
        invalid.pinned = Some(crate::native::sand_placement::Pinned {
            anchor: [0.5; 2],
            scale: 1.0,
        });
        assert!(!invalid.valid());
    }
}
