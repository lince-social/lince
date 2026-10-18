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
struct Refreshed(std::time::Instant);

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
mod registry;
mod rooms;
mod snapshot;
use apply::apply;
pub(crate) use lifecycle::{restore_content, spawn};
use registry::kind_id;
pub use registry::registry;
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
    let mut before = world
        .get::<Workspaces>(root)
        .ok_or("Canvas has no workspaces.")?
        .canvas_state
        .clone()
        .map(Ok)
        .unwrap_or_else(|| api::State::new(snapshot.clone(), registry()))?;
    before.synchronize(snapshot)?;
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
    if next.snapshot != before.snapshot {
        apply(world, root, &before.snapshot, &next.snapshot)?;
        crate::native::topology::rooms::synchronize(world);
        next.normalize_applied_snapshot(capture(world, root)?)?;
        crate::native::workspace::request_save(world);
    }
    world.get_mut::<Workspaces>(root).unwrap().canvas_state = Some(next);
    Ok(response)
}

fn update(world: &mut World) {
    if !crate::native::laboratory::normal(world) {
        return;
    }
    crate::native::topology::rooms::synchronize(world);
    let roots: Vec<_> = world
        .query_filtered::<Entity, (With<crate::native::container::BoxRoot>, With<Workspaces>)>()
        .iter(world)
        .filter(|root| !crate::native::laboratory::blocked(world, *root))
        .collect();
    for root in roots {
        if world
            .get::<Host>(root)
            .is_some_and(|host| !host.0.has_pending())
            && world.get::<Refreshed>(root).is_some_and(|refreshed| {
                refreshed.0.elapsed() < std::time::Duration::from_millis(200)
            })
        {
            continue;
        }
        world
            .entity_mut(root)
            .insert(Refreshed(std::time::Instant::now()));
        let result = (|| {
            let snapshot = capture(world, root)?;
            let mut state = world
                .get_mut::<Workspaces>(root)
                .unwrap()
                .canvas_state
                .clone()
                .map(Ok)
                .unwrap_or_else(|| api::State::new(snapshot.clone(), registry()))?;
            state.synchronize(snapshot)?;
            state.mark_saved_through(crate::native::workspace::saved_canvas_revision(world, root));
            world.get_mut::<Workspaces>(root).unwrap().canvas_state = Some(state);
            if world.get::<Host>(root).is_none() {
                if let Some(runtime) = world.get_resource::<crate::native::app::CellHandle>() {
                    let id = world.get::<Workspaces>(root).unwrap().canvas_id.clone();
                    let receiver =
                        runtime
                            .0
                            .engine
                            .register_canvas(id, "Lince canvas".into(), registry())?;
                    if let Some(wake) = world
                        .get_resource::<crate::native::wake::WakeSignal>()
                        .cloned()
                    {
                        receiver.set_wake(move || wake.ring());
                    }
                    world.entity_mut(root).insert(Host(receiver));
                }
            }
            for _ in 0..8 {
                let call = world
                    .get_mut::<Host>(root)
                    .and_then(|mut host| host.0.try_recv().ok());
                let Some(call) = call else { break };
                if call.is_cancelled() {
                    continue;
                }
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
            crate::native::notifications::report(world, "interface::canvas", &error);
        }
    }
}

pub(crate) mod composition;
#[cfg(test)]
mod tests;
