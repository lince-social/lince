use super::*;
use crate::layout::desktop::{DropSlot, TransferKind, Workspace as Model};
use crate::native::workspace::allocation::{self, Numbered};

mod projection;

pub struct Prepared {
    root: Entity,
    destination: u64,
    allocation: Option<Numbered>,
    movement: Option<Movement>,
}

struct Member {
    entity: Entity,
    layout: LayoutBox,
    parent: Option<Entity>,
    registration: Option<Geometry>,
    proposed_rules: Option<crate::layout::Rules>,
    edit_geometry: Option<Geometry>,
}

struct Movement {
    active: u64,
    source: u64,
    original_source: Model,
    original_destination: Option<Model>,
    source_model: Model,
    destination_model: Model,
    members: Vec<Member>,
    selected: Entity,
    input: Option<Entity>,
}

pub struct Selection {
    root: Entity,
    workspace: u64,
    target: Entity,
    input: Option<Entity>,
}

pub(super) fn contains(world: &World, mut input: Entity, target: Entity) -> bool {
    for _ in 0..super::super::MAX_DEPTH {
        if input == target {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(input) else {
            return false;
        };
        input = parent.parent();
    }
    false
}

fn layout_error(error: crate::layout::desktop::LayoutError) -> String {
    use crate::layout::desktop::LayoutError;
    match error {
        LayoutError::DuplicateItem => "The destination already contains this placement.",
        LayoutError::TooManyItems => "The destination has reached its item limit.",
        LayoutError::MissingItem => "The window is no longer managed.",
        LayoutError::InvalidItem => "The window has invalid layout data.",
        LayoutError::InvalidState => "The workspace layout cannot accept this move.",
    }
    .into()
}

fn observed_geometry(world: &World, entity: Entity) -> Option<Geometry> {
    let item = world.get::<CanvasItem>(entity)?;
    Some(Geometry {
        origin: (item.position - item.size.as_dvec2() * 0.5).to_array(),
        size: item.size.as_dvec2().to_array(),
    })
}

pub(super) fn register_unmanaged(
    world: &World,
    model: &mut Model,
    entity: Entity,
) -> Result<Option<Geometry>, String> {
    let layout = world
        .get::<LayoutBox>(entity)
        .ok_or("The item has no managed placement.")?;
    let id = PlacementId(layout.id);
    if model.items().iter().any(|item| item.id == id) {
        return Ok(None);
    }
    if model.mode() != WorkspaceLayout::Free
        || world.get::<Managed>(entity).is_some()
        || world.get::<ApplicationWindow>(entity).is_some()
        || !candidate(world, entity)
    {
        return Err(layout_error(
            crate::layout::desktop::LayoutError::MissingItem,
        ));
    }
    let geometry = observed_geometry(world, entity).ok_or("The item's geometry is unavailable.")?;
    model
        .insert_inactive(Item {
            id,
            kind: ItemKind::Native,
            rules: layout.rules,
            free: geometry,
            free_width: None,
            floating: world.get::<TransientParent>(entity).is_some(),
        })
        .map_err(layout_error)?;
    Ok(Some(geometry))
}

pub(super) fn owning_item(
    world: &World,
    root: Entity,
    source: u64,
    selected: Entity,
) -> Result<Entity, String> {
    if !focus::eligible(world, root, source, selected) {
        return Err("Focus a window or item before moving it.".into());
    }
    let mut owner = selected;
    let mut visited = BTreeSet::new();
    while let Some(parent) = world.get::<TransientParent>(owner) {
        if !visited.insert(owner) || visited.len() > super::super::MAX_DEPTH {
            return Err("The dialog's parent relationship is invalid.".into());
        }
        owner = parent.0;
        if !focus::eligible(world, root, source, owner) {
            return Err("The dialog's parent is unavailable.".into());
        }
    }
    Ok(owner)
}

pub(super) fn row_available(world: &World, root: Entity, source: u64, selected: Entity) -> bool {
    let Some(id) = owning_item(world, root, source, selected)
        .ok()
        .and_then(|entity| world.get::<LayoutBox>(entity))
        .map(|layout| PlacementId(layout.id))
    else {
        return false;
    };
    world
        .get::<Workspaces>(root)
        .and_then(|spaces| spaces.entries.iter().find(|entry| entry.id == source))
        .is_some_and(|workspace| {
            workspace.desktop.mode() == WorkspaceLayout::Tiled
                && workspace
                    .desktop
                    .items()
                    .iter()
                    .any(|item| item.id == id && !item.floating)
        })
}

fn movement(
    world: &mut World,
    root: Entity,
    destination: u64,
    destination_model: Model,
    kind: TransferKind,
) -> Result<Option<Movement>, String> {
    let source = world
        .get::<Workspaces>(root)
        .ok_or("The workspace is unavailable.")?
        .active;
    let selected = focused_entity(world, root)
        .filter(|entity| focus::eligible(world, root, source, *entity))
        .ok_or("Focus a window or item before moving it.")?;
    movement_for(
        world,
        root,
        (source, selected),
        destination,
        destination_model,
        kind,
        None,
        false,
    )
}

fn movement_for(
    world: &mut World,
    root: Entity,
    captured: (u64, Entity),
    destination: u64,
    destination_model: Model,
    kind: TransferKind,
    slot: Option<DropSlot>,
    scratch: bool,
) -> Result<Option<Movement>, String> {
    let (source, selected) = captured;
    let spaces = world
        .get::<Workspaces>(root)
        .ok_or("The workspace is unavailable.")?;
    let active = spaces.active;
    let original_source = spaces
        .entries
        .iter()
        .find(|entry| entry.id == source)
        .ok_or("The source workspace is unavailable.")?
        .desktop
        .clone();
    let original_destination = spaces
        .entries
        .iter()
        .find(|entry| entry.id == destination)
        .map(|entry| entry.desktop.clone());
    if source == destination && slot.is_none() && !scratch {
        return Ok(None);
    }
    let owner = if scratch {
        let placement = world.get::<LayoutBox>(selected).map(|layout| PlacementId(layout.id));
        if !candidate(world, selected)
            || world.get::<TransientParent>(selected).is_some()
            || world.get::<ChildOf>(selected).map(ChildOf::parent) != Some(root)
            || !placement.is_some_and(|id| original_source.scratch(id).is_some())
        { return Err("The captured scratch item is unavailable.".into()); }
        selected
    } else { owning_item(world, root, source, selected)? };
    let id = PlacementId(
        world
            .get::<LayoutBox>(owner)
            .ok_or("The item has no managed placement.")?
            .id,
    );
    let rows: Vec<_> = world
        .query::<(Entity, &LayoutBox, &WorkspaceMember, &ChildOf)>()
        .iter(world)
        .filter(|(_, _, member, parent)| parent.parent() == root && member.0 == source)
        .map(|(entity, layout, _, _)| (PlacementId(layout.id), entity))
        .collect();
    let mut entities = BTreeMap::new();
    for (id, entity) in rows {
        if entities.insert(id, entity).is_some() {
            return Err("The source contains duplicate placement identities.".into());
        }
    }
    let mut source_model = original_source.clone();
    let mut destination_model = destination_model;
    let mut registrations = BTreeMap::new();
    if source != destination && kind == TransferKind::Item {
        for entity in entities
            .values()
            .copied()
            .filter(|entity| *entity == owner || transients::descendant(world, *entity, owner))
        {
            if let Some(geometry) = register_unmanaged(world, &mut source_model, entity)? {
                registrations.insert(entity, geometry);
            }
        }
    }
    let mut ids = if source == destination {
        if let Some(slot) = slot {
            if !destination_model.place_near(id, slot).map_err(layout_error)? { return Ok(None); }
        }
        source_model = destination_model.clone();
        vec![id]
    } else {
        let ids = source_model
            .transfer_to(&mut destination_model, id, kind)
            .map_err(layout_error)?;
        if let Some(slot) = slot {
            destination_model
                .place_near(id, slot)
                .map_err(layout_error)?;
        }
        ids
    };
    let owners: Vec<_> = ids
        .iter()
        .map(|id| {
            entities
                .get(id)
                .copied()
                .ok_or("A window closed before the move was ready.")
        })
        .collect::<Result<_, _>>()?;
    let descendants: Vec<_> = entities
        .iter()
        .filter(|(id, entity)| {
            !ids.contains(id)
                && owners
                    .iter()
                    .any(|owner| transients::descendant(world, **entity, *owner))
        })
        .map(|(id, _)| *id)
        .collect();
    let fullscreen = original_source
        .fullscreen()
        .filter(|id| ids.contains(id) || descendants.contains(id));
    for id in descendants {
        if source != destination {
            source_model
                .transfer_to(&mut destination_model, id, TransferKind::Item)
                .map_err(layout_error)?;
        }
        ids.push(id);
    }
    if let Some(id) = fullscreen {
        destination_model.request_state(
            id,
            crate::host::WindowRequest {
                fullscreen: Some(true),
                ..default()
            },
        );
    }
    if source != destination
        && world
            .query::<(&LayoutBox, &WorkspaceMember, &ChildOf)>()
            .iter(world)
            .any(|(layout, member, parent)| {
                parent.parent() == root
                    && member.0 == destination
                    && ids.contains(&PlacementId(layout.id))
            })
    {
        return Err("The destination already contains this placement.".into());
    }
    let members: Vec<_> = ids
        .iter()
        .map(|id| {
            let entity = *entities
                .get(id)
                .ok_or("A window closed before the move was ready.")?;
            if !candidate(world, entity) {
                return Err("The item's placement changed; try the move again.");
            }
            Ok(Member {
                proposed_rules: None,
                edit_geometry: None,
                entity,
                layout: *world.get::<LayoutBox>(entity).unwrap(),
                parent: world.get::<TransientParent>(entity).map(|parent| parent.0),
                registration: registrations.get(&entity).copied(),
            })
        })
        .collect::<Result<_, _>>()?;
    if !members.iter().any(|member| member.entity == selected) {
        return Err("The focused window changed; try the move again.".into());
    }
    let input = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get())
        .filter(|input| contains(world, *input, selected));
    Ok(Some(Movement {
        active,
        source,
        original_source,
        original_destination,
        source_model,
        destination_model,
        members,
        selected,
        input,
    }))
}

