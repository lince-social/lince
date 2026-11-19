use super::*;
use crate::native::{
    canvas_selection::SandGroup,
    sand_placement::Pinned,
    scoped_events::EventBoundary,
    topology::{self, Attachment, Spatial, appearance, groups::GroupPose, rooms, terrain},
};

pub(super) fn capture(world: &World, entity: Entity) -> api::Metadata {
    let spatial = topology::spatial(world, entity);
    api::Metadata {
        room: world
            .get::<rooms::Room>(entity)
            .map(|value| value.0.clone()),
        room_member: world
            .get::<rooms::Member>(entity)
            .map(|value| value.0.clone()),
        material: world
            .get::<appearance::Material>(entity)
            .map(|value| value.0.clone()),
        light: world
            .get::<appearance::Light>(entity)
            .map(|value| value.0.clone()),
        binding: world
            .get::<terrain::Bound>(entity)
            .map(|value| value.0.clone()),
        follow_slope: world.get::<terrain::FollowSlope>(entity).is_some(),
        order: world.get::<ZIndex>(entity).map_or(0, |value| value.0),
        pinned: world.get::<Pinned>(entity).map(|value| api::Pinned {
            anchor: value.anchor,
            scale: value.scale,
        }),
        event_boundary: world
            .get::<EventBoundary>(entity)
            .map(|value| value.0.clone())
            .unwrap_or_default(),
        spatial: api::Spatial {
            elevation: spatial.elevation,
            rotation: spatial.rotation,
            depth: spatial.depth,
            world_pinned: spatial.world_pinned,
        },
        attachment: world.get::<Attachment>(entity).map(|value| api::Pose {
            position: value.position,
            rotation: value.rotation,
        }),
        group_pose: world.get::<GroupPose>(entity).map(|value| api::Pose {
            position: value.position,
            rotation: value.rotation,
        }),
    }
}

pub(super) fn apply(world: &mut World, entity: Entity, metadata: &api::Metadata) {
    replace(
        world,
        entity,
        metadata.room.as_ref(),
        |value: &rooms::Room| &value.0,
        |value| rooms::Room(value.clone()),
    );
    replace(
        world,
        entity,
        metadata.room_member.as_ref(),
        |value: &rooms::Member| &value.0,
        |value| rooms::Member(value.clone()),
    );
    replace(
        world,
        entity,
        metadata.material.as_ref(),
        |value: &appearance::Material| &value.0,
        |value| appearance::Material(value.clone()),
    );
    replace(
        world,
        entity,
        metadata.light.as_ref(),
        |value: &appearance::Light| &value.0,
        |value| appearance::Light(value.clone()),
    );
    replace(
        world,
        entity,
        metadata.binding.as_ref(),
        |value: &terrain::Bound| &value.0,
        |value| terrain::Bound(value.clone()),
    );
    if world.get::<terrain::FollowSlope>(entity).is_some() != metadata.follow_slope {
        changed();
        if metadata.follow_slope {
            world.entity_mut(entity).insert(terrain::FollowSlope);
        } else {
            world.entity_mut(entity).remove::<terrain::FollowSlope>();
        }
    }
    set(world, entity, ZIndex(metadata.order));
    if world.get::<EventBoundary>(entity).map(|value| &value.0) != Some(&metadata.event_boundary) {
        changed();
        world
            .entity_mut(entity)
            .insert(EventBoundary(metadata.event_boundary.clone()));
    }
    set(
        world,
        entity,
        Spatial {
            elevation: metadata.spatial.elevation,
            rotation: metadata.spatial.rotation,
            depth: metadata.spatial.depth,
            world_pinned: metadata.spatial.world_pinned,
        },
    );
    set_optional(
        world,
        entity,
        metadata.pinned.as_ref().map(|value| Pinned {
            anchor: value.anchor,
            scale: value.scale,
        }),
    );
    set_optional(
        world,
        entity,
        metadata.pinned.as_ref().map(|_| GlobalZIndex(1)),
    );
    set_optional(
        world,
        entity,
        metadata.attachment.as_ref().map(|value| Attachment {
            position: value.position,
            rotation: value.rotation,
        }),
    );
    set_optional(
        world,
        entity,
        metadata.group_pose.as_ref().map(|value| GroupPose {
            position: value.position,
            rotation: value.rotation,
        }),
    );
}

pub(super) fn decode_group(group: Option<&str>) -> Result<Option<SandGroup>, String> {
    group
        .map(|source| -> Result<SandGroup, String> {
            if source.len() != 32 || !source.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("Invalid native group identity.".into());
            }
            let mut bytes = [0; 16];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&source[index * 2..index * 2 + 2], 16)
                    .map_err(|_| "Invalid native group identity.")?;
            }
            Ok(SandGroup(bytes))
        })
        .transpose()
}

pub(super) fn set<C: Component + PartialEq>(world: &mut World, entity: Entity, value: C) {
    if world.get::<C>(entity) != Some(&value) {
        changed();
        world.entity_mut(entity).insert(value);
    }
}

fn replace<C: Component, V: PartialEq + ?Sized>(
    world: &mut World,
    entity: Entity,
    desired: Option<&V>,
    project: impl FnOnce(&C) -> &V,
    make: impl FnOnce(&V) -> C,
) {
    if world.get::<C>(entity).map(project) == desired {
        return;
    }
    changed();
    world.entity_mut(entity).remove::<C>();
    if let Some(value) = desired {
        world.entity_mut(entity).insert(make(value));
    }
}

pub(super) fn set_optional<C: Component + PartialEq>(
    world: &mut World,
    entity: Entity,
    desired: Option<C>,
) {
    if world.get::<C>(entity) == desired.as_ref() {
        return;
    }
    changed();
    world.entity_mut(entity).remove::<C>();
    if let Some(value) = desired {
        world.entity_mut(entity).insert(value);
    }
}

fn changed() {
    nucleus::performance::add("native-canvas-placement-values-updated", 1);
}
