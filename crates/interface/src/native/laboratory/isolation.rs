use bevy::{ecs::system::SystemParam, prelude::*};
use super::workspace::{Laboratory, LaboratoryRoot, SuspendedRoot};
use crate::native::workspace::WorkspaceMember;

#[derive(Component)]
pub(crate) struct LaboratoryOwner;

#[derive(SystemParam)]
pub(crate) struct OwnerAccess<'w, 's> {
    laboratory: Option<Res<'w, Laboratory>>,
    ancestry: Query<'w, 's, (
        Option<&'static ChildOf>, Option<&'static WorkspaceMember>,
        Has<LaboratoryOwner>, Has<LaboratoryRoot>, Has<SuspendedRoot>,
    )>,
}

impl OwnerAccess<'_, '_> {
    pub(crate) fn blocked(&self, entity: Entity) -> bool {
        if self.laboratory.as_ref().is_some_and(|lab| lab.global_pause()) { return true; }
        let scope = self.laboratory.as_ref().and_then(|lab| lab.owner_scope());
        belongs(entity, scope, |entity| {
            let (parent, member, temporary, laboratory, suspended) = self.ancestry.get(entity).ok()?;
            Some((parent.map(ChildOf::parent), member.map(|member| member.0), temporary || laboratory, suspended))
        }, true)
    }
}

fn belongs(
    mut entity: Entity,
    scope: Option<(Entity, u64)>,
    mut ancestry: impl FnMut(Entity) -> Option<(Option<Entity>, Option<u64>, bool, bool)>,
    include_suspended: bool,
) -> bool {
    let mut workspace = None;
    for _ in 0..crate::native::layout::MAX_DEPTH {
        let Some((parent, member, temporary, suspended)) = ancestry(entity) else { return true };
        if temporary || include_suspended && suspended { return true; }
        if workspace.is_none() { workspace = member; }
        if let Some((root, temporary)) = scope
            && entity == root { return workspace == Some(temporary); }
        let Some(parent) = parent else { return false };
        entity = parent;
    }
    true
}

pub fn temporary_owner(world: &World, entity: Entity) -> bool {
    let scope = super::scope(world).map(|(root, _, workspace, _)| (root, workspace));
    belongs(entity, scope, |entity| {
        if world.get_entity(entity).is_err() { return None; }
        Some((world.get::<ChildOf>(entity).map(ChildOf::parent),
            world.get::<WorkspaceMember>(entity).map(|member| member.0),
            world.get::<LaboratoryOwner>(entity).is_some() || world.get::<LaboratoryRoot>(entity).is_some(),
            world.get::<SuspendedRoot>(entity).is_some()))
    }, false)
}

pub fn blocked(world: &World, entity: Entity) -> bool {
    !super::normal(world) || temporary_owner(world, entity) || super::suspended(world, entity)
}

pub(crate) fn source(world: &World, entity: Entity) -> Option<String> {
    temporary_owner(world, entity).then(|| format!("laboratory-internal:{}", entity.to_bits()))
}

pub(crate) fn reserved(source: &str) -> bool { source.starts_with("laboratory-internal:") }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_visibility_deadlines_run_during_scoped_laboratory_and_pause_in_sandbox() {
        let mut app = App::new();
        app.add_plugins(crate::native::time_limit::TimeLimitPlugin);
        let root = app.world_mut().spawn_empty().id();
        let ordinary = app.world_mut().spawn((
            ChildOf(root), WorkspaceMember(1),
            crate::native::time_limit::TimeLimit(std::time::Duration::ZERO),
        )).id();
        app.insert_resource(Laboratory::scoped_for_test(root, 2, 1));
        app.update();
        assert_eq!(app.world().get::<Visibility>(ordinary), Some(&Visibility::Hidden));
        let mut laboratory = Laboratory::default();
        laboratory.root = Some(root);
        app.insert_resource(laboratory);
        app.world_mut().entity_mut(ordinary).insert(Visibility::Inherited);
        app.update();
        assert_eq!(app.world().get::<Visibility>(ordinary), Some(&Visibility::Inherited));
    }

    #[test]
    fn scoped_access_and_exclusive_world_access_agree_and_sandbox_remains_paused() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let ordinary = world.spawn((ChildOf(root), WorkspaceMember(1))).id();
        let temporary = world.spawn((ChildOf(root), WorkspaceMember(2))).id();
        world.insert_resource(Laboratory::scoped_for_test(root, 2, 1));
        assert!(super::super::normal(&world));
        let mut state = bevy::ecs::system::SystemState::<OwnerAccess>::new(&mut world);
        let access = state.get(&world).expect("Owner access is available");
        assert!(!access.blocked(ordinary));
        assert!(access.blocked(temporary));
        assert!(!blocked(&world, ordinary));
        assert!(blocked(&world, temporary));
        let mut laboratory = Laboratory::default();
        laboratory.root = Some(root);
        world.insert_resource(laboratory);
        assert!(!super::super::normal(&world));
        assert!(blocked(&world, ordinary));
        let access = state.get(&world).expect("Owner access is available");
        assert!(access.blocked(ordinary));
    }

    #[test]
    fn temporary_ancestry_is_denied_without_blocking_an_ordinary_sibling() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let ordinary = world.spawn((ChildOf(root), WorkspaceMember(1))).id();
        let benchmark = world.spawn((ChildOf(root), WorkspaceMember(2), LaboratoryOwner)).id();
        let editor = world.spawn(ChildOf(benchmark)).id();
        assert!(!blocked(&world, ordinary));
        assert!(blocked(&world, editor));
        assert!(reserved(&source(&world, editor).unwrap()));
        assert!(source(&world, ordinary).is_none());
        world.despawn(benchmark);
        assert!(blocked(&world, editor));
    }

    #[test]
    fn workspace_boundary_uses_the_owners_ancestry() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let ordinary = world.spawn((ChildOf(root), WorkspaceMember(1))).id();
        let benchmark = world.spawn((ChildOf(root), WorkspaceMember(2))).id();
        let editor = world.spawn(ChildOf(benchmark)).id();
        let inspect = |entity| Some((world.get::<ChildOf>(entity).map(ChildOf::parent), world.get::<WorkspaceMember>(entity).map(|member| member.0), false, false));
        assert!(!belongs(ordinary, Some((root, 2)), inspect, true));
        assert!(belongs(editor, Some((root, 2)), inspect, true));
        assert!(!belongs(root, Some((root, 2)), inspect, true));
    }
}
