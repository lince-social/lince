//! Room membership, local poses, and placement hierarchy validation.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Placement, Pose};

/// Maximum rooms in one workspace.
pub const MAX_ROOMS: usize = 16;
/// Maximum room members in one workspace.
pub const MAX_MEMBERS: usize = 256;
/// Maximum number of members in an ancestry path.
pub const MAX_DEPTH: usize = 8;

mod projection;
pub use projection::{compose, relative};
pub(super) use projection::{project, relocalize_move};

/// Returns the pose with zero translation and identity rotation.
pub fn identity_pose() -> Pose {
    Pose {
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
    }
}

/// Checks finite coordinates and a unit quaternion within the rotation tolerance.
pub fn valid_pose(pose: &Pose) -> bool {
    pose.position.iter().all(|value| value.is_finite())
        && pose.rotation.iter().all(|value| value.is_finite())
        && (pose.rotation.iter().map(|value| value * value).sum::<f64>() - 1.0).abs() < 1e-6
}

#[expect(missing_docs, reason = "Rustdoc feeds the published Room schema")]
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
    /// Checks the display name and home pose.
    pub fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 256
            && !self.name.chars().any(char::is_control)
            && valid_pose(&self.home)
    }
}

#[expect(
    missing_docs,
    clippy::exhaustive_enums,
    reason = "Rustdoc feeds the published schema and callers match every role"
)]
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

#[expect(missing_docs, reason = "Rustdoc feeds the published membership schema")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub room: String,
    pub parent: Option<String>,
    pub local: Pose,
    pub role: Role,
}

impl Member {
    /// Checks placement identifiers and the local pose.
    pub fn valid(&self) -> bool {
        super::placement_id(&self.room).is_ok()
            && self
                .parent
                .as_deref()
                .is_none_or(|parent| super::placement_id(parent).is_ok() && parent != self.room)
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
    let mut visited = Vec::new();
    for item in placements {
        if let Some(group) = &item.group {
            let parent = item
                .metadata
                .room_member
                .as_ref()
                .map(|member| (member.room.as_str(), member.parent.as_deref()));
            let key = (item.workspace, group);
            if let Some(previous) = groups.insert(key, parent)
                && previous != parent
            {
                return Err("Keep every Sand in a group under the same room parent.".into());
            }
        }
        let (rooms, members) = counts.entry(item.workspace).or_default();
        *rooms += usize::from(item.metadata.room.is_some());
        *members += usize::from(item.metadata.room_member.is_some());
        if *rooms > MAX_ROOMS || *members > MAX_MEMBERS {
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
        visited.clear();
        visited.push(item.id.as_str());
        let mut parent = member.parent.as_deref();
        while let Some(id) = parent {
            if visited.len() >= MAX_DEPTH || visited.contains(&id) {
                return Err("Room parenting must be acyclic and at most eight levels deep.".into());
            }
            visited.push(id);
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
    let affected: Vec<_> = {
        let mut detached = BTreeSet::from([removed]);
        loop {
            let previous = detached.len();
            for item in placements.iter() {
                if item.metadata.room_member.as_ref().is_some_and(|member| {
                    detached.contains(member.room.as_str())
                        || member
                            .parent
                            .as_deref()
                            .is_some_and(|parent| detached.contains(parent))
                }) {
                    detached.insert(item.id.as_str());
                }
            }
            if detached.len() == previous {
                break;
            }
        }
        if detached.len() == 1 {
            return Vec::new();
        }
        placements
            .iter()
            .map(|item| item.id != removed && detached.contains(item.id.as_str()))
            .collect()
    };
    placements
        .iter_mut()
        .zip(affected)
        .filter(|(_, affected)| *affected)
        .map(|(item, _)| {
            item.metadata.room_member = None;
            item.id.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests;
