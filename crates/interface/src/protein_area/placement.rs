use super::{Config, RecordBinding, SpawnPlacement, filter};
use crate::{
    area::{InfluenceArea, RecordProperties},
    topology::Spatial,
    workspace::WorkspaceMember,
};
use bevy::{
    math::{DVec2, DVec3},
    prelude::*,
};

#[derive(Component)]
pub(super) struct Placed;

#[derive(Component)]
pub(crate) struct Pending;

#[derive(Component)]
struct Settling {
    point: DVec3,
    velocity: DVec3,
    remaining: u16,
}

#[derive(Resource)]
struct Budget(usize);

pub(super) fn begin_frame(world: &mut World) {
    world.insert_resource(Budget(4096));
}

struct Target {
    entity: Entity,
    area: InfluenceArea,
    spatial: Spatial,
    order: usize,
}

pub(super) fn initial(
    world: &mut World,
    entity: Entity,
    source: Entity,
    config: &Config,
    fallback: DVec3,
    size: Vec2,
) -> Option<DVec3> {
    if config.placement == SpawnPlacement::Source {
        return Some(fallback);
    }
    let root = world.get::<ChildOf>(source)?.parent();
    let workspace = world.get::<WorkspaceMember>(source)?.0;
    let record = world.get::<RecordProperties>(entity)?.clone();
    let binding = world.get::<RecordBinding>(entity)?.clone();
    let candidates: Vec<_> = world
        .query::<(Entity, &InfluenceArea, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(e, area, parent, member)| {
            *e != source
                && parent.parent() == root
                && member.0 == workspace
                && area.enabled
                && (config.spawn_targets.is_empty() || config.spawn_targets.contains(&area.id))
        })
        .map(|(e, area, _, _)| (e, area.clone()))
        .collect();
    let mut targets = Vec::new();
    for (target, area) in candidates {
        let matches = if area.filter.is_some() {
            let Some(matches) = world.get::<filter::Matches>(target).filter(|m| m.current) else {
                return None;
            };
            matches.allows(&record, Some(&binding))
        } else {
            area.matches(&record)
        };
        if !matches {
            continue;
        }
        let order = super::ordered_records(world, target)
            .and_then(|(_, ids)| {
                ids.iter().position(|id| id == &binding.uid).map(|index| {
                    if area.sorting.as_ref().is_some_and(|sort| sort.reverse) {
                        ids.len() - 1 - index
                    } else {
                        index
                    }
                })
            })
            .unwrap_or(0);
        targets.push(Target {
            entity: target,
            area,
            spatial: crate::topology::spatial(world, target),
            order,
        });
    }
    targets.sort_by(|a, b| a.area.id.cmp(&b.area.id));
    if config.placement == SpawnPlacement::MatchingAreas {
        let Some(target) = targets.first() else {
            return Some(fallback);
        };
        let center = DVec2::from_array(target.area.center);
        let mut offset = target.area.target_position() - center;
        if let Some(sort) = &target.area.sorting {
            let available = if sort.horizontal {
                target.area.size[0]
            } else {
                target.area.size[1]
            };
            let extent = if sort.horizontal { size.x } else { size.y };
            let value = -available * 0.5
                + 12.0
                + f64::from(extent) * 0.5
                + target.order as f64 * f64::from(extent + config.gap);
            let half = ((available - f64::from(extent)) * 0.5).max(0.0);
            let value = value.clamp(-half, half);
            if sort.horizontal {
                offset.x = value;
            } else {
                offset.y = value;
            }
        }
        return Some(
            target.spatial.position(center)
                + target.spatial.rotation() * DVec3::new(offset.x, 0.0, offset.y),
        );
    }
    let mut state = world
        .entity_mut(entity)
        .take::<Settling>()
        .unwrap_or(Settling {
            point: fallback,
            velocity: DVec3::ZERO,
            remaining: config.settling_ticks,
        });
    let allowance = world
        .get_resource::<Budget>()
        .map_or(16, |b| b.0 / targets.len().max(1));
    let steps = usize::from(state.remaining).min(16).min(allowance);
    if let Some(mut budget) = world.get_resource_mut::<Budget>() {
        budget.0 -= steps * targets.len().max(1);
    }
    for _ in 0..steps {
        let mut force = DVec3::ZERO;
        for target in &targets {
            if crate::topology::influence::blocked(
                world,
                root,
                workspace,
                target.entity,
                state.point,
                Some(&record),
                Some(&binding),
            ) {
                continue;
            }
            force += crate::topology::influence::attraction_force(
                &target.area,
                target.spatial,
                state.point,
            );
            if let Some(sort) = &target.area.sorting {
                if let Some(point) = world
                    .get_resource::<crate::area_effects::Influences>()
                    .and_then(|fields| fields.topology_target(target.entity, entity))
                {
                    let center = DVec2::from_array(target.area.center);
                    let offset = point - center;
                    let destination = target.spatial.position(center)
                        + target.spatial.rotation() * DVec3::new(offset.x, 0.0, offset.y);
                    force += (destination - state.point).clamp_length_max(1.0) * sort.strength;
                }
            }
        }
        step(&mut state.point, &mut state.velocity, force);
        state.remaining -= 1;
    }
    if state.remaining == 0 {
        Some(state.point)
    } else {
        world.entity_mut(entity).insert(state);
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
        None
    }
}

