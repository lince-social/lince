use crate::native::area::InfluenceArea;
use crate::native::workspace::WorkspaceMember;
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
        .filter(|(entity, area, _, _)| {
            area.enabled
                && !area.paused
                && area.changes_enabled
                && !area.changes.is_empty()
                && !super::armed(world, *entity)
        })
        .filter(|(entity, area, parent, member)| {
            examined += 1;
            !crate::native::laboratory::blocked(world, *entity)
                && crate::native::influence_report::activity(
                    world,
                    *entity,
                    area,
                    parent.parent(),
                    member.0,
                ) == crate::native::influence_report::Outcome::Active
                && crate::native::practice_cells::sender(world, *entity).is_some()
                && world
                    .get::<super::Suspended>(*entity)
                    .is_none_or(|s| &s.0 != *area)
                && crate::native::topology::holes::live(world, parent.parent(), member.0)
        })
        .map(|(entity, _, parent, _)| (entity, parent.parent()))
        .collect();
    nucleus::performance::add("area-mutation-activation-candidates", examined);
    candidates
}

pub(super) fn same_configuration(
    current: &InfluenceArea,
    previous: &InfluenceArea,
    follows: bool,
) -> bool {
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
