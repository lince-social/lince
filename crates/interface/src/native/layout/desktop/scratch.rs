use super::*;
use crate::layout::desktop::Workspace as Model;
use crate::native::{full_record::window::Metadata, protein_area::RecordBinding};
use bevy::input_focus::{InputFocus, FocusCause};

mod chooser;
mod properties;
pub(crate) use properties::{item_panel, window_panel};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    root: Entity,
    canvas: String,
    workspace: u64,
    entity: Entity,
    placement: PlacementId,
    surface: Option<SurfaceId>,
    record: Option<String>,
    inspector: Option<(Entity, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Destination {
    root: Entity,
    canvas: String,
    workspace: u64,
    output: Option<OutputId>,
    epoch: u64,
}

struct Return {
    destination: Destination,
    previous: Option<Target>,
    input: Option<Entity>,
    following: bool,
    unfocused: bool,
}

#[derive(Resource, Default)]
struct Reveals(BTreeMap<(Entity, PlacementId), Return>);

#[derive(Resource, Default)]
struct Epoch(u64);

#[derive(Component)]
struct ApplicationOwner {
    placement: PlacementId,
    surface: SurfaceId,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Reveals>().init_resource::<Epoch>().add_systems(PostUpdate,
        (chooser::refresh, properties::refresh, retire)
            .after(crate::native::actions::ApplyActions).before(bevy::ui::UiSystems::Layout));
}

pub(super) fn cancel(world: &mut World) {
    world.init_resource::<Epoch>();
    {
        let mut epoch = world.resource_mut::<Epoch>();
        epoch.0 = epoch.0.checked_add(1).expect("scratch interaction epoch");
    }
    chooser::cancel(world);
}

fn guard(world: &World, modal: Option<Entity>) -> Result<(), String> {
    if world.get_resource::<crate::host::input::InputActivity>().is_some_and(|activity| !activity.active()) {
        return Err("Scratch controls are unavailable while the desktop view is inactive or locked.".into());
    }
    if world.get_resource::<crate::host::control::Status>().is_some_and(|status| status.available && (status.locked || !status.active)) {
        return Err("Scratch controls are unavailable while Park is locked or inactive.".into());
    }
    if compact::busy(world) { return Err("Finish the current drag before changing scratch items.".into()); }
    if crate::native::shortcuts::keyboard_owned(world)
        && !modal.is_some_and(|owner| world.get_resource::<crate::native::shortcuts::KeyboardModalOwner>().is_some_and(|state| state.0 == Some(owner)))
    { return Err("Finish the current dialog or key capture first.".into()); }
    Ok(())
}

fn destination(world: &World, root: Entity) -> Result<Destination, String> {
    let spaces = world.get::<Workspaces>(root).ok_or("The canvas is unavailable.")?;
    if !spaces.entries.iter().any(|space| space.id == spaces.active) { return Err("The active workspace is unavailable.".into()); }
    if crate::native::laboratory::temporary(world, root, spaces.active) { return Err("Scratch items cannot move into a temporary laboratory workspace.".into()); }
    Ok(Destination { root, canvas: spaces.canvas_id.clone(), workspace: spaces.active, output: world.get::<HostViewport>(root).map(|viewport| viewport.0.id), epoch: world.get_resource::<Epoch>().map_or(0, |epoch| epoch.0) })
}

fn current_destination(world: &World, captured: &Destination) -> Result<(), String> {
    if destination(world, captured.root)? != *captured { return Err("The destination canvas, workspace or output changed. Choose the scratch item again.".into()); }
    Ok(())
}

fn capture(world: &World, root: Entity, entity: Entity) -> Result<Target, String> {
    let spaces = world.get::<Workspaces>(root).ok_or("The original canvas is unavailable.")?;
    let workspace = world.get::<WorkspaceMember>(entity).ok_or("The original workspace is unavailable.")?.0;
    if !spaces.entries.iter().any(|space| space.id == workspace)
        || crate::native::laboratory::temporary(world, root, workspace)
        || world.get::<ChildOf>(entity).map(ChildOf::parent) != Some(root)
        || !candidate(world, entity) || world.get::<TransientParent>(entity).is_some()
        || world.get::<bevy::ecs::entity_disabling::Disabled>(entity).is_some()
        || world.get::<crate::native::castle::Pending>(entity).is_some()
        || world.get::<CanvasItem>(entity).is_none_or(|item| !item.position.is_finite() || !item.size.is_finite() || item.size.min_element() <= 0.0)
    { return Err("Choose an existing item, rather than a dialog, hidden editor or temporary item.".into()); }
    let placement = PlacementId(world.get::<LayoutBox>(entity).ok_or("The original placement is not ready.")?.id);
    if world.get::<Managed>(entity).is_some_and(|managed| managed.id != placement) { return Err("The original placement changed.".into()); }
    Ok(Target { root, canvas: spaces.canvas_id.clone(), workspace, entity, placement,
        surface: world.get::<ApplicationWindow>(entity).map(|window| window.surface),
        record: world.get::<RecordBinding>(entity).map(|binding| binding.uid.clone()), inspector: None })
}

fn model<'a>(world: &'a World, target: &Target) -> Result<&'a Model, String> {
    let current = capture(world, target.root, target.entity)?;
    if current.canvas != target.canvas || current.workspace != target.workspace || current.placement != target.placement
        || current.surface != target.surface || target.record.as_ref().is_some_and(|record| current.record.as_ref() != Some(record))
    { return Err("The original scratch item or its canvas, workspace or Record association changed.".into()); }
    if let Some((inspector, uid)) = &target.inspector
        && world.get::<Metadata>(*inspector).is_none_or(|metadata| !metadata.live || metadata.uid != *uid || metadata.placement != target.placement.0)
    { return Err("The original window association is closed or changed.".into()); }
    if let Some((_, uid)) = &target.inspector
        && world.get::<RecordBinding>(target.entity).is_none_or(|binding| binding.uid != *uid
            || binding.area != target.entity || binding.source != crate::native::protein_area::Source::Local)
    { return Err("The original window Record association changed.".into()); }
    let model = &world.get::<Workspaces>(target.root).unwrap().entries.iter().find(|space| space.id == target.workspace).unwrap().desktop;
    if model.scratch(target.placement).is_some() && target.surface.is_some()
        && world.get::<ApplicationOwner>(target.entity).is_none_or(|owner| owner.placement != target.placement || Some(owner.surface) != target.surface)
    { return Err("The original application lifetime ended. Its replacement is not a scratch item.".into()); }
    Ok(model)
}

