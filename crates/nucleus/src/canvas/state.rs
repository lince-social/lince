use super::*;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedReceipt {
    id: String,
    hash: [u8; 32],
    receipt: Receipt,
    before: Snapshot,
    after: Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub snapshot: Snapshot,
    next_workspace_id: u64,
    registry: Vec<Descriptor>,
    receipts: Vec<SavedReceipt>,
}

impl State {
    pub fn new(mut snapshot: Snapshot, registry: Vec<Descriptor>) -> Result<Self, String> {
        if snapshot.next_offset.is_some() {
            return Err(
                "Initialize canvas state from a complete snapshot, not an inspection page.".into(),
            );
        }
        snapshot.validate_full()?;
        room::project(&mut snapshot.placements)?;
        validate_registry(&registry)?;
        for placement in &snapshot.placements {
            placement.component.validate_snapshot(&registry)?;
        }
        let next_workspace_id = snapshot
            .workspaces
            .iter()
            .map(|w| w.id)
            .max()
            .unwrap_or_default()
            .checked_add(1)
            .ok_or("Workspace identity overflow.")?;
        Ok(Self {
            next_workspace_id,
            snapshot,
            registry,
            receipts: Vec::new(),
        })
    }

    pub fn normalize_applied_snapshot(&mut self, mut snapshot: Snapshot) -> Result<(), String> {
        snapshot.revision = self.snapshot.revision;
        snapshot.next_offset = None;
        snapshot.validate_full()?;
        room::project(&mut snapshot.placements)?;
        for placement in &snapshot.placements {
            placement.component.validate_snapshot(&self.registry)?;
        }
        self.snapshot = snapshot;
        if let Some(receipt) = self.receipts.last_mut()
            && receipt.receipt.revision == self.snapshot.revision
        {
            receipt.after = self.snapshot.clone();
        }
        Ok(())
    }

