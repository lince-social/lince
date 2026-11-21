// non-lean-yet
// Scope: struct WorldInstance, struct InstanceOf, struct Instances, struct AnchoredAt.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (34, 59, 113,
//   140).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (105).
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 65, 66.

use super::*;
use bevy::ecs::{entity_disabling::Disabled, query::Allow, system::InRef};

#[derive(Component)]
pub(super) struct WorldInstance {
    pub extent: f32,
    pub requested_visible: bool,
}

#[derive(Component)]
#[relationship(relationship_target = Instances)]
pub(super) struct InstanceOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = InstanceOf)]
pub(super) struct Instances(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = AnchoredMesh)]
pub(super) struct AnchoredAt(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = AnchoredAt)]
pub(super) struct AnchoredMesh(Entity);

pub(super) fn at(world: &World, anchor: Entity) -> Option<Entity> {
    let entity = world.get::<AnchoredMesh>(anchor)?.0;
    world.get::<WorldInstance>(entity).map(|_| entity)
}

pub(super) fn visible(world: &mut World, owner: Entity) -> bool {
    world
        .run_system_cached_with(any_visible, owner)
        .expect("read current spatial instance visibility")
}

fn any_visible(
    In(owner): In<Entity>,
    world: &mut World,
    instances: &mut QueryState<(&'static WorldInstance, &'static Visibility)>,
) -> bool {
    world.get::<Instances>(owner).is_some_and(|owned| {
        instances
            .iter_many(world, owned.0.iter().copied()).matched()
            .any(|(_, visibility)| *visibility == Visibility::Visible)
    })
}

pub(super) fn publish(
    world: &mut World,
    owner: Entity,
    mesh: &Handle<Mesh>,
    material: &Handle<MeshMaterial>,
    extent: f32,
    volume: bool,
) {
    world
        .run_system_cached_with(publish_current, (owner, mesh, material, (extent, volume)))
        .expect("publish current owned spatial instances");
}

fn publish_current(
    (In(owner), InRef(mesh), InRef(material), In((extent, volume))): (
        In<Entity>,
        InRef<Handle<Mesh>>,
        InRef<Handle<MeshMaterial>>,
        In<(f32, bool)>,
    ),
    world: &mut World,
    instances: &mut QueryState<(
        Entity,
        &'static WorldInstance,
        &'static Transform,
        &'static Visibility,
    )>,
    mut targets: Local<Vec<(Entity, f32, Transform, bool)>>,
) {
    let _publication = nucleus::performance::Span::new("shader-instance-publication");
    targets.clear();
    if let Some(owned) = world.get::<Instances>(owner) {
        targets.extend(instances.iter_many(world, owned.0.iter().copied()).matched().map(
            |(entity, instance, transform, visibility)| {
                (
                    entity,
                    instance.extent,
                    *transform,
                    *visibility == Visibility::Visible,
                )
            },
        ));
    }
    nucleus::performance::add("shader-instances-prepared", targets.len() as u64);
    if targets.is_empty() {
        *targets = Vec::new();
        return;
    }
    if volume && targets.iter().any(|(_, _, _, visible)| *visible) {
        volume::world_views(world);
    }
    for (entity, previous_extent, mut transform, _) in targets.drain(..) {
        transform.scale *= previous_extent / extent;
        publish_body(world, entity, mesh, material, extent, None, volume);
        publish_transform(world, entity, transform);
        if previous_extent != extent {
            world.get_mut::<WorldInstance>(entity).unwrap().extent = extent;
        }
    }
}

pub(super) fn remove(world: &mut World, owner: Entity) {
    world
        .run_system_cached_with(remove_owned, owner)
        .expect("retire owned spatial instances");
}

fn remove_owned(In(owner): In<Entity>, world: &mut World, mut targets: Local<Vec<Entity>>) {
    targets.clear();
    if let Some(owned) = world.get::<Instances>(owner) {
        targets.extend(owned.0.iter().copied());
    }
    for entity in targets.drain(..) {
        if world.despawn(entity) {
            nucleus::performance::add("shader-instances-retired", 1);
        }
    }
    if world.get::<Instances>(owner).is_none() {
        *targets = Vec::new();
    }
}

#[derive(Default)]
struct Cleanup {
    orphans: Vec<Entity>,
    visibility: Vec<(Entity, Visibility)>,
}

pub(super) fn prune(world: &mut World) {
    world
        .run_system_cached(prune_current)
        .expect("retire spatial orphans and publish visibility");
}

fn prune_current(
    world: &mut World,
    orphans: &mut QueryState<
        Entity,
        (
            With<WorldInstance>,
            Or<(Without<InstanceOf>, Without<AnchoredAt>)>,
            Allow<Disabled>,
        ),
    >,
    instances: &mut QueryState<(
        Entity,
        &'static InstanceOf,
        &'static WorldInstance,
        &'static Visibility,
    )>,
    mut scratch: Local<Cleanup>,
) {
    let _maintenance = nucleus::performance::Span::new("shader-instance-maintenance");
    let Cleanup {
        orphans: retired,
        visibility,
    } = &mut *scratch;
    retired.clear();
    retired.extend(orphans.iter(world));
    for entity in retired.drain(..) {
        if world.despawn(entity) {
            nucleus::performance::add("shader-instances-retired", 1);
        }
    }
    visibility.clear();
    visibility.extend(
        instances
            .iter(world)
            .filter_map(|(entity, owner, instance, current)| {
                let next = if instance.requested_visible && enabled(world, owner.0) {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
                (*current != next).then_some((entity, next))
            }),
    );
    for (entity, visibility) in visibility.drain(..) {
        world.entity_mut(entity).insert(visibility);
    }
    if instances.iter(world).next().is_none() {
        *scratch = default();
    }
}
