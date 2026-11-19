use super::*;
use crate::{
    layout::desktop::Workspace,
    native::shortcuts::configuration::{ApplicationOpening, LayoutSettings},
};
use std::sync::LazyLock;

static DEFAULTS: LazyLock<LayoutSettings> = LazyLock::new(LayoutSettings::default);

#[derive(Clone, Copy)]
pub(super) struct OpeningSettings {
    pub(super) default_width: [u16; 2],
    application_opening: ApplicationOpening,
}

#[derive(Resource)]
struct Synchronization {
    roots: QueryState<(Entity, &'static Workspaces)>,
    changed: Vec<(Entity, u64)>,
}

impl FromWorld for Synchronization {
    fn from_world(world: &mut World) -> Self {
        Self {
            roots: world.query(),
            changed: Vec::new(),
        }
    }
}

pub(super) fn synchronize(world: &mut World) -> OpeningSettings {
    let _synchronization = engine::performance::Span::new("desktop-layout-preferences");
    let settings = world
        .get_resource::<LayoutSettings>()
        .unwrap_or_else(|| &DEFAULTS);
    let gap = f64::from(settings.gaps);
    let opening = OpeningSettings {
        default_width: settings.default_width,
        application_opening: settings.application_opening,
    };
    world.init_resource::<Synchronization>();
    world.resource_scope(|world, mut synchronization: Mut<Synchronization>| {
        let Synchronization { roots, changed } = &mut *synchronization;
        changed.clear();
        changed.extend(roots.iter(world).flat_map(|(root, spaces)| {
            spaces.entries.iter().filter_map(move |workspace| {
                (workspace.desktop.gap() != gap).then_some((root, workspace.id))
            })
        }));
        for &(root, id) in changed.iter() {
            if let Some(workspace) = world.get_mut::<Workspaces>(root).and_then(|spaces| {
                spaces
                    .into_inner()
                    .entries
                    .iter_mut()
                    .find(|workspace| workspace.id == id)
            }) {
                if workspace.desktop.gap() != gap && workspace.desktop.set_gap(gap).is_ok() {
                    engine::performance::add("desktop-layout-gaps-written", 1);
                }
            }
        }
        if !changed.is_empty() {
            let mut runtime = world.resource_mut::<Runtime>();
            for key in changed.iter() {
                runtime.influences.remove(key);
                runtime.views.remove(key);
            }
        }
    });
    opening
}

pub(super) fn opening(
    world: &mut World,
    root: Entity,
    workspace: u64,
    entity: Entity,
    id: PlacementId,
    model: &mut Workspace,
    settings: &OpeningSettings,
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
    let retained = world
        .get::<super::opening::Decision>(entity)
        .is_some_and(|decision| decision.retained);
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
