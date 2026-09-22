use super::{Spatial, spatial};
use crate::area::{InfluenceArea, ReachMode, RecordProperties};
use bevy::{
    math::{DVec2, DVec3},
    prelude::*,
};
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct Forces {
    pub totals: HashMap<Entity, DVec3>,
    simple: HashMap<Entity, Destination>,
}

struct Destination {
    area: InfluenceArea,
    placement: Spatial,
    point: DVec3,
    strength: f64,
}

pub fn local(area: &InfluenceArea, placement: Spatial, point: DVec3) -> DVec3 {
    placement.local_point(DVec2::from_array(area.center), point)
}

pub fn contains(area: &InfluenceArea, placement: Spatial, point: DVec3) -> bool {
    let local = local(area, placement, point);
    local.y >= -area.depth - 1e-7
        && local.y <= 1e-7
        && area.contains(DVec2::from_array(area.center) + DVec2::new(local.x, local.z))
}

pub fn update(world: &mut World) {
    world.init_resource::<Forces>();
    let mut forces = world.remove_resource::<Forces>().unwrap();
    forces.totals.clear();
    let areas: Vec<_> = world
        .query::<(
            Entity,
            &InfluenceArea,
            &ChildOf,
            &crate::workspace::WorkspaceMember,
        )>()
        .iter(world)
        .filter(|(e, a, _, _)| {
            a.enabled
                && (a.validate()
                    || world
                        .get::<crate::protein_area::grouping::GeneratedGroup>(*e)
                        .is_some())
        })
        .map(|(e, a, p, m)| {
            (
                e,
                {
                    let mut area = a.clone();
                    if !area.attraction_enabled {
                        area.strength = 0.0;
                    }
                    area
                },
                p.parent(),
                m.0,
                spatial(world, e),
                world
                    .get::<crate::canvas_selection::SandGroup>(e)
                    .or_else(|| {
                        world
                            .get::<crate::protein_area::grouping::GeneratedGroup>(e)
                            .and_then(|generated| {
                                world.get::<crate::canvas_selection::SandGroup>(generated.owner)
                            })
                    })
                    .copied(),
            )
        })
        .collect();
    let sands: Vec<_> = world
        .query_filtered::<(
            Entity,
            &crate::canvas::CanvasItem,
            &ChildOf,
            &crate::workspace::WorkspaceMember,
        ), Without<InfluenceArea>>()
        .iter(world)
        .map(|(e, i, p, m)| (e, *i, p.parent(), m.0))
        .collect();
    let mut retained = std::collections::HashSet::new();
    let living: std::collections::HashSet<_> = sands.iter().map(|(entity, ..)| *entity).collect();
    let stale: Vec<_> = world
        .query_filtered::<Entity, Or<(
            With<crate::area_effects::AreaScale>,
            With<crate::area::AreaForces>,
        )>>()
        .iter(world)
        .filter(|entity| !living.contains(entity))
        .collect();
    for entity in stale {
        world
            .entity_mut(entity)
            .remove::<(crate::area_effects::AreaScale, crate::area::AreaForces)>();
    }
    for (entity, item, root, workspace) in sands {
        if world.get::<crate::sand_placement::Pinned>(entity).is_some()
            || !world
                .get::<crate::workspace::Workspaces>(root)
                .is_some_and(|spaces| spaces.active == workspace)
        {
            world
                .entity_mut(entity)
                .remove::<(crate::area_effects::AreaScale, crate::area::AreaForces)>();
            continue;
        }
        let point = spatial(world, entity).position(item.position);
        let record = world.get::<RecordProperties>(entity).cloned();
        let binding = world.get::<crate::protein_area::RecordBinding>(entity);
        let group = world
            .get::<crate::canvas_selection::SandGroup>(entity)
            .copied();
        let matches = |id, area: &InfluenceArea, all: bool| {
            if area.filter.is_some() {
                record.as_ref().is_some_and(|r| {
                    world
                        .get::<crate::protein_area::filter::Matches>(id)
                        .is_some_and(|f| f.allows(r, binding))
                })
            } else {
                all && area.rules.is_empty() || record.as_ref().is_some_and(|r| area.matches(r))
            }
        };
        let mut total = DVec3::ZERO;
        let mut displayed = crate::area::AreaForces::default();
        let mut scale = 1.0f64;
        for (id, area, area_root, member, placement, area_group) in &areas {
            if *area_root != root || *member != workspace {
                continue;
            }
            let blocked = areas.iter().any(
                |(shield_id, shield, shield_root, shield_member, shield_placement, _)| {
                    if shield_id == id
                        || *shield_root != root
                        || *shield_member != workspace
                        || !contains(shield, *shield_placement, point)
                        || !matches(*shield_id, shield, true)
                    {
                        return false;
                    }
                    let source_inside = contains(
                        shield,
                        *shield_placement,
                        placement.position(DVec2::from_array(area.center)),
                    );
                    match shield.immunity {
                        crate::area_effects::Immunity::None => false,
                        crate::area_effects::Immunity::All => true,
                        crate::area_effects::Immunity::Internal => source_inside,
                        crate::area_effects::Immunity::External => !source_inside,
                    }
                },
            );
            if blocked {
                continue;
            }
            if area.scale != 1.0 && contains(area, *placement, point) && matches(*id, area, true) {
                scale *= f64::from(area.scale);
            }
            if group.is_some() && group == *area_group {
                continue;
            }
            let relative = local(area, *placement, point);
            let planar = DVec2::from_array(area.center) + DVec2::new(relative.x, relative.z);
            if area.reach.mode == ReachMode::Limited
                && (relative.y < -area.depth - 1e-7 || relative.y > 1e-7 || !area.reaches(planar))
            {
                continue;
            }
            let before = total;
            let sorting_target = area
                .sorting
                .as_ref()
                .filter(|sort| sort.strength > 0.0)
                .and_then(|_| {
                    world
                        .resource::<crate::area_effects::Influences>()
                        .topology_target(*id, entity)
                });
            if let Some(sort) = &area.sorting
                && let Some(target) = sorting_target
            {
                let target = target - DVec2::from_array(area.center);
                let delta = DVec3::new(target.x, 0.0, target.y) - relative;
                total += placement.rotation()
                    * delta.normalize_or_zero()
                    * crate::area_effects::simple_strength(
                        delta.length(),
                        area.size[0].min(area.size[1]) * 0.5,
                        sort.strength,
                    );
            }
            if let Some(generated) = world.get::<crate::protein_area::grouping::GeneratedGroup>(*id)
            {
                let force = generated.force(area, entity, planar);
                total += placement.rotation() * DVec3::new(force.x, 0.0, force.y);
                display_force(&mut displayed, *id, total - before);
                continue;
            }
            if !matches(*id, area, false) {
                display_force(&mut displayed, *id, total - before);
                continue;
            }
            let sign = if area.direction == crate::area::Direction::Attract {
                1.0
            } else {
                -1.0
            };
            let destination = || {
                let target = area.target_position() - DVec2::from_array(area.center);
                placement.position(DVec2::from_array(area.center))
                    + placement.rotation() * DVec3::new(target.x, 0.0, target.y)
            };
            let force = if area.force_mode == crate::area_effects::ForceMode::Simple {
                retained.insert(*id);
                let create = || Destination {
                    area: area.clone(),
                    placement: *placement,
                    point: destination(),
                    strength: area.strength * sign,
                };
                let cached = forces.simple.entry(*id).or_insert_with(create);
                if cached.area != *area || cached.placement != *placement {
                    *cached = create();
                }
                let target = sorting_target
                    .filter(|_| area.direction == crate::area::Direction::Attract)
                    .map_or(cached.point, |target| {
                        let target = target - DVec2::from_array(area.center);
                        placement.position(DVec2::from_array(area.center))
                            + placement.rotation() * DVec3::new(target.x, 0.0, target.y)
                    });
                let delta = target - point;
                delta.normalize_or_zero()
                    * crate::area_effects::simple_strength(
                        delta.length(),
                        area.size[0].min(area.size[1]) * 0.5,
                        cached.strength,
                    )
            } else {
                attraction_force(area, *placement, point)
            };
            total += force;
            display_force(&mut displayed, *id, total - before);
        }
        if world.get::<crate::area::AreaForces>(entity) != Some(&displayed) {
            world.entity_mut(entity).insert(displayed);
        }
        forces.totals.insert(
            entity,
            if total.is_finite() {
                total.clamp_length_max(1_000_000.0)
            } else {
                DVec3::ZERO
            },
        );
        let scale = scale.clamp(0.05, 20.0) as f32;
        if scale == 1.0 {
            world
                .entity_mut(entity)
                .remove::<crate::area_effects::AreaScale>();
        } else {
            world
                .entity_mut(entity)
                .insert(crate::area_effects::AreaScale(scale));
        }
    }
    forces.simple.retain(|key, _| retained.contains(key));
    world.insert_resource(forces);
}

