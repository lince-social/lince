use std::{
    collections::{BTreeMap, BTreeSet},
    f64::consts::FRAC_PI_2,
};

use crate::{
    json::encoded_len,
    terrain::{Anchor, SpatialBinding},
};

use super::{
    super::{
        MAX_BYTES, MAX_PLACEMENTS, MAX_STATE_PLACEMENTS, Metadata, Pose, Snapshot, View,
        appearance, room,
    },
    placement_id, workspace_name,
};

const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;

impl View {
    fn validate(&self) -> bool {
        self.position.iter().all(|n| n.is_finite())
            && self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.pitch.abs() < FRAC_PI_2
            && self.plane.is_finite()
            && self.selection_depth.is_finite()
            && self.selection_depth > 0.0
    }
}

impl Metadata {
    pub(in crate::canvas) fn validate(&self) -> bool {
        let rotation = |quaternion: &[f64; 4]| {
            (quaternion.iter().map(|value| value * value).sum::<f64>() - 1.0).abs() < 1e-6
        };
        let pose = |p: &Pose| p.position.iter().all(|n| n.is_finite()) && rotation(&p.rotation);
        self.room.as_ref().is_none_or(room::Room::valid)
            && self.room_member.as_ref().is_none_or(room::Member::valid)
            && !(self.room.is_some() && self.room_member.is_some())
            && !((self.room.is_some() || self.room_member.is_some()) && self.pinned.is_some())
            && !(self.room_member.is_some() && (self.binding.is_some() || self.follow_slope))
            && self
                .material
                .as_ref()
                .is_none_or(appearance::Material::valid)
            && self.light.as_ref().is_none_or(appearance::Light::valid)
            && self.binding.as_ref().is_none_or(SpatialBinding::valid)
            && self.pinned.as_ref().is_none_or(|p| {
                p.anchor.iter().all(|n| n.is_finite()) && (0.1..=3.0).contains(&p.scale)
            })
            && self.event_boundary.len() <= 64
            && self
                .event_boundary
                .iter()
                .all(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
            && self.spatial.elevation.is_finite()
            && rotation(&self.spatial.rotation)
            && self
                .spatial
                .depth
                .is_none_or(|n| n.is_finite() && n > 0.0 && n <= 100_000.0)
            && self.attachment.as_ref().is_none_or(pose)
            && self.group_pose.as_ref().is_none_or(pose)
    }
}

impl Snapshot {
    /// Checks the bounds and relationships of a snapshot page.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid workspaces, placements, relationships, or exceeded limits.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_with_limits(MAX_PLACEMENTS, MAX_BYTES, false)
    }
    pub(in crate::canvas) fn validate_full(&self) -> Result<(), String> {
        self.validate_with_limits(MAX_STATE_PLACEMENTS, MAX_STATE_BYTES, true)
    }
    fn validate_with_limits(
        &self,
        placements: usize,
        bytes: usize,
        complete: bool,
    ) -> Result<(), String> {
        if self.workspaces.is_empty()
            || self.workspaces.len() > 64
            || self.placements.len() > placements
        {
            return Err("Canvas snapshot exceeds workspace/placement bounds.".into());
        }
        let mut spaces = BTreeSet::new();
        for space in &self.workspaces {
            workspace_name(&space.name)?;
            if space.id == 0
                || !spaces.insert(space.id)
                || space.center.iter().any(|n| !n.is_finite())
                || !space.zoom.is_finite()
                || space.zoom <= 0.0
                || space.view.as_ref().is_some_and(|view| !view.validate())
            {
                return Err("Invalid workspace identity or view.".into());
            }
        }
        if !spaces.contains(&self.active_workspace) {
            return Err("The active workspace is missing.".into());
        }
        let mut ids = BTreeSet::new();
        for placement in &self.placements {
            placement_id(&placement.id)?;
            placement.geometry.validate()?;
            if !placement.metadata.validate() {
                return Err("Invalid canvas placement metadata.".into());
            }
            if !ids.insert(&placement.id)
                || !spaces.contains(&placement.workspace)
                || placement.group.as_ref().is_some_and(|g| g.len() > 80)
            {
                return Err("Invalid or duplicate canvas placement.".into());
            }
        }
        room::validate(&self.placements, complete)?;
        let bindings: BTreeMap<_, _> = self
            .placements
            .iter()
            .filter_map(|p| match p.metadata.binding.as_ref().map(|b| &b.anchor) {
                Some(Anchor::Item(id)) => Some((p.id.as_str(), id.as_str())),
                _ => None,
            })
            .collect();
        let mut checked = BTreeSet::new();
        for start in bindings.keys() {
            let mut path = BTreeSet::new();
            let mut cursor = *start;
            while !checked.contains(cursor) {
                if !path.insert(cursor) {
                    return Err("Spatial bindings cannot form a cycle.".into());
                }
                let Some(next) = bindings.get(cursor) else {
                    break;
                };
                cursor = next;
            }
            checked.extend(path);
        }
        if encoded_len(self).map_err(|error| error.to_string())? > bytes {
            return Err("Canvas snapshot exceeds its byte limit.".into());
        }
        Ok(())
    }
}
