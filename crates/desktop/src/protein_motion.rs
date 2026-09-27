use crate::{area::InfluenceArea, canvas::CanvasItem, protein_area::RecordBinding};
use bevy::{
    math::{DQuat, DVec3},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub center: f64,
    pub repulsion: f64,
    pub cooling: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            center: 4.0,
            repulsion: 1600.0,
            cooling: 0.8,
        }
    }
}

impl Settings {
    pub fn valid(&self) -> bool {
        self.center.is_finite()
            && (0.0..=100.0).contains(&self.center)
            && self.repulsion.is_finite()
            && (0.0..=100_000.0).contains(&self.repulsion)
            && self.cooling.is_finite()
            && (0.1..=10.0).contains(&self.cooling)
    }
}

#[derive(Clone, PartialEq)]
struct Member {
    entity: Entity,
    position: DVec3,
    size: Vec2,
    rotation: [f64; 4],
    held: bool,
    revision: u32,
}

struct State {
    settings: Settings,
    center: DVec3,
    rotation: DQuat,
    members: Vec<Member>,
    alpha: f64,
}

#[derive(Resource, Default)]
pub(crate) struct Motion {
    states: HashMap<Entity, State>,
    pub forces: HashMap<Entity, DVec3>,
    evaluations: u64,
}

pub(crate) fn prepare(world: &mut World) {
    let mut motion = world.remove_resource::<Motion>().unwrap_or_default();
    motion.forces.clear();
    let sources: HashMap<_, _> = world
        .query::<(Entity, &InfluenceArea)>()
        .iter(world)
        .filter_map(|(owner, area)| {
            area.protein
                .as_ref()
                .filter(|config| {
                    area.enabled
                        && area.attraction_enabled
                        && config.enabled
                        && config.motion.is_some()
                        && config.valid()
                })
                .map(|config| (owner, config.motion.clone().unwrap()))
        })
        .collect();
    if sources.is_empty() {
        motion.states.clear();
        world.insert_resource(motion);
        return;
    }
    let mut groups: HashMap<Entity, Vec<Member>> = HashMap::new();
    for (entity, item, binding, parent, workspace) in world
        .query::<(
            Entity,
            &CanvasItem,
            &RecordBinding,
            &ChildOf,
            &crate::workspace::WorkspaceMember,
        )>()
        .iter(world)
    {
        if !sources.contains_key(&binding.area)
            || world.get::<crate::arrow_sand::ArrowSand>(entity).is_some()
            || world
                .get::<crate::protein_area::placement::Pending>(entity)
                .is_some()
            || world.get::<ChildOf>(binding.area).map(ChildOf::parent) != Some(parent.parent())
            || world.get::<crate::workspace::WorkspaceMember>(binding.area) != Some(workspace)
            || world
                .get::<crate::workspace::Workspaces>(parent.parent())
                .is_none_or(|spaces| spaces.active != workspace.0)
            || !crate::workspace_config::enabled(world, parent.parent(), workspace.0)
        {
            continue;
        }
        let spatial = crate::topology::spatial(world, entity);
        let position = spatial.position(item.position);
        if !position.is_finite() || !item.size.is_finite() {
            continue;
        }
        let held = spatial.world_pinned
            || world.get::<crate::sand_placement::Pinned>(entity).is_some()
            || crate::physics::held(world, entity)
            || crate::area_mutation::pending(world, entity)
            || world
                .get_resource::<crate::topology::input::PointerState>()
                .is_some_and(|pointer| pointer.drag.is_some_and(|(drag, _)| drag == entity));
        let scale = world
            .get::<crate::area_effects::AreaScale>(entity)
            .map_or(1.0, |scale| scale.0);
        let revision = world
            .entity(entity)
            .get_ref::<crate::area::RecordProperties>()
            .map_or(0, |record| record.last_changed().get());
        groups.entry(binding.area).or_default().push(Member {
            entity,
            position,
            size: item.size * scale,
            rotation: spatial.rotation,
            held,
            revision,
        });
    }
    motion.states.retain(|owner, _| groups.contains_key(owner));
    let dt = world
        .get_resource::<Time<Real>>()
        .map_or(1.0 / 60.0, |time| time.delta_secs_f64())
        .clamp(0.0, 0.05);
    for (owner, mut members) in groups {
        members.sort_by_key(|member| member.entity);
        let settings = sources[&owner].clone();
        let center = crate::topology::position(world, owner).unwrap_or_default();
        let rotation = crate::topology::spatial(world, owner).rotation();
        let state = motion.states.entry(owner).or_insert_with(|| State {
            settings: settings.clone(),
            center,
            rotation,
            members: Vec::new(),
            alpha: 1.0,
        });
        if state.members != members
            || state.settings != settings
            || state.center != center
            || state.rotation != rotation
        {
            state.alpha = 1.0;
        }
        state.members = members;
        state.settings = settings;
        state.center = center;
        state.rotation = rotation;
        if state.alpha == 0.0 {
            continue;
        }
        let forces = forces(
            &state.members,
            center,
            rotation,
            &state.settings,
            state.alpha,
        );
        motion.evaluations += 1;
        motion.forces.extend(forces);
        state.alpha *= (-state.settings.cooling * dt).exp();
        if state.alpha < 0.01 {
            state.alpha = 0.0;
        }
    }
    world.insert_resource(motion);
}