    pub fn synchronize(&mut self, mut snapshot: Snapshot) -> Result<(), String> {
        snapshot.revision = self.snapshot.revision;
        snapshot.next_offset = None;
        snapshot.validate_full()?;
        room::project(&mut snapshot.placements)?;
        for placement in &snapshot.placements {
            placement.component.validate_snapshot(&self.registry)?;
        }
        if snapshot != self.snapshot {
            snapshot.revision = snapshot
                .revision
                .checked_add(1)
                .ok_or("Canvas revision overflow.")?;
            self.next_workspace_id = self.next_workspace_id.max(
                snapshot
                    .workspaces
                    .iter()
                    .map(|w| w.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or("Workspace identity overflow.")?,
            );
            self.snapshot = snapshot;
        }
        Ok(())
    }

    pub fn reserve_workspace_id(&mut self, id: u64) -> Result<(), String> {
        self.next_workspace_id = self
            .next_workspace_id
            .max(id.checked_add(1).ok_or("Workspace identity overflow.")?);
        Ok(())
    }

    pub fn next_workspace_id(&self) -> u64 {
        self.next_workspace_id
    }

    pub fn validate(&self) -> Result<(), String> {
        self.snapshot.validate_full()?;
        if self
            .snapshot
            .workspaces
            .iter()
            .any(|w| w.id >= self.next_workspace_id)
        {
            return Err("The workspace identity counter is invalid.".into());
        }
        validate_registry(&self.registry)?;
        for placement in &self.snapshot.placements {
            placement.component.validate_snapshot(&self.registry)?;
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 33 * 1024 * 1024 {
            return Err("Canvas recovery exceeds 33 MiB.".into());
        }
        if self.receipts.len() > 16 {
            return Err("Canvas recovery has too many undo receipts.".into());
        }
        let mut ids = BTreeSet::new();
        for receipt in &self.receipts {
            request_identifier(&receipt.id)?;
            if receipt.receipt.request_id != receipt.id
                || !ids.insert(&receipt.id)
                || receipt.receipt.revision > self.snapshot.revision
            {
                return Err("Invalid canvas recovery receipt.".into());
            }
            receipt.before.validate_full()?;
            receipt.after.validate_full()?;
        }
        Ok(())
    }

    pub fn handle(&mut self, request: &Request) -> Result<Response, String> {
        request.validate()?;
        match request {
            Request::InspectElement { placement, .. } => {
                let placement = self.snapshot.placements.iter().find(|item| &item.id == placement).ok_or("The placement no longer exists.")?.clone();
                Ok(Response::Element { element: authoring::Element { revision: self.snapshot.revision, placement, parts: Vec::new(), total_parts: 0, next_offset: None, capabilities: vec!["placement-crud".into(), "authored-parts".into(), "tokens".into(), "event-links".into(), "event-action-behaviors".into(), "groups".into(), "export".into()] } })
            }
            Request::Export { placement, name } => {
                let placement = self.snapshot.placements.iter().find(|item| &item.id == placement).ok_or("The placement no longer exists.")?;
                let mut edits = authoring::edits(&placement.component);
                edits.size = Some(placement.geometry.size);
                if let Some(root) = edits.parts.get_mut("root") { root.geometry = None; }
                let component = Component::Authored { base: Box::new(authoring::base(&placement.component).clone()), edits };
                let document = Document { format: Document::FORMAT.into(), name: name.clone(), component };
                let hash = authoring::hash(&document)?;
                Ok(Response::Export { document, hash, revision: self.snapshot.revision })
            }
            Request::Registry => Ok(Response::Registry { components: self.registry.clone() }),
            Request::Inspect { workspace, selected_only, offset, limit } => {
                if workspace.is_some_and(|id| !self.snapshot.workspaces.iter().any(|w| w.id == id)) { return Err("The workspace no longer exists.".into()); }
                let placements: Vec<_> = self.snapshot.placements.iter().filter(|p| workspace.is_none_or(|id| p.workspace == id) && (!selected_only || p.selected)).collect();
                let mut snapshot = self.snapshot.clone();
                snapshot.placements = placements.iter().skip(*offset).take(*limit).map(|p| (*p).clone()).collect();
                snapshot.next_offset = if offset.saturating_add(*limit) < placements.len() { Some(offset + limit) } else { None };
                snapshot.validate()?;
                Ok(Response::Snapshot { snapshot })
            }
            Request::Receipt { request_id } => Ok(Response::Receipt { receipt: self.receipts.iter().find(|r| &r.id == request_id).ok_or("This request has no retained receipt. Inspect actual canvas state before retrying.")?.receipt.clone() }),
            Request::Mutate { request_id, expected_revision, mutation } => {
                let hash: [u8; 32] = Sha256::digest(serde_json::to_vec(request).map_err(|e| e.to_string())?).into();
                if let Some(receipt) = self.receipts.iter().find(|r| &r.id == request_id) {
                    if receipt.hash != hash { return Err("This request ID already belongs to another canvas operation.".into()); }
                    return Ok(Response::Receipt { receipt: receipt.receipt.clone() });
                }
                if *expected_revision != self.snapshot.revision { return Err(format!("Canvas changed: expected revision {expected_revision}, current {}. Inspect before editing again.", self.snapshot.revision)); }
                let before = self.snapshot.clone();
                let mut next = before.clone();
                let (mut affected_placements, workspace) = self.apply(&mut next, mutation)?;
                let affected_count = if let Mutation::Undo { receipt } | Mutation::UndoChanges { receipt } = mutation { self.receipts.iter().find(|r| &r.id == receipt).ok_or("The undo receipt is no longer retained.")?.receipt.affected_count } else { affected_placements.len() };
                affected_placements.truncate(MAX_PLACEMENTS);
                next.revision = next.revision.checked_add(1).ok_or("Canvas revision overflow.")?;
                next.validate_full()?;
                room::project(&mut next.placements)?;
                let receipt = Receipt { request_id: request_id.clone(), revision: next.revision, persistence: Persistence::Applied, affected_placements, affected_count, workspace };
                if matches!(mutation, Mutation::CreateWorkspace { .. }) { self.next_workspace_id = self.next_workspace_id.checked_add(1).ok_or("Workspace identity overflow.")?; }
                self.snapshot = next;
                self.receipts.push(SavedReceipt { id: request_id.clone(), hash, receipt: receipt.clone(), before, after: self.snapshot.clone() });
                let mut sizes = self.receipts.iter().map(|r| serde_json::to_vec(&(&r.before, &r.after)).map(|bytes| bytes.len())).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
                let mut bytes = sizes.iter().sum::<usize>();
                while self.receipts.len() > 1 && (self.receipts.len() > 16 || bytes > 16 * 1024 * 1024) {
                    bytes -= sizes.remove(0);
                    self.receipts.remove(0);
                }
                Ok(Response::Receipt { receipt })
            }
        }
    }

    pub fn mark_saved_through(&mut self, revision: u64) {
        for receipt in &mut self.receipts {
            if receipt.receipt.revision <= revision {
                receipt.receipt.persistence = Persistence::Saved;
            }
        }
    }

    pub fn mark_saved(&mut self, request_id: &str) -> Result<(), String> {
        let receipt = self
            .receipts
            .iter_mut()
            .find(|r| r.id == request_id)
            .ok_or("This canvas receipt is no longer retained.")?;
        receipt.receipt.persistence = Persistence::Saved;
        Ok(())
    }

    fn apply(
        &self,
        next: &mut Snapshot,
        mutation: &Mutation,
    ) -> Result<(Vec<String>, Option<u64>), String> {
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
                for (axis, token) in ["Width", "Height"].into_iter().enumerate() {
                    if let Some(authoring::TokenValue::Number(value)) = edits
                        .parts
                        .get("root")
                        .and_then(|part| part.tokens.get(token))
                    {
                        selected.geometry.size[axis] = f64::from(*value);
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
                    metadata: Default::default(),
                });
                Ok((vec![placement.clone()], Some(*workspace)))
            }
            Mutation::AddWithMetadata {
                placement,
                workspace,
                component,
                geometry,
                metadata,
            } => {
                let result = self.apply(
                    next,
                    &Mutation::Add {
                        placement: placement.clone(),
                        workspace: *workspace,
                        component: component.clone(),
                        geometry: geometry.clone(),
                    },
                )?;
                find(next, placement)?.metadata = metadata.clone();
                Ok(result)
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
                        && matches!(&binding.anchor, crate::terrain::Anchor::Item(id) if id == placement)
                    {
                        binding.anchor = crate::terrain::Anchor::Fixed;
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
                        .unwrap()
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
            Mutation::Undo { receipt } => {
                let previous = self
                    .receipts
                    .iter()
                    .find(|r| &r.id == receipt)
                    .ok_or("The undo receipt is no longer retained.")?;
                if previous.receipt.revision != next.revision {
                    return Err(
                        "Undo would overwrite later changes; inspect the current canvas first."
                            .into(),
                    );
                }
                let revision = next.revision;
                *next = previous.before.clone();
                next.revision = revision;
                Ok((
                    previous.receipt.affected_placements.clone(),
                    Some(next.active_workspace),
                ))
            }
            Mutation::UndoChanges { receipt } => {
                let previous = self
                    .receipts
                    .iter()
                    .find(|r| &r.id == receipt)
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

fn undo_field<T: Clone + PartialEq>(before: &T, after: &T, current: &mut T) -> Result<(), String> {
    if before != after {
        if current != after {
            return Err("Undo conflicts with a later edit to the same field.".into());
        }
        *current = before.clone();
    }
    Ok(())
}

fn authored_binding(
    binding: &Option<crate::terrain::SpatialBinding>,
) -> Option<crate::terrain::SpatialBinding> {
    binding.clone().map(|mut b| {
        if b.anchor != crate::terrain::Anchor::Fixed {
            b.position = [0.0; 3];
        }
        if b.follow_orientation {
            b.rotation = [0.0, 0.0, 0.0, 1.0];
        }
        b
    })
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

fn undo_placements(
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
                || authored_binding(&found.metadata.binding)
                    != authored_binding(&added.metadata.binding)
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
        if authored_binding(&before.metadata.binding) != authored_binding(&after.metadata.binding) {
            if authored_binding(&found.metadata.binding)
                != authored_binding(&after.metadata.binding)
            {
                return Err("Undo conflicts with a later spatial binding edit.".into());
            }
            found.metadata.binding = before.metadata.binding.clone();
        }
    }
    Ok(())
}

fn find<'a>(snapshot: &'a mut Snapshot, id: &str) -> Result<&'a mut Placement, String> {
    snapshot
        .placements
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| "The placement no longer exists.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> State {
        State::new(
            Snapshot {
                revision: 0,
                active_workspace: 1,
                workspaces: vec![Workspace {
                    id: 1,
                    name: "Main".into(),
                    center: [0.0, 0.0],
                    zoom: 1.0,
                    view: None,
                }],
                placements: vec![],
                next_offset: None,
            },
            vec![],
        )
        .unwrap()
    }
    fn mutate(state: &State, mutation: Mutation) -> Request {
        Request::Mutate {
            request_id: crate::new_uid("request"),
            expected_revision: state.snapshot.revision,
            mutation,
        }
    }
    #[test]
    fn crud_persists_receipts_rejects_stale_edits_and_preserves_binding() {
        let mut state = state();
        let placement = crate::new_uid("placement");
        let record = crate::new_uid("r");
        let add = mutate(
            &state,
            Mutation::Add {
                placement: placement.clone(),
                workspace: 1,
                component: Component::Builtin {
                    state: ComponentState::Record {
                        record: record.clone(),
                        mode: Default::default(),
                        start_call: None,
                    },
                },
                geometry: Geometry {
                    position: [1.0, 2.0],
                    size: [100.0, 100.0],
                },
            },
        );
        let first = state.handle(&add).unwrap();
        assert_eq!(state.handle(&add).unwrap(), first);
        assert_eq!(state.snapshot.placements.len(), 1);
        let mut restored: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.handle(&add).unwrap(), first);
        let stale = Request::Mutate {
            request_id: crate::new_uid("request"),
            expected_revision: 0,
            mutation: Mutation::Remove {
                placement: placement.clone(),
            },
        };
        assert!(restored.handle(&stale).is_err());
        let request = mutate(
            &restored,
            Mutation::Move {
                placement: placement.clone(),
                workspace: 1,
                position: [9.0, 9.0],
            },
        );
        restored.handle(&request).unwrap();
        assert_eq!(
            restored.snapshot.placements[0].component.records(),
            vec![record.as_str()]
        );
        let request = mutate(
            &restored,
            Mutation::Remove {
                placement: placement.clone(),
            },
        );
        let Response::Receipt { receipt } = restored.handle(&request).unwrap() else {
            panic!("receipt");
        };
        assert!(restored.snapshot.placements.is_empty());
        let undo = mutate(
            &restored,
            Mutation::Undo {
                receipt: receipt.request_id,
            },
        );
        restored.handle(&undo).unwrap();
        assert_eq!(restored.snapshot.placements[0].id, placement);
    }
    #[test]
    fn removing_a_workspace_removes_contents_and_keeps_last_workspace() {
        let mut state = state();
        let create = mutate(
            &state,
            Mutation::CreateWorkspace {
                name: "Second".into(),
            },
        );
        state.handle(&create).unwrap();
        let rename = mutate(
            &state,
            Mutation::RenameWorkspace {
                workspace: 1,
                name: "Renamed inactive".into(),
            },
        );
        state.handle(&rename).unwrap();
        assert_eq!(state.snapshot.workspaces[0].name, "Renamed inactive");
        let remove = mutate(&state, Mutation::RemoveWorkspace { workspace: 2 });
        state.handle(&remove).unwrap();
        assert_eq!(state.snapshot.active_workspace, 1);
        let remove = mutate(&state, Mutation::RemoveWorkspace { workspace: 1 });
        assert!(state.handle(&remove).is_err());
    }
    #[test]
    fn generated_compositions_cannot_be_reconfigured_to_drop_protection() {
        let mut state = state();
        let id = crate::new_uid("placement");
        let component = Component::Builtin {
            state: ComponentState::Composition {
                composition: crate::component::Composition {
                    name: "Protected".into(),
                    origin: Some(crate::component::composition::Origin {
                        agent: crate::new_uid("r"),
                        thread: crate::new_uid("r"),
                    }),
                    parts: vec![crate::component::Part {
                        settings: Default::default(),
                        id: "text".into(),
                        position: [0, 0],
                        size: [100, 100],
                        component: ComponentState::Text {
                            text: "Inside".into(),
                        },
                        events: vec![],
                    }],
                },
            },
        };
        state
            .handle(&mutate(
                &state,
                Mutation::Add {
                    placement: id.clone(),
                    workspace: 1,
                    component,
                    geometry: Geometry {
                        position: [0.0, 0.0],
                        size: [100.0, 100.0],
                    },
                },
            ))
            .unwrap();
        let change = mutate(
            &state,
            Mutation::Configure {
                placement: id,
                component: Component::Builtin {
                    state: ComponentState::Text {
                        text: "Unprotected".into(),
                    },
                },
            },
        );
        assert!(state.handle(&change).is_err());
    }
    #[test]
    fn populated_canvas_has_bounded_cost_and_never_replays_evicted_requests() {
        let mut state = state();
        let mut first = None;
        for index in 0..MAX_PLACEMENTS {
            let request = mutate(
                &state,
                Mutation::Add {
                    placement: crate::new_uid("placement"),
                    workspace: 1,
                    component: Component::Builtin {
                        state: ComponentState::Text {
                            text: format!("Panel {index}"),
                        },
                    },
                    geometry: Geometry {
                        position: [index as f64, 0.0],
                        size: [100.0, 100.0],
                    },
                },
            );
            if first.is_none() {
                first = Some(request.clone());
            }
            state.handle(&request).unwrap();
        }
        assert_eq!(state.receipts.len(), 16);
        state.validate().unwrap();
        assert!(state.handle(&first.unwrap()).is_err());
        let before = state.snapshot.clone();
        let request = mutate(
            &state,
            Mutation::Add {
                placement: state.snapshot.placements[0].id.clone(),
                workspace: 1,
                component: Component::Builtin {
                    state: ComponentState::Text {
                        text: "Duplicate".into(),
                    },
                },
                geometry: Geometry {
                    position: [0.0, 0.0],
                    size: [100.0, 100.0],
                },
            },
        );
        assert!(state.handle(&request).is_err());
        assert_eq!(state.snapshot, before);
    }
    #[test]
    fn inspection_pages_do_not_limit_the_whole_canvas_to_one_page() {
        let mut state = state();
        for _ in 0..300 {
            state.snapshot.placements.push(Placement {
                id: crate::new_uid("placement"),
                workspace: 1,
                component: Component::Builtin {
                    state: ComponentState::Text {
                        text: "Existing panel".into(),
                    },
                },
                geometry: Geometry {
                    position: [0.0, 0.0],
                    size: [100.0, 100.0],
                },
                selected: false,
                group: None,
                metadata: Default::default(),
            });
        }
        state.validate().unwrap();
        let response = state
            .handle(&Request::Inspect {
                workspace: None,
                selected_only: false,
                offset: 0,
                limit: 256,
            })
            .unwrap();
        let Response::Snapshot { snapshot } = response else {
            panic!("snapshot");
        };
        assert_eq!(snapshot.placements.len(), 256);
        assert_eq!(snapshot.next_offset, Some(256));
        let remove = mutate(
            &state,
            Mutation::Remove {
                placement: state.snapshot.placements[299].id.clone(),
            },
        );
        state.handle(&remove).unwrap();
        assert_eq!(state.snapshot.placements.len(), 299);
        state
            .handle(&mutate(
                &state,
                Mutation::CreateWorkspace {
                    name: "Destination".into(),
                },
            ))
            .unwrap();
        let response = state
            .handle(&mutate(&state, Mutation::RemoveWorkspace { workspace: 1 }))
            .unwrap();
        let Response::Receipt { receipt } = response else {
            panic!("receipt");
        };
        assert_eq!(receipt.affected_count, 299);
        assert_eq!(receipt.affected_placements.len(), 256);
        assert!(state.snapshot.placements.is_empty());
    }
    #[test]
    fn deleted_workspace_identifiers_are_never_reassigned() {
        let mut state = state();
        state
            .handle(&mutate(
                &state,
                Mutation::CreateWorkspace { name: "Old".into() },
            ))
            .unwrap();
        state
            .handle(&mutate(&state, Mutation::RemoveWorkspace { workspace: 2 }))
            .unwrap();
        let mut restored: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        restored.validate().unwrap();
        restored
            .handle(&mutate(
                &restored,
                Mutation::CreateWorkspace { name: "New".into() },
            ))
            .unwrap();
        assert_eq!(restored.snapshot.active_workspace, 3);
        assert!(
            restored
                .handle(&mutate(
                    &restored,
                    Mutation::SwitchWorkspace { workspace: 2 }
                ))
                .is_err()
        );
    }
}