fn selected(world: &World, root: Entity) -> Option<Entity> {
    let spaces = world.get::<Workspaces>(root)?;
    let model = &spaces.entries.iter().find(|space| space.id == spaces.active)?.desktop;
    world.get::<Children>(root)?.iter().find(|entity| world.get::<LayoutBox>(*entity).is_some_and(|layout| Some(PlacementId(layout.id)) == model.focused())
        && focus::eligible(world, root, spaces.active, *entity))
        .or_else(|| crate::native::canvas_selection::selected(world, root).into_iter().find(|entity| focus::eligible(world, root, spaces.active, *entity)))
}

pub(super) fn can_mark(world: &World, root: Entity) -> bool {
    selected(world, root).is_some_and(|entity| candidate(world, entity) && world.get::<TransientParent>(entity).is_none()
        && world.get::<LayoutBox>(entity).is_none_or(|layout| world.get::<Workspaces>(root).is_some_and(|spaces|
            spaces.entries.iter().find(|space| space.id == spaces.active).is_some_and(|space|
                space.desktop.compact_group(PlacementId(layout.id)).is_none() && space.desktop.scratch(PlacementId(layout.id)).is_none()))))
}

pub(super) fn marked(world: &World, root: Entity) -> bool {
    selected(world, root).and_then(|entity| capture(world, root, entity).ok()).is_some_and(|target|
        model(world, &target).is_ok_and(|model| model.scratch(target.placement).is_some()))
}

