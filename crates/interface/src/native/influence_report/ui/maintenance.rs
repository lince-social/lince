use super::{
    InfluenceArea, Inspected, Report, View, WorkspaceMember, Workspaces, controls, description,
};
use crate::native::{area_panel::owns, canvas_selection::eligible, workspace_config::rules};
use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Preparation {
    views: Vec<View>,
    areas: Vec<Entity>,
    affected: HashSet<Entity>,
}

pub(super) fn update(
    world: &mut World,
    views_query: &mut QueryState<&'static View>,
    area_query: &mut QueryState<
        (Entity, &'static ChildOf, &'static WorkspaceMember),
        With<InfluenceArea>,
    >,
    mut preparation: Local<Preparation>,
) {
    let _presentation = nucleus::performance::Span::new("influence-panel-update");
    let Preparation {
        views,
        areas,
        affected,
    } = &mut *preparation;
    views.clear();
    views.extend(views_query.iter(world).copied());
    if views.is_empty() {
        *preparation = Preparation::default();
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
            .filter(|sand| eligible(world, root, *sand));
        description::update(world, text, root, sand, area);
        let rules = rules(world, root, workspace);
        affected.clear();
        areas.clear();
        let owned = |entity, parent: &ChildOf, member: &WorkspaceMember| {
            parent.parent() == root && member.0 == workspace && owns(world, root, entity)
        };
        if let Some(entity) = area {
            if area_query
                .get(world, entity)
                .is_ok_and(|(entity, parent, member)| owned(entity, parent, member))
            {
                areas.push(entity);
            }
        } else if let Some(report) = sand
            .and_then(|sand| world.get::<Report>(sand))
            .filter(|report| !report.entries.is_empty())
        {
            affected.extend(report.entries.iter().map(|entry| entry.area));
            areas.extend(
                area_query
                    .iter(world)
                    .filter_map(|(entity, parent, member)| {
                        (affected.contains(&entity) && owned(entity, parent, member))
                            .then_some(entity)
                    }),
            );
        }
        controls::update(world, controls, root, workspace, rules, areas);
    }
    areas.clear();
    affected.clear();
}
