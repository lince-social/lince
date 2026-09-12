use crate::{
    area::{InfluenceArea, RecordProperties},
    canvas::CanvasItem,
    sand_placement::Pinned,
    workspace::{WorkspaceMember, Workspaces},
};
use avian3d::{physics_transform::PhysicsTransformConfig, prelude::*};
use bevy::{
    ecs::{schedule::ScheduleLabel, system::SystemParam},
    input_focus::InputFocus,
    math::{DVec2, DVec3},
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc,
    thread,
    time::Duration,
};

const STEP: Duration = Duration::from_nanos(8_333_333);

fn collider(size: Vec2) -> Collider {
    Collider::cuboid(
        f64::from(size.x),
        f64::from(size.y),
        f64::from(size.max_element()) * 2.0,
    )
}

#[derive(Component)]
struct BodyLink {
    sand: Entity,
    root: Entity,
    workspace: u64,
    origin: DVec2,
    last: DVec2,
    size: Vec2,
    held: bool,
    group: Option<crate::canvas_selection::SandGroup>,
}

#[derive(SystemParam)]
struct WorkspaceContacts<'w, 's> {
    bodies: Query<'w, 's, &'static BodyLink>,
}

impl CollisionHooks for WorkspaceContacts<'_, '_> {
    fn filter_pairs(&self, first: Entity, second: Entity, _: &mut Commands) -> bool {
        let Ok([first, second]) = self.bodies.get_many([first, second]) else {
            return false;
        };
        first.root == second.root
            && first.workspace == second.workspace
            && (first.group.is_none() || first.group != second.group)
    }
}

#[derive(Resource, Default)]
struct Simulation {
    accumulated: Duration,
    active: bool,
    areas: Vec<(Entity, Entity, u64, InfluenceArea)>,
    valid_areas: Vec<usize>,
    origins: HashMap<(Entity, u64), DVec2>,
    timer: Option<MotionTimer>,
}

struct MotionTimer {
    sender: Option<mpsc::Sender<bool>>,
    task: Option<thread::JoinHandle<()>>,
}