pub(super) fn has_items(world: &World, root: Entity) -> bool {
    world.get::<Children>(root).is_some_and(|children| children.iter().any(|entity| capture(world, root, entity).ok().is_some_and(|target|
        model(world, &target).is_ok_and(|model| model.scratch(target.placement).is_some()))))
}

fn candidates(world: &World, root: Entity) -> Vec<Target> {
    let mut choices = world.get::<Children>(root).into_iter().flat_map(|children| children.iter())
        .filter_map(|entity| capture(world, root, entity).ok())
        .filter(|target| model(world, target).is_ok_and(|model| model.scratch(target.placement).is_some())).collect::<Vec<_>>();
    choices.sort_by_key(|target| (target.workspace, target.placement));
    choices
}

fn invalidate(world: &mut World, root: Entity, workspace: u64) {
    if let Some(mut runtime) = world.get_resource_mut::<Runtime>() {
        runtime.views.remove(&(root, workspace));
        runtime.influences.remove(&(root, workspace));
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
}

fn return_state(world: &World, destination: &Destination, target: Entity) -> Return {
    let previous = selected(world, destination.root).filter(|entity| *entity != target)
        .and_then(|entity| capture(world, destination.root, entity).ok());
    Return { destination: destination.clone(), previous, input: world.get_resource::<InputFocus>().and_then(InputFocus::get),
        following: focus::following(world, destination.root), unfocused: !focus::status(world, destination.root).0 }
}

fn restore_focus(world: &mut World, root: Entity, returned: Option<Return>) {
    if let Some(returned) = returned.filter(|returned| current_destination(world, &returned.destination).is_ok()) {
        if returned.unfocused {
            focus::apply(world, root, DesktopAction::Unfocus);
            return;
        }
        if let Some(target) = returned.previous.filter(|target| model(world, target).is_ok() && focus::eligible(world, root, target.workspace, target.entity)) {
            focus::select(world, root, target.workspace, target.entity, returned.following);
            if let Some(input) = returned.input.filter(|input| transfers::contains(world, *input, target.entity) && crate::native::shortcuts::input_eligible(world, root, &target.canvas, *input))
                && let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.set(input, FocusCause::Navigated); }
            return;
        }
    }
    focus::clear_mode(world, root);
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() { focus.clear(); }
    let workspace = world.get::<Workspaces>(root).map(|spaces| spaces.active);
    let target = selected(world, root).or_else(|| world.get::<Children>(root).and_then(|children|
        children.iter().find(|entity| workspace.is_some_and(|workspace| focus::eligible(world, root, workspace, *entity)))));
    if let Some(target) = target {
        let workspace = world.get::<WorkspaceMember>(target).unwrap().0;
        focus::select(world, root, workspace, target, false);
    } else if let Some(mut spaces) = world.get_mut::<Workspaces>(root) {
        let active = spaces.active;
        if let Some(space) = spaces.entries.iter_mut().find(|space| space.id == active) { space.desktop.clear_focus(); }
    }
}