pub fn prepare_identity(world: &World, root: Entity, destination: u64) -> Result<Prepared, String> {
    if !world
        .get::<Workspaces>(root)
        .is_some_and(|spaces| spaces.entries.iter().any(|entry| entry.id == destination))
    {
        return Err("The destination is unavailable.".into());
    }
    Ok(Prepared {
        root,
        destination,
        allocation: None,
        movement: None,
    })
}

pub fn prepare_creation(world: &World, root: Entity) -> Result<Prepared, String> {
    let allocation = allocation::new(world, root, 1)?;
    Ok(Prepared {
        root,
        destination: allocation.id(),
        allocation: Some(allocation),
        movement: None,
    })
}

pub fn prepare_selection(world: &World, root: Entity, number: u32) -> Result<Prepared, String> {
    let allocation = allocation::numbered(world, root, number)?;
    let destination = crate::native::workspace::navigation::numeric_destination(world, root, allocation.id());
    if destination != allocation.id() {
        return prepare_identity(world, root, destination);
    }
    Ok(Prepared {
        root,
        destination: allocation.id(),
        allocation: Some(allocation),
        movement: None,
    })
}

pub fn prepare_numbered_move(
    world: &mut World,
    root: Entity,
    number: u32,
    kind: TransferKind,
) -> Result<Prepared, String> {
    let allocation = allocation::numbered(world, root, number)?;
    let destination = allocation.id();
    let model = allocation
        .target(world, root)
        .ok_or("The destination is unavailable.")?
        .desktop
        .clone();
    let movement = movement(world, root, destination, model, kind)?;
    Ok(Prepared {
        root,
        destination,
        allocation: Some(allocation),
        movement,
    })
}

