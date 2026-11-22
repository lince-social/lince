// non-lean-yet
// Scope: struct Settings, struct Member, struct State, struct Motion.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 80, 173, 177.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   173, 262, 271, 275, 276, 293, 296, 298, 299.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (80).

#[cfg(test)]
mod area_tests;
mod forces;
use forces::forces;

pub use crate::native::area::graph_layout::Settings;
use crate::native::{area::InfluenceArea, canvas::CanvasItem, protein_area::RecordBinding};
use crate::native::{arrow_sand::ArrowSand, topology, workspace::WorkspaceMember};
use bevy::{
    ecs::query::QueryState,
    math::{DQuat, DVec2, DVec3},
    prelude::*,
};
use std::collections::HashMap;

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
    links: Vec<(Entity, Entity)>,
    alpha: f64,
}

#[derive(Resource, Default)]
pub(crate) struct Motion {
    states: HashMap<Entity, State>,
    pub forces: HashMap<Entity, DVec3>,
    evaluations: u64,
}

pub(crate) fn prepare(world: &mut World) {
    let _timing = crate::host::performance::Span::new(
        crate::host::performance::Stage::NativeProteinMotionPrepare,
    );
    world
        .run_system_cached(prepare_graphs)
        .expect("prepare Area graph layouts");
}

type Cards = QueryState<
    (
        Entity,
        &'static CanvasItem,
        &'static ChildOf,
        &'static WorkspaceMember,
        Option<&'static RecordBinding>,
    ),
    (
        Without<InfluenceArea>,
        Without<ArrowSand>,
        Without<topology::terrain::Effect>,
        Without<crate::native::canvas_item::Ephemeral>,
        Without<crate::native::protein_area::placement::Pending>,
        Without<crate::native::time_castle::AttachedCard>,
    ),
>;