fn forces(
    members: &[Member],
    center: DVec3,
    rotation: DQuat,
    settings: &Settings,
    alpha: f64,
) -> Vec<(Entity, DVec3)> {
    let mut forces: Vec<_> = members
        .iter()
        .map(|member| (center - member.position) * settings.center)
        .collect();
    for first in 0..members.len() {
        for second in first + 1..members.len() {
            let mut delta =
                rotation.inverse() * (members[first].position - members[second].position);
            delta.y = 0.0;
            let distance = delta.length();
            let direction = if distance > 1e-6 {
                delta / distance
            } else {
                let angle = (first * 17 + second * 31) as f64 * 2.399963229728653;
                DVec3::new(angle.cos(), 0.0, angle.sin())
            };
            let spacing =
                f64::from((members[first].size.length() + members[second].size.length()) * 0.5)
                    + 40.0;
            let force = (rotation * direction)
                * (settings.repulsion * spacing.powi(2) / distance.max(spacing * 0.25).powi(2))
                    .min(100_000.0);
            forces[first] += force;
            forces[second] -= force;
        }
    }
    members
        .iter()
        .zip(forces)
        .map(|(member, force)| {
            (
                member.entity,
                if member.held {
                    DVec3::ZERO
                } else {
                    (force * alpha).clamp_length_max(100_000.0)
                },
            )
        })
        .collect()
}

