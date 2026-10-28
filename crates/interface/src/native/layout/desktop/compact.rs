use super::*;
use crate::layout::desktop::{CompactGroup, Workspace as Model};
use crate::native::{actions::Action, full_record::window::Metadata, protein_area::RecordBinding};

mod chooser;
mod properties;
mod strip;
pub(crate) use properties::{item_panel, window_panel};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    root: Entity,
    canvas: String,
    workspace: u64,
    output: Option<OutputId>,
    entity: Entity,
    placement: PlacementId,
    surface: Option<SurfaceId>,
    record: Option<String>,
    inspector: Option<(Entity, String)>,
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(PostUpdate, (chooser::refresh, properties::refresh)
        .after(crate::native::actions::ApplyActions).before(bevy::ui::UiSystems::Layout))
        .add_systems(PostUpdate, strip::refresh.after(super::super::ResolveFollow)
            .after(crate::native::topology::presentation::synchronize)
            .before(bevy::camera::CameraUpdateSystems).before(bevy::ui::UiSystems::Prepare));
}

fn capture(world: &World, root: Entity, entity: Entity, hidden: bool) -> Result<Target, &'static str> {
    if world.get_resource::<crate::host::input::InputActivity>().is_some_and(|activity| !activity.active()) {
        return Err("The desktop view is inactive or locked.");
    }
    let spaces = world.get::<Workspaces>(root).ok_or("The original workspace is unavailable.")?;
    let workspace = world.get::<WorkspaceMember>(entity).filter(|member| member.0 == spaces.active)
        .ok_or("The original item is no longer in the active workspace.")?.0;
    if world.get::<ChildOf>(entity).is_none_or(|parent| parent.parent() != root)
        || !candidate(world, entity) || world.get::<TransientParent>(entity).is_some()
        || world.get::<bevy::ecs::entity_disabling::Disabled>(entity).is_some()
        || world.get::<crate::native::castle::Pending>(entity).is_some()
        || crate::native::inspection::excluded(world, entity)
        || world.get::<CanvasItem>(entity).is_none_or(|item| !item.position.is_finite() || !item.size.is_finite() || item.size.min_element() <= 0.0)
    { return Err("Select an eligible item, rather than a dialog or editor control."); }
    let placement = world.get::<Managed>(entity).ok_or("The original item is not managed yet.")?.id;
    if world.get::<LayoutBox>(entity).is_none_or(|layout| layout.id != placement.0) { return Err("The original placement changed."); }
    let model = &spaces.entries.iter().find(|space| space.id == workspace).ok_or("The original workspace is unavailable.")?.desktop;
    if !hidden && model.hidden(placement)
        || !model.items().iter().any(|item| item.id == placement)
    { return Err("The original item is hidden, minimized or unavailable."); }
    if model.fullscreen().is_some_and(|full| full != placement && model.compact_group(placement).is_none_or(|group| !group.members.contains(&full))) {
        return Err("The original item is hidden by fullscreen presentation.");
    }
    Ok(Target {
        root, canvas: spaces.canvas_id.clone(), workspace,
        output: world.get::<HostViewport>(root).map(|viewport| viewport.0.id), entity, placement,
        surface: world.get::<ApplicationWindow>(entity).map(|window| window.surface),
        record: world.get::<RecordBinding>(entity).map(|binding| binding.uid.clone()), inspector: None,
    })
}

fn model<'a>(world: &'a World, target: &Target, hidden: bool) -> Result<&'a Model, &'static str> {
    let current = capture(world, target.root, target.entity, hidden)?;
    if current.canvas != target.canvas || current.workspace != target.workspace || current.output != target.output
        || current.placement != target.placement || current.surface != target.surface
        || target.record.as_ref().is_some_and(|record| current.record.as_ref() != Some(record))
    { return Err("The original item, canvas, output or Record association changed."); }
    if let Some((inspector, uid)) = &target.inspector
        && world.get::<Metadata>(*inspector).is_none_or(|metadata| metadata.uid != *uid || metadata.placement != target.placement.0 || !metadata.live)
    { return Err("The original window association is closed or changed."); }
    if let Some((_, uid)) = &target.inspector
        && world.get::<RecordBinding>(target.entity).is_none_or(|binding| binding.uid != *uid
            || binding.area != target.entity || binding.source != crate::native::protein_area::Source::Local)
    { return Err("The original window Record association changed."); }
    Ok(&world.get::<Workspaces>(target.root).unwrap().entries.iter().find(|space| space.id == target.workspace).unwrap().desktop)
}

