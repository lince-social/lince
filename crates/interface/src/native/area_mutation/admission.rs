use super::{Suspended, armed};
use crate::native::{
    area::InfluenceArea,
    influence_report::{Outcome, activity},
    laboratory::blocked,
    practice_cells::sender,
    topology::holes::live,
    workspace::WorkspaceMember,
};
use bevy::prelude::*;

pub(super) fn candidates(
    world: &mut World,
    areas: &mut QueryState<(
        Entity,
        &'static InfluenceArea,
        &'static ChildOf,
        &'static WorkspaceMember,
    )>,
) -> Vec<(Entity, Entity)> {
    let mut examined = 0;
    let candidates = areas
        .iter(world)
        .filter_map(|(entity, area, parent, member)| {
            if !area.enabled
                || area.paused
                || !area.changes_enabled
                || area.changes.is_empty()
                || armed(world, entity)
            {
                return None;
            }
            examined += 1;
            let root = parent.parent();
            let workspace = member.0;
            (!blocked(world, entity)
                && activity(world, entity, area, root, workspace) == Outcome::Active
                && sender(world, entity).is_some()
                && world
                    .get::<Suspended>(entity)
                    .is_none_or(|suspended| &suspended.0 != area)
                && live(world, root, workspace))
            .then_some((entity, root))
        })
        .collect();
    nucleus::performance::add("area-mutation-activation-candidates", examined);
    candidates
}

pub(super) fn same_configuration(
    current: &InfluenceArea,
    previous: &InfluenceArea,
    follows: bool,
) -> bool {
    // Color and creation edits leave admission intact. Followed Areas can move, but NaN
    // still invalidates the current center or opacity.
    let InfluenceArea {
        backend_components,
        records,
        id,
        name,
        enabled,
        paused,
        include_right_edge,
        attraction_enabled,
        changes_enabled,
        change_filter,
        color: _,
        opacity,
        center,
        size,
        depth,
        target,
        shape,
        direction,
        strength,
        reach,
        rules,
        match_all,
        changes,
        creation: _,
        protein,
        filter,
        sound,
        sorting,
        immunity,
        scale,
        graph_layout,
        force_mode,
        hole,
    } = current;
    backend_components == &previous.backend_components
        && enabled == &previous.enabled
        && paused == &previous.paused
        && include_right_edge == &previous.include_right_edge
        && attraction_enabled == &previous.attraction_enabled
        && changes_enabled == &previous.changes_enabled
        && !opacity.is_nan()
        && if follows {
            center.iter().all(|value| !value.is_nan())
        } else {
            center == &previous.center
        }
        && size == &previous.size
        && depth == &previous.depth
        && target == &previous.target
        && direction == &previous.direction
        && strength == &previous.strength
        && reach == &previous.reach
        && match_all == &previous.match_all
        && immunity == &previous.immunity
        && scale == &previous.scale
        && graph_layout == &previous.graph_layout
        && force_mode == &previous.force_mode
        && hole == &previous.hole
        && id == &previous.id
        && name == &previous.name
        && shape == &previous.shape
        && rules == &previous.rules
        && changes == &previous.changes
        && sound == &previous.sound
        && sorting == &previous.sorting
        && change_filter == &previous.change_filter
        && protein == &previous.protein
        && filter == &previous.filter
        && records == &previous.records
}
