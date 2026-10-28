use super::*;
use crate::{host::{control::{Arguments, Context, Error, Failure, Invocation, Item as Target}, launch::Capability}, native::shortcuts::Shortcut};

#[derive(Component)]
struct StatusLabel;

#[derive(Component)]
struct RevokeButton;

#[derive(Clone, Copy)]
struct Revoke;

impl crate::native::actions::Action for Revoke {
    fn apply(&self, world: &mut World, _: Entity) {
        if crate::native::shortcuts::keyboard_owned(world) { return; }
        let Some(status) = world.get_resource::<crate::host::control::Status>().filter(|status| status.available) else { return; };
        if !status.active || status.locked || status.readers == 0 && status.controllers == 0 { return; }
        if !host_request(world, crate::host::HostRequest::ControlRevoke) {
            crate::native::notifications::report(world, "Desktop automation", "The host connection is busy. Retry revoking these connections.");
        }
    }
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<crate::host::control::Status>()
        .add_systems(PostUpdate, refresh.after(crate::native::actions::ApplyActions).before(bevy::ui::UiSystems::Layout));
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    crate::native::edit_mode::label(world, parent, "Desktop automation", 20.0);
    let label = crate::native::edit_mode::label(world, parent, "", 14.0);
    world.entity_mut(label).insert((StatusLabel, TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter)));
    world.get_mut::<Node>(label).unwrap().width = percent(100);
    world.get_mut::<Node>(label).unwrap().min_width = px(0);
    let button = input_settings::button(world, parent, parent, "Revoke automation connections", Revoke).0;
    world.entity_mut(button).insert(RevokeButton);
    refresh(world);
}