pub(crate) fn remember(world: &mut World) {
    let Some(mut motion) = world.remove_resource::<Motion>() else {
        return;
    };
    for state in motion.states.values_mut() {
        for member in &mut state.members {
            if let Some(position) = crate::topology::position(world, member.entity) {
                member.position = position;
            }
        }
    }
    world.insert_resource(motion);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        area::RecordProperties, canvas::CanvasItem, protein_area::Source,
        workspace::WorkspaceMember,
    };
    use bevy::math::DVec2;

    fn record(
        world: &mut World,
        root: Entity,
        owner: Entity,
        index: usize,
        position: DVec2,
    ) -> Entity {
        world
            .spawn((
                CanvasItem {
                    position,
                    size: Vec2::new(200.0, 80.0),
                },
                RecordBinding {
                    area: owner,
                    uid: format!("record-{index}"),
                    source: Source::Local,
                },
                RecordProperties(serde_json::json!({"quantity":index})),
                ChildOf(root),
                WorkspaceMember(1),
            ))
            .id()
    }

    #[test]
    fn center_repulsion_cool_and_sleep_in_both_canvas_modes() {
        for spatial in [false, true] {
            let mut app = App::new();
            crate::laboratory::isolate(app.world_mut());
            app.add_plugins((MinimalPlugins, crate::physics::WorkspacePhysicsPlugin))
                .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                    std::time::Duration::from_secs_f64(1.0 / 60.0),
                ));
            if spatial {
                app.init_resource::<crate::topology::physics::Runtime>();
            }
            app.finish();
            let root = app
                .world_mut()
                .spawn(crate::workspace::Workspaces::default())
                .id();
            let owner = crate::relation_castle::spawn(app.world_mut(), root).unwrap();
            let first = record(app.world_mut(), root, owner, 0, DVec2::new(900.0, 0.0));
            let second = record(app.world_mut(), root, owner, 1, DVec2::new(900.0, 0.0));
            let unrelated = app
                .world_mut()
                .spawn((
                    CanvasItem {
                        position: DVec2::new(-5000.0, 0.0),
                        size: Vec2::splat(20.0),
                    },
                    ChildOf(root),
                    WorkspaceMember(1),
                ))
                .id();
            for _ in 0..1000 {
                app.update();
            }
            let a = app.world().get::<CanvasItem>(first).unwrap().position;
            let b = app.world().get::<CanvasItem>(second).unwrap().position;
            assert!((a + b).length() * 0.5 < 400.0, "{spatial}: {a:?} {b:?}");
            assert!(a.distance(b) > 180.0, "{spatial}: {a:?} {b:?}");
            assert_eq!(
                app.world().get::<CanvasItem>(unrelated).unwrap().position,
                DVec2::new(-5000.0, 0.0)
            );
            let evaluations = app.world().resource::<Motion>().evaluations;
            assert_eq!(app.world().resource::<Motion>().states[&owner].alpha, 0.0);
            for _ in 0..40 {
                app.update();
            }
            assert_eq!(app.world().resource::<Motion>().evaluations, evaluations);
            assert_eq!(app.world().get::<CanvasItem>(first).unwrap().position, a);
            app.world_mut()
                .get_mut::<RecordProperties>(first)
                .unwrap()
                .0["quantity"] = serde_json::json!(42);
            app.update();
            assert!(app.world().resource::<Motion>().evaluations > evaluations);
            assert!(app.world().resource::<Motion>().states[&owner].alpha > 0.9);
            app.world_mut()
                .get_mut::<CanvasItem>(first)
                .unwrap()
                .position
                .x += 400.0;
            app.update();
            assert!(app.world().resource::<Motion>().states[&owner].alpha > 0.9);
        }
    }

    #[test]
    fn sleeping_two_hundred_records_skip_pair_forces_and_sources_stay_isolated() {
        let mut world = World::new();
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let owner = crate::relation_castle::spawn(&mut world, root).unwrap();
        let second_owner = crate::relation_castle::spawn(&mut world, root).unwrap();
        let mut records = Vec::new();
        for index in 0..200 {
            records.push(record(
                &mut world,
                root,
                owner,
                index,
                DVec2::new(index as f64 * 220.0, 0.0),
            ));
        }
        let other = record(&mut world, root, second_owner, 0, DVec2::new(100.0, 0.0));
        for _ in 0..400 {
            prepare(&mut world);
            remember(&mut world);
        }
        let evaluations = world.resource::<Motion>().evaluations;
        for _ in 0..100 {
            prepare(&mut world);
        }
        assert_eq!(world.resource::<Motion>().evaluations, evaluations);
        assert!(world.resource::<Motion>().forces.is_empty());
        world.get_mut::<CanvasItem>(other).unwrap().position.x += 50.0;
        prepare(&mut world);
        assert_eq!(world.resource::<Motion>().evaluations, evaluations + 1);
        assert!(
            records
                .iter()
                .all(|entity| !world.resource::<Motion>().forces.contains_key(entity))
        );
        assert!(world.resource::<Motion>().forces[&other].x < 0.0);
        world
            .entity_mut(other)
            .insert(crate::sand_placement::Pinned {
                anchor: [0.5, 0.5],
                scale: 1.0,
            });
        prepare(&mut world);
        assert_eq!(world.resource::<Motion>().forces[&other], DVec3::ZERO);
        crate::workspace_config::set_physics(&mut world, root, 1, false);
        prepare(&mut world);
        assert!(world.resource::<Motion>().forces.is_empty());
    }
}
