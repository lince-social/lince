use crate::native::{
    canvas::{CanvasItem, CanvasView},
    sand_store::{SandKind, StoredSand},
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{math::DVec2, prelude::*};
use nucleus::canvas::{self as api, Component as ContentKind};
use std::collections::{BTreeMap, HashMap};

#[derive(Component, Clone)]
pub struct Identity(pub String);
#[derive(Component, Clone)]
pub struct Content(pub ContentKind);
#[derive(Component)]
struct Host(engine::canvas::Receiver);

#[derive(Component)]
struct Failure(String);

pub struct Plugin;
impl bevy::prelude::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            authoring::maintain
                .after(crate::native::layout::ResolveLayout)
                .after(crate::native::token_style::ApplyTokenStyles)
                .before(bevy::ui::UiSystems::Prepare),
        );
        app.add_systems(
            Update,
            update
                .after(crate::native::workspace::PrepareWorkspaces)
                .after(crate::native::cell_bridge::ReceiveCell),
        );
    }
}

mod apply;
mod area;
pub(crate) mod authoring;
mod factories;
mod kanban;
mod lifecycle;
mod metadata;
mod registry;
mod rooms;
mod snapshot;
use apply::apply;
pub(crate) use lifecycle::{restore_content, spawn};
use registry::kind_id;
pub use registry::registry;
pub(crate) use registry::shared_registry;
pub use snapshot::capture;
use snapshot::{capture_content, record_references};

pub(crate) fn handle(
    world: &mut World,
    root: Entity,
    request: &api::Request,
) -> Result<api::Response, String> {
    handle_checked(world, root, request, || false)
}

fn handle_checked(
    world: &mut World,
    root: Entity,
    request: &api::Request,
    cancelled: impl Fn() -> bool,
) -> Result<api::Response, String> {
    crate::native::topology::rooms::synchronize(world);
    let snapshot = capture(world, root)?;
    let existing = world
        .get::<Workspaces>(root)
        .ok_or("Canvas has no workspaces.")?
        .canvas_state
        .clone();
    let mut before = if let Some(mut state) = existing {
        state.refresh_registry(shared_registry())?;
        state.synchronize(snapshot)?;
        state
    } else {
        api::State::new(snapshot, shared_registry())?
    };
    before.mark_saved_through(crate::native::workspace::saved_canvas_revision(world, root));
    let mut next = before.clone();
    let mut response = next.handle(request)?;
    if let (
        api::Request::InspectElement { offset, limit, .. },
        api::Response::Element { element },
    ) = (request, &mut response)
    {
        authoring::inspect(world, root, element, *offset, *limit)?;
    }
    if cancelled() {
        return Err("Canvas request cancelled before applying.".into());
    }
    if !std::sync::Arc::ptr_eq(&next.snapshot, &before.snapshot) && next.snapshot != before.snapshot
    {
        apply(world, root, &before.snapshot, &next.snapshot)?;
        crate::native::topology::rooms::synchronize(world);
        next.normalize_applied_snapshot(capture(world, root)?)?;
        crate::native::workspace::request_save(world);
    }
    world
        .get_mut::<Workspaces>(root)
        .ok_or("Canvas has no workspaces.")?
        .canvas_state = Some(next);
    Ok(response)
}

pub(crate) fn synchronize_saved_state(world: &mut World, root: Entity) -> Result<(), String> {
    if world
        .get::<Workspaces>(root)
        .is_none_or(|spaces| spaces.canvas_state.is_none())
    {
        return Ok(());
    }
    crate::native::topology::rooms::synchronize(world);
    let snapshot = capture(world, root)?;
    let saved = crate::native::workspace::saved_canvas_revision(world, root);
    let state = world
        .get_mut::<Workspaces>(root)
        .ok_or("Canvas has no workspaces.")?
        .into_inner()
        .canvas_state
        .as_mut()
        .ok_or("Canvas recovery state is unavailable.")?;
    state.refresh_registry(shared_registry())?;
    state.synchronize(snapshot)?;
    state.mark_saved_through(saved);
    Ok(())
}

fn update(world: &mut World) {
    if !crate::native::laboratory::normal(world) {
        return;
    }
    let roots: Vec<_> = world
        .query_filtered::<Entity, (With<crate::native::container::BoxRoot>, With<Workspaces>)>()
        .iter(world)
        .filter(|root| !crate::native::laboratory::blocked(world, *root))
        .collect();
    for root in roots {
        let result = (|| {
            if world.get::<Host>(root).is_none() {
                let Some(runtime) = world.get_resource::<crate::native::app::CellHandle>() else {
                    return Ok(());
                };
                let Some(spaces) = world.get::<Workspaces>(root) else {
                    return Ok(());
                };
                let id = spaces.canvas_id.clone();
                let receiver = runtime.0.engine.register_canvas(
                    id,
                    "Lince canvas".into(),
                    shared_registry(),
                )?;
                if let Some(wake) = world
                    .get_resource::<crate::native::wake::WakeSignal>()
                    .cloned()
                {
                    receiver.set_wake(move || wake.ring());
                }
                world.entity_mut(root).insert(Host(receiver));
            }
            let call = world
                .get_mut::<Host>(root)
                .and_then(|mut host| host.0.try_recv().ok());
            if let Some(call) = call
                && !call.is_cancelled()
            {
                let _measurement = crate::host::performance::Span::new(
                    crate::host::performance::Stage::NativeCanvasSync,
                );
                crate::host::performance::count(crate::host::performance::Count::NativeCanvasCalls);
                let result = handle_checked(world, root, &call.request, || call.is_cancelled());
                let _ = call.complete(result);
            }
            if world
                .get::<Host>(root)
                .is_some_and(|host| host.0.has_pending())
            {
                if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
                    wake.ring();
                }
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            if world
                .get::<Failure>(root)
                .is_none_or(|failure| failure.0 != error)
            {
                crate::native::notifications::report(world, "interface::canvas", &error);
                if let Ok(mut entity) = world.get_entity_mut(root) {
                    entity.insert(Failure(error));
                }
            }
        } else if let Ok(mut entity) = world.get_entity_mut(root) {
            entity.remove::<Failure>();
        }
    }
}

pub(crate) mod composition;
#[cfg(test)]
mod tests;