fn apply_target(world: &mut World, target: &Target, action: DesktopAction, destination: &Destination, modal: Option<Entity>) -> Result<(), String> {
    guard(world, modal)?;
    current_destination(world, destination)?;
    let current = model(world, target)?;
    let association = current.scratch(target.placement).cloned();
    if action == DesktopAction::MarkScratch {
        if association.is_some() { return Ok(()); }
        if target.workspace != destination.workspace || !focus::eligible(world, target.root, target.workspace, target.entity) {
            return Err("The item must be visible in the current workspace before marking it.".into());
        }
        if current.compact_group(target.placement).is_some() { return Err("Remove the item from its compact group before marking it as scratch.".into()); }
        let mut proposed = current.clone();
        transfers::register_unmanaged(world, &mut proposed, target.entity)?;
        proposed.mark_scratch(target.placement, target.workspace).map_err(|_| "This item cannot be marked safely.")?;
        if let Some(gate) = world.get_resource::<crate::host::capture::PolicyGate>() { gate.invalidate(); }
        world.get_mut::<Workspaces>(target.root).unwrap().entries.iter_mut().find(|space| space.id == target.workspace).unwrap().desktop = proposed;
        if let Some(surface) = target.surface { world.entity_mut(target.entity).insert(ApplicationOwner { placement: target.placement, surface }); }
        invalidate(world, target.root, target.workspace);
        return Ok(());
    }
    let association = association.ok_or("The original item is no longer marked as scratch.")?;
    let owns_focus = selected(world, target.root).is_some_and(|entity| entity == target.entity || transients::descendant(world, entity, target.entity));
    if action == DesktopAction::HideScratch {
        if !association.shown() { return Ok(()); }
        let mut proposed = current.clone();
        proposed.hide_scratch(target.placement).map_err(|_| "The scratch item cannot be hidden safely.")?;
        if let Some(gate) = world.get_resource::<crate::host::capture::PolicyGate>() { gate.invalidate(); }
        world.get_mut::<Workspaces>(target.root).unwrap().entries.iter_mut().find(|space| space.id == target.workspace).unwrap().desktop = proposed;
        let returned = world.get_resource_mut::<Reveals>().and_then(|mut reveals| reveals.0.remove(&(target.root, target.placement)));
        if owns_focus && target.workspace == destination.workspace { restore_focus(world, target.root, returned); }
        invalidate(world, target.root, target.workspace);
        return Ok(());
    }
    let home_exists = world.get::<Workspaces>(target.root).unwrap().entries.iter().any(|space| space.id == association.home_workspace());
    let next = if action == DesktopAction::ShowScratch { destination.workspace }
        else if action == DesktopAction::UnmarkScratch { if home_exists { association.home_workspace() } else { destination.workspace } }
        else { return Err("This command is not a scratch action.".into()); };
    if action == DesktopAction::ShowScratch && world.get::<Workspaces>(target.root).unwrap().entries.iter().find(|space| space.id == next)
        .is_some_and(|space| space.desktop.fullscreen().is_some_and(|id| id != target.placement))
    { return Err("Leave fullscreen before showing a scratch item here.".into()); }
    let returned = return_state(world, destination, target.entity);
    let geometry = association.home_geometry();
    let prepared = transfers::prepare_scratch(world, target.root, target.workspace, target.placement, next)?
        .configure_item(world, target.entity, |model, id, _| {
            if action == DesktopAction::ShowScratch { model.show_scratch(id, geometry) }
            else { model.unmark_scratch(id, home_exists) }.map_err(|_| "The scratch placement cannot be restored safely.")?;
            Ok(model.items().iter().find(|item| item.id == id).map(|item| item.rules))
        })?;
    let prepared = if action == DesktopAction::UnmarkScratch { prepared.preserve_destination_selection() } else { prepared };
    model(world, target)?;
    guard(world, modal)?;
    current_destination(world, destination)?;
    if let Some(gate) = world.get_resource::<crate::host::capture::PolicyGate>() { gate.invalidate(); }
    prepared.commit(world)?;
    world.init_resource::<Reveals>();
    if action == DesktopAction::ShowScratch {
        if !owns_focus || target.workspace != destination.workspace {
            world.resource_mut::<Reveals>().0.insert((target.root, target.placement), returned);
        }
        focus::select(world, target.root, destination.workspace, target.entity, true);
    } else {
        world.entity_mut(target.entity).remove::<ApplicationOwner>();
        let returned = world.resource_mut::<Reveals>().0.remove(&(target.root, target.placement));
        if owns_focus && next != destination.workspace { restore_focus(world, target.root, returned); }
        if !home_exists { crate::native::notifications::report(world, "Scratch item", "The original home workspace was deleted. The item was restored in the current workspace."); }
    }
    Ok(())
}

pub(crate) fn checked(world: &mut World, root: Entity, action: DesktopAction) -> Result<bool, String> {
    guard(world, None)?;
    let destination = destination(world, root)?;
    if action == DesktopAction::ShowScratch {
        let mut choices = candidates(world, root);
        if choices.len() == 1 {
            apply_target(world, &choices.remove(0), action, &destination, None)?;
            return Ok(false);
        }
        chooser::open(world, destination, choices)?;
        return Ok(true);
    }
    let entity = selected(world, root).ok_or("Focus or select an eligible existing item first.")?;
    if world.get::<LayoutBox>(entity).is_none() {
        let size = world.get::<CanvasItem>(entity).ok_or("The selected item is unavailable.")?.size;
        world.entity_mut(entity).insert(LayoutBox::new(size));
    }
    let target = capture(world, root, entity)?;
    apply_target(world, &target, action, &destination, None)?;
    Ok(false)
}