impl MotionTimer {
    fn new(wake: crate::wake::WakeSignal) -> Self {
        let (sender, receiver) = mpsc::channel();
        let task = thread::Builder::new()
            .name("workspace-physics".into())
            .spawn(move || {
                let mut active = false;
                loop {
                    let result = if active {
                        receiver.recv_timeout(Duration::from_millis(16))
                    } else {
                        receiver
                            .recv()
                            .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                    };
                    match result {
                        Ok(next) => active = next,
                        Err(mpsc::RecvTimeoutError::Timeout) => wake.ring(),
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("start physics wake timer");
        Self {
            sender: Some(sender),
            task: Some(task),
        }
    }
}

impl Drop for MotionTimer {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct WorkspaceStep;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SimulateWorkspaces;

pub struct WorkspacePhysicsPlugin;

impl Plugin for WorkspacePhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PhysicsTransformConfig {
            propagate_before_physics: false,
            transform_to_position: false,
            position_to_transform: false,
            transform_to_collider_scale: false,
        })
        .add_plugins(
            PhysicsPlugins::new(WorkspaceStep)
                .with_length_unit(100.0)
                .with_collision_hooks::<WorkspaceContacts>(),
        )
        .insert_resource(Gravity(DVec3::ZERO))
        .init_resource::<Simulation>()
        .add_systems(WorkspaceStep, apply_forces.before(PhysicsSystems::First))
        .add_systems(
            Update,
            simulate
                .in_set(SimulateWorkspaces)
                .after(crate::workspace::PrepareWorkspaces)
                .after(crate::record_view::ReceiveRecords),
        );
    }
}

fn held(world: &World, sand: Entity) -> bool {
    if crate::canvas_pan::dragged(world) == Some(sand) {
        return true;
    }
    let mut focus = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    while let Some(entity) = focus {
        if entity == sand {
            return true;
        }
        focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

fn synchronize(world: &mut World) -> bool {
    let mut changed = false;
    let mut edited = HashSet::new();
    let existing: HashMap<_, _> = world
        .query::<(Entity, &BodyLink)>()
        .iter(world)
        .map(|(entity, link)| (link.sand, entity))
        .collect();
    let sands: Vec<_> = world
        .query::<(
            Entity,
            &CanvasItem,
            &ChildOf,
            &WorkspaceMember,
            Option<&Pinned>,
        )>()
        .iter(world)
        .filter(|(entity, item, parent, member, pin)| {
            world.get::<crate::area::InfluenceArea>(*entity).is_none()
                && pin.is_none()
                && item.position.is_finite()
                && item.size.is_finite()
                && item.size.min_element() > 0.0
                && item.size.max_element() <= 100_000.0
                && world
                    .get::<Workspaces>(parent.parent())
                    .is_some_and(|spaces| spaces.active == member.0)
                && crate::workspace_config::enabled(world, parent.parent(), member.0)
        })
        .map(|(entity, item, parent, member, _)| (entity, *item, parent.parent(), member.0))
        .collect();
    let dragged: HashSet<_> = crate::canvas_pan::dragged(world)
        .and_then(|entity| {
            world
                .get::<ChildOf>(entity)
                .map(|parent| (entity, parent.parent()))
        })
        .map(|(entity, root)| {
            crate::canvas_selection::companions(world, root, entity)
                .into_iter()
                .collect()
        })
        .unwrap_or_default();
    let held_groups: HashSet<_> = world
        .query::<(
            Entity,
            &crate::canvas_selection::SandGroup,
            &ChildOf,
            &WorkspaceMember,
        )>()
        .iter(world)
        .filter(|(entity, _, _, _)| {
            world.get::<Pinned>(*entity).is_some()
                || dragged.contains(entity)
                || held(world, *entity)
        })
        .map(|(_, group, parent, member)| (parent.parent(), member.0, *group))
        .collect();
    let mut retained = HashSet::new();
    let mut origins = std::mem::take(&mut world.resource_mut::<Simulation>().origins);
    let mut active_workspaces = HashSet::new();
    for (sand, item, root, workspace) in sands {
        active_workspaces.insert((root, workspace));
        let origin = *origins.entry((root, workspace)).or_insert(item.position);
        let position = item.position - origin;
        if !position.as_vec2().is_finite() {
            continue;
        }
        let group = world
            .get::<crate::canvas_selection::SandGroup>(sand)
            .copied();
        let held = dragged.contains(&sand)
            || held(world, sand)
            || group.is_some_and(|group| held_groups.contains(&(root, workspace, group)));
        let body = if let Some(entity) = existing.get(&sand).copied() {
            let link = world.get::<BodyLink>(entity).unwrap();
            if link.root != root || link.workspace != workspace {
                world.despawn(entity);
                None
            } else {
                Some(entity)
            }
        } else {
            None
        };
        if let Some(entity) = body {
            let link = world.get::<BodyLink>(entity).unwrap();
            let moved = link.last != item.position;
            let resized = link.size != item.size;
            let changed_held = link.held != held;
            if moved || resized || link.group != group {
                edited.insert(entity);
            }
            if link.group != group {
                changed = true;
                world.get_mut::<BodyLink>(entity).unwrap().group = group;
            }
            if moved || changed_held {
                world
                    .entity_mut(entity)
                    .insert((Position(position.extend(0.0)), LinearVelocity::ZERO));
                world.entity_mut(entity).remove::<Sleeping>();
            }
            if changed_held {
                world.entity_mut(entity).insert(if held {
                    RigidBody::Static
                } else {
                    RigidBody::Dynamic
                });
            }
            if resized {
                world.entity_mut(entity).insert(collider(item.size));
                world.entity_mut(entity).remove::<Sleeping>();
            }
            if moved || resized || changed_held {
                changed = true;
                let mut link = world.get_mut::<BodyLink>(entity).unwrap();
                link.last = item.position;
                link.size = item.size;
                link.held = held;
            }
            retained.insert(entity);
        } else {
            changed = true;
            let body = world
                .spawn((
                    BodyLink {
                        sand,
                        root,
                        workspace,
                        origin,
                        last: item.position,
                        size: item.size,
                        held,
                        group,
                    },
                    Position(position.extend(0.0)),
                    Rotation::default(),
                    Transform::default(),
                    if held {
                        RigidBody::Static
                    } else {
                        RigidBody::Dynamic
                    },
                    collider(item.size),
                    ColliderDensity(0.0),
                    Mass(1.0),
                    AngularInertia::new(Vec3::ONE),
                    LockedAxes::ROTATION_LOCKED.lock_translation_z(),
                    LinearDamping(4.0),
                    MaxLinearSpeed(1000.0),
                    ConstantForce::default(),
                    SleepThreshold {
                        linear: 0.005,
                        angular: 0.005,
                    },
                    ActiveCollisionHooks::FILTER_PAIRS,
                ))
                .id();
            retained.insert(body);
        }
    }
    for body in existing.into_values() {
        if !retained.contains(&body) && world.get_entity(body).is_ok() {
            changed = true;
            world.despawn(body);
        }
    }
    changed |= synchronize_groups(world, &edited);
    let mut areas: Vec<_> = world
        .query::<(Entity, &InfluenceArea, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .map(|(entity, area, parent, member)| (entity, parent.parent(), member.0, area.clone()))
        .collect();
    areas.sort_by(|a, b| a.3.id.cmp(&b.3.id).then(a.0.cmp(&b.0)));
    let mut simulation = world.resource_mut::<Simulation>();
    origins.retain(|key, _| active_workspaces.contains(key));
    simulation.origins = origins;
    if simulation.areas != areas {
        simulation.valid_areas = areas
            .iter()
            .enumerate()
            .filter(|(_, (_, _, _, area))| area.validate())
            .map(|(index, _)| index)
            .collect();
        simulation.areas = areas;
    }
    changed
}

#[derive(Component)]
struct GroupJoint;

fn synchronize_groups(world: &mut World, edited: &HashSet<Entity>) -> bool {
    let mut groups = HashMap::<_, Vec<_>>::new();
    for (entity, link) in world.query::<(Entity, &BodyLink)>().iter(world) {
        if let Some(group) = link.group {
            groups
                .entry((link.root, link.workspace, group))
                .or_default()
                .push((entity, link.last));
        }
    }
    let mut existing: HashMap<_, _> = world
        .query_filtered::<(Entity, &FixedJoint), With<GroupJoint>>()
        .iter(world)
        .map(|(entity, joint)| ((joint.body1, joint.body2), entity))
        .collect();
    let mut changed = false;
    for members in groups.values_mut() {
        members.sort_by_key(|(entity, _)| *entity);
        let (first, position) = members[0];
        for (second, next) in members.iter().copied().skip(1) {
            let joint =
                FixedJoint::new(first, second).with_local_anchor1((next - position).extend(0.0));
            if let Some(entity) = existing.remove(&(first, second)) {
                if edited.contains(&first) || edited.contains(&second) {
                    world.entity_mut(entity).insert(joint);
                    changed = true;
                }
            } else {
                world.spawn((GroupJoint, joint));
                changed = true;
            }
        }
    }
    for entity in existing.into_values() {
        world.despawn(entity);
        changed = true;
    }
    changed
}

fn apply_forces(
    simulation: Res<Simulation>,
    records: Query<&RecordProperties>,
    mut bodies: Query<(Entity, &BodyLink, &Position, &mut ConstantForce)>,
    mut commands: Commands,
) {
    for (entity, link, position, mut force) in &mut bodies {
        let mut total = DVec2::ZERO;
        if !link.held
            && let Ok(record) = records.get(link.sand)
        {
            for index in &simulation.valid_areas {
                let (_, root, workspace, area) = &simulation.areas[*index];
                if *root == link.root && *workspace == link.workspace {
                    total += area.force(position.0.truncate() + link.origin, record);
                }
            }
        }
        let next = if total.is_finite() {
            total.clamp_length_max(1_000_000.0).extend(0.0)
        } else {
            DVec3::ZERO
        };
        if force.0 != next {
            force.0 = next;
            commands.entity(entity).remove::<Sleeping>();
        }
    }
}

fn simulate(world: &mut World) {
    let changed = synchronize(world);
    world
        .run_system_cached(apply_forces)
        .expect("update workspace forces");
    let awake = world
        .query::<(&BodyLink, Has<Sleeping>)>()
        .iter(world)
        .any(|(body, sleeping)| !body.held && !sleeping);
    let delta = world
        .get_resource::<Time<Real>>()
        .map_or(STEP, Time::delta)
        .min(STEP * 8);
    let was_active = world.resource::<Simulation>().active;
    if awake || changed {
        let mut simulation = world.resource_mut::<Simulation>();
        simulation.accumulated = if was_active {
            simulation.accumulated + delta
        } else {
            STEP
        };
        let mut ticks = 0;
        while world.resource::<Simulation>().accumulated >= STEP && ticks < 8 {
            world.resource_mut::<Simulation>().accumulated -= STEP;
            let original = *world.resource::<Time>();
            let mut step = original;
            step.advance_by(STEP);
            world.insert_resource(step);
            world.run_schedule(WorkspaceStep);
            world.insert_resource(original);
            ticks += 1;
        }
    } else {
        world.resource_mut::<Simulation>().accumulated = Duration::ZERO;
    }
    let updates: Vec<_> = world
        .query::<(Entity, &BodyLink, &Position)>()
        .iter(world)
        .filter(|(_, link, position)| {
            !link.held && position.0.truncate() + link.origin != link.last
        })
        .map(|(entity, link, position)| {
            (
                entity,
                link.sand,
                position.0.truncate() + link.origin,
                link.last,
                link.origin,
            )
        })
        .collect();
    for (body, sand, position, last, origin) in updates {
        if position.is_finite() {
            if let Some(mut item) = world.get_mut::<CanvasItem>(sand) {
                item.position = position;
            }
            world.get_mut::<BodyLink>(body).unwrap().last = position;
        } else {
            world.entity_mut(body).insert((
                Position((last - origin).extend(0.0)),
                LinearVelocity::ZERO,
                Sleeping,
            ));
        }
    }
    let active = world
        .query::<(&BodyLink, Has<Sleeping>)>()
        .iter(world)
        .any(|(body, sleeping)| !body.held && !sleeping);
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
        && active
        && world.resource::<Simulation>().timer.is_none()
    {
        let timer = MotionTimer::new(wake);
        let _ = timer.sender.as_ref().unwrap().send(active);
        world.resource_mut::<Simulation>().timer = Some(timer);
    }
    if active != was_active {
        if let Some(timer) = &world.resource::<Simulation>().timer {
            let _ = timer.sender.as_ref().unwrap().send(active);
        }
        world.resource_mut::<Simulation>().active = active;
    }
}

pub(crate) mod tests {
    use super::*;
    use crate::area::{AreaShape, Direction, Property, PropertyRule};
    use bevy::time::TimeUpdateStrategy;

    fn fixture(offset: DVec2) -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins((MinimalPlugins, WorkspacePhysicsPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
        app.finish();
        let root = app.world_mut().spawn(Workspaces::default()).id();
        let mut area = InfluenceArea::new(AreaShape::Square, offset, DVec2::splat(1000.0));
        area.rules = vec![PropertyRule {
            property: Property::Quantity,
            value: "-3".into(),
        }];
        area.strength = 400.0;
        let area = crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
        let sand = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: offset + DVec2::new(200.0, 0.0),
                    size: Vec2::splat(40.0),
                },
                RecordProperties(serde_json::json!({"quantity":-3})),
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        app.update();
        (app, root, area, sand)
    }

    fn advance(app: &mut App, ticks: usize) {
        for _ in 0..ticks {
            app.update();
        }
    }

    #[cfg_attr(test, test)]
    fn grouped_sands_keep_their_spacing_under_forces_and_release_joints_when_ungrouped() {
        let (mut app, root, _, first) = fixture(DVec2::ZERO);
        let group = crate::canvas_selection::SandGroup([7; 16]);
        let position = app.world().get::<CanvasItem>(first).unwrap().position;
        let second = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: position + DVec2::new(100.0, 0.0),
                    size: Vec2::splat(40.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
                group,
            ))
            .id();
        app.world_mut().entity_mut(first).insert(group);
        crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 120);
        let moved = app.world().get::<CanvasItem>(first).unwrap().position;
        let other = app.world().get::<CanvasItem>(second).unwrap().position;
        assert!(moved.x < position.x - 1.0);
        assert!((other - moved - DVec2::new(100.0, 0.0)).length() < 0.1);
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<GroupJoint>>()
                .iter(app.world())
                .count(),
            1
        );
        app.world_mut()
            .entity_mut(first)
            .remove::<crate::canvas_selection::SandGroup>();
        app.world_mut()
            .entity_mut(second)
            .remove::<crate::canvas_selection::SandGroup>();
        advance(&mut app, 2);
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<GroupJoint>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[cfg_attr(test, test)]
    fn toggles_start_real_motion_and_stop_at_current_positions_without_stale_velocity() {
        let (mut app, root, _, sand) = fixture(DVec2::ZERO);
        let start = app.world().get::<CanvasItem>(sand).unwrap().position;
        advance(&mut app, 10);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, start);
        crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 120);
        let moved = app.world().get::<CanvasItem>(sand).unwrap().position;
        assert!(moved.x < start.x - 50.0 && moved.x > 0.0, "{moved:?}");
        crate::workspace_config::set_physics(app.world_mut(), root, 1, false);
        advance(&mut app, 30);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, moved);
        assert_eq!(
            app.world_mut()
                .query::<&BodyLink>()
                .iter(app.world())
                .count(),
            0
        );
        crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
        app.world_mut().get_mut::<RecordProperties>(sand).unwrap().0 =
            serde_json::json!({"quantity":0});
        advance(&mut app, 30);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, moved);
    }

    #[cfg_attr(test, test)]
    fn repulsion_moves_outwards_and_pins_and_other_workspaces_are_unchanged() {
        let (mut app, root, area, sand) = fixture(DVec2::ZERO);
        app.world_mut()
            .get_mut::<InfluenceArea>(area)
            .unwrap()
            .direction = Direction::Repel;
        crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
        let pinned = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::new(150.0, 0.0),
                    size: Vec2::splat(40.0),
                },
                Pinned {
                    anchor: [0.5; 2],
                    scale: 1.0,
                },
                RecordProperties(serde_json::json!({"quantity":-3})),
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        let inactive = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::new(100.0, 0.0),
                    size: Vec2::splat(40.0),
                },
                RecordProperties(serde_json::json!({"quantity":-3})),
                WorkspaceMember(2),
                ChildOf(root),
            ))
            .id();
        advance(&mut app, 120);
        assert!(app.world().get::<CanvasItem>(sand).unwrap().position.x > 250.0);
        assert_eq!(
            app.world().get::<CanvasItem>(pinned).unwrap().position.x,
            150.0
        );
        assert_eq!(
            app.world().get::<CanvasItem>(inactive).unwrap().position.x,
            100.0
        );
        let last = app.world().get::<CanvasItem>(sand).unwrap().position;
        app.world_mut().get_mut::<Workspaces>(root).unwrap().active = 2;
        advance(&mut app, 10);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, last);
    }

    #[cfg_attr(test, test)]
    fn collisions_separate_sands_and_settle_without_crossing_box_boundaries() {
        let (mut app, root, area, sand) = fixture(DVec2::ZERO);
        app.world_mut().despawn(area);
        app.world_mut()
            .get_mut::<CanvasItem>(sand)
            .unwrap()
            .position = DVec2::new(10.0, 0.0);
        let other = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::new(-10.0, 0.0),
                    size: Vec2::splat(40.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        let foreign_root = app.world_mut().spawn(Workspaces::default()).id();
        let foreign = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(400.0),
                },
                WorkspaceMember(1),
                ChildOf(foreign_root),
            ))
            .id();
        crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
        crate::workspace_config::set_physics(app.world_mut(), foreign_root, 1, true);
        advance(&mut app, 360);
        let a = app.world().get::<CanvasItem>(sand).unwrap().position;
        let b = app.world().get::<CanvasItem>(other).unwrap().position;
        assert!((a.x - b.x).abs() >= 39.0, "{a:?} {b:?}");
        assert!(a.y.abs() < 1e-6 && b.y.abs() < 1e-6);
        assert_eq!(
            app.world().get::<CanvasItem>(foreign).unwrap().position,
            DVec2::ZERO
        );
        assert!(!app.world().resource::<Simulation>().active);
    }

    #[cfg_attr(test, test)]
    fn repeated_fixed_steps_agree_within_one_millionth_of_a_canvas_unit() {
        let run = || {
            let (mut app, root, _, sand) = fixture(DVec2::new(1e9, -1e9));
            crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
            advance(&mut app, 120);
            app.world().get::<CanvasItem>(sand).unwrap().position
        };
        let first = run();
        let second = run();
        assert!(first.distance(second) <= 1e-6);
        assert!(first.x < 1e9 + 150.0);
        assert!((first.y + 1e9).abs() <= 1e-6);
    }

    #[cfg_attr(test, test)]
    fn editing_a_sand_holds_it_and_releasing_focus_resumes_the_force() {
        let (mut app, root, _, sand) = fixture(DVec2::ZERO);
        let editor = app.world_mut().spawn(ChildOf(sand)).id();
        let mut focus = InputFocus::default();
        focus.set(editor, bevy::input_focus::FocusCause::Pressed);
        app.insert_resource(focus);
        crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 30);
        assert_eq!(
            app.world().get::<CanvasItem>(sand).unwrap().position.x,
            200.0
        );
        app.world_mut().resource_mut::<InputFocus>().clear();
        advance(&mut app, 120);
        assert!(app.world().get::<CanvasItem>(sand).unwrap().position.x < 150.0);
    }

    #[cfg_attr(test, test)]
    fn motion_timer_sleeps_until_started_and_releases_its_thread_on_drop() {
        let (sender, receiver) = mpsc::channel();
        let timer = MotionTimer::new(crate::wake::WakeSignal::new(move || {
            let _ = sender.send(());
        }));
        assert!(receiver.recv_timeout(Duration::from_millis(40)).is_err());
        timer.sender.as_ref().unwrap().send(true).unwrap();
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        timer.sender.as_ref().unwrap().send(false).unwrap();
        drop(timer);
        while receiver.try_recv().is_ok() {}
        assert!(receiver.recv_timeout(Duration::from_millis(40)).is_err());
    }

    crate::laboratory_cases! {
        grouped_sands_keep_their_spacing_under_forces_and_release_joints_when_ungrouped,
        toggles_start_real_motion_and_stop_at_current_positions_without_stale_velocity,
        repulsion_moves_outwards_and_pins_and_other_workspaces_are_unchanged,
        collisions_separate_sands_and_settle_without_crossing_box_boundaries,
        repeated_fixed_steps_agree_within_one_millionth_of_a_canvas_unit,
        editing_a_sand_holds_it_and_releasing_focus_resumes_the_force,
        motion_timer_sleeps_until_started_and_releases_its_thread_on_drop,
    }
}

pub(crate) fn resource_usage(world: &mut World) -> HashMap<Entity, (usize, usize)> {
    let mut usage = HashMap::new();
    for (link, sleeping) in world.query::<(&BodyLink, Has<Sleeping>)>().iter(world) {
        let count = usage.entry(link.sand).or_insert((0, 0));
        count.0 += 1;
        count.1 += usize::from(!link.held && !sleeping);
    }
    usage
}
