use std::sync::Arc;

use super::{State, undo::undo_placements};
use crate::{
    canvas::{
        Component, MAX_PLACEMENTS, Mutation, Placement, Snapshot, Workspace, authoring, room,
    },
    performance,
    terrain::Anchor,
};

impl State {
    pub(super) fn apply(
        &self,
        next: &mut Arc<Snapshot>,
        mutation: &Mutation,
    ) -> Result<(Vec<String>, Option<u64>), String> {
        if let Mutation::Undo { receipt } = mutation {
            let previous = self
                .retained_receipt(receipt)
                .ok_or("The undo receipt is no longer retained.")?;
            if previous.receipt.revision != next.revision {
                return Err(
                    "Undo would overwrite later changes; inspect the current canvas first.".into(),
                );
            }
            let revision = next.revision;
            *next = Arc::clone(&previous.before);
            Arc::make_mut(next).revision = revision;
            performance::add("canvas-mutation-snapshots-copied", 1);
            return Ok((
                previous.receipt.affected_placements.clone(),
                Some(next.active_workspace),
            ));
        }
        let next = Arc::make_mut(next);
        performance::add("canvas-mutation-snapshots-copied", 1);
        let require_workspace = |id| {
            if next.workspaces.iter().any(|w| w.id == id) {
                Ok(())
            } else {
                Err("The workspace no longer exists.")
            }
        };
        match mutation {
            Mutation::SetRoomMembers { members } => {
                if members.is_empty() || members.len() > MAX_PLACEMENTS {
                    return Err("Assign between one and 256 room members at a time.".into());
                }
                let mut workspace = None;
                for (id, member) in members {
                    let selected = find(next, id)?;
                    if workspace.is_some_and(|workspace| workspace != selected.workspace) {
                        return Err("A room assignment stays within one workspace.".into());
                    }
                    workspace = Some(selected.workspace);
                    selected.metadata.room_member = member.clone();
                }
                Ok((members.keys().cloned().collect(), workspace))
            }
            Mutation::Rebind { placement, records } => {
                let selected = find(next, placement)?;
                let component = authoring::rebind(&selected.component, records)?;
                component.validate(&self.registry, true)?;
                selected.component = component;
                Ok((vec![placement.clone()], Some(selected.workspace)))
            }
            Mutation::Patch { placement, changes } => {
                let selected = find(next, placement)?;
                let mut edits = authoring::edits(&selected.component);
                for change in changes {
                    edits.apply(change)?;
                }
                if let Some(geometry) = edits
                    .parts
                    .get("root")
                    .and_then(|part| part.geometry.clone())
                {
                    selected.geometry = geometry;
                }
                for (dimension, token) in selected.geometry.size.iter_mut().zip(["Width", "Height"])
                {
                    if let Some(authoring::TokenValue::Number(value)) = edits
                        .parts
                        .get("root")
                        .and_then(|part| part.tokens.get(token))
                    {
                        *dimension = f64::from(*value);
                    }
                }
                if edits.size.is_some() {
                    edits.size = Some(selected.geometry.size);
                }
                let component = Component::Authored {
                    base: Box::new(authoring::base(&selected.component).clone()),
                    edits,
                };
                component.validate_snapshot(&self.registry)?;
                selected.component = component;
                Ok((vec![placement.clone()], Some(selected.workspace)))
            }
            Mutation::Duplicate {
                placement,
                copy,
                workspace,
                position,
            } => {
                require_workspace(*workspace)?;
                if next.placements.iter().any(|item| &item.id == copy) {
                    return Err("The copy identity is already used.".into());
                }
                let mut duplicate = find(next, placement)?.clone();
                let old_position = duplicate.geometry.position;
                duplicate.id = copy.clone();
                duplicate.workspace = *workspace;
                duplicate.geometry.position = *position;
                if let Some(binding) = &mut duplicate.metadata.binding {
                    binding.translate([
                        position[0] - old_position[0],
                        0.0,
                        position[1] - old_position[1],
                    ]);
                }
                duplicate.group = None;
                duplicate.selected = false;
                duplicate.metadata.attachment = None;
                duplicate.metadata.group_pose = None;
                duplicate.metadata.room_member = None;
                if let Some(room) = &mut duplicate.metadata.room {
                    room.home.position = [
                        position[0],
                        duplicate.metadata.spatial.elevation,
                        position[1],
                    ];
                }
                next.placements.push(duplicate);
                Ok((vec![copy.clone()], Some(*workspace)))
            }
            Mutation::Group { placements, group } => {
                let mut workspace = None;
                for id in placements {
                    let selected = find(next, id)?;
                    if workspace.is_some_and(|workspace| workspace != selected.workspace) {
                        return Err("A transform group stays within one workspace.".into());
                    }
                    workspace = Some(selected.workspace);
                    selected.group = group.clone();
                    selected.metadata.group_pose = None;
                }
                Ok((placements.clone(), workspace))
            }
            Mutation::Add {
                placement,
                workspace,
                component,
                geometry,
            }
            | Mutation::AddWithMetadata {
                placement,
                workspace,
                component,
                geometry,
                ..
            } => {
                require_workspace(*workspace)?;
                component.validate(&self.registry, false)?;
                if next.placements.iter().any(|p| &p.id == placement) {
                    return Err("The placement ID is already used.".into());
                }
                next.placements.push(Placement {
                    id: placement.clone(),
                    workspace: *workspace,
                    component: component.clone(),
                    geometry: geometry.clone(),
                    selected: false,
                    group: None,
                    metadata: match mutation {
                        Mutation::AddWithMetadata { metadata, .. } => metadata.clone(),
                        _ => Default::default(),
                    },
                });
                Ok((vec![placement.clone()], Some(*workspace)))
            }
            Mutation::Configure {
                placement,
                component,
            } => {
                component.validate(&self.registry, true)?;
                let selected = find(next, placement)?;
                if selected.component.origin().is_some()
                    && selected.component.origin() != component.origin()
                {
                    return Err(
                        "A Fiote composition must retain its protected host and origin.".into(),
                    );
                }
                selected.component = component.clone();
                Ok((vec![placement.clone()], Some(selected.workspace)))
            }
            Mutation::SetMetadata {
                placement,
                metadata,
            } => {
                let selected = find(next, placement)?;
                let projected_pose = (
                    selected.metadata.spatial.elevation,
                    selected.metadata.spatial.rotation,
                );
                selected.metadata = metadata.clone();
                if selected.metadata.room_member.is_some() {
                    selected.metadata.spatial.elevation = projected_pose.0;
                    selected.metadata.spatial.rotation = projected_pose.1;
                }
                Ok((vec![placement.clone()], Some(selected.workspace)))
            }
            Mutation::Move {
                placement,
                workspace,
                position,
            } => {
                require_workspace(*workspace)?;
                room::relocalize_move(&mut next.placements, placement, *position)?;
                let selected = find(next, placement)?;
                if let Some(binding) = &mut selected.metadata.binding {
                    binding.translate([
                        position[0] - selected.geometry.position[0],
                        0.0,
                        position[1] - selected.geometry.position[1],
                    ]);
                }
                selected.workspace = *workspace;
                selected.geometry.position = *position;
                if let Component::Authored { edits, .. } = &mut selected.component
                    && let Some(root) = edits.parts.get_mut("root")
                    && let Some(geometry) = &mut root.geometry
                {
                    geometry.position = *position;
                }
                Ok((vec![placement.clone()], Some(*workspace)))
            }
            Mutation::Resize { placement, size } => {
                let selected = find(next, placement)?;
                selected.geometry.size = *size;
                if let Component::Authored { edits, .. } = &mut selected.component {
                    edits.size = Some(*size);
                    if let Some(root) = edits.parts.get_mut("root") {
                        root.tokens.remove("Width");
                        root.tokens.remove("Height");
                        if let Some(geometry) = &mut root.geometry {
                            geometry.size = *size;
                        }
                    }
                }
                Ok((vec![placement.clone()], Some(selected.workspace)))
            }
            Mutation::Remove { placement } => {
                let workspace = find(next, placement)?.workspace;
                let mut affected = vec![placement.clone()];
                affected.extend(room::detach_descendants(&mut next.placements, placement));
                next.placements.retain(|p| &p.id != placement);
                for item in &mut next.placements {
                    if let Some(binding) = &mut item.metadata.binding
                        && matches!(&binding.anchor, Anchor::Item(id) if id == placement)
                    {
                        binding.anchor = Anchor::Fixed;
                        binding.offset = [0.0; 3];
                        affected.push(item.id.clone());
                    }
                }
                Ok((affected, Some(workspace)))
            }
            Mutation::CreateWorkspace { name } => {
                let id = self.next_workspace_id;
                id.checked_add(1).ok_or("Workspace identity overflow.")?;
                next.workspaces.push(Workspace {
                    id,
                    name: name.trim().into(),
                    center: [0.0, 0.0],
                    zoom: 1.0,
                    view: None,
                });
                next.active_workspace = id;
                Ok((vec![], Some(id)))
            }
            Mutation::RenameWorkspace { workspace, name } => {
                let selected = next
                    .workspaces
                    .iter_mut()
                    .find(|w| w.id == *workspace)
                    .ok_or("The workspace no longer exists.")?;
                selected.name = name.trim().into();
                Ok((vec![], Some(*workspace)))
            }
            Mutation::SwitchWorkspace { workspace } => {
                require_workspace(*workspace)?;
                next.active_workspace = *workspace;
                Ok((vec![], Some(*workspace)))
            }
            Mutation::RemoveWorkspace { workspace } => {
                require_workspace(*workspace)?;
                if next.workspaces.len() <= 1 {
                    return Err("Keep at least one workspace.".into());
                }
                let destination = if next.active_workspace != *workspace {
                    next.active_workspace
                } else {
                    next.workspaces
                        .iter()
                        .find(|w| w.id != *workspace)
                        .ok_or("Keep at least one workspace.")?
                        .id
                };
                let affected = next
                    .placements
                    .iter()
                    .filter(|placement| placement.workspace == *workspace)
                    .map(|placement| placement.id.clone())
                    .collect();
                next.placements
                    .retain(|placement| placement.workspace != *workspace);
                next.workspaces.retain(|w| w.id != *workspace);
                next.active_workspace = destination;
                Ok((affected, Some(destination)))
            }
            Mutation::Undo { .. } => {
                unreachable!("Full undo is handled before copying the canvas.")
            }
            Mutation::UndoChanges { receipt } => {
                let previous = self
                    .retained_receipt(receipt)
                    .ok_or("The undo receipt is no longer retained.")?;
                undo_placements(next, &previous.before, &previous.after)?;
                Ok((
                    previous.receipt.affected_placements.clone(),
                    Some(next.active_workspace),
                ))
            }
        }
    }
}

fn find<'a>(snapshot: &'a mut Snapshot, id: &str) -> Result<&'a mut Placement, String> {
    snapshot
        .placements
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| "The placement no longer exists.".into())
}
