//! Per-canvas and per-output history for numeric workspace reselection.

use super::Workspaces;
use crate::{
    host::OutputId,
    native::{
        laboratory::temporary, layout::desktop::HostViewport, output_views::OutputViews,
        shortcuts::configuration::LayoutSettings,
    },
};
use bevy::prelude::{Component, Entity, World};
use std::collections::BTreeMap;

#[derive(Component, Default)]
struct History {
    canvas: String,
    previous: BTreeMap<Option<OutputId>, u64>,
}

#[derive(Clone)]
pub struct Selection {
    root: Entity,
    canvas: String,
    output: Option<OutputId>,
    workspace: u64,
}

fn output(world: &World, root: Entity) -> Option<OutputId> {
    world
        .get::<HostViewport>(root)
        .map(|viewport| viewport.0.id)
}

fn live_workspace(world: &World, root: Entity, spaces: &Workspaces, workspace: u64) -> bool {
    spaces.entries.iter().any(|entry| entry.id == workspace) && !temporary(world, root, workspace)
}

pub fn capture(world: &World, root: Entity) -> Option<Selection> {
    let spaces = world.get::<Workspaces>(root)?;
    let workspace = spaces.active;
    if !live_workspace(world, root, spaces, workspace) {
        return None;
    }
    Some(Selection {
        root,
        canvas: spaces.canvas_id.clone(),
        output: output(world, root),
        workspace,
    })
}

impl Selection {
    pub fn remember(self, world: &mut World) {
        let Some(spaces) = world.get::<Workspaces>(self.root) else {
            return;
        };
        let active = spaces.active;
        if active == self.workspace
            || spaces.canvas_id != self.canvas
            || output(world, self.root) != self.output
            || !live_workspace(world, self.root, spaces, active)
            || !live_workspace(world, self.root, spaces, self.workspace)
        {
            return;
        }
        let outputs = world
            .get_resource::<OutputViews>()
            .filter(|views| views.applies_to(self.root));
        let stale: Vec<_> = world
            .get::<History>(self.root)
            .filter(|history| history.canvas == self.canvas)
            .into_iter()
            .flat_map(|history| history.previous.iter())
            .filter_map(|(&output, &workspace)| {
                if output == self.output {
                    return None;
                }
                let present = outputs
                    .is_some_and(|views| views.outputs.iter().any(|view| output == Some(view.id)));
                (!present || !live_workspace(world, self.root, spaces, workspace)).then_some(output)
            })
            .collect();
        let Ok(mut root) = world.get_entity_mut(self.root) else {
            return;
        };
        let mut history = root.entry::<History>().or_default().into_mut();
        if history.canvas != self.canvas {
            history.canvas = self.canvas;
            history.previous.clear();
        }
        for output in stale {
            history.previous.remove(&output);
        }
        history.previous.insert(self.output, self.workspace);
    }
}

pub fn numeric_destination(world: &World, root: Entity, requested: u64) -> u64 {
    let enabled = world
        .get_resource::<LayoutSettings>()
        .is_some_and(|settings| settings.workspace_back_and_forth);
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return requested;
    };
    if !enabled || spaces.active != requested {
        return requested;
    }
    world
        .get::<History>(root)
        .filter(|history| history.canvas == spaces.canvas_id)
        .and_then(|history| history.previous.get(&output(world, root)))
        .copied()
        .filter(|previous| *previous != requested && live_workspace(world, root, spaces, *previous))
        .unwrap_or(requested)
}

pub(crate) fn forget(world: &mut World, root: Entity, workspace: u64) {
    if let Some(mut history) = world.get_mut::<History>(root) {
        history
            .previous
            .retain(|_, previous| *previous != workspace);
    }
}
