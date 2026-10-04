use crate::{
    area::InfluenceArea,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{math::DVec3, prelude::*};
use std::collections::HashSet;

pub(crate) mod tests;
pub(crate) mod ui;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    Active,
    Disabled,
    Paused,
    Filter,
    Reach,
    Outside,
    Immune(Entity),
    Group,
    Pinned,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Evaluation {
    pub area: Entity,
    pub motion: Outcome,
    pub force: DVec3,
    pub slot: Option<DVec3>,
    pub size: Outcome,
    pub scale: f32,
    pub inside: bool,
}

impl Evaluation {
    pub fn new(area: Entity) -> Self {
        Self {
            area,
            motion: Outcome::Active,
            force: DVec3::ZERO,
            slot: None,
            size: Outcome::Outside,
            scale: 1.0,
            inside: false,
        }
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub(crate) struct Report {
    pub entries: Vec<Evaluation>,
    pub total: DVec3,
    pub scale: f32,
}

#[derive(Component)]
pub(crate) struct Inspected(pub Entity);

#[derive(Resource, Default)]
pub(crate) struct Tracked(pub HashSet<Entity>);

pub(crate) fn tracked(world: &World, sand: Entity) -> bool {
    world
        .get_resource::<Tracked>()
        .is_some_and(|tracked| tracked.0.contains(&sand))
}

pub(crate) fn store(world: &mut World, sand: Entity, report: Report) {
    if world.get::<Report>(sand) != Some(&report) {
        world.entity_mut(sand).insert(report);
    }
}

pub(crate) fn track(world: &mut World) {
    let roots: Vec<_> = world
        .query::<(Entity, &Workspaces)>()
        .iter(world)
        .map(|(e, _)| e)
        .collect();
    let mut tracked = HashSet::new();
    for root in roots {
        let chosen = crate::canvas_selection::selected(world, root)
            .into_iter()
            .find(|sand| world.get::<InfluenceArea>(*sand).is_none())
            .or_else(|| {
                world
                    .get::<crate::inspection::Inspection>(root)
                    .and_then(|i| i.selected)
            });
        if let Some(sand) = chosen.filter(|sand| {
            world.get::<InfluenceArea>(*sand).is_none()
                && crate::canvas_selection::eligible(world, root, *sand)
        }) {
            world.entity_mut(root).insert(Inspected(sand));
        }
        if let Some(sand) = world.get::<Inspected>(root).map(|s| s.0) {
            if crate::canvas_selection::eligible(world, root, sand) {
                tracked.insert(sand);
            } else {
                world.entity_mut(root).remove::<Inspected>();
            }
        }
    }
    let stale: Vec<_> = world
        .query_filtered::<Entity, With<Report>>()
        .iter(world)
        .filter(|sand| !tracked.contains(sand))
        .collect();
    for sand in stale {
        world.entity_mut(sand).remove::<Report>();
    }
    world.insert_resource(Tracked(tracked));
}

pub(crate) fn activity(
    world: &World,
    entity: Entity,
    area: &InfluenceArea,
    root: Entity,
    workspace: u64,
) -> Outcome {
    if !area.enabled {
        return Outcome::Disabled;
    }
    let owner_paused = world
        .get::<crate::protein_area::grouping::GeneratedGroup>(entity)
        .and_then(|group| world.get::<InfluenceArea>(group.owner))
        .is_some_and(|owner| owner.paused || !owner.enabled);
    if area.paused
        || owner_paused
        || !crate::workspace_config::rules_enabled(world, root, workspace)
    {
        Outcome::Paused
    } else {
        Outcome::Active
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Control {
    Workspace,
    Area(Entity),
    Pending(u16),
    Repeats(u16),
}

impl crate::actions::Action for Control {
    fn apply(&self, world: &mut World, root: Entity) {
        let Some(workspace) = world.get::<Workspaces>(root).map(|spaces| spaces.active) else {
            return;
        };
        if let Self::Area(area) = self {
            if !crate::area_panel::owns(world, root, *area) {
                return;
            }
            let paused = !world.get::<InfluenceArea>(*area).unwrap().paused;
            world.get_mut::<InfluenceArea>(*area).unwrap().paused = paused;
            crate::area_mutation::disarm(
                world,
                *area,
                if paused {
                    "Area paused. Already submitted changes may still finish."
                } else {
                    "Area resumed. Future crossings can apply configured changes."
                },
            );
            return;
        }
        let mut rules = crate::workspace_config::rules(world, root, workspace);
        match self {
            Self::Workspace => rules.paused = !rules.paused,
            Self::Pending(value) => rules.max_pending = *value,
            Self::Repeats(value) => rules.max_repeats = *value,
            Self::Area(_) => unreachable!(),
        }
        if crate::workspace_config::set_rules(world, root, workspace, rules) && rules.paused {
            let areas: Vec<_> = world
                .query::<(Entity, &InfluenceArea, &ChildOf, &WorkspaceMember)>()
                .iter(world)
                .filter(|(_, _, p, m)| p.parent() == root && m.0 == workspace)
                .map(|(e, ..)| e)
                .collect();
            for area in areas {
                crate::area_mutation::disarm(
                    world,
                    area,
                    "Workspace rules paused. Already submitted changes may still finish.",
                );
            }
        }
    }
}
