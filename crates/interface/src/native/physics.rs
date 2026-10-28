use crate::native::{
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

#[cfg(test)]
mod manipulation;

fn collider(size: Vec2) -> Collider {
    Collider::cuboid(
        f64::from(size.x),
        f64::from(size.y),
        f64::from(size.max_element()) * 2.0,
    )
}

pub(crate) fn sleep_threshold(force: DVec3, mass: f64) -> SleepThreshold {
    SleepThreshold {
        linear: if force == DVec3::ZERO {
            0.005
        } else {
            (force.length() / (mass * 4000.0)).min(0.005) as f32
        },
        angular: 0.005,
    }
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
    group: Option<crate::native::canvas_selection::SandGroup>,
}

#[derive(SystemParam)]
struct WorkspaceContacts<'w, 's> {
    ground: Query<'w, 's, &'static crate::native::topology::terrain::Ground>,
    bodies: Query<'w, 's, &'static BodyLink>,
    spatial: Query<'w, 's, &'static crate::native::topology::physics::Body>,
    colliders: Query<'w, 's, &'static ColliderOf>,
}

impl CollisionHooks for WorkspaceContacts<'_, '_> {
    fn filter_pairs(&self, first: Entity, second: Entity, _: &mut Commands) -> bool {
        let first = self
            .colliders
            .get(first)
            .map_or(first, |collider| collider.body);
        let second = self
            .colliders
            .get(second)
            .map_or(second, |collider| collider.body);
        if first == second {
            return false;
        }
        for (ground, body) in [(first, second), (second, first)] {
            if let (Ok(ground), Ok(body)) = (self.ground.get(ground), self.spatial.get(body)) {
                return ground.root == body.root && ground.workspace == body.workspace;
            }
        }
        if let Ok([first, second]) = self.spatial.get_many([first, second]) {
            return first.root == second.root && first.workspace == second.workspace;
        }
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
    origins: HashMap<(Entity, u64), DVec2>,
    timer: Option<MotionTimer>,
}

struct MotionTimer {
    sender: Option<mpsc::Sender<bool>>,
    task: Option<thread::JoinHandle<()>>,
}

impl MotionTimer {
    fn new(wake: crate::native::wake::WakeSignal) -> Self {
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
                .with_collision_hooks::<WorkspaceContacts>()
                .build()
                .disable::<ColliderHierarchyPlugin>(),
        )
        .insert_resource(Gravity(DVec3::ZERO))
        .init_resource::<Simulation>()
        .add_systems(WorkspaceStep, apply_forces.before(PhysicsSystems::First))
        .add_systems(
            WorkspaceStep,
            crate::native::topology::terrain::step.before(apply_forces),
        )
        .add_systems(
            Update,
            simulate
                .in_set(SimulateWorkspaces)
                .after(crate::native::workspace::PrepareWorkspaces)
                .after(crate::native::record_view::ReceiveRecords),
        );
    }
}

pub(crate) fn held(world: &World, sand: Entity) -> bool {
    if world
        .get::<crate::native::protein_area::placement::Pending>(sand)
        .is_some()
        || crate::native::canvas_pan::dragged(world) == Some(sand)
        || crate::native::layout::desktop::manipulation::held(world, sand)
    {
        return true;
    }
    let mut focus = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    if focus == Some(sand)
        && world
            .get::<crate::native::layout::desktop::Managed>(sand)
            .is_some()
        && world.get::<bevy::text::EditableText>(sand).is_none()
    {
        return false;
    }
    let mut cursor = focus;
    while let Some(entity) = cursor {
        if world
            .get::<crate::native::protein_area::RecordBinding>(entity)
            .is_some()
            || world
                .get::<crate::native::record_binding::TextBinding>(entity)
                .is_some()
        {
            return false;
        }
        cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    while let Some(entity) = focus {
        if entity == sand {
            return true;
        }
        focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

pub(crate) fn room_transform(
    world: &mut World,
    sand: Entity,
    before: &nucleus::canvas::Pose,
    after: &nucleus::canvas::Pose,
) {
    let Some(point) = world.get::<CanvasItem>(sand).map(|item| item.position) else {
        return;
    };
    let rotation = bevy::math::DQuat::from_array(after.rotation)
        * bevy::math::DQuat::from_array(before.rotation).inverse();
    let bodies: Vec<_> = world
        .query::<(Entity, &BodyLink)>()
        .iter(world)
        .filter(|(_, link)| link.sand == sand)
        .map(|(entity, link)| (entity, link.origin))
        .collect();
    for (entity, origin) in bodies {
        world
            .entity_mut(entity)
            .insert(Position((point - origin).extend(0.0)));
        world.get_mut::<BodyLink>(entity).unwrap().last = point;
        if let Some(mut velocity) = world.get_mut::<LinearVelocity>(entity) {
            let rotated = rotation * DVec3::new(velocity.0.x, 0.0, velocity.0.y);
            velocity.0 = DVec3::new(rotated.x, rotated.z, 0.0);
        }
    }
}

fn synchronize(world: &mut World) -> bool {
    crate::native::topology::holes::refresh(world);
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
            !crate::native::layout::desktop::tiled(world, *entity)
                && world
                    .get::<crate::native::layout::desktop::Suppressed>(*entity)
                    .is_none()
                && world
                    .get::<crate::native::canvas_item::Ephemeral>(*entity)
                    .is_none()
                && world
                    .get::<crate::native::area::InfluenceArea>(*entity)
                    .is_none()
                && !crate::native::topology::rooms::fixed(world, *entity)
                && world
                    .get::<crate::native::arrow_sand::ArrowSand>(*entity)
                    .is_none()
                && world
                    .get::<crate::native::time_castle::AttachedCard>(*entity)
                    .is_none()
                && world
                    .get::<crate::native::protein_area::placement::Pending>(*entity)
                    .is_none()
                && world
                    .get::<crate::native::layout::LayoutBox>(*entity)
                    .is_none_or(|layout| layout.parent.is_none())
                && pin.is_none()
                && item.position.is_finite()
                && item.size.is_finite()
                && item.size.min_element() > 0.0
                && item.size.max_element() <= 100_000.0
                && crate::native::topology::holes::live(world, parent.parent(), member.0)
                && crate::native::workspace_config::enabled(world, parent.parent(), member.0)
        })
        .map(|(entity, item, parent, member, _)| {
            let mut item = crate::native::canvas::presented(world, entity).unwrap_or(*item);
            item.size *= world
                .get::<crate::native::area_effects::AreaScale>(entity)
                .map_or(1.0, |scale| scale.0);
            (entity, item, parent.parent(), member.0)
        })
        .collect();
    let dragged: HashSet<_> = crate::native::canvas_pan::dragged(world)
        .and_then(|entity| {
            world
                .get::<ChildOf>(entity)
                .map(|parent| (entity, parent.parent()))
        })
        .map(|(entity, root)| {
            crate::native::canvas_selection::companions(world, root, entity)
                .into_iter()
                .collect()
        })
        .unwrap_or_default();
    let held_groups: HashSet<_> = world
        .query::<(
            Entity,
            &crate::native::canvas_selection::SandGroup,
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
            .get::<crate::native::canvas_selection::SandGroup>(sand)
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
                    .resource_mut::<crate::native::area_effects::Influences>()
                    .forget(sand);
                world.entity_mut(entity).insert((
                    Position(position.extend(0.0)),
                    LinearVelocity::ZERO,
                    AngularVelocity::ZERO,
                ));
                world.entity_mut(entity).remove::<Sleeping>();
            }
            if changed_held {
                world.entity_mut(entity).insert(if held {
                    RigidBody::Kinematic
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
                        RigidBody::Kinematic
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
            world.entity_mut(body).insert(ColliderOf { body });
            retained.insert(body);
        }
    }
    for body in existing.into_values() {
        if !retained.contains(&body) && world.get_entity(body).is_ok() {
            changed = true;
            world.entity_mut(body).remove::<Collider>();
            world.despawn(body);
        }
    }
    changed |= synchronize_groups(world, &edited);
    let mut simulation = world.resource_mut::<Simulation>();
    origins.retain(|key, _| active_workspaces.contains(key));
    simulation.origins = origins;
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
    mut influences: ResMut<crate::native::area_effects::Influences>,
    motion: Option<Res<crate::native::protein_motion::Motion>>,
    records: Query<
        (
            Option<&RecordProperties>,
            Option<&crate::native::protein_area::RecordBinding>,
        ),
        With<CanvasItem>,
    >,
    mut bodies: Query<(Entity, &BodyLink, &Position, &mut ConstantForce)>,
    mut commands: Commands,
) {
    for (entity, link, position, mut force) in &mut bodies {
        let total = if !link.held
            && let Ok((record, binding)) = records.get(link.sand)
        {
            influences.total(
                link.sand,
                link.root,
                link.workspace,
                position.0.truncate() + link.origin,
                record,
                binding,
            )
        } else {
            DVec2::ZERO
        };
        let extra = if link.held {
            DVec3::ZERO
        } else {
            motion
                .as_ref()
                .and_then(|motion| motion.forces.get(&link.sand))
                .copied()
                .unwrap_or_default()
        };
        let next = (total + DVec2::new(extra.x, extra.z)).extend(0.0);
        if force.0 != next {
            force.0 = next;
            commands
                .entity(entity)
                .insert((SleepTimer(0.0), sleep_threshold(next, 1.0)))
                .remove::<Sleeping>();
        }
    }
}

fn simulate(world: &mut World) {
    crate::native::topology::terrain::prepare(world);
    crate::native::protein_motion::prepare(world);
    crate::native::area_effects::update(world);
    let spatial = world.contains_resource::<crate::native::topology::physics::Runtime>();
    let mut changed = if spatial {
        crate::native::topology::physics::synchronize(world)
    } else {
        synchronize(world)
    };
    if spatial {
        changed |= crate::native::topology::terrain::support(world);
    }
    world
        .run_system_cached(apply_forces)
        .expect("update workspace forces");
    let awake = spatial && crate::native::topology::physics::awake(world)
        || world
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
    if spatial {
        crate::native::topology::physics::apply(world);
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
    let mut active = spatial && crate::native::topology::physics::awake(world)
        || world
            .query::<(&BodyLink, Has<Sleeping>)>()
            .iter(world)
            .any(|(body, sleeping)| !body.held && !sleeping);
    if !active && (awake || changed) {
        crate::native::area_effects::update(world);
        if spatial {
            crate::native::topology::physics::synchronize(world);
        } else {
            world
                .run_system_cached(apply_forces)
                .expect("refresh settled workspace forces");
        }
        active = spatial && crate::native::topology::physics::awake(world)
            || world
                .query::<(&BodyLink, Has<Sleeping>)>()
                .iter(world)
                .any(|(body, sleeping)| !body.held && !sleeping);
    }
    if let Some(wake) = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned()
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
    crate::native::protein_motion::remember(world);
}

pub(crate) mod tests {
    use super::*;
    use crate::native::area::{AreaShape, Direction, Property, PropertyRule};
    use bevy::time::TimeUpdateStrategy;

    fn fixture(offset: DVec2) -> (App, Entity, Entity, Entity) {
        desktop_fixture(offset, false)
    }

    pub(super) fn desktop_fixture(offset: DVec2, desktop: bool) -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins((MinimalPlugins, WorkspacePhysicsPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(STEP));
        if desktop {
            app.add_plugins(crate::native::layout::LayoutPlugin)
                .insert_resource(crate::native::layout::desktop::DefaultLayout(
                    crate::host::WorkspaceLayout::Free,
                ));
        }
        app.finish();
        let root = app.world_mut().spawn(Workspaces::default()).id();
        if desktop {
            app.world_mut().entity_mut(root).insert((
                crate::native::canvas::CanvasView::default(),
                ComputedNode {
                    size: Vec2::new(1000.0, 700.0),
                    inverse_scale_factor: 1.0,
                    ..default()
                },
            ));
        }
        let mut area = InfluenceArea::new(AreaShape::Square, offset, DVec2::splat(1000.0));
        area.rules = vec![PropertyRule {
            property: Property::Quantity,
            value: "-3".into(),
        }];
        area.strength = 1000.0;
        let area = crate::native::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
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

    pub(super) fn advance(app: &mut App, ticks: usize) {
        for _ in 0..ticks {
            app.update();
        }
    }

    #[cfg_attr(test, test)]
    fn tiling_and_fullscreen_suppression_remove_physics_until_released() {
        let (mut app, root, _, sand) = fixture(DVec2::ZERO);
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 30);
        let start = app.world().get::<CanvasItem>(sand).unwrap().position;
        app.world_mut()
            .entity_mut(sand)
            .insert(crate::native::layout::desktop::Managed {
                id: crate::layout::desktop::PlacementId([1; 16]),
                floating: false,
            });
        advance(&mut app, 60);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, start);
        assert_eq!(
            app.world_mut()
                .query::<&BodyLink>()
                .iter(app.world())
                .count(),
            0
        );
        app.world_mut()
            .entity_mut(sand)
            .remove::<crate::native::layout::desktop::Managed>();
        app.world_mut()
            .entity_mut(sand)
            .insert(crate::native::layout::desktop::Suppressed);
        advance(&mut app, 60);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, start);
        app.world_mut()
            .entity_mut(sand)
            .remove::<crate::native::layout::desktop::Suppressed>();
        advance(&mut app, 60);
        assert!(app.world().get::<CanvasItem>(sand).unwrap().position.x < start.x - 10.0);
    }

    #[cfg_attr(test, test)]
    fn workspace_tiling_pauses_motion_and_free_mode_resumes_from_the_authored_position() {
        use crate::{host::DesktopAction, native::actions::Action};

        let (mut app, root, _, sand) = desktop_fixture(DVec2::ZERO, true);
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 30);
        let free = app.world().get::<CanvasItem>(sand).unwrap().position;
        assert!(free.x < 200.0);
        DesktopAction::ToggleTiling.apply(app.world_mut(), root);
        advance(&mut app, 4);
        let tiled = app.world().get::<CanvasItem>(sand).unwrap().position;
        assert_ne!(tiled, free);
        advance(&mut app, 60);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, tiled);
        assert_eq!(
            app.world_mut()
                .query::<&BodyLink>()
                .iter(app.world())
                .count(),
            0
        );
        DesktopAction::ToggleTiling.apply(app.world_mut(), root);
        advance(&mut app, 60);
        let resumed = app.world().get::<CanvasItem>(sand).unwrap().position;
        assert!(resumed.x < free.x - 10.0, "{free:?} -> {resumed:?}");
        assert!(resumed.y.abs() < 0.000_001, "{resumed:?}");
        assert_eq!(
            app.world_mut()
                .query::<&BodyLink>()
                .iter(app.world())
                .count(),
            1
        );
        DesktopAction::CycleItemWidth.apply(app.world_mut(), root);
        advance(&mut app, 2);
        let width = app.world().get::<CanvasItem>(sand).unwrap().size.x;
        advance(&mut app, 30);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().size.x, width);
        assert!(
            app.world().get::<Workspaces>(root).unwrap().entries[0]
                .desktop
                .items()[0]
                .free_width
                .is_some()
        );
    }

    #[cfg_attr(test, test)]
    fn following_uses_the_current_physics_position_without_relative_screen_motion() {
        use crate::{host::DesktopAction, native::actions::Action};
        let (mut app, root, _, sand) = desktop_fixture(DVec2::ZERO, true);
        app.world_mut()
            .entity_mut(root)
            .insert(crate::native::canvas_selection::SandSelection(vec![sand]));
        DesktopAction::Unfocus.apply(app.world_mut(), root);
        DesktopAction::ToggleFocus.apply(app.world_mut(), root);
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        let start = app.world().get::<CanvasItem>(sand).unwrap().position;
        for _ in 0..60 {
            app.update();
            let item = app.world().get::<CanvasItem>(sand).unwrap();
            let view = app
                .world()
                .get::<crate::native::canvas::CanvasView>(root)
                .unwrap();
            let top_left = view
                .screen_position(item, Vec2::new(1000.0, 700.0))
                .unwrap();
            let center = top_left + item.size * view.zoom as f32 * 0.5;
            assert!(
                center.distance(Vec2::new(500.0, 350.0)) < 0.001,
                "{center:?}"
            );
        }
        assert!(app.world().get::<CanvasItem>(sand).unwrap().position.x < start.x - 1.0);
    }

    #[test]
    fn room_floor_proxy_stops_a_body_and_releases_its_collision_when_removed() {
        use crate::native::{
            canvas_host::Identity,
            topology::{
                self,
                rooms::{Member, Room},
            },
        };
        use nucleus::canvas::{Pose, room as model};
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins((MinimalPlugins, WorkspacePhysicsPlugin))
            .init_resource::<topology::physics::Runtime>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(STEP))
            .insert_resource(Gravity(-DVec3::Y * 500.0));
        app.finish();
        let world = app.world_mut();
        let root = world.spawn(Workspaces::default()).id();
        crate::native::workspace_config::set_physics(world, root, 1, true);
        let room_id = nucleus::new_uid("placement");
        world.spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(500.0),
            },
            Room(default()),
            Identity(room_id.clone()),
            ChildOf(root),
            WorkspaceMember(1),
        ));
        let floor = world
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(500.0),
                },
                Member(model::Member {
                    room: room_id,
                    parent: None,
                    local: Pose {
                        position: [0.0, 40.0, 0.0],
                        ..model::identity_pose()
                    },
                    role: model::Role::Floor,
                }),
                topology::Spatial {
                    depth: Some(4.0),
                    world_pinned: true,
                    ..default()
                },
                ChildOf(root),
                WorkspaceMember(1),
            ))
            .id();
        let moving = world
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(40.0),
                },
                topology::Spatial {
                    elevation: 120.0,
                    depth: Some(20.0),
                    ..default()
                },
                ChildOf(root),
                WorkspaceMember(1),
            ))
            .id();
        advance(&mut app, 240);
        let height = topology::spatial(app.world(), moving).elevation;
        assert!(
            (59.0..=61.0).contains(&height),
            "Expected the body on the raised floor, got {height}"
        );
        let floor_body = app
            .world_mut()
            .query::<(Entity, &topology::physics::Body)>()
            .iter(app.world())
            .find(|(_, body)| body.members == [floor])
            .unwrap()
            .0;
        assert_eq!(
            app.world().get::<RigidBody>(floor_body),
            Some(&RigidBody::Static)
        );
        app.world_mut().despawn(floor);
        advance(&mut app, 120);
        assert!(app.world().get_entity(floor_body).is_err());
        assert!(topology::spatial(app.world(), moving).elevation < 40.0);
    }

    #[test]
    fn room_owners_and_members_release_independent_workspace_physics_bodies() {
        let (mut app, root, _, child) = fixture(DVec2::ZERO);
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 2);
        let position = app.world().get::<CanvasItem>(child).unwrap().position;
        let id = nucleus::new_uid("placement");
        let room = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(500.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
                crate::native::canvas_host::Identity(id.clone()),
                crate::native::topology::rooms::Room(nucleus::canvas::room::Room::default()),
            ))
            .id();
        app.world_mut()
            .entity_mut(child)
            .insert(crate::native::topology::rooms::Member(
                nucleus::canvas::room::Member {
                    room: id,
                    parent: None,
                    local: nucleus::canvas::Pose {
                        position: [position.x, 0.0, position.y],
                        ..nucleus::canvas::room::identity_pose()
                    },
                    role: nucleus::canvas::room::Role::Content,
                },
            ));
        advance(&mut app, 30);
        assert_eq!(
            app.world().get::<CanvasItem>(child).unwrap().position,
            position
        );
        assert!(
            !app.world_mut()
                .query::<&BodyLink>()
                .iter(app.world())
                .any(|link| link.sand == room || link.sand == child)
        );
    }

    #[cfg(test)]
    #[test]
    fn workspace_holes_keep_hidden_physics_active_and_collisions_separate() {
        for spatial in [false, true] {
            let (mut app, root, _, moving) = fixture(DVec2::ZERO);
            if spatial {
                app.world_mut()
                    .init_resource::<crate::native::topology::physics::Runtime>();
            }
            let mut space = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
            space.id = 2;
            {
                let mut spaces = app.world_mut().get_mut::<Workspaces>(root).unwrap();
                spaces.entries.push(space);
                spaces.active = 2;
            }
            crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
            crate::native::workspace_config::set_physics(app.world_mut(), root, 2, true);
            let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(50.0));
            area.hole = true;
            let opening = crate::native::area::spawn_area(app.world_mut(), root, 2, area).unwrap();
            let above = app
                .world_mut()
                .spawn((
                    CanvasItem {
                        position: DVec2::new(200.0, 0.0),
                        size: Vec2::splat(40.0),
                    },
                    WorkspaceMember(2),
                    ChildOf(root),
                ))
                .id();
            let start = app.world().get::<CanvasItem>(moving).unwrap().position;
            advance(&mut app, 120);
            assert!(app.world().get::<CanvasItem>(moving).unwrap().position.x < start.x - 10.0);
            assert_eq!(
                app.world().get::<CanvasItem>(above).unwrap().position,
                DVec2::new(200.0, 0.0),
            );
            app.world_mut()
                .get_mut::<InfluenceArea>(opening)
                .unwrap()
                .enabled = false;
            advance(&mut app, 1);
            let stopped = app.world().get::<CanvasItem>(moving).unwrap().position;
            advance(&mut app, 30);
            assert_eq!(
                app.world().get::<CanvasItem>(moving).unwrap().position,
                stopped
            );
        }
    }

    #[cfg(test)]
    #[test]
    fn attached_clock_cards_cannot_push_their_clock_or_gain_workspace_bodies() {
        let (mut baseline, root, _, clock) = fixture(DVec2::ZERO);
        crate::native::workspace_config::set_physics(baseline.world_mut(), root, 1, true);
        advance(&mut baseline, 120);
        let expected = baseline.world().get::<CanvasItem>(clock).unwrap().position;
        let (mut app, root, _, clock) = fixture(DVec2::ZERO);
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        let position = app.world().get::<CanvasItem>(clock).unwrap().position;
        let card = app
            .world_mut()
            .spawn((
                crate::native::time_castle::AttachedCard,
                CanvasItem {
                    position,
                    size: Vec2::splat(100.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        advance(&mut app, 120);
        assert!(
            (app.world().get::<CanvasItem>(clock).unwrap().position - expected).length() < 0.001
        );
        assert!(
            !app.world_mut()
                .query::<&BodyLink>()
                .iter(app.world())
                .any(|body| body.sand == card)
        );
        assert_eq!(
            app.world().get::<CanvasItem>(card).unwrap().position,
            position
        );
    }

    #[cfg(test)]
    #[test]
    fn layout_children_resume_independent_motion_only_after_detaching() {
        let (mut app, root, area, sand) = fixture(DVec2::ZERO);
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        crate::native::layout::attach(app.world_mut(), sand, area).unwrap();
        let before = app.world().get::<CanvasItem>(sand).unwrap().position;
        advance(&mut app, 120);
        assert_eq!(
            app.world().get::<CanvasItem>(sand).unwrap().position,
            before
        );
        crate::native::layout::detach(app.world_mut(), sand);
        advance(&mut app, 120);
        assert_ne!(
            app.world().get::<CanvasItem>(sand).unwrap().position,
            before
        );
    }

    #[cfg(test)]
    #[test]
    fn settled_sands_wake_when_quantity_switches_between_loose_areas() {
        for spatial in [false, true] {
            for sorted in [false, true] {
                let (mut app, root, owner, sand) = fixture(DVec2::ZERO);
                if spatial {
                    app.init_resource::<crate::native::topology::physics::Runtime>();
                }
                {
                    let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
                    area.size = [100.0; 2];
                    area.strength = 1200.0;
                    area.reach.mode = crate::native::area::ReachMode::Unlimited;
                    area.rules[0].value = "-1".into();
                    area.sorting = sorted.then(Default::default);
                }
                let mut other = app.world().get::<InfluenceArea>(owner).unwrap().clone();
                other.id = "a".repeat(32);
                other.center = [200.0, 0.0];
                other.rules[0].value = "-2".into();
                crate::native::area::spawn_area(app.world_mut(), root, 1, other).unwrap();
                crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
                for (quantity, x) in [(-1, 0.0), (-2, 200.0), (-1, 0.0), (-2, 200.0)] {
                    app.world_mut().get_mut::<RecordProperties>(sand).unwrap().0 =
                        serde_json::json!({"uid":"r_moving", "quantity":quantity});
                    for _ in 0..1200 {
                        app.update();
                        if !app.world().resource::<Simulation>().active {
                            break;
                        }
                    }
                    let point = app.world().get::<CanvasItem>(sand).unwrap().position;
                    assert!(
                        point.distance(DVec2::new(x, 0.0)) < 1.0,
                        "spatial={spatial} sorted={sorted} quantity={quantity}: {point:?}"
                    );
                    assert!(
                        !app.world().resource::<Simulation>().active,
                        "Simple attraction must stop its motion timer after arrival"
                    );
                    if spatial {
                        assert_eq!(
                            app.world()
                                .resource::<crate::native::topology::influence::Forces>()
                                .totals[&sand],
                            DVec3::ZERO
                        );
                    } else {
                        assert_eq!(
                            app.world()
                                .get::<crate::native::area::AreaForces>(sand)
                                .unwrap()
                                .total(),
                            DVec2::ZERO
                        );
                    }
                    advance(&mut app, 10);
                    assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, point);
                }
            }
        }
    }

    #[cfg(test)]
    #[test]
    fn weak_attraction_reaches_its_target_before_sleeping() {
        for spatial in [false, true] {
            let (mut app, root, owner, sand) = fixture(DVec2::ZERO);
            if spatial {
                app.init_resource::<crate::native::topology::physics::Runtime>();
            }
            app.world_mut()
                .get_mut::<CanvasItem>(sand)
                .unwrap()
                .position = DVec2::X * 2.0;
            {
                let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
                area.size = [100.0; 2];
                area.strength = 100.0;
            }
            crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
            for _ in 0..1200 {
                app.update();
                if !app.world().resource::<Simulation>().active {
                    break;
                }
            }
            let point = app.world().get::<CanvasItem>(sand).unwrap().position;
            assert!(point.length() <= 0.5, "spatial={spatial}: {point:?}");
            assert!(!app.world().resource::<Simulation>().active);
        }
    }

    #[cfg(test)]
    #[test]
    fn weak_constant_attraction_keeps_moving_outside_the_area() {
        for spatial in [false, true] {
            let (mut app, root, owner, sand) = fixture(DVec2::ZERO);
            if spatial {
                app.init_resource::<crate::native::topology::physics::Runtime>();
            }
            {
                let mut area = app.world_mut().get_mut::<InfluenceArea>(owner).unwrap();
                area.size = [100.0; 2];
                area.strength = 1.0;
                area.reach.mode = crate::native::area::ReachMode::Unlimited;
            }
            crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
            advance(&mut app, 120);
            let first = app.world().get::<CanvasItem>(sand).unwrap().position;
            advance(&mut app, 120);
            let second = app.world().get::<CanvasItem>(sand).unwrap().position;
            assert!(
                second.x < first.x - 0.1,
                "spatial={spatial}: {first:?} -> {second:?}"
            );
            assert!(app.world().resource::<Simulation>().active);
        }
    }

    #[cfg_attr(test, test)]
    fn area_reach_changes_wake_offscreen_bodies_and_apply_real_motion() {
        let (mut app, root, area, sand) = fixture(DVec2::ZERO);
        let start = DVec2::new(20_000.0, 0.0);
        app.world_mut()
            .get_mut::<CanvasItem>(sand)
            .unwrap()
            .position = start;
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 120);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, start);
        app.world_mut()
            .get_mut::<InfluenceArea>(area)
            .unwrap()
            .reach
            .mode = crate::native::area::ReachMode::Unlimited;
        advance(&mut app, 120);
        let moved = app.world().get::<CanvasItem>(sand).unwrap().position;
        assert!(moved.x < start.x - 50.0);
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, false);
        advance(&mut app, 2);
        {
            let mut area = app.world_mut().get_mut::<InfluenceArea>(area).unwrap();
            area.reach.mode = crate::native::area::ReachMode::Limited;
            area.reach.radius = 20_000.0;
        }
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 120);
        assert!(app.world().get::<CanvasItem>(sand).unwrap().position.x < moved.x - 50.0);
    }

    #[cfg_attr(test, test)]
    fn grouped_sands_keep_their_spacing_under_forces_and_release_joints_when_ungrouped() {
        let (mut app, root, _, first) = fixture(DVec2::ZERO);
        let group = crate::native::canvas_selection::SandGroup([7; 16]);
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
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
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
            .remove::<crate::native::canvas_selection::SandGroup>();
        app.world_mut()
            .entity_mut(second)
            .remove::<crate::native::canvas_selection::SandGroup>();
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
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 120);
        let moved = app.world().get::<CanvasItem>(sand).unwrap().position;
        assert!(moved.x < start.x - 50.0 && moved.x > 0.0, "{moved:?}");
        crate::native::actions::Action::apply(
            &crate::native::influence_report::Control::Workspace,
            app.world_mut(),
            root,
        );
        advance(&mut app, 30);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, moved);
        assert!(crate::native::workspace_config::physics_configured(
            app.world(),
            root,
            1
        ));
        crate::native::actions::Action::apply(
            &crate::native::influence_report::Control::Workspace,
            app.world_mut(),
            root,
        );
        advance(&mut app, 10);
        let resumed = app.world().get::<CanvasItem>(sand).unwrap().position;
        assert!(resumed.x < moved.x, "{resumed:?}");
        let moved = resumed;
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, false);
        advance(&mut app, 30);
        assert_eq!(app.world().get::<CanvasItem>(sand).unwrap().position, moved);
        assert_eq!(
            app.world_mut()
                .query::<&BodyLink>()
                .iter(app.world())
                .count(),
            0
        );
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
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
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
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
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        crate::native::workspace_config::set_physics(app.world_mut(), foreign_root, 1, true);
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
    fn moving_a_held_sand_keeps_pushing_after_other_sands_sleep() {
        let (mut app, root, area, sand) = fixture(DVec2::ZERO);
        app.world_mut().despawn(area);
        app.world_mut()
            .get_mut::<CanvasItem>(sand)
            .unwrap()
            .position = DVec2::ZERO;
        let other = app
            .world_mut()
            .spawn((
                CanvasItem {
                    position: DVec2::new(-80.0, 0.0),
                    size: Vec2::splat(40.0),
                },
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        let mut focus = InputFocus::default();
        focus.set(other, bevy::input_focus::FocusCause::Pressed);
        app.insert_resource(focus);
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 360);
        assert!(!app.world().resource::<Simulation>().active);
        for x in -79..80 {
            app.world_mut()
                .get_mut::<CanvasItem>(other)
                .unwrap()
                .position
                .x = f64::from(x);
            advance(&mut app, 2);
            let pushed = app.world().get::<CanvasItem>(sand).unwrap().position;
            assert!(
                pushed.x - f64::from(x) >= 38.0,
                "held {x}, pushed {pushed:?}"
            );
        }
        advance(&mut app, 360);
        let pushed = app.world().get::<CanvasItem>(sand).unwrap().position;
        assert!(pushed.x >= 117.0, "{pushed:?}");
    }

    #[cfg_attr(test, test)]
    fn repeated_fixed_steps_agree_within_one_millionth_of_a_canvas_unit() {
        let run = || {
            let (mut app, root, _, sand) = fixture(DVec2::new(1e9, -1e9));
            crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
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
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 30);
        assert_eq!(
            app.world().get::<CanvasItem>(sand).unwrap().position.x,
            200.0
        );
        app.world_mut().resource_mut::<InputFocus>().clear();
        advance(&mut app, 120);
        assert!(app.world().get::<CanvasItem>(sand).unwrap().position.x < 150.0);
    }

    #[test]
    fn bound_record_fields_keep_moving_while_focused() {
        let (mut app, root, _, sand) = fixture(DVec2::ZERO);
        app.world_mut()
            .entity_mut(sand)
            .insert(crate::native::protein_area::RecordBinding {
                area: root,
                uid: nucleus::new_uid("r"),
                source: crate::native::protein_area::Source::Local,
            });
        let editor = app.world_mut().spawn(ChildOf(sand)).id();
        let mut focus = InputFocus::default();
        focus.set(editor, bevy::input_focus::FocusCause::Pressed);
        app.insert_resource(focus);
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 120);
        assert!(app.world().get::<CanvasItem>(sand).unwrap().position.x < 150.0);
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(editor));
    }

    #[cfg_attr(test, test)]
    fn motion_timer_sleeps_until_started_and_releases_its_thread_on_drop() {
        let (sender, receiver) = mpsc::channel();
        let timer = MotionTimer::new(crate::native::wake::WakeSignal::new(move || {
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

    #[cfg_attr(test, test)]
    fn protein_membership_wakes_only_matching_source_bodies_and_stops_force_on_disconnect() {
        use crate::native::protein_area::{Config, RecordBinding, Source, filter::Matches};
        let (mut app, root, area, sand) = fixture(DVec2::ZERO);
        {
            let mut influence = app.world_mut().get_mut::<InfluenceArea>(area).unwrap();
            influence.filter = Some(Config::default());
            influence.rules.clear();
        }
        app.world_mut().entity_mut(sand).insert((
            RecordProperties(serde_json::json!({"uid":"task"})),
            RecordBinding {
                area,
                uid: "task".into(),
                source: Source::Organ("remote".into()),
            },
        ));
        app.world_mut().entity_mut(area).insert(Matches {
            source: Source::Local,
            uids: HashSet::from(["task".into()]),
            current: true,
        });
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 120);
        assert_eq!(
            app.world().get::<CanvasItem>(sand).unwrap().position,
            DVec2::new(200.0, 0.0)
        );
        app.world_mut().get_mut::<Matches>(area).unwrap().source = Source::Organ("remote".into());
        advance(&mut app, 120);
        assert!(app.world().get::<CanvasItem>(sand).unwrap().position.x < 150.0);
        app.world_mut().get_mut::<Matches>(area).unwrap().current = false;
        advance(&mut app, 1);
        assert!(
            app.world_mut()
                .query::<&ConstantForce>()
                .iter(app.world())
                .all(|force| force.0 == DVec3::ZERO)
        );
    }

    crate::native::laboratory_cases! {
        tiling_and_fullscreen_suppression_remove_physics_until_released,
        workspace_tiling_pauses_motion_and_free_mode_resumes_from_the_authored_position,
        following_uses_the_current_physics_position_without_relative_screen_motion,
        size_and_immunity_update_colliders_and_saved_force_without_resizing_the_sand,
        protein_membership_wakes_only_matching_source_bodies_and_stops_force_on_disconnect,
        area_reach_changes_wake_offscreen_bodies_and_apply_real_motion,
        grouped_sands_keep_their_spacing_under_forces_and_release_joints_when_ungrouped,
        toggles_start_real_motion_and_stop_at_current_positions_without_stale_velocity,
        repulsion_moves_outwards_and_pins_and_other_workspaces_are_unchanged,
        collisions_separate_sands_and_settle_without_crossing_box_boundaries,
        moving_a_held_sand_keeps_pushing_after_other_sands_sleep,
        repeated_fixed_steps_agree_within_one_millionth_of_a_canvas_unit,
        editing_a_sand_holds_it_and_releasing_focus_resumes_the_force,
        motion_timer_sleeps_until_started_and_releases_its_thread_on_drop,
    }

    #[cfg_attr(test, test)]
    fn size_and_immunity_update_colliders_and_saved_force_without_resizing_the_sand() {
        let (mut app, root, area, sand) = fixture(DVec2::ZERO);
        app.world_mut()
            .get_mut::<InfluenceArea>(area)
            .unwrap()
            .scale = 2.0;
        crate::native::workspace_config::set_physics(app.world_mut(), root, 1, true);
        advance(&mut app, 2);
        let body = app
            .world_mut()
            .query::<(Entity, &BodyLink)>()
            .iter(app.world())
            .find(|(_, link)| link.sand == sand)
            .unwrap()
            .0;
        assert_eq!(
            app.world().get::<BodyLink>(body).unwrap().size,
            Vec2::splat(80.0)
        );
        assert_eq!(
            app.world().get::<CanvasItem>(sand).unwrap().size,
            Vec2::splat(40.0)
        );
        assert!(app.world().get::<ConstantForce>(body).unwrap().0.x < 0.0);
        let mut shield = InfluenceArea::new(
            AreaShape::Square,
            DVec2::new(200.0, 0.0),
            DVec2::splat(200.0),
        );
        shield.immunity = crate::native::area_effects::Immunity::All;
        let shield = crate::native::area::spawn_area(app.world_mut(), root, 1, shield).unwrap();
        advance(&mut app, 2);
        assert_eq!(
            app.world().get::<ConstantForce>(body).unwrap().0,
            DVec3::ZERO
        );
        assert_eq!(
            app.world().get::<BodyLink>(body).unwrap().size,
            Vec2::splat(40.0)
        );
        app.world_mut().despawn(shield);
        advance(&mut app, 2);
        assert!(app.world().get::<ConstantForce>(body).unwrap().0.x < 0.0);
        assert_eq!(
            app.world().get::<BodyLink>(body).unwrap().size,
            Vec2::splat(80.0)
        );
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