pub fn prepare_move(
    world: &mut World,
    root: Entity,
    destination: u64,
    kind: TransferKind,
) -> Result<Prepared, String> {
    let model = world
        .get::<Workspaces>(root)
        .and_then(|spaces| spaces.entries.iter().find(|entry| entry.id == destination))
        .ok_or("The destination is unavailable.")?
        .desktop
        .clone();
    let movement = movement(world, root, destination, model, kind)?;
    Ok(Prepared {
        root,
        destination,
        allocation: None,
        movement,
    })
}

pub fn prepare_captured(
    world: &mut World,
    root: Entity,
    source: u64,
    placement: PlacementId,
    destination: u64,
    slot: Option<DropSlot>,
) -> Result<Prepared, String> {
    prepare_target(world, root, source, placement, destination, slot, false)
}

pub(super) fn prepare_scratch(
    world: &mut World,
    root: Entity,
    source: u64,
    placement: PlacementId,
    destination: u64,
) -> Result<Prepared, String> {
    prepare_target(world, root, source, placement, destination, None, true)
}

fn prepare_target(
    world: &mut World,
    root: Entity,
    source: u64,
    placement: PlacementId,
    destination: u64,
    slot: Option<DropSlot>,
    scratch: bool,
) -> Result<Prepared, String> {
    if crate::native::laboratory::temporary(world, root, source)
        || crate::native::laboratory::temporary(world, root, destination)
    {
        return Err("Temporary laboratory workspaces cannot receive this move.".into());
    }
    let selected = world
        .get::<Children>(root)
        .and_then(|children| {
            children.iter().find(|entity| {
                world
                    .get::<LayoutBox>(*entity)
                    .is_some_and(|layout| layout.id == placement.0)
                    && world
                        .get::<WorkspaceMember>(*entity)
                        .is_some_and(|member| member.0 == source)
            })
        })
        .ok_or("The captured item is no longer available in its source workspace.")?;
    let model = world
        .get::<Workspaces>(root)
        .and_then(|spaces| spaces.entries.iter().find(|entry| entry.id == destination))
        .ok_or("The destination is unavailable.")?
        .desktop
        .clone();
    let movement = movement_for(
        world,
        root,
        (source, selected),
        destination,
        model,
        TransferKind::Item,
        slot,
        scratch,
    )?;
    Ok(Prepared {
        root,
        destination,
        allocation: None,
        movement,
    })
}