pub(super) fn groupable(world: &World, root: Entity, entity: Entity) -> bool {
    capture(world, root, entity, false).ok().is_some_and(|target| model(world, &target, false).is_ok_and(|model| {
        model.mode() == WorkspaceLayout::Tiled && model.fullscreen().is_none()
            && model.compact_group(target.placement).is_none()
            && model.scratch(target.placement).is_none()
            && model.items().iter().any(|item| item.id == target.placement && !item.floating)
    }))
}

fn candidates(world: &World, target: &Target) -> Vec<Target> {
    world.get::<Children>(target.root).into_iter().flat_map(|children| children.iter())
        .filter(|entity| *entity != target.entity && groupable(world, target.root, *entity))
        .filter_map(|entity| capture(world, target.root, entity, false).ok()).collect()
}

pub(super) fn has_candidates(world: &World, root: Entity, entity: Entity) -> bool {
    capture(world, root, entity, false).ok().is_some_and(|target| world.get::<Children>(root).is_some_and(|children|
        children.iter().any(|entity| entity != target.entity && groupable(world, root, entity))))
}

fn unavailable(world: &World, target: &Target, action: DesktopAction) -> Option<&'static str> {
    if busy(world) { return Some("Finish the current drag before changing compact membership or selection."); }
    let model = match model(world, target, false) { Ok(model) => model, Err(reason) => return Some(reason) };
    let grouped = model.compact_group(target.placement).is_some();
    match action {
        DesktopAction::DissolveCompactGroup | DesktopAction::DetachCompactMember if grouped => None,
        _ if model.mode() != WorkspaceLayout::Tiled => Some("Compact presentation is available while the workspace is tiled."),
        DesktopAction::CreateCompactGroup if !groupable(world, target.root, target.entity) => Some("Choose an eligible non-floating item outside a compact group."),
        DesktopAction::CreateCompactGroup | DesktopAction::AddCompactMembers if model.fullscreen().is_some() => Some("Leave fullscreen before changing compact membership."),
        DesktopAction::CreateCompactGroup | DesktopAction::AddCompactMembers if !has_candidates(world, target.root, target.entity) => Some("There are no other eligible items in this workspace."),
        DesktopAction::CreateCompactGroup => None,
        _ if !grouped => Some("The original item is no longer in a compact group."),
        DesktopAction::MoveCompactMember(Direction::Up | Direction::Down) => Some("Compact order moves earlier or later."),
        _ => None,
    }
}

pub(super) fn busy(world: &World) -> bool {
    world.get_resource::<crate::host::input::DragActive>().is_some_and(|state| state.0) || overview::active_file_drag(world)
}

fn commit(world: &mut World, target: &Target, proposed: Model, selection: Option<PlacementId>) -> Result<(), &'static str> {
    let current = model(world, target, true)?;
    if !proposed.valid() { return Err("The compact arrangement is invalid; nothing was changed."); }
    if current == &proposed { return Ok(()); }
    if let Some(gate) = world.get_resource::<crate::host::capture::PolicyGate>() { gate.invalidate(); }
    if selection.is_some() && let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>() { focus.clear(); }
    world.get_mut::<Workspaces>(target.root).unwrap().entries.iter_mut().find(|space| space.id == target.workspace).unwrap().desktop = proposed;
    if let Some(mut runtime) = world.get_resource_mut::<Runtime>() {
        runtime.views.remove(&(target.root, target.workspace));
        runtime.influences.remove(&(target.root, target.workspace));
    }
    if let Some(selected) = selection.and_then(|id| world.get::<Children>(target.root).and_then(|children|
        children.iter().find(|entity| world.get::<Managed>(*entity).is_some_and(|managed| managed.id == id))))
    { focus_item(world, selected); }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
    Ok(())
}