fn step(point: &mut DVec3, velocity: &mut DVec3, force: DVec3) {
    let dt = 1.0 / 120.0;
    *velocity = (*velocity + force.clamp_length_max(1_000_000.0) * dt) / (1.0 + 4.0 * dt);
    *point += *velocity * dt;
}

pub(super) fn finish(world: &mut World, entity: Entity, point: DVec3) {
    crate::topology::set_position(world, entity, point);
    world.entity_mut(entity).remove::<Pending>().insert(Placed);
    world.entity_mut(entity).insert(Visibility::Inherited);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_waits_for_matches_respects_source_and_ignores_other_workspaces() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let source = world.spawn((ChildOf(root), WorkspaceMember(1))).id();
        let record = RecordProperties(serde_json::json!({"uid":"r_task", "quantity":-1}));
        let sand = world
            .spawn((
                record,
                RecordBinding {
                    area: source,
                    uid: "r_task".into(),
                    source: super::super::Source::Local,
                },
            ))
            .id();
        let mut area = InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::new(500.0, 0.0),
            DVec2::splat(300.0),
        );
        area.filter = Some(Config::default());
        let target = world
            .spawn((area.clone(), ChildOf(root), WorkspaceMember(1)))
            .id();
        let config = Config {
            placement: SpawnPlacement::MatchingAreas,
            spawn_targets: vec![area.id.clone()],
            ..Default::default()
        };
        let fallback = DVec3::new(-200.0, 0.0, 500.0);
        assert!(
            initial(
                &mut world,
                sand,
                source,
                &config,
                fallback,
                Vec2::splat(80.0)
            )
            .is_none()
        );
        world.entity_mut(target).insert(filter::Matches {
            source: super::super::Source::Local,
            current: true,
            uids: std::collections::HashSet::from(["r_task".into()]),
        });
        assert_eq!(
            initial(
                &mut world,
                sand,
                source,
                &config,
                fallback,
                Vec2::splat(80.0)
            ),
            Some(DVec3::new(500.0, 0.0, 0.0))
        );
        world.get_mut::<WorkspaceMember>(target).unwrap().0 = 2;
        assert_eq!(
            initial(
                &mut world,
                sand,
                source,
                &config,
                fallback,
                Vec2::splat(80.0)
            ),
            Some(fallback)
        );
        world.get_mut::<WorkspaceMember>(target).unwrap().0 = 1;
        world.get_mut::<filter::Matches>(target).unwrap().source =
            super::super::Source::Organ("o_remote".into());
        assert_eq!(
            initial(
                &mut world,
                sand,
                source,
                &config,
                fallback,
                Vec2::splat(80.0)
            ),
            Some(fallback)
        );
        world.get_mut::<InfluenceArea>(target).unwrap().enabled = false;
        assert_eq!(
            initial(
                &mut world,
                sand,
                source,
                &config,
                fallback,
                Vec2::splat(80.0)
            ),
            Some(fallback)
        );
    }

    #[test]
    fn initial_physics_finishes_in_bounded_batches_without_pointer_events_or_record_changes() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let source = world.spawn((ChildOf(root), WorkspaceMember(1))).id();
        let properties = serde_json::json!({"uid":"r_task", "quantity":-1});
        let sand = world
            .spawn((
                RecordProperties(properties.clone()),
                RecordBinding {
                    area: source,
                    uid: "r_task".into(),
                    source: super::super::Source::Local,
                },
            ))
            .id();
        let mut area = InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(100.0),
        );
        area.strength = 100.0;
        area.reach.mode = crate::area::ReachMode::Unlimited;
        area.rules.push(crate::area::PropertyRule {
            property: crate::area::Property::Quantity,
            value: "-1".into(),
        });
        let config = Config {
            placement: SpawnPlacement::Physics,
            settling_ticks: 120,
            spawn_targets: vec![area.id.clone()],
            ..Default::default()
        };
        world.spawn((
            area,
            Spatial {
                elevation: 80.0,
                ..Default::default()
            },
            ChildOf(root),
            WorkspaceMember(1),
        ));
        let start = DVec3::new(200.0, 0.0, 0.0);
        for _ in 0..7 {
            begin_frame(&mut world);
            assert!(initial(&mut world, sand, source, &config, start, Vec2::splat(40.0)).is_none());
        }
        begin_frame(&mut world);
        let point = initial(&mut world, sand, source, &config, start, Vec2::splat(40.0)).unwrap();
        assert!(point.x < start.x && point.y > start.y);
        assert_eq!(world.get::<RecordProperties>(sand).unwrap().0, properties);
    }

    #[test]
    fn fixed_steps_are_independent_of_interaction_and_frame_grouping() {
        let mut point = DVec3::ZERO;
        let mut velocity = DVec3::ZERO;
        let mut grouped = point;
        let mut grouped_velocity = velocity;
        for _ in 0..120 {
            step(&mut point, &mut velocity, DVec3::X * 100.0);
        }
        for _ in 0..15 {
            for _ in 0..8 {
                step(&mut grouped, &mut grouped_velocity, DVec3::X * 100.0);
            }
        }
        assert_eq!(point, grouped);
        assert_eq!(velocity, grouped_velocity);
        assert!(point.x > 15.0);
    }
}