pub(super) fn command(world: &mut World, root: Entity, action: DesktopAction) {
    if let Err(reason) = checked(world, root, action) { crate::native::notifications::report(world, "Scratch item", &reason); }
}

fn retire(world: &mut World) {
    let retired = world.query::<(Entity, &Workspaces)>().iter(world).flat_map(|(root, spaces)| spaces.entries.iter().chain(spaces.trash.iter().map(|entry| &entry.workspace)).flat_map(move |space|
        space.desktop.scratch_items().filter_map(move |(id, _)| {
            space.desktop.items().iter().any(|item| item.id == id && item.kind == ItemKind::Application).then_some((root, space.id, id))
        }))).filter(|(root, workspace, id)| !world.get::<Children>(*root).is_some_and(|children| children.iter().any(|entity|
            world.get::<WorkspaceMember>(entity).is_some_and(|member| member.0 == *workspace)
                && world.get::<LayoutBox>(entity).is_some_and(|layout| layout.id == id.0)
                && world.get::<ApplicationOwner>(entity).is_some_and(|owner| owner.placement == *id
                    && world.get::<ApplicationWindow>(entity).is_some_and(|window| window.surface == owner.surface)))))
        .collect::<Vec<_>>();
    for (root, workspace, id) in retired {
        if let Some(gate) = world.get_resource::<crate::host::capture::PolicyGate>() { gate.invalidate(); }
        if let Some(mut spaces) = world.get_mut::<Workspaces>(root) {
            if let Some(space) = spaces.entries.iter_mut().find(|space| space.id == workspace) { space.desktop.retire_scratch(id); }
            else if let Some(entry) = spaces.trash.iter_mut().find(|entry| entry.workspace.id == workspace) { entry.workspace.desktop.retire_scratch(id); }
        }
        invalidate(world, root, workspace);
    }
    let stale = world.get_resource::<Reveals>().map(|reveals| reveals.0.keys().copied().filter(|(root, id)|
        !world.get::<Children>(*root).is_some_and(|children| children.iter().any(|entity| capture(world, *root, entity).ok().is_some_and(|target|
            target.placement == *id && model(world, &target).is_ok_and(|model| model.scratch(*id).is_some()))))).collect::<Vec<_>>()).unwrap_or_default();
    if let Some(mut reveals) = world.get_resource_mut::<Reveals>() { for key in stale { reveals.0.remove(&key); } }
}

pub(super) fn before_unmap(world: &mut World, entity: Entity) {
    let Some(root) = root(world, entity) else { return; };
    let Some(owner) = world.get::<ApplicationOwner>(entity) else { return; };
    if world.get::<ApplicationWindow>(entity).is_none_or(|window| window.surface != owner.surface) { return; }
    let id = owner.placement;
    let Some(workspace) = world.get::<WorkspaceMember>(entity).map(|member| member.0) else { return; };
    if let Some(gate) = world.get_resource::<crate::host::capture::PolicyGate>() { gate.invalidate(); }
    if let Some(mut spaces) = world.get_mut::<Workspaces>(root) {
        if let Some(space) = spaces.entries.iter_mut().find(|space| space.id == workspace) { space.desktop.remove(id); }
        else if let Some(entry) = spaces.trash.iter_mut().find(|entry| entry.workspace.id == workspace) { entry.workspace.desktop.remove(id); }
    }
    world.entity_mut(entity).remove::<ApplicationOwner>();
    if let Some(mut reveals) = world.get_resource_mut::<Reveals>() { reveals.0.remove(&(root, id)); }
    invalidate(world, root, workspace);
}

#[cfg(test)]
mod tests;
