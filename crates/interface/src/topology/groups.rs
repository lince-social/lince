use super::{Attachment, spatial};
use bevy::{
    math::{DQuat, DVec3},
    prelude::*,
};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupPose {
    pub position: [f64; 3],
    pub rotation: [f64; 4],
}

impl GroupPose {
    pub fn valid(&self) -> bool {
        Attachment {
            position: self.position,
            rotation: self.rotation,
        }
        .valid()
    }
}

pub fn attach(world: &mut World, members: &[Entity]) {
    let Some(origin) = members.first().and_then(|e| super::position(world, *e)) else {
        return;
    };
    let pose = GroupPose {
        position: origin.to_array(),
        rotation: DQuat::IDENTITY.to_array(),
    };
    for entity in members {
        if let Some(position) = super::position(world, *entity) {
            let rotation = spatial(world, *entity).rotation;
            world.entity_mut(*entity).insert((
                pose,
                Attachment {
                    position: (position - origin).to_array(),
                    rotation,
                },
            ));
        }
    }
}

pub fn members(world: &World, entity: Entity) -> Vec<Entity> {
    let Some(group) = world.get::<crate::canvas_selection::SandGroup>(entity) else {
        return vec![entity];
    };
    let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
        return vec![entity];
    };
    world
        .get::<Children>(root)
        .map_or_else(Vec::new, |children| {
            children
                .iter()
                .filter(|e| {
                    world.get::<crate::canvas_selection::SandGroup>(*e) == Some(group)
                        && world.get::<crate::workspace::WorkspaceMember>(*e)
                            == world.get::<crate::workspace::WorkspaceMember>(entity)
                })
                .collect()
        })
}

pub fn transform(world: &mut World, entity: Entity, translation: DVec3, rotation: DQuat) {
    let members = members(world, entity);
    if members.len() > 1 && world.get::<GroupPose>(entity).is_none() {
        attach(world, &members);
    }
    if let Some(mut pose) = world.get::<GroupPose>(entity).copied() {
        pose.position = (DVec3::from_array(pose.position) + translation).to_array();
        pose.rotation = (rotation * DQuat::from_array(pose.rotation))
            .normalize()
            .to_array();
        apply(world, &members, pose);
    } else {
        if let Some(position) = super::position(world, entity) {
            super::set_position(world, entity, position + translation);
        }
        let mut placement = spatial(world, entity);
        placement.rotation = (rotation * placement.rotation()).normalize().to_array();
        world.entity_mut(entity).insert(placement);
    }
    for entity in members {
        if world.get::<crate::area::InfluenceArea>(entity).is_some() {
            crate::area_mutation::disarm(
                world,
                entity,
                "Disarmed after a group edit. Preview again to arm.",
            );
        }
    }
}

pub fn apply(world: &mut World, members: &[Entity], pose: GroupPose) {
    let origin = DVec3::from_array(pose.position);
    let rotation = DQuat::from_array(pose.rotation);
    for entity in members {
        if let Some(local) = world.get::<Attachment>(*entity).copied() {
            super::set_position(
                world,
                *entity,
                origin + rotation * DVec3::from_array(local.position),
            );
            let mut spatial = spatial(world, *entity);
            spatial.rotation = (rotation * DQuat::from_array(local.rotation))
                .normalize()
                .to_array();
            world.entity_mut(*entity).insert((spatial, pose));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{canvas::CanvasItem, canvas_selection::SandGroup, workspace::WorkspaceMember};
    use bevy::math::DVec2;

    #[test]
    fn mixed_group_preserves_offsets_through_repeated_rotations_and_moves() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let mut members = Vec::new();
        for point in [DVec2::new(1e9, -1e9), DVec2::new(1e9 + 40.0, -1e9 + 10.0)] {
            members.push(
                world
                    .spawn((
                        CanvasItem {
                            position: point,
                            size: Vec2::splat(20.0),
                        },
                        ChildOf(root),
                        WorkspaceMember(1),
                        SandGroup([1; 16]),
                    ))
                    .id(),
            );
        }
        attach(&mut world, &members);
        let offset = *world.get::<Attachment>(members[1]).unwrap();
        for _ in 0..100 {
            transform(
                &mut world,
                members[0],
                DVec3::new(1.0, 2.0, 3.0),
                DQuat::from_rotation_y(0.1),
            );
        }
        assert_eq!(*world.get::<Attachment>(members[1]).unwrap(), offset);
        let distance = super::super::position(&world, members[0])
            .unwrap()
            .distance(super::super::position(&world, members[1]).unwrap());
        assert!((distance - 1700.0_f64.sqrt()).abs() < 1e-5);
    }
}
