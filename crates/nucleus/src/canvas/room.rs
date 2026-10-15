use super::{Placement, Pose};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_ROOMS: usize = 16;
pub const MAX_MEMBERS: usize = 256;
pub const MAX_DEPTH: usize = 8;

mod projection;
pub use projection::{compose, relative};
pub(super) use projection::{project, relocalize_move};

pub fn identity_pose() -> Pose {
    Pose {
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    }
}

pub fn valid_pose(pose: &Pose) -> bool {
    pose.position.iter().all(|value| value.is_finite())
        && pose.rotation.iter().all(|value| value.is_finite())
        && (pose.rotation.iter().map(|value| value * value).sum::<f64>() - 1.0).abs() < 1e-6
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Room {
    pub name: String,
    pub home: Pose,
    pub ambient_enabled: bool,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            name: "Room".into(),
            home: identity_pose(),
            ambient_enabled: true,
        }
    }
}

impl Room {
    pub fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 256
            && !self.name.chars().any(char::is_control)
            && valid_pose(&self.home)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    #[default]
    Content,
    Floor,
    Wall,
    Desk,
    Computer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub room: String,
    pub parent: Option<String>,
    pub local: Pose,
    pub role: Role,
}

impl Member {
    pub fn valid(&self) -> bool {
        super::placement_id(&self.room).is_ok()
            && self
                .parent
                .as_ref()
                .is_none_or(|parent| super::placement_id(parent).is_ok() && parent != &self.room)
            && valid_pose(&self.local)
    }
}

pub(super) fn validate(placements: &[Placement], complete: bool) -> Result<(), String> {
    if placements
        .iter()
        .all(|item| item.metadata.room.is_none() && item.metadata.room_member.is_none())
    {
        return Ok(());
    }
    let by_id: BTreeMap<_, _> = placements
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let mut counts = BTreeMap::<u64, (usize, usize)>::new();
    let mut groups = BTreeMap::new();
    for item in placements {
        if let Some(group) = &item.group {
            let parent = item
                .metadata
                .room_member
                .as_ref()
                .map(|member| (&member.room, &member.parent));
            let key = (item.workspace, group);
            if let Some(previous) = groups.insert(key, parent)
                && previous != parent
            {
                return Err("Keep every Sand in a group under the same room parent.".into());
            }
        }
        let count = counts.entry(item.workspace).or_default();
        count.0 += usize::from(item.metadata.room.is_some());
        count.1 += usize::from(item.metadata.room_member.is_some());
        if count.0 > MAX_ROOMS || count.1 > MAX_MEMBERS {
            return Err("A workspace supports sixteen rooms and 256 room members.".into());
        }
        let Some(member) = &item.metadata.room_member else {
            continue;
        };
        let Some(room) = by_id.get(member.room.as_str()) else {
            if complete {
                return Err("The parent room is missing.".into());
            }
            continue;
        };
        if room.workspace != item.workspace || room.metadata.room.is_none() {
            return Err("Room members need a room in the same workspace.".into());
        }
        let mut visited = BTreeSet::from([item.id.as_str()]);
        let mut parent = member.parent.as_deref();
        while let Some(id) = parent {
            if !visited.insert(id) || visited.len() > MAX_DEPTH {
                return Err("Room parenting must be acyclic and at most eight levels deep.".into());
            }
            let Some(anchor) = by_id.get(id) else {
                if complete {
                    return Err("The room anchor is missing.".into());
                }
                break;
            };
            let Some(anchor_member) = &anchor.metadata.room_member else {
                return Err("A room anchor needs room membership.".into());
            };
            if anchor.workspace != item.workspace
                || anchor_member.room != member.room
                || anchor_member.role == Role::Content
            {
                return Err(
                    "Select a floor, wall, desk or computer anchor in the same room.".into(),
                );
            }
            parent = anchor_member.parent.as_deref();
        }
    }
    Ok(())
}

pub(super) fn detach_descendants(placements: &mut [Placement], removed: &str) -> Vec<String> {
    let mut detached = BTreeSet::from([removed.to_string()]);
    loop {
        let previous = detached.len();
        for item in placements.iter() {
            if item.metadata.room_member.as_ref().is_some_and(|member| {
                detached.contains(&member.room)
                    || member
                        .parent
                        .as_ref()
                        .is_some_and(|parent| detached.contains(parent))
            }) {
                detached.insert(item.id.clone());
            }
        }
        if detached.len() == previous {
            break;
        }
    }
    let mut affected = Vec::new();
    for item in placements {
        if item.id != removed && detached.contains(&item.id) {
            item.metadata.room_member = None;
            affected.push(item.id.clone());
        }
    }
    affected
}

#[cfg(test)]
mod tests;