impl Prepared {
    pub(super) fn preserve_destination_selection(mut self) -> Self {
        if let Some(movement) = &mut self.movement {
            if let Some(previous) = &movement.original_destination { movement.destination_model.preserve_selection(previous); }
            if movement.source == self.destination { movement.source_model = movement.destination_model.clone(); }
        }
        self
    }

    pub(crate) fn configure_item(
        mut self,
        world: &World,
        target: Entity,
        configure: impl FnOnce(&mut Model, PlacementId, crate::layout::Rules) -> Result<Option<crate::layout::Rules>, String>,
    ) -> Result<Self, String> {
        let layout = *world.get::<LayoutBox>(target).ok_or("The application placement is unavailable.")?;
        let geometry = observed_geometry(world, target).ok_or("The application geometry is unavailable.")?;
        if self.movement.is_none() {
            let source = world.get::<WorkspaceMember>(target).ok_or("The source workspace is unavailable.")?.0;
            if source != self.destination || world.get::<ChildOf>(target).map(ChildOf::parent) != Some(self.root) {
                return Err("The application moved before its opening preferences could be applied.".into());
            }
            let spaces = world.get::<Workspaces>(self.root).ok_or("The canvas is unavailable.")?;
            let model = spaces.entries.iter().find(|space| space.id == source).ok_or("The source workspace is unavailable.")?.desktop.clone();
            self.movement = Some(Movement {
                active: spaces.active,
                source,
                original_source: model.clone(),
                original_destination: Some(model.clone()),
                source_model: model.clone(),
                destination_model: model,
                members: vec![Member { entity: target, layout, parent: world.get::<TransientParent>(target).map(|parent| parent.0), registration: None, proposed_rules: None, edit_geometry: Some(geometry) }],
                selected: target,
                input: None,
            });
        }
        let movement = self.movement.as_mut().unwrap();
        let rules = configure(&mut movement.destination_model, PlacementId(layout.id), layout.rules)?;
        if !movement.destination_model.valid() { return Err("The opening preferences produce invalid placement data.".into()); }
        let member = movement.members.iter_mut().find(|member| member.entity == target).ok_or("The captured application is not part of this transfer.")?;
        member.proposed_rules = rules;
        member.edit_geometry = Some(geometry);
        if movement.source == self.destination { movement.source_model = movement.destination_model.clone(); }
        Ok(self)
    }

    pub fn destination(&self) -> u64 {
        self.destination
    }

