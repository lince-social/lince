use super::{MAX_DEPTH, valid_pose};
use crate::canvas::{Placement, Pose};
use glam::{DQuat, DVec3};
use std::collections::{BTreeMap, BTreeSet};

pub fn compose(parent: &Pose, local: &Pose) -> Pose {
    let rotation = DQuat::from_array(parent.rotation);
    Pose {
        position: (DVec3::from_array(parent.position)
            + rotation * DVec3::from_array(local.position))
        .to_array(),
        rotation: (rotation * DQuat::from_array(local.rotation))
            .normalize()
            .to_array(),
    }
}

pub fn relative(parent: &Pose, point: &Pose) -> Pose {
    let inverse = DQuat::from_array(parent.rotation).inverse();
    Pose {
        position: (inverse
            * (DVec3::from_array(point.position) - DVec3::from_array(parent.position)))
        .to_array(),
        rotation: (inverse * DQuat::from_array(point.rotation))
            .normalize()
            .to_array(),
    }
}

fn pose(item: &Placement) -> Pose {
    Pose {
        position: [
            item.geometry.position[0],
            item.metadata.spatial.elevation,
            item.geometry.position[1],
        ],
        rotation: item.metadata.spatial.rotation,
    }
}

fn resolve(
    id: &str,
    items: &BTreeMap<&str, &Placement>,
    resolved: &mut BTreeMap<String, Pose>,
    path: &mut BTreeSet<String>,
) -> Result<Pose, String> {
    if let Some(pose) = resolved.get(id) {
        return Ok(pose.clone());
    }
    let item = items.get(id).ok_or("The room parent is unavailable.")?;
    let Some(member) = &item.metadata.room_member else {
        return Ok(pose(item));
    };
    if path.len() >= MAX_DEPTH || !path.insert(id.into()) {
        return Err("Room parenting must be acyclic and at most eight levels deep.".into());
    }
    let parent = member.parent.as_deref().unwrap_or(&member.room);
    let point = compose(&resolve(parent, items, resolved, path)?, &member.local);
    if !valid_pose(&point) {
        return Err("The room transform exceeds finite coordinate bounds.".into());
    }
    path.remove(id);
    resolved.insert(id.into(), point.clone());
    Ok(point)
}

pub(crate) fn project(placements: &mut [Placement]) -> Result<(), String> {
    if !placements
        .iter()
        .any(|item| item.metadata.room_member.is_some())
    {
        return Ok(());
    }
    let items: BTreeMap<_, _> = placements
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let mut resolved = BTreeMap::new();
    for item in placements
        .iter()
        .filter(|item| item.metadata.room_member.is_some())
    {
        resolve(&item.id, &items, &mut resolved, &mut BTreeSet::new())?;
    }
    for item in placements {
        if let Some(point) = resolved.get(&item.id) {
            let previous = pose(item);
            if previous != *point
                && let Some(group) = &mut item.metadata.group_pose
            {
                *group = compose(point, &relative(&previous, group));
            }
            item.geometry.position = [point.position[0], point.position[2]];
            item.metadata.spatial.elevation = point.position[1];
            item.metadata.spatial.rotation = point.rotation;
        }
    }
    Ok(())
}

pub(crate) fn relocalize_move(
    placements: &mut [Placement],
    id: &str,
    position: [f64; 2],
) -> Result<(), String> {
    let item = placements
        .iter()
        .find(|item| item.id == id)
        .ok_or("The placement is unavailable.")?;
    let Some(member) = &item.metadata.room_member else {
        return Ok(());
    };
    let parent = member.parent.as_deref().unwrap_or(&member.room);
    let parent = placements
        .iter()
        .find(|item| item.id == parent)
        .ok_or("The room parent is unavailable.")?;
    let mut next = pose(item);
    next.position[0] = position[0];
    next.position[2] = position[1];
    let local = relative(&pose(parent), &next);
    if !valid_pose(&local) {
        return Err("The room-local move exceeds finite coordinate bounds.".into());
    }
    placements
        .iter_mut()
        .find(|item| item.id == id)
        .unwrap()
        .metadata
        .room_member
        .as_mut()
        .unwrap()
        .local = local;
    Ok(())
}
