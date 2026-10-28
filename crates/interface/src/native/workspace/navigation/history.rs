use super::*;
use crate::host::OutputId;
use std::collections::{BTreeMap, BTreeSet};

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
        .get::<crate::native::layout::desktop::HostViewport>(root)
        .map(|viewport| viewport.0.id)
}

pub fn capture(world: &World, root: Entity) -> Option<Selection> {
    let spaces = world.get::<Workspaces>(root)?;
    let workspace = spaces.active;
    if !spaces.entries.iter().any(|entry| entry.id == workspace)
        || crate::native::laboratory::temporary(world, root, workspace)
    {
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
            || crate::native::laboratory::temporary(world, self.root, active)
        {
            return;
        }
        let workspaces: BTreeSet<_> = spaces
            .entries
            .iter()
            .filter(|entry| !crate::native::laboratory::temporary(world, self.root, entry.id))
            .map(|entry| entry.id)
            .collect();
        if !workspaces.contains(&active) || !workspaces.contains(&self.workspace) {
            return;
        }
        let outputs: BTreeSet<_> = world
            .get_resource::<crate::native::output_views::OutputViews>()
            .filter(|views| views.applies_to(self.root))
            .map(|views| views.outputs.iter().map(|output| Some(output.id)).collect())
            .unwrap_or_else(|| BTreeSet::from([self.output]));
        let mut root = world.entity_mut(self.root);
        if !root.contains::<History>() {
            root.insert(History::default());
        }
        let mut history = root.get_mut::<History>().unwrap();
        if history.canvas != self.canvas {
            history.canvas = self.canvas;
            history.previous.clear();
        }
        history
            .previous
            .retain(|output, workspace| outputs.contains(output) && workspaces.contains(workspace));
        history.previous.insert(self.output, self.workspace);
    }
}

pub fn numeric_destination(world: &World, root: Entity, requested: u64) -> u64 {
    let enabled = world
        .get_resource::<crate::native::shortcuts::configuration::LayoutSettings>()
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
        .filter(|previous| {
            *previous != requested
                && spaces.entries.iter().any(|entry| entry.id == *previous)
                && !crate::native::laboratory::temporary(world, root, *previous)
        })
        .unwrap_or(requested)
}

pub(crate) fn forget(world: &mut World, root: Entity, workspace: u64) {
    if let Some(mut history) = world.get_mut::<History>(root) {
        history
            .previous
            .retain(|_, previous| *previous != workspace);
    }
}
