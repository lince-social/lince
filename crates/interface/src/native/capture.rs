use crate::native::{
    actions::Action,
    layout::{LayoutBox, LayoutRuntime, MAX_DEPTH, desktop::input_settings},
    workspace::Workspaces,
};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

mod ui;
mod status;
pub mod render;
pub(crate) use ui::{item_panel, window_panel};
pub(crate) use status::panel as status_panel;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowPolicy {
    pub placement: [u8; 16],
    pub uid: String,
    pub excluded: bool,
}

#[derive(Component, Clone)]
pub struct WindowSources(pub Vec<crate::host::SurfaceId>);

pub fn private_window_sources(world: &mut World, policy: &crate::host::capture::Policy) -> BTreeSet<Entity> {
    world.query_filtered::<(Entity, &WindowSources), bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>>()
        .iter(world).filter(|(_, sources)| sources.0.len() > 512 || sources.0.iter().any(|surface| policy.windows.get(surface) != Some(&false)))
        .map(|(entity, _)| entity).collect()
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub request: u64,
    pub owner: Entity,
    pub placement: [u8; 16],
    pub uid: String,
    pub revision: u64,
    pub excluded: bool,
    page: Entity,
}

#[derive(Resource, Default)]
struct Windows {
    revision: u64,
    policies: BTreeMap<[u8; 16], WindowPolicy>,
    available: bool,
    requests: VecDeque<Edit>,
    next: u64,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Windows>().init_resource::<crate::host::capture::Status>().add_systems(
        PostUpdate,
        (ui::refresh, status::refresh)
            .after(crate::native::actions::ApplyActions)
            .before(bevy::ui::UiSystems::Layout),
    );
}

pub fn requested_windows(world: &mut World) -> Vec<([u8; 16], String)> {
    let mut targets = BTreeSet::new();
    for page in world.query::<&ui::Page>().iter(world) {
        if let ui::Target::Window { placement, uid } = &page.target {
            targets.insert((*placement, uid.clone()));
        }
    }
    targets.into_iter().take(128).collect()
}

pub fn publish_windows(world: &mut World, available: bool, policies: Vec<WindowPolicy>) {
    world.init_resource::<Windows>();
    let policies = policies.into_iter().take(128).map(|policy| (policy.placement, policy)).collect();
    let mut windows = world.resource_mut::<Windows>();
    if windows.available != available || windows.policies != policies {
        windows.revision = windows.revision.checked_add(1).expect("capture policy revision");
        windows.available = available;
        windows.policies = policies;
        drop(windows);
        if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.after(std::time::Duration::ZERO); }
    }
}

pub fn take_edits(world: &mut World) -> Vec<Edit> {
    world.resource_mut::<Windows>().requests.drain(..).collect()
}

pub fn validate_edit(world: &World, edit: &Edit) -> Result<(), &'static str> {
    let windows = world.get_resource::<Windows>().ok_or("Window settings are unavailable.")?;
    let page = world.get::<ui::Page>(edit.page).ok_or("The original settings control was closed.")?;
    if page.pending != Some(edit.request) || page.root != edit.owner || !ui::eligible(world, page) {
        return Err("The original settings owner is no longer available.");
    }
    if !crate::native::shortcuts::input_eligible(world, page.root, &page.canvas, edit.page) {
        return Err("The original settings control is hidden or disabled.");
    }
    if !windows.available || windows.revision != edit.revision {
        return Err("Window settings changed. Cancel to refresh before trying again.");
    }
    let policy = windows.policies.get(&edit.placement).filter(|policy| policy.uid == edit.uid)
        .ok_or("The original window profile is unavailable.")?;
    if policy.excluded == edit.excluded {
        return Err("This exclusion setting is already active. Cancel to refresh.");
    }
    Ok(())
}