fn refresh(world: &mut World) {
    let status = world.get_resource::<crate::host::control::Status>().cloned().unwrap_or_default();
    let message = if !status.available { "External desktop automation is available in Park. Sandbox keeps control of its own window.".into() }
        else { format!("Connected helpers: {} may observe, {} may issue commands · {} pending requests{}. Helpers use the permissions assigned to their launch connection. Revoking closes those automation connections; your external service manager controls restarts.", status.readers, status.controllers, status.pending, if status.locked { " · paused while locked" } else if !status.active { " · paused while inactive" } else { "" }) };
    let labels: Vec<_> = world.query_filtered::<Entity, With<StatusLabel>>().iter(world).collect();
    for label in labels { input_settings::text(world, label, &message); }
    let buttons: Vec<_> = world.query_filtered::<Entity, With<RevokeButton>>().iter(world).collect();
    for button in buttons { input_settings::disable(world, button, !status.available || !status.active || status.locked || status.readers == 0 && status.controllers == 0); }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prepared {
    Action(DesktopAction),
    Workspace { owner: u64, id: u64 },
}

pub fn context(world: &World, root: Entity) -> Option<Context> {
    let spaces = world.get::<Workspaces>(root)?;
    let workspace = spaces.entries.iter().find(|workspace| workspace.id == spaces.active)?;
    let item = workspace.desktop.focused().and_then(|id| {
        let entity = world.get::<Children>(root)?.iter().find(|entity| world.get::<Managed>(*entity).is_some_and(|managed| managed.id == id) && focus::eligible(world, root, workspace.id, *entity))?;
        Some(world.get::<ApplicationWindow>(entity).map_or(Target::Native { placement: id.0 }, |window| Target::Application { placement: id.0, surface: window.surface.0 }))
    });
    let output = world.get_resource::<crate::native::output_views::OutputViews>().filter(|views| views.applies_to(root)).map(|views| views.focused.0);
    Some(Context { owner: root.to_bits(), canvas: spaces.canvas_id.clone(), workspace: workspace.id, output, item })
}

pub fn catalog(world: &World, root: Entity) -> crate::host::control::Catalog {
    let available = CommandContext::new(world, root);
    let owned = input_owned(world, root);
    crate::host::control::Catalog {
        context: context(world, root),
        commands: Shortcut::ALL.into_iter().filter(|shortcut| shortcut.desktop_action().is_some()).map(|shortcut| crate::host::control::Command {
            id: shortcut.id().into(), label: shortcut.label().into(),
            unavailable: if owned { Some("Finish the current dialog or input interaction.".into()) } else { available.shortcut(shortcut).map(str::to_owned) },
        }).collect(),
    }
}

pub fn capture_requests(world: &mut World, root: Entity, action: DesktopAction) -> Result<Vec<crate::host::HostRequest>, Failure> {
    use crate::native::actions::Action;
    let previous = world.remove_resource::<HostRequests>().ok_or_else(|| Failure::new(Error::Unavailable, "The Park host connection is unavailable."))?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    world.insert_resource(HostRequests(sender));
    action.apply(world, root);
    world.insert_resource(previous);
    let requests: Vec<_> = receiver.try_iter().collect();
    if requests.len() > 1 { return Err(Failure::new(Error::Unavailable, "This command requires more than one host transaction.")); }
    Ok(requests)
}

fn need(grants: &[Capability], required: Capability) -> Result<(), Failure> {
    if grants.contains(&required) { Ok(()) }
    else { Err(Failure::new(Error::Denied, format!("The connection lacks {required:?} permission for this command."))) }
}

fn authority(grants: &[Capability], action: DesktopAction) -> Result<(), Failure> {
    need(grants, Capability::DesktopControl)?;
    match action {
        DesktopAction::OpenTerminal | DesktopAction::OpenLauncher | DesktopAction::Lock | DesktopAction::External(_) => need(grants, Capability::Launch)?,
        DesktopAction::FocusOutput(_) | DesktopAction::Magnify(_) => need(grants, Capability::OutputControl)?,
        DesktopAction::MoveToOutput(_) | DesktopAction::MoveRowToOutput(_) | DesktopAction::MoveWorkspaceToOutput(_) => {
            need(grants, Capability::WindowControl)?;
            need(grants, Capability::OutputControl)?;
        }
        DesktopAction::InspectWindowRecord => need(grants, Capability::WindowList)?,
        DesktopAction::ToggleEdit | DesktopAction::OpenOperation | DesktopAction::ToggleOverview | DesktopAction::Workspace(_)
            | DesktopAction::PreviousWorkspace | DesktopAction::NextWorkspace | DesktopAction::Logout => {}
        _ => need(grants, Capability::WindowControl)?,
    }
    if matches!(action, DesktopAction::External(crate::host::external::Action::ScreenshotScreen | crate::host::external::Action::ScreenshotRegion)) {
        need(grants, Capability::Capture)?;
    }
    Ok(())
}

fn input_owned(world: &World, root: Entity) -> bool {
    if crate::native::shortcuts::keyboard_owned(world) || manipulation::active(world)
        || world.get::<crate::native::edit_mode::EditMode>(root).is_some_and(|mode| mode.enabled)
    { return true; }
    let mut focused = world.get_resource::<bevy::input_focus::InputFocus>().and_then(bevy::input_focus::InputFocus::get);
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some(entity) = focused else { return false; };
        if world.get::<bevy::input_focus::tab_navigation::TabGroup>(entity).is_some_and(|group| group.modal)
            || world.get::<bevy::text::EditableText>(entity).is_some_and(|text| text.is_composing() || !text.pending_edits.is_empty() || text.pending_paste.is_some())
        { return true; }
        if entity == root { return false; }
        focused = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    true
}

pub fn prepare(world: &World, root: Entity, invocation: &Invocation, grants: &[Capability]) -> Result<Prepared, Failure> {
    invocation.validate()?;
    need(grants, Capability::DesktopControl)?;
    let shortcut = Shortcut::from_id(&invocation.command).ok_or_else(|| Failure::new(Error::UnknownCommand, "This command ID is not registered."))?;
    let action = shortcut.desktop_action().ok_or_else(|| Failure::new(Error::Unavailable, "This command requires its ordinary native input interaction."))?;
    authority(grants, action)?;
    if context(world, root).as_ref() != Some(&invocation.context) { return Err(Failure::new(Error::StaleTarget, "The captured canvas, workspace, output or selected item changed.")); }
    if input_owned(world, root) {
        return Err(Failure::new(Error::Unavailable, "Finish the current dialog, key capture or management gesture before using desktop automation."));
    }
    if let Some(reason) = CommandContext::new(world, root).shortcut(shortcut) { return Err(Failure::new(Error::Unavailable, reason)); }
    match (action, invocation.arguments) {
        (DesktopAction::Workspace(number), Arguments::Workspace { id }) => {
            if number == 0 || crate::native::workspace::numbers(world, root).get(number.saturating_sub(1) as usize) != Some(&id) {
                return Err(Failure::new(Error::StaleTarget, "The numbered destination no longer refers to the captured workspace identity."));
            }
            Ok(Prepared::Workspace { owner: invocation.context.owner, id })
        }
        (DesktopAction::MoveToWorkspace(number), Arguments::Workspace { id }) => {
            if number == 0 || crate::native::workspace::numbers(world, root).get(number.saturating_sub(1) as usize) != Some(&id) {
                return Err(Failure::new(Error::StaleTarget, "The destination workspace changed; refresh its published identity."));
            }
            Ok(Prepared::Action(action))
        }
        (DesktopAction::Workspace(_) | DesktopAction::MoveToWorkspace(_), _) => Err(Failure::new(Error::InvalidArguments, "Workspace commands require the published destination identity.")),
        (_, Arguments::None) => Ok(Prepared::Action(action)),
        _ => Err(Failure::new(Error::InvalidArguments, "This command does not accept a workspace argument.")),
    }
}

#[cfg(test)]
mod tests;
