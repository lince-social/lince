use std::collections::BTreeMap;

use crate::{
    canvas::{Placement, Pose, Snapshot, room},
    terrain::{Anchor, SpatialBinding},
};

fn undo_field<T: Clone + PartialEq>(before: &T, after: &T, current: &mut T) -> Result<(), String> {
    if before != after {
        if current != after {
            return Err("Undo conflicts with a later edit to the same field.".into());
        }
        *current = before.clone();
    }
    Ok(())
}

fn same_authored_binding(left: Option<&SpatialBinding>, right: Option<&SpatialBinding>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            let SpatialBinding {
                anchor,
                position,
                offset,
                rotation,
                follow_orientation,
                camera_floor,
            } = left;
            anchor == &right.anchor
                && offset == &right.offset
                && follow_orientation == &right.follow_orientation
                && camera_floor == &right.camera_floor
                && (*anchor != Anchor::Fixed || position == &right.position)
                && (*follow_orientation || rotation == &right.rotation)
        }
        _ => false,
    }
}

fn room_world_pose(item: &Placement) -> Pose {
    Pose {
        position: [
            item.geometry.position[0],
            item.metadata.spatial.elevation,
            item.geometry.position[1],
        ],
        rotation: item.metadata.spatial.rotation,
    }
}

pub(super) fn undo_placements(
    current: &mut Snapshot,
    before: &Snapshot,
    after: &Snapshot,
) -> Result<(), String> {
    if before.workspaces != after.workspaces || before.active_workspace != after.active_workspace {
        return Err("Use ordinary canvas undo for workspace changes.".into());
    }
    let old: BTreeMap<_, _> = before.placements.iter().map(|p| (&p.id, p)).collect();
    let new: BTreeMap<_, _> = after.placements.iter().map(|p| (&p.id, p)).collect();
    for (id, added) in &new {
        if !old.contains_key(id) {
            let found = current
                .placements
                .iter()
                .find(|p| &p.id == *id)
                .ok_or("The added placement was already removed.")?;
            if found.component != added.component
                || found.metadata.room != added.metadata.room
                || found.metadata.room_member != added.metadata.room_member
                || found.metadata.material != added.metadata.material
                || found.metadata.light != added.metadata.light
                || !same_authored_binding(
                    found.metadata.binding.as_ref(),
                    added.metadata.binding.as_ref(),
                )
            {
                return Err("Undo would remove a placement with later authored changes.".into());
            }
            current.placements.retain(|p| &p.id != *id);
        }
    }
    for (id, before) in old {
        let Some(after) = new.get(id) else {
            if current.placements.iter().any(|p| &p.id == id) {
                return Err("The removed identity is already used.".into());
            }
            current.placements.push(before.clone());
            continue;
        };
        if before == *after {
            continue;
        }
        let found = current
            .placements
            .iter_mut()
            .find(|p| &p.id == id)
            .ok_or("The edited placement was removed.")?;
        undo_field(&before.component, &after.component, &mut found.component)?;
        undo_field(&before.workspace, &after.workspace, &mut found.workspace)?;
        let projected =
            before.metadata.room_member.is_some() || after.metadata.room_member.is_some();
        if before.metadata.room_member.is_some()
            && after.metadata.room_member.is_none()
            && (found.geometry != after.geometry
                || found.metadata.spatial != after.metadata.spatial)
        {
            return Err(
                "Undo conflicts with a world placement edit made after detaching from the room."
                    .into(),
            );
        }
        if !projected {
            undo_field(&before.geometry, &after.geometry, &mut found.geometry)?;
        } else {
            undo_field(
                &before.geometry.size,
                &after.geometry.size,
                &mut found.geometry.size,
            )?;
        }
        undo_field(
            &before.metadata.room,
            &after.metadata.room,
            &mut found.metadata.room,
        )?;
        undo_field(
            &before.metadata.room_member,
            &after.metadata.room_member,
            &mut found.metadata.room_member,
        )?;
        undo_field(&before.group, &after.group, &mut found.group)?;
        undo_field(
            &before.metadata.order,
            &after.metadata.order,
            &mut found.metadata.order,
        )?;
        undo_field(
            &before.metadata.pinned,
            &after.metadata.pinned,
            &mut found.metadata.pinned,
        )?;
        undo_field(
            &before.metadata.event_boundary,
            &after.metadata.event_boundary,
            &mut found.metadata.event_boundary,
        )?;
        if !projected {
            undo_field(
                &before.metadata.spatial,
                &after.metadata.spatial,
                &mut found.metadata.spatial,
            )?;
        } else {
            undo_field(
                &before.metadata.spatial.depth,
                &after.metadata.spatial.depth,
                &mut found.metadata.spatial.depth,
            )?;
            undo_field(
                &before.metadata.spatial.world_pinned,
                &after.metadata.spatial.world_pinned,
                &mut found.metadata.spatial.world_pinned,
            )?;
        }
        undo_field(
            &before.metadata.attachment,
            &after.metadata.attachment,
            &mut found.metadata.attachment,
        )?;
        if !projected {
            undo_field(
                &before.metadata.group_pose,
                &after.metadata.group_pose,
                &mut found.metadata.group_pose,
            )?;
        } else if before.group != after.group {
            found.metadata.group_pose = before.metadata.group_pose.as_ref().map(|group| {
                let old = room_world_pose(before);
                let current = room_world_pose(found);
                room::compose(&current, &room::relative(&old, group))
            });
        }
        if before.metadata.room_member.is_none() && after.metadata.room_member.is_some() {
            let current = room_world_pose(found);
            let original = room_world_pose(before);
            found.metadata.group_pose = found
                .metadata
                .group_pose
                .as_ref()
                .map(|group| room::compose(&original, &room::relative(&current, group)));
            found.geometry.position = before.geometry.position;
            found.metadata.spatial.elevation = before.metadata.spatial.elevation;
            found.metadata.spatial.rotation = before.metadata.spatial.rotation;
        }
        undo_field(
            &before.metadata.follow_slope,
            &after.metadata.follow_slope,
            &mut found.metadata.follow_slope,
        )?;
        let after_binding = after.metadata.binding.as_ref();
        if !same_authored_binding(before.metadata.binding.as_ref(), after_binding) {
            if !same_authored_binding(found.metadata.binding.as_ref(), after_binding) {
                return Err("Undo conflicts with a later spatial binding edit.".into());
            }
            found.metadata.binding = before.metadata.binding.clone();
        }
    }
    Ok(())
}