fn prepare_graphs(
    world: &mut World,
    sources: &mut QueryState<(
        Entity,
        &'static InfluenceArea,
        &'static ChildOf,
        &'static WorkspaceMember,
    )>,
    items: &mut Cards,
    arrows: &mut QueryState<&'static ArrowSand>,
    blocker_query: &mut topology::influence::BlockerQuery,
) {
    let mut motion = world.remove_resource::<Motion>().unwrap_or_default();
    motion.forces.clear();
    let owners: Vec<_> = sources
        .iter(world)
        .filter(|(entity, area, parent, member)| {
            area.graph_settings().is_some_and(Settings::valid)
                && area.attraction_enabled
                && area.protein.as_ref().is_none_or(|p| p.enabled)
                && topology::holes::live(world, parent.parent(), member.0)
                && crate::native::workspace_config::enabled(world, parent.parent(), member.0)
                && crate::native::influence_report::activity(
                    world,
                    *entity,
                    area,
                    parent.parent(),
                    member.0,
                ) == crate::native::influence_report::Outcome::Active
        })
        .map(|(entity, _, parent, member)| (entity, parent.parent(), member.0))
        .collect();
    motion
        .states
        .retain(|owner, _| owners.iter().any(|(entity, ..)| entity == owner));
    if owners.is_empty() {
        world.insert_resource(motion);
        return;
    }
    let held_items = crate::native::physics::HeldItems::capture(world);
    let dt = world
        .get_resource::<Time<Real>>()
        .map_or(1.0 / 60.0, Time::delta_secs_f64)
        .min(8.0 / 120.0);
    for (owner, root, workspace) in owners {
        let area = world.get::<InfluenceArea>(owner).unwrap();
        let settings = *area.graph_settings().unwrap();
        let plane = topology::spatial(world, owner);
        let origin = DVec2::from_array(area.center);
        let offset = area.target_position() - origin;
        let rotation = plane.rotation();
        let center = plane.position(origin) + rotation * DVec3::new(offset.x, 0.0, offset.y);
        let blockers = topology::influence::Blockers::prepare(
            world,
            blocker_query,
            root,
            workspace,
            owner,
            true,
        );
        let filter = world.get::<crate::native::protein_area::filter::Matches>(owner);
        let source_group = world.get::<crate::native::canvas_selection::SandGroup>(owner);
        let mut members = Vec::new();
        for (entity, stored, parent, member, binding) in items.iter(world) {
            if parent.parent() != root
                || member.0 != workspace
                || area.protein.is_some() && !binding.is_some_and(|b| b.area == owner)
                || topology::rooms::fixed(world, entity)
                || source_group.is_some()
                    && world.get::<crate::native::canvas_selection::SandGroup>(entity)
                        == source_group
            {
                continue;
            }
            let item = crate::native::canvas::presented(world, entity).unwrap_or(*stored);
            let spatial = crate::native::canvas::presented_spatial(world, entity);
            let position = spatial.position(item.position);
            let record = world.get::<crate::native::area::RecordProperties>(entity);
            let point = topology::participation_point(world, entity, &item);
            if !position.is_finite()
                || !item.size.is_finite()
                || blockers.blocks(point, record, binding)
            {
                continue;
            }
            if area.protein.is_none() {
                let relative = plane.local_point(origin, point);
                if area.reach.mode == crate::native::area::ReachMode::Limited
                    && (relative.y < -area.depth - 1e-7
                        || relative.y > 1e-7
                        || !area.reaches(origin + DVec2::new(relative.x, relative.z)))
                {
                    continue;
                }
            }
            if area.filter.is_some() {
                if !record.is_some_and(|r| filter.is_some_and(|f| f.allows(r, binding))) {
                    continue;
                }
            } else if !area.rules.is_empty() && !record.is_some_and(|r| area.matches(r)) {
                continue;
            }
            let held = spatial.world_pinned
                || world
                    .get::<crate::native::sand_placement::Pinned>(entity)
                    .is_some()
                || held_items.contains(world, entity)
                || crate::native::area_mutation::pending(world, entity)
                || world
                    .get::<crate::native::canvas::ItemPreview>(entity)
                    .is_some()
                || world
                    .get_resource::<topology::input::PointerState>()
                    .is_some_and(|p| p.drag.is_some_and(|(drag, _)| drag == entity));
            let scale = world
                .get::<crate::native::area_effects::AreaScale>(entity)
                .map_or(1.0, |scale| scale.0);
            let revision = world
                .entity(entity)
                .get_ref::<crate::native::area::RecordProperties>()
                .map_or(0, |r| r.last_changed().get());
            members.push(Member {
                entity,
                position,
                size: item.size * scale,
                rotation: spatial.rotation,
                held,
                revision,
            });
        }
        members.sort_by_key(|member| member.entity);
        let ids: std::collections::HashSet<_> = members.iter().map(|m| m.entity).collect();
        let mut edges: Vec<_> = arrows
            .iter(world)
            .filter(|arrow| ids.contains(&arrow.from) && ids.contains(&arrow.to))
            .map(|arrow| (arrow.from.min(arrow.to), arrow.from.max(arrow.to)))
            .collect();
        edges.sort_unstable();
        edges.dedup();
        let state = motion.states.entry(owner).or_insert_with(|| State {
            settings,
            center,
            rotation,
            members: Vec::new(),
            links: Vec::new(),
            alpha: 1.0,
        });
        if state.members != members
            || state.links != edges
            || state.settings != settings
            || state.center != center
            || state.rotation != rotation
        {
            state.alpha = 1.0;
        } else {
            state.alpha *= (-state.settings.cooling * dt).exp();
            if state.alpha < 0.01 {
                state.alpha = 0.0;
            }
        }
        state.members = members;
        state.links = edges;
        state.settings = settings;
        state.center = center;
        state.rotation = rotation;
        if state.alpha == 0.0 {
            continue;
        }
        let forces = forces(
            &state.members,
            &state.links,
            center,
            rotation,
            &state.settings,
            state.alpha,
        );
        motion.evaluations += 1;
        for (entity, force) in forces {
            *motion.forces.entry(entity).or_default() += force;
        }
    }
    world.insert_resource(motion);
}

pub(crate) fn room_transform(
    world: &mut World,
    owner: Entity,
    before: &nucleus::canvas::Pose,
    after: &nucleus::canvas::Pose,
) {
    let Some(mut motion) = world.remove_resource::<Motion>() else {
        return;
    };
    let rotation = DQuat::from_array(after.rotation) * DQuat::from_array(before.rotation).inverse();
    if let Some(state) = motion.states.get_mut(&owner) {
        state.center = DVec3::from_array(after.position);
        state.rotation = DQuat::from_array(after.rotation);
        for member in &mut state.members {
            if let Some(position) = crate::native::topology::position(world, member.entity) {
                member.position = position;
                member.rotation = crate::native::topology::spatial(world, member.entity).rotation;
            }
            if let Some(force) = motion.forces.get_mut(&member.entity) {
                *force = rotation * *force;
            }
        }
    }
    world.insert_resource(motion);
}