fn display_force(forces: &mut crate::area::AreaForces, area: Entity, force: DVec3) {
    if force.is_finite() && force != DVec3::ZERO {
        forces.0.push(crate::area::AreaForce {
            area,
            force: DVec2::new(force.x, force.z),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (World, Entity, Entity, Entity) {
        let mut world = World::new();
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let mut area = InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(100.0),
        );
        area.depth = 20.0;
        area.strength = 100.0;
        area.rules.push(crate::area::PropertyRule {
            property: crate::area::Property::Quantity,
            value: "-1".into(),
        });
        let owner = crate::area::spawn_area(&mut world, root, 1, area).unwrap();
        let sand = world
            .spawn((
                crate::canvas::CanvasItem {
                    position: DVec2::new(20.0, 0.0),
                    size: Vec2::splat(10.0),
                },
                RecordProperties(serde_json::json!({"uid":"r_one", "quantity":-1})),
                crate::workspace::WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        (world, root, owner, sand)
    }

    #[test]
    fn disabled_area_and_attraction_toggle_remove_cached_forces() {
        let (mut world, _, owner, sand) = fixture();
        update(&mut world);
        assert!(world.resource::<Forces>().totals[&sand].length() > 0.0);
        world
            .get_mut::<InfluenceArea>(owner)
            .unwrap()
            .attraction_enabled = false;
        update(&mut world);
        assert_eq!(
            world
                .resource::<Forces>()
                .totals
                .get(&sand)
                .copied()
                .unwrap_or_default(),
            DVec3::ZERO
        );
        world
            .get_mut::<InfluenceArea>(owner)
            .unwrap()
            .attraction_enabled = true;
        update(&mut world);
        assert!(world.resource::<Forces>().totals[&sand].length() > 0.0);
        world.get_mut::<InfluenceArea>(owner).unwrap().enabled = false;
        update(&mut world);
        assert_eq!(
            world
                .resource::<Forces>()
                .totals
                .get(&sand)
                .copied()
                .unwrap_or_default(),
            DVec3::ZERO
        );
    }

    #[test]
    fn simple_destinations_steer_in_three_dimensions_and_follow_area_edits() {
        let (mut world, _, owner, sand) = fixture();
        world.get_mut::<InfluenceArea>(owner).unwrap().reach.mode = ReachMode::Unlimited;
        let placement = Spatial {
            elevation: 70.0,
            rotation: bevy::math::DQuat::from_rotation_z(0.7).to_array(),
            ..default()
        };
        world.entity_mut(owner).insert(placement);
        let destination = placement.position(DVec2::ZERO);
        for offset in [
            DVec3::X * 100.0,
            -DVec3::X * 100.0,
            DVec3::Y * 100.0,
            DVec3::new(30.0, -20.0, 70.0),
            DVec3::ZERO,
        ] {
            super::super::set_position(&mut world, sand, destination + offset);
            update(&mut world);
            let forces = world.resource::<Forces>();
            assert!((forces.totals[&sand] + offset.normalize_or_zero() * 100.0).length() < 1e-10);
            assert_eq!(forces.simple[&owner].point, destination);
        }
        world.get_mut::<InfluenceArea>(owner).unwrap().target =
            crate::area::AttractionTarget::Point([200.0, -50.0]);
        update(&mut world);
        let shifted = placement.rotation() * DVec3::new(200.0, 0.0, -50.0);
        assert!(
            (world.resource::<Forces>().totals[&sand] - shifted.normalize() * 100.0).length()
                < 1e-10
        );
        world.get_mut::<InfluenceArea>(owner).unwrap().direction = crate::area::Direction::Repel;
        update(&mut world);
        assert!(
            (world.resource::<Forces>().totals[&sand] + shifted.normalize() * 100.0).length()
                < 1e-10
        );
        super::super::set_position(&mut world, owner, destination + DVec3::Y * 50.0);
        update(&mut world);
        assert!(
            (world.resource::<Forces>().simple[&owner].point
                - destination
                - shifted
                - DVec3::Y * 50.0)
                .length()
                < 1e-10
        );
        world.get_mut::<RecordProperties>(sand).unwrap().0["quantity"] = serde_json::json!(0);
        update(&mut world);
        assert_eq!(world.resource::<Forces>().totals[&sand], DVec3::ZERO);
        assert!(world.resource::<Forces>().simple.is_empty());
    }

    #[test]
    fn depth_gates_reach_scale_sorting_and_immunity_in_the_rotated_volume() {
        let (mut world, root, owner, sand) = fixture();
        let placement = Spatial {
            elevation: 50.0,
            rotation: bevy::math::DQuat::from_rotation_z(0.5).to_array(),
            ..default()
        };
        world.entity_mut(owner).insert(placement);
        world.get_mut::<InfluenceArea>(owner).unwrap().scale = 2.0;
        let point =
            |y| placement.position(DVec2::ZERO) + placement.rotation() * DVec3::new(20.0, y, 0.0);
        for (y, inside) in [
            (-10.0, true),
            (-21.0, false),
            (1.0, false),
            (-20.0, true),
            (0.0, true),
        ] {
            super::super::set_position(&mut world, sand, point(y));
            update(&mut world);
            assert_eq!(
                world.resource::<Forces>().totals[&sand].length() > 0.0,
                inside
            );
            assert_eq!(
                world.get::<crate::area_effects::AreaScale>(sand).is_some(),
                inside
            );
        }
        super::super::set_position(&mut world, sand, point(-21.0));
        world.get_mut::<InfluenceArea>(owner).unwrap().reach.mode = ReachMode::Unlimited;
        update(&mut world);
        assert!(world.resource::<Forces>().totals[&sand].length() > 0.0);
        assert!(world.get::<crate::area_effects::AreaScale>(sand).is_none());
        let mut shield = InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(100.0),
        );
        shield.depth = 20.0;
        shield.immunity = crate::area_effects::Immunity::All;
        let shield = crate::area::spawn_area(&mut world, root, 1, shield).unwrap();
        world.entity_mut(shield).insert(placement);
        update(&mut world);
        assert!(world.resource::<Forces>().totals[&sand].length() > 0.0);
        world.get_mut::<InfluenceArea>(shield).unwrap().depth = 30.0;
        update(&mut world);
        assert_eq!(world.resource::<Forces>().totals[&sand], DVec3::ZERO);
        world.despawn(shield);
        {
            let mut area = world.get_mut::<InfluenceArea>(owner).unwrap();
            area.reach.mode = ReachMode::Limited;
            area.strength = 0.0;
            area.sorting = Some(crate::area_effects::Sorting::default());
        }
        crate::area_effects::refresh(&mut world);
        update(&mut world);
        assert_eq!(world.resource::<Forces>().totals[&sand], DVec3::ZERO);
        world.get_mut::<InfluenceArea>(owner).unwrap().depth = 30.0;
        update(&mut world);
        assert!(world.resource::<Forces>().totals[&sand].length() > 0.0);
        world
            .entity_mut(sand)
            .insert(crate::sand_placement::Pinned {
                anchor: [0.5; 2],
                scale: 1.0,
            });
        update(&mut world);
        assert!(!world.resource::<Forces>().totals.contains_key(&sand));
        assert!(world.get::<crate::area::AreaForces>(sand).is_none());
        assert!(world.get::<crate::area_effects::AreaScale>(sand).is_none());
    }

    #[test]
    fn area_depth_and_rotation_control_actual_membership() {
        let mut area = InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(100.0),
        );
        area.depth = 20.0;
        let placement = Spatial {
            elevation: 50.0,
            rotation: bevy::math::DQuat::from_rotation_z(0.5).to_array(),
            ..default()
        };
        let point = |p| placement.position(DVec2::ZERO) + placement.rotation() * p;
        assert!(contains(
            &area,
            placement,
            point(DVec3::new(0.0, -10.0, 0.0))
        ));
        assert!(!contains(
            &area,
            placement,
            point(DVec3::new(0.0, -21.0, 0.0))
        ));
        assert!(!contains(
            &area,
            placement,
            point(DVec3::new(51.0, -10.0, 0.0))
        ));
    }

    #[test]
    fn an_attached_area_cannot_drive_its_group_but_can_drive_another_sand() {
        let mut world = World::new();
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let group = crate::canvas_selection::SandGroup([7; 16]);
        let mut area = InfluenceArea::new(
            crate::area::AreaShape::Square,
            DVec2::ZERO,
            DVec2::splat(1000.0),
        );
        area.strength = 100.0;
        area.rules.push(crate::area::PropertyRule {
            property: crate::area::Property::Quantity,
            value: "-1".into(),
        });
        let area = crate::area::spawn_area(&mut world, root, 1, area).unwrap();
        world.entity_mut(area).insert(group);
        let mut sands = Vec::new();
        for x in [100.0, 200.0] {
            sands.push(
                world
                    .spawn((
                        crate::canvas::CanvasItem {
                            position: DVec2::new(x, 0.0),
                            size: Vec2::splat(20.0),
                        },
                        ChildOf(root),
                        crate::workspace::WorkspaceMember(1),
                        RecordProperties(serde_json::json!({"quantity": -1})),
                    ))
                    .id(),
            );
        }
        world.entity_mut(sands[0]).insert(group);
        update(&mut world);
        assert_eq!(world.resource::<Forces>().totals[&sands[0]], DVec3::ZERO);
        assert!(world.resource::<Forces>().totals[&sands[1]].x < 0.0);
    }
}

pub(crate) fn blocked(
    world: &mut World,
    root: Entity,
    workspace: u64,
    source: Entity,
    point: DVec3,
    record: Option<&RecordProperties>,
    binding: Option<&crate::protein_area::RecordBinding>,
) -> bool {
    let Some(center) = super::position(world, source) else {
        return false;
    };
    world
        .query::<(
            Entity,
            &InfluenceArea,
            &ChildOf,
            &crate::workspace::WorkspaceMember,
            Option<&crate::protein_area::filter::Matches>,
        )>()
        .iter(world)
        .any(|(entity, shield, parent, member, filter)| {
            if !shield.enabled
                || entity == source
                || parent.parent() != root
                || member.0 != workspace
                || !contains(shield, spatial(world, entity), point)
            {
                return false;
            }
            let inside = contains(shield, spatial(world, entity), center);
            let blocks = match shield.immunity {
                crate::area_effects::Immunity::None => false,
                crate::area_effects::Immunity::All => true,
                crate::area_effects::Immunity::Internal => inside,
                crate::area_effects::Immunity::External => !inside,
            };
            blocks
                && if shield.filter.is_some() {
                    record.is_some_and(|r| filter.is_some_and(|f| f.allows(r, binding)))
                } else {
                    shield.rules.is_empty() || record.is_some_and(|r| shield.matches(r))
                }
        })
}

pub(crate) fn attraction_force(area: &InfluenceArea, placement: Spatial, point: DVec3) -> DVec3 {
    if !area.enabled || !area.attraction_enabled || area.strength == 0.0 {
        return DVec3::ZERO;
    }
    let relative = local(area, placement, point);
    let center = DVec2::from_array(area.center);
    if area.reach.mode == ReachMode::Limited
        && (relative.y < -area.depth - 1e-7
            || relative.y > 1e-7
            || !area.reaches(center + DVec2::new(relative.x, relative.z)))
    {
        return DVec3::ZERO;
    }
    let offset = area.target_position() - center;
    let target =
        placement.position(center) + placement.rotation() * DVec3::new(offset.x, 0.0, offset.y);
    let delta = target - point;
    let sign = if area.direction == crate::area::Direction::Attract {
        1.0
    } else {
        -1.0
    };
    let strength = if area.force_mode == crate::area_effects::ForceMode::Newtonian {
        let radius = area.size[0].min(area.size[1]) * 0.5;
        area.strength * sign * (radius / delta.length().max(radius)).powi(2)
    } else {
        crate::area_effects::simple_strength(
            delta.length(),
            area.size[0].min(area.size[1]) * 0.5,
            area.strength * sign,
        )
    };
    delta.normalize_or_zero() * strength
}
