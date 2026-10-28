use super::*;
use crate::native::{actions::Action, full_record::window::Metadata, protein_area::RecordBinding};

#[derive(Clone, Debug)]
struct Target {
    root: Entity,
    canvas: String,
    workspace: u64,
    output: Option<OutputId>,
    entity: Entity,
    placement: PlacementId,
    surface: SurfaceId,
}

#[derive(Component)]
struct Page {
    root: Entity,
    canvas: String,
    inspector: Entity,
    placement: [u8; 16],
    uid: String,
    target: Option<Target>,
    button: (Entity, Entity),
    status: Entity,
    message: String,
}

#[derive(Clone, Copy)]
struct Toggle;

pub(crate) fn install(app: &mut App) {
    app.add_systems(PostUpdate, refresh.after(crate::native::actions::ApplyActions).before(bevy::ui::UiSystems::Layout));
}

fn capture(world: &World, root: Entity, entity: Entity) -> Result<Target, &'static str> {
    if !world.contains_resource::<HostRequests>() { return Err("This application command requires a Park session."); }
    if world.get_resource::<crate::host::input::InputActivity>().is_some_and(|activity| !activity.active()) {
        return Err("The Park seat is inactive or locked.");
    }
    let spaces = world.get::<Workspaces>(root).ok_or("The original workspace is unavailable.")?;
    let window = world.get::<ApplicationWindow>(entity).ok_or("Windowed fullscreen requires an application window.")?;
    if world.get::<TransientParent>(entity).is_some() { return Err("Select the parent application window, rather than its dialog."); }
    let workspace = world.get::<WorkspaceMember>(entity).ok_or("The original window is unavailable.")?.0;
    if workspace != spaces.active || !focus::eligible(world, root, workspace, entity) { return Err("The original application window is hidden or unavailable."); }
    let placement = world.get::<Managed>(entity).ok_or("The original window is not managed yet.")?.id;
    if world.get::<LayoutBox>(entity).is_none_or(|layout| layout.id != placement.0) { return Err("The original placement changed."); }
    Ok(Target { root, canvas: spaces.canvas_id.clone(), workspace, output: world.get::<HostViewport>(root).map(|viewport| viewport.0.id), entity, placement, surface: window.surface })
}

fn state(world: &World, target: &Target) -> Result<bool, &'static str> {
    let current = capture(world, target.root, target.entity)?;
    if current.canvas != target.canvas || current.workspace != target.workspace || current.output != target.output || current.placement != target.placement || current.surface != target.surface {
        return Err("The original application context changed.");
    }
    let workspace = world.get::<Workspaces>(target.root).unwrap().entries.iter().find(|space| space.id == target.workspace)
        .ok_or("The original workspace is unavailable.")?;
    Ok(workspace.desktop.windowed_fullscreen(target.placement))
}

fn toggle(world: &mut World, target: &Target) -> Result<(), &'static str> {
    let enabled = state(world, target)?;
    let mut spaces = world.get_mut::<Workspaces>(target.root).unwrap();
    let workspace = spaces.entries.iter_mut().find(|space| space.id == target.workspace).ok_or("The original workspace is unavailable.")?;
    workspace.desktop.set_windowed_fullscreen(target.placement, !enabled).map_err(|_| "The original application placement is unavailable.")?;
    drop(spaces);
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
    Ok(())
}

pub(super) fn focused(world: &mut World, root: Entity) {
    let result = CommandContext::new(world, root).unavailable(DesktopAction::ToggleWindowedFullscreen).map_or_else(|| {
        let entity = focused_entity(world, root).ok_or("Focus an application window to change windowed fullscreen.")?;
        let target = capture(world, root, entity)?;
        toggle(world, &target)
    }, Err);
    if let Err(reason) = result { crate::native::notifications::report(world, "Windowed fullscreen", reason); }
}

fn source(world: &World, page: &Page) -> Result<Target, &'static str> {
    if world.get::<Workspaces>(page.root).is_none_or(|spaces| spaces.canvas_id != page.canvas) { return Err("The original canvas changed."); }
    if world.get::<Metadata>(page.inspector).is_none_or(|metadata| metadata.placement != page.placement || metadata.uid != page.uid || !metadata.live) {
        return Err("The original window association is closed or changed.");
    }
    let target = page.target.as_ref().ok_or("The original application window is unavailable.")?;
    if world.get::<RecordBinding>(target.entity).is_none_or(|binding| binding.uid != page.uid
        || binding.area != target.entity || binding.source != crate::native::protein_area::Source::Local)
    { return Err("The original window Record association changed."); }
    state(world, target)?;
    Ok(target.clone())
}

impl Action for Toggle {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(page) = world.get::<Page>(entity) else { return; };
        if crate::native::shortcuts::keyboard_owned(world) || !crate::native::shortcuts::input_eligible(world, page.root, &page.canvas, entity) { return; }
        let target = source(world, page);
        let result = target.and_then(|target| toggle(world, &target));
        if let Some(mut page) = world.get_mut::<Page>(entity) {
            page.message = result.err().unwrap_or_default().into();
        }
    }
}

pub(crate) fn panel(world: &mut World, parent: Entity, binding: &RecordBinding) {
    let Some(metadata) = world.get::<Metadata>(binding.area).filter(|metadata| metadata.uid == binding.uid).cloned() else { return; };
    let Some(root) = root(world, binding.area) else { return; };
    let Some(spaces) = world.get::<Workspaces>(root) else { return; };
    let canvas = spaces.canvas_id.clone();
    let target = world.get::<Children>(root).and_then(|children| children.iter().find(|entity| world.get::<LayoutBox>(*entity).is_some_and(|layout| layout.id == metadata.placement)
        && world.get::<ApplicationWindow>(*entity).is_some())).and_then(|entity| capture(world, root, entity).ok());
    let page = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }, ChildOf(parent), Name::new("window.windowed-fullscreen"))).id();
    crate::native::edit_mode::label(world, page, "Request fullscreen presentation inside this window's existing slot. The workspace and bars stay available.", 14.0);
    let button = input_settings::button(world, page, page, "Windowed fullscreen", Toggle);
    let status = input_settings::feedback(world, page);
    world.entity_mut(page).insert(Page { root, canvas, inspector: binding.area, placement: metadata.placement, uid: binding.uid.clone(), target, button, status, message: String::new() });
}

fn refresh(world: &mut World) {
    let pages: Vec<_> = world.query_filtered::<Entity, With<Page>>().iter(world).collect();
    for entity in pages {
        let page = world.get::<Page>(entity).unwrap();
        let value = source(world, page).and_then(|target| state(world, &target));
        let (button, label) = page.button;
        let status = page.status;
        let message = if page.message.is_empty() { value.as_ref().err().copied().unwrap_or_default().to_owned() } else { page.message.clone() };
        let enabled = value.as_ref().copied().unwrap_or(false);
        input_settings::text(world, label, if enabled { "Windowed fullscreen: On · leave" } else { "Windowed fullscreen: Off · enter" });
        input_settings::disable(world, button, value.is_err());
        input_settings::text(world, status, message);
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) {
            let label = if enabled { "Leave windowed fullscreen" } else { "Enter windowed fullscreen" };
            let toggled = if enabled { accesskit::Toggled::True } else { accesskit::Toggled::False };
            if node.label() != Some(label) { node.set_label(label); }
            if node.toggled() != Some(toggled) { node.set_toggled(toggled); }
        }
    }
}

#[cfg(test)]
mod tests;
