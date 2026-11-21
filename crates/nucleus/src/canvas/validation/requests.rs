use std::collections::BTreeSet;

use crate::valid_uid;

use super::{
    super::{Geometry, MAX_PLACEMENTS, Mutation, Request},
    bounded, placement_id, request_identifier, workspace_id, workspace_name,
};

impl Request {
    /// Reports whether processing the request can change the Canvas.
    pub fn is_mutation(&self) -> bool {
        matches!(self, Self::Mutate { .. })
    }

    /// Checks request identifiers, bounds, and the mutation's local invariants.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input or a request larger than the protocol limit.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::InspectElement {
                placement,
                offset,
                limit,
            } => {
                placement_id(placement)?;
                if *limit == 0 || *limit > 128 || *offset > 16_384 {
                    return Err("Inspect 1–128 parts at a bounded offset.".into());
                }
            }
            Self::Export { placement, name } => {
                placement_id(placement)?;
                workspace_name(name)?;
            }
            Self::Inspect { limit, offset, .. }
                if *limit == 0 || *limit > MAX_PLACEMENTS || *offset > 1_000_000 =>
            {
                return Err("Inspect 1–256 placements at a bounded offset.".into());
            }
            Self::Mutate {
                request_id,
                mutation,
                ..
            } => {
                request_identifier(request_id)?;
                match mutation {
                    Mutation::Rebind { placement, records } => {
                        placement_id(placement)?;
                        if records.is_empty()
                            || records.len() > 32
                            || records
                                .iter()
                                .any(|(from, to)| !valid_uid(from, "r") || !valid_uid(to, "r"))
                        {
                            return Err(
                                "Rebind 1–32 existing Record UIDs to resolved Record UIDs.".into(),
                            );
                        }
                    }
                    Mutation::Patch { placement, changes } => {
                        placement_id(placement)?;
                        if changes.is_empty() || changes.len() > 256 {
                            return Err("Apply 1–256 authored changes in a patch.".into());
                        }
                        for change in changes {
                            change.validate()?;
                        }
                    }
                    Mutation::Duplicate {
                        placement,
                        copy,
                        workspace,
                        position,
                    } => {
                        placement_id(placement)?;
                        placement_id(copy)?;
                        workspace_id(*workspace)?;
                        Geometry {
                            position: *position,
                            size: [1.0; 2],
                        }
                        .validate()?;
                    }
                    Mutation::Group { placements, group } => {
                        if placements.is_empty()
                            || placements.len() > MAX_PLACEMENTS
                            || group.as_ref().is_some_and(|group| {
                                group.len() != 32
                                    || !group.bytes().all(|byte| byte.is_ascii_hexdigit())
                            })
                        {
                            return Err("Group 1–256 placements with a 32-digit hexadecimal group identity, or null to ungroup.".into());
                        }
                        let mut ids = BTreeSet::new();
                        for placement in placements {
                            placement_id(placement)?;
                            if !ids.insert(placement) {
                                return Err("Group members must be unique.".into());
                            }
                        }
                    }
                    Mutation::Add {
                        placement,
                        geometry,
                        workspace,
                        ..
                    }
                    | Mutation::AddWithMetadata {
                        placement,
                        geometry,
                        workspace,
                        ..
                    } => {
                        placement_id(placement)?;
                        geometry.validate()?;
                        workspace_id(*workspace)?;
                        if let Mutation::AddWithMetadata { metadata, .. } = mutation
                            && !metadata.validate()
                        {
                            return Err("Invalid placement metadata.".into());
                        }
                    }
                    Mutation::SetRoomMembers { members } => {
                        if members.is_empty() || members.len() > MAX_PLACEMENTS {
                            return Err("Assign between one and 256 room members at a time.".into());
                        }
                        for (id, member) in members {
                            placement_id(id)?;
                            if member.as_ref().is_some_and(|member| !member.valid()) {
                                return Err("Invalid room membership.".into());
                            }
                        }
                    }
                    Mutation::SetMetadata {
                        placement,
                        metadata,
                    } => {
                        placement_id(placement)?;
                        if !metadata.validate() {
                            return Err("Invalid canvas placement metadata.".into());
                        }
                    }
                    Mutation::Configure { placement, .. } | Mutation::Remove { placement } => {
                        placement_id(placement)?
                    }
                    Mutation::Move {
                        placement,
                        workspace,
                        position,
                    } => {
                        placement_id(placement)?;
                        workspace_id(*workspace)?;
                        Geometry {
                            position: *position,
                            size: [1.0, 1.0],
                        }
                        .validate()?;
                    }
                    Mutation::Resize { placement, size } => {
                        placement_id(placement)?;
                        Geometry {
                            position: [0.0, 0.0],
                            size: *size,
                        }
                        .validate()?;
                    }
                    Mutation::CreateWorkspace { name } => workspace_name(name)?,
                    Mutation::RenameWorkspace { workspace, name } => {
                        workspace_id(*workspace)?;
                        workspace_name(name)?;
                    }
                    Mutation::SwitchWorkspace { workspace }
                    | Mutation::RemoveWorkspace { workspace } => workspace_id(*workspace)?,
                    Mutation::Undo { receipt } | Mutation::UndoChanges { receipt } => {
                        request_identifier(receipt)?
                    }
                }
            }
            Self::Receipt { request_id } => request_identifier(request_id)?,
            _ => {}
        }
        bounded(self)
    }
}
