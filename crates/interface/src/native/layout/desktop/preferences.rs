use super::*;
use crate::{
    layout::desktop::Workspace,
    native::shortcuts::configuration::{ApplicationOpening, LayoutSettings},
};

pub(super) fn synchronize(world: &mut World) -> LayoutSettings {
    let settings = world
        .get_resource::<LayoutSettings>()
        .cloned()
        .unwrap_or_default();
    let gap = f64::from(settings.gaps);
    let changed: Vec<_> = world
        .query::<(Entity, &Workspaces)>()
        .iter(world)
        .flat_map(|(root, spaces)| {
            spaces.entries.iter().filter_map(move |workspace| {
                (workspace.desktop.gap() != gap).then_some((root, workspace.id))
            })
        })
        .collect();
    for (root, id) in &changed {
        if let Some(workspace) = world.get_mut::<Workspaces>(*root).and_then(|spaces| {
            spaces
                .into_inner()
                .entries
                .iter_mut()
                .find(|workspace| workspace.id == *id)
        }) {
            let _ = workspace.desktop.set_gap(gap);
        }
    }
    let mut runtime = world.resource_mut::<Runtime>();
    for key in changed {
        runtime.influences.remove(&key);
        runtime.views.remove(&key);
    }
    settings
}

pub(super) fn opening(
    world: &mut World,
    root: Entity,
    workspace: u64,
    entity: Entity,
    id: PlacementId,
    model: &mut Workspace,
    settings: &LayoutSettings,
) {
    if world
        .entity_mut(entity)
        .take::<ApplicationOpeningPending>()
        .is_none()
        || world.get::<ApplicationWindow>(entity).is_none()
        || world.get::<TransientParent>(entity).is_some()
    {
        return;
    }
    let retained = world.get::<super::opening::Decision>(entity).is_some_and(|decision| decision.retained);
    if !retained && model.mode() == WorkspaceLayout::Free {
        let scale = world
            .resource::<Runtime>()
            .fullscreen_views
            .get(&(root, workspace))
            .map(|saved| saved.canvas.zoom)
            .unwrap_or_else(|| crate::native::output_views::canvas(world, root, workspace).zoom);
        let _ = model.set_free_width(id, settings.default_width, scale);
    }
    if !retained && settings.application_opening == ApplicationOpening::Maximized {
        model.request_state(
            id,
            crate::host::WindowRequest {
                maximized: Some(true),
                ..default()
            },
        );
    }
    super::opening::apply_initial(world, root, workspace, entity, id, model);
}

#[cfg(test)]
mod tests;