    pub fn commit(self, world: &mut World) -> Result<Option<Selection>, String> {
        if self.allocation.is_none()
            && !world.get::<Workspaces>(self.root).is_some_and(|spaces| {
                spaces
                    .entries
                    .iter()
                    .any(|entry| entry.id == self.destination)
            })
        {
            return Err("The destination is unavailable.".into());
        }
        if let Some(movement) = &self.movement {
            let spaces = world
                .get::<Workspaces>(self.root)
                .ok_or("The workspace is unavailable.")?;
            if spaces.active != movement.active
                || spaces
                    .entries
                    .iter()
                    .find(|entry| entry.id == movement.source)
                    .map(|entry| &entry.desktop)
                    != Some(&movement.original_source)
                || spaces
                    .entries
                    .iter()
                    .find(|entry| entry.id == self.destination)
                    .map(|entry| &entry.desktop)
                    != movement.original_destination.as_ref()
                || movement.members.iter().any(|member| {
                    world.get::<LayoutBox>(member.entity) != Some(&member.layout)
                        || world
                            .get::<WorkspaceMember>(member.entity)
                            .map(|member| member.0)
                            != Some(movement.source)
                        || world.get::<ChildOf>(member.entity).map(ChildOf::parent)
                            != Some(self.root)
                        || world
                            .get::<TransientParent>(member.entity)
                            .map(|parent| parent.0)
                            != member.parent
                        || member.registration.is_some_and(|geometry| {
                            observed_geometry(world, member.entity) != Some(geometry)
                        })
                        || member.edit_geometry.is_some_and(|geometry| observed_geometry(world, member.entity) != Some(geometry))
                })
            {
                return Err("The windows or workspaces changed; try the move again.".into());
            }
        }
        if let Some(allocation) = self.allocation {
            allocation.commit(world, self.root)?;
        }
        let Some(movement) = self.movement else {
            return Ok(None);
        };
        {
            let mut spaces = world.get_mut::<Workspaces>(self.root).unwrap();
            spaces
                .entries
                .iter_mut()
                .find(|entry| entry.id == movement.source)
                .unwrap()
                .desktop = movement.source_model;
            if movement.source != self.destination {
                spaces
                    .entries
                    .iter_mut()
                    .find(|entry| entry.id == self.destination)
                    .unwrap()
                    .desktop = movement.destination_model;
            }
        }
        for member in movement.members {
            if let Some(rules) = member.proposed_rules {
                world.get_mut::<LayoutBox>(member.entity).unwrap().rules = rules;
            }
            world
                .entity_mut(member.entity)
                .insert(WorkspaceMember(self.destination));
        }
        if let Some(mut runtime) = world.get_resource_mut::<Runtime>() {
            for workspace in [movement.source, self.destination] {
                runtime.views.remove(&(self.root, workspace));
                runtime.influences.remove(&(self.root, workspace));
                runtime.entering.remove(&(self.root, workspace));
                runtime.restoring_free.remove(&(self.root, workspace));
            }
        }
        projection::refresh(world, self.root, movement.source);
        if movement.source != self.destination {
            projection::refresh(world, self.root, self.destination);
        }
        if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
            wake.ring();
        }
        Ok(Some(Selection {
            root: self.root,
            workspace: self.destination,
            target: movement.selected,
            input: movement.input,
        }))
    }
}

impl Selection {
    pub fn activate(self, world: &mut World) -> bool {
        if !focus::eligible(world, self.root, self.workspace, self.target) {
            return false;
        }
        world.init_resource::<bevy::input_focus::InputFocus>();
        crate::native::workspace::switch_output(world, self.root, self.workspace);
        focus::select(world, self.root, self.workspace, self.target, true);
        if let Some(input) = self.input.filter(|input| {
            world.get_entity(*input).is_ok() && contains(world, *input, self.target)
        }) {
            world
                .resource_mut::<bevy::input_focus::InputFocus>()
                .set(input, bevy::input_focus::FocusCause::Navigated);
        }
        true
    }
}

pub fn workspace_action(
    world: &mut World,
    root: Entity,
    action: DesktopAction,
) -> Result<bool, String> {
    let previous = crate::native::workspace::navigation::capture(world, root);
    let prepared = match action {
        DesktopAction::PreviousWorkspace | DesktopAction::NextWorkspace => {
            let offset = if action == DesktopAction::PreviousWorkspace {
                -1
            } else {
                1
            };
            let destination =
                crate::native::workspace::navigation::destination(world, root, offset)?;
            prepare_identity(world, root, destination)?
        }
        DesktopAction::MoveWorkspaceEarlier | DesktopAction::MoveWorkspaceLater => {
            let offset = if action == DesktopAction::MoveWorkspaceEarlier {
                -1
            } else {
                1
            };
            crate::native::workspace::navigation::reorder(world, root, offset)?;
            return Ok(true);
        }
        DesktopAction::Workspace(number) => prepare_selection(world, root, number)?,
        DesktopAction::MoveToWorkspace(number) => {
            prepare_numbered_move(world, root, number, TransferKind::Item)?
        }
        _ => return Ok(false),
    };
    let destination = prepared.destination();
    if let Some(selection) = prepared.commit(world)? {
        selection.activate(world);
    } else {
        world.init_resource::<bevy::input_focus::InputFocus>();
        crate::native::workspace::switch(world, root, destination);
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    if let Some(previous) = previous {
        previous.remember(world);
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