fn apply_target(world: &mut World, target: &Target, action: DesktopAction) -> Result<(), &'static str> {
    if crate::native::shortcuts::keyboard_owned(world) { return Err("Finish the current dialog or key capture first."); }
    if let Some(reason) = unavailable(world, target, action) { return Err(reason); }
    if matches!(action, DesktopAction::CreateCompactGroup | DesktopAction::AddCompactMembers) {
        return chooser::open(world, target.clone(), action == DesktopAction::AddCompactMembers);
    }
    let mut proposed = model(world, target, false)?.clone();
    let group = proposed.compact_group(target.placement).cloned().ok_or("The compact group is unavailable.")?;
    let mut selected = None;
    match action {
        DesktopAction::NextCompactMember | DesktopAction::PreviousCompactMember => {
            let index = group.members.iter().position(|id| *id == group.active).unwrap();
            let next = if action == DesktopAction::NextCompactMember { (index + 1) % group.members.len() }
                else { (index + group.members.len() - 1) % group.members.len() };
            proposed.select_compact_member(group.members[next]).map_err(|_| "The selected member is unavailable.")?;
            selected = Some(group.members[next]);
        }
        DesktopAction::MoveCompactMember(direction) => { proposed.reorder_compact_member(target.placement, direction).map_err(|_| "This member cannot be reordered.")?; }
        DesktopAction::DetachCompactMember => proposed.detach_compact_member(target.placement).map_err(|_| "The member cannot be removed safely.")?,
        DesktopAction::DissolveCompactGroup => proposed.dissolve_compact_group(target.placement).map_err(|_| "The group cannot be restored safely.")?,
        _ => return Err("This command is not a compact-group action."),
    }
    commit(world, target, proposed, selected)
}

pub(crate) fn checked(world: &mut World, root: Entity, action: DesktopAction) -> Result<(), &'static str> {
    if let Some(reason) = CommandContext::new(world, root).unavailable(action) { return Err(reason); }
    let selected = focused_entity(world, root).ok_or("Focus an eligible item first.")?;
    let target = capture(world, root, selected, false)?;
    apply_target(world, &target, action)
}

pub(super) fn command(world: &mut World, root: Entity, action: DesktopAction) {
    if let Err(reason) = checked(world, root, action) { crate::native::notifications::report(world, "Compact group", reason); }
}

pub(super) fn activate(world: &mut World, root: Entity, entity: Entity) -> Result<(), &'static str> {
    let Some(id) = world.get::<Managed>(entity).map(|managed| managed.id) else { return Ok(()); };
    let Some(member) = world.get::<WorkspaceMember>(entity) else { return Ok(()); };
    let grouped = world.get::<Workspaces>(root).and_then(|spaces| spaces.entries.iter().find(|space| space.id == member.0))
        .is_some_and(|space| space.desktop.mode() == WorkspaceLayout::Tiled && space.desktop.compact_group(id).is_some());
    if !grouped { return Ok(()); }
    let target = capture(world, root, entity, true)?;
    let current = model(world, &target, true)?;
    let Some(group) = current.compact_group(target.placement).filter(|_| current.mode() == WorkspaceLayout::Tiled) else { return Ok(()); };
    if group.active == target.placement { return Ok(()); }
    if crate::native::shortcuts::keyboard_owned(world) { return Err("Finish the current dialog before activating another compact member."); }
    if busy(world) { return Err("Finish the current drag before activating another compact member."); }
    let mut proposed = current.clone();
    proposed.select_compact_member(target.placement).map_err(|_| "The requested compact member is unavailable.")?;
    commit(world, &target, proposed, Some(target.placement))
}

#[derive(Clone)]
enum Change {
    Action(Target, DesktopAction),
    Select(Target),
}

impl Action for Change {
    fn apply(&self, world: &mut World, owner: Entity) {
        let target = match self { Self::Action(target, _) | Self::Select(target) => target };
        if crate::native::shortcuts::keyboard_owned(world) || !crate::native::shortcuts::input_eligible(world, target.root, &target.canvas, owner) { return; }
        if busy(world) { crate::native::notifications::report(world, "Compact group", "Finish the current drag before selecting another compact member."); return; }
        let result = match self {
            Self::Action(target, action) => apply_target(world, target, *action),
            Self::Select(target) => (|| {
                let mut proposed = model(world, target, true)?.clone();
                proposed.select_compact_member(target.placement).map_err(|_| "The compact member is unavailable.")?;
                let active = proposed.compact_group(target.placement).unwrap().active;
                commit(world, target, proposed, Some(active))
            })(),
        };
        if let Err(reason) = result { crate::native::notifications::report(world, "Compact group", reason); }
    }
}

#[cfg(test)]
mod tests;
