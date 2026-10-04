use crate::{
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
            Update,
            update
                .after(crate::workspace::PrepareWorkspaces)
                .after(crate::cell_bridge::ReceiveCell),
        );
    }
}

mod apply;
mod lifecycle;
mod registry;
mod snapshot;
use apply::apply;
pub(crate) use lifecycle::{restore_content, spawn};
use registry::kind_id;
pub use registry::registry;
pub use snapshot::capture;
use snapshot::{capture_content, record_references};

fn update(world: &mut World) {
    if crate::laboratory::active(world) {
        return;
    }
    let roots: Vec<_> = world
        .query_filtered::<Entity, (With<crate::container::BoxRoot>, With<Workspaces>)>()
        .iter(world)
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
            state.mark_saved_through(crate::workspace::saved_canvas_revision(world, root));
            world.get_mut::<Workspaces>(root).unwrap().canvas_state = Some(state);
            if world.get::<Host>(root).is_none() {
                if let Some(runtime) = world.get_resource::<crate::app::CellHandle>() {
                    let id = world.get::<Workspaces>(root).unwrap().canvas_id.clone();
                    let receiver =
                        runtime
                            .0
                            .engine
                            .register_canvas(id, "Lince canvas".into(), registry())?;
                    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned() {
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
                let result = (|| {
                    let before = world
                        .get::<Workspaces>(root)
                        .unwrap()
                        .canvas_state
                        .as_ref()
                        .unwrap()
                        .clone();
                    let mut next = before.clone();
                    let response = next.handle(&call.request)?;
                    if call.is_cancelled() {
                        return Err("Canvas request cancelled before applying.".into());
                    }
                    if next.snapshot != before.snapshot {
                        apply(world, root, &before.snapshot, &next.snapshot)?;
                        next.normalize_applied_snapshot(capture(world, root)?)?;
                        crate::workspace::request_save(world);
                    }
                    world.get_mut::<Workspaces>(root).unwrap().canvas_state = Some(next);
                    Ok(response)
                })();
                let _ = call.complete(result);
            }
            if world
                .get::<Host>(root)
                .is_some_and(|host| host.0.has_pending())
            {
                if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                    wake.ring();
                }
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            crate::notifications::report(world, "interface::canvas", &error);
        }
    }
}

pub(crate) mod composition;
#[cfg(test)]
mod tests;
