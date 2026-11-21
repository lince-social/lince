// non-lean-yet
// Scope: struct Preparation, fn update.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;

#[derive(Default)]
pub(super) struct Preparation {
    views: Vec<View>,
    areas: Vec<Entity>,
    affected: HashSet<Entity>,
}

pub(super) fn update(
    world: &mut World,
    views_query: &mut QueryState<(Entity, &'static View)>,
    area_query: &mut QueryState<(
        Entity,
        &'static InfluenceArea,
        &'static ChildOf,
        &'static WorkspaceMember,
    )>,
    mut preparation: Local<Preparation>,
) {
    let _presentation = nucleus::performance::Span::new("influence-panel-update");
    let Preparation {
        views,
        areas,
        affected,
    } = &mut *preparation;
    views.clear();
    views.extend(views_query.iter(world).map(|(_, view)| *view));
    if views.is_empty() {
        *views = Vec::new();
        *areas = Vec::new();
        *affected = HashSet::new();
        return;
    }
    for View {
        root,
        area,
        text,
        controls,
    } in views.drain(..)
    {
        let Some(workspace) = world.get::<Workspaces>(root).map(|spaces| spaces.active) else {
            continue;
        };
        let sand = world
            .get::<Inspected>(root)
            .map(|inspected| inspected.0)
            .filter(|sand| crate::native::canvas_selection::eligible(world, root, *sand));
        description::update(world, text, root, sand, area);
        let rules = crate::native::workspace_config::rules(world, root, workspace);
        affected.clear();
        if area.is_none()
            && let Some(report) = sand.and_then(|sand| world.get::<Report>(sand))
        {
            affected.extend(report.entries.iter().map(|entry| entry.area));
        }
        areas.clear();
        areas.extend(
            area_query
                .iter(world)
                .filter(|(entity, _, parent, member)| {
                    parent.parent() == root
                        && member.0 == workspace
                        && area.is_none_or(|area| area == *entity)
                        && crate::native::area_panel::owns(world, root, *entity)
                        && (area.is_some() || affected.contains(entity))
                })
                .map(|(entity, ..)| entity),
        );
        controls::update(world, controls, root, workspace, rules, areas);
    }
    areas.clear();
    affected.clear();
}