pub fn complete_edit(world: &mut World, edit: &Edit, result: Result<(), String>) {
    let revision = world.resource::<Windows>().revision;
    if let Some(mut page) = world.get_mut::<ui::Page>(edit.page)
        && page.pending == Some(edit.request)
    {
        page.pending = None;
        match result {
            Ok(()) => {
                page.revision = revision;
                page.origin = Some(edit.excluded);
                page.draft = edit.excluded;
                page.message = "Capture exclusion saved.".into();
            }
            Err(error) => page.message = error.chars().take(4096).collect(),
        }
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.after(std::time::Duration::ZERO); }
}

pub fn excluded(world: &World, entity: Entity) -> bool {
    excluded_with(world, entity, &BTreeSet::new())
}

pub fn semantic_sources(world: &World, entity: Entity) -> BTreeSet<Entity> {
    fn visit(world: &World, entity: Entity, path: &mut BTreeSet<Entity>, budget: &mut usize, sources: &mut BTreeSet<Entity>) {
        if *budget == 0 || path.len() >= MAX_DEPTH || !path.insert(entity) || world.get_entity(entity).is_err() {
            sources.insert(entity);
            return;
        }
        *budget -= 1;
        let boundary = world.get::<LayoutBox>(entity).is_some() || world.get::<Workspaces>(entity).is_some()
            || world.get::<WindowSources>(entity).is_some() || world.get::<crate::native::full_record::window::Metadata>(entity).is_some()
            || world.get::<crate::native::layout::desktop::ApplicationWindow>(entity).is_some();
        if boundary { sources.insert(entity); }
        let mut linked = false;
        if let Some(owner) = world.get::<crate::native::topology::presentation::VisualOwner>(entity) {
            linked = true;
            visit(world, owner.0, path, budget, sources);
        }
        if let Some(owner) = world.get::<render::TargetSource>(entity) {
            linked = true;
            visit(world, owner.0, path, budget, sources);
        }
        for owner in crate::native::shader_effects::capture_sources(world, entity) {
            linked = true;
            visit(world, owner, path, budget, sources);
        }
        if !boundary {
            if let Some(parent) = world.get::<ChildOf>(entity) { visit(world, parent.parent(), path, budget, sources); }
            else if !linked { sources.insert(entity); }
        }
        path.remove(&entity);
    }
    let mut sources = BTreeSet::new();
    visit(world, entity, &mut BTreeSet::new(), &mut 1024, &mut sources);
    sources
}

pub fn excluded_with(world: &World, entity: Entity, additional: &BTreeSet<Entity>) -> bool {
    fn visit(world: &World, current: Entity, additional: &BTreeSet<Entity>, path: &mut BTreeSet<Entity>, budget: &mut usize) -> bool {
        if path.len() >= MAX_DEPTH || *budget == 0 || !path.insert(current) || world.get_entity(current).is_err() { return true; }
        *budget -= 1;
        if additional.contains(&current) { return true; }
        if let Some(layout) = world.get::<LayoutBox>(current) {
            if layout.capture_excluded { return true; }
            if let Some(root) = crate::native::layout::desktop::root(world, current)
                && !crate::native::layout::desktop::visible_item(world, root, current) { return true; }
            if let Some(parent) = layout.parent {
                let Some(owner) = world.get::<LayoutRuntime>(current).and_then(|runtime| runtime.parent)
                    .filter(|owner| world.get::<LayoutBox>(*owner).is_some_and(|layout| layout.id == parent))
                else { return true; };
                if visit(world, owner, additional, path, budget) { return true; }
            }
        }
        if let Some(owner) = world.get::<crate::native::topology::presentation::VisualOwner>(current) {
            if visit(world, owner.0, additional, path, budget) { return true; }
        }
        if let Some(owner) = world.get::<render::TargetSource>(current)
            && visit(world, owner.0, additional, path, budget)
        { return true; }
        for owner in crate::native::shader_effects::capture_sources(world, current) {
            if visit(world, owner, additional, path, budget) { return true; }
        }
        if world.get::<Workspaces>(current).is_none()
            && let Some(parent) = world.get::<ChildOf>(current)
        { if visit(world, parent.parent(), additional, path, budget) { return true; } }
        path.remove(&current);
        false
    }
    visit(world, entity, additional, &mut BTreeSet::new(), &mut 1024)
}

#[cfg(test)]
mod tests;