pub(crate) fn remember(world: &mut World) {
    let Some(mut motion) = world.remove_resource::<Motion>() else {
        return;
    };
    for state in motion.states.values_mut() {
        for member in &mut state.members {
            if !member.held
                && let Some(position) = crate::native::topology::position(world, member.entity)
            {
                member.position = position;
            }
        }
    }
    world.insert_resource(motion);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{
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
                RecordProperties(serde_json::json!({"quantity":index}).into()),
                ChildOf(root),
                WorkspaceMember(1),
            ))
            .id()
    }

    #[test]
    fn room_rebase_rotates_cached_forces_without_reheating_the_motion_graph() {
        use crate::native::topology;
        use nucleus::canvas::{
            Pose,
            room::{compose, relative},
        };
        let mut world = World::new();
        let root = world
            .spawn(crate::native::workspace::Workspaces::default())
            .id();
        let owner = crate::native::relation_castle::spawn(&mut world, root).unwrap();
        world.get_mut::<InfluenceArea>(owner).unwrap().graph_layout = Some(Settings::default());
        let row = record(&mut world, root, owner, 0, DVec2::new(300.0, 100.0));
        prepare(&mut world);
        world
            .resource_mut::<Motion>()
            .states
            .get_mut(&owner)
            .unwrap()
            .alpha = 0.25;
        let force = world.resource::<Motion>().forces[&row];
        let before = Pose {
            position: topology::position(&world, owner).unwrap().to_array(),
            rotation: topology::spatial(&world, owner).rotation,
        };
        let after = Pose {
            position: [1000.0, 50.0, 2000.0],
            rotation: DQuat::from_rotation_y(0.7).to_array(),
        };
        let old_row = Pose {
            position: topology::position(&world, row).unwrap().to_array(),
            rotation: topology::spatial(&world, row).rotation,
        };
        let new_row = compose(&after, &relative(&before, &old_row));
        topology::set_position(&mut world, owner, DVec3::from_array(after.position));
        world.entity_mut(owner).insert(topology::Spatial {
            rotation: after.rotation,
            elevation: after.position[1],
            ..default()
        });
        topology::set_position(&mut world, row, DVec3::from_array(new_row.position));
        world.entity_mut(row).insert(topology::Spatial {
            rotation: new_row.rotation,
            elevation: new_row.position[1],
            ..default()
        });
        room_transform(&mut world, owner, &before, &after);
        let motion = world.resource::<Motion>();
        assert_eq!(motion.states[&owner].alpha, 0.25);
        assert!((motion.forces[&row] - DQuat::from_array(after.rotation) * force).length() < 1e-8);
        prepare(&mut world);
        assert!(world.resource::<Motion>().states[&owner].alpha < 0.25);
    }

    #[test]
    fn links_pull_both_records_wake_sleeping_graphs_and_respect_pins_and_membership() {
        let mut world = World::new();
        let root = world
            .spawn(crate::native::workspace::Workspaces::default())
            .id();
        let owner = crate::native::relation_castle::spawn(&mut world, root).unwrap();
        world.get_mut::<InfluenceArea>(owner).unwrap().graph_layout = Some(Settings {
            center: 0.0,
            repulsion: 0.0,
            ..default()
        });
        let first = record(&mut world, root, owner, 0, DVec2::new(-500.0, 0.0));
        let second = record(&mut world, root, owner, 1, DVec2::new(500.0, 0.0));
        for _ in 0..700 {
            prepare(&mut world);
        }
        assert_eq!(world.resource::<Motion>().states[&owner].alpha, 0.0);
        let link = world
            .spawn((
                crate::native::relation_castle::RelationLink {
                    owner,
                    uid: "link".into(),
                },
                crate::native::arrow_sand::ArrowSand {
                    from: first,
                    to: second,
                    label: "depends-on".into(),
                },
            ))
            .id();
        prepare(&mut world);
        let pull = world.resource::<Motion>().forces[&first];
        assert!(pull.x > 0.0);
        assert_eq!(pull, -world.resource::<Motion>().forces[&second]);
        assert!(world.resource::<Motion>().states[&owner].alpha > 0.9);
        world
            .entity_mut(first)
            .insert(crate::native::sand_placement::Pinned {
                anchor: [0.5, 0.5],
                scale: 1.0,
            });
        prepare(&mut world);
        assert_eq!(world.resource::<Motion>().forces[&first], DVec3::ZERO);
        assert!(world.resource::<Motion>().forces[&second].x < 0.0);
        let other_owner = crate::native::relation_castle::spawn(&mut world, root).unwrap();
        let outsider = record(&mut world, root, other_owner, 2, DVec2::new(1000.0, 0.0));
        world
            .get_mut::<crate::native::arrow_sand::ArrowSand>(link)
            .unwrap()
            .to = outsider;
        prepare(&mut world);
        assert_eq!(world.resource::<Motion>().forces[&second], DVec3::ZERO);
        world.despawn(link);
        prepare(&mut world);
        assert!(world.resource::<Motion>().states[&owner].links.is_empty());
    }

    #[test]
    fn link_attraction_balances_repulsion_without_collapsing_cards() {
        let members = [
            Member {
                entity: Entity::from_raw_u32(1).unwrap(),
                position: DVec3::ZERO,
                size: Vec2::new(200.0, 80.0),
                rotation: DQuat::IDENTITY.to_array(),
                held: false,
                revision: 0,
            },
            Member {
                entity: Entity::from_raw_u32(2).unwrap(),
                position: DVec3::new(1000.0, 0.0, 0.0),
                size: Vec2::new(200.0, 80.0),
                rotation: DQuat::IDENTITY.to_array(),
                held: false,
                revision: 0,
            },
        ];
        let settings = Settings {
            center: 0.0,
            ..default()
        };
        let links = [(members[0].entity, members[1].entity)];
        let far = forces(
            &members,
            &links,
            DVec3::ZERO,
            DQuat::IDENTITY,
            &settings,
            1.0,
        );
        assert!(far[0].1.x > 0.0);
        assert_eq!(far[0].1, -far[1].1);
        let mut nearby = members.clone();
        nearby[1].position.x = 220.0;
        let close = forces(
            &nearby,
            &links,
            DVec3::ZERO,
            DQuat::IDENTITY,
            &settings,
            1.0,
        );
        assert!(close[0].1.x < 0.0);
        assert_eq!(close[0].1, -close[1].1);
    }

    #[test]
    fn center_repulsion_cool_and_sleep_in_both_canvas_modes() {
        for (spatial, linked) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut app = App::new();
            crate::native::laboratory::isolate(app.world_mut());
            app.add_plugins((
                MinimalPlugins,
                crate::native::physics::WorkspacePhysicsPlugin,
            ))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f64(1.0 / 60.0),
            ));
            if spatial {
                app.init_resource::<crate::native::topology::physics::Runtime>();
            }
            app.finish();
            let root = app
                .world_mut()
                .spawn(crate::native::workspace::Workspaces::default())
                .id();
            let owner = crate::native::relation_castle::spawn(app.world_mut(), root).unwrap();
            let first = record(app.world_mut(), root, owner, 0, DVec2::new(900.0, 0.0));
            let second = record(app.world_mut(), root, owner, 1, DVec2::new(900.0, 0.0));
            if linked {
                app.world_mut().spawn((
                    crate::native::relation_castle::RelationLink {
                        owner,
                        uid: "link".into(),
                    },
                    crate::native::arrow_sand::ArrowSand {
                        from: first,
                        to: second,
                        label: "depends-on".into(),
                    },
                ));
            }
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
            std::sync::Arc::make_mut(
                &mut app
                    .world_mut()
                    .get_mut::<RecordProperties>(first)
                    .unwrap()
                    .0,
            )["quantity"] = serde_json::json!(42);
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
        let root = world
            .spawn(crate::native::workspace::Workspaces::default())
            .id();
        let owner = crate::native::relation_castle::spawn(&mut world, root).unwrap();
        let second_owner = crate::native::relation_castle::spawn(&mut world, root).unwrap();
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
        for _ in 0..700 {
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
            .insert(crate::native::sand_placement::Pinned {
                anchor: [0.5, 0.5],
                scale: 1.0,
            });
        prepare(&mut world);
        assert_eq!(world.resource::<Motion>().forces[&other], DVec3::ZERO);
        crate::native::workspace_config::set_physics(&mut world, root, 1, false);
        prepare(&mut world);
        assert!(world.resource::<Motion>().forces.is_empty());
    }
}
