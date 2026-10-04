use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventBoundary(pub Vec<String>);

impl EventBoundary {
    pub fn valid(&self) -> bool {
        self.0.len() <= 64
            && self
                .0
                .iter()
                .all(|name| !name.is_empty() && name.len() <= 128)
    }
}

#[derive(Component)]
pub struct EventListener(pub Vec<String>);

#[derive(Component)]
pub(crate) struct IsolatedEvents;

#[derive(EntityEvent, Clone, Debug)]
pub struct SandEvent {
    pub entity: Entity,
    pub source: Entity,
    pub name: String,
    pub value: Value,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Entity(Entity),
    Workspace(Entity, u64),
    Group(Entity, u64, crate::canvas_selection::SandGroup),
}

fn scope(world: &World, mut entity: Entity, name: &str) -> Scope {
    let mut workspace = None;
    loop {
        if workspace.is_none() { workspace = world.get::<crate::workspace::WorkspaceMember>(entity).map(|member| member.0); }
        if world.get::<IsolatedEvents>(entity).is_some() {
            return world.get::<crate::workspace::Workspaces>(entity).map_or(Scope::Entity(entity), |spaces| Scope::Workspace(entity, workspace.unwrap_or(spaces.active)));
        }
        let parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
        if let (Some(parent), Some(group)) = (
            parent,
            world.get::<crate::canvas_selection::SandGroup>(entity),
        ) {
            let workspace = world
                .get::<crate::workspace::WorkspaceMember>(entity)
                .map_or(0, |m| m.0);
            let blocked = world.get::<Children>(parent).is_some_and(|children| {
                children.iter().any(|sibling| {
                    world.get::<crate::canvas_selection::SandGroup>(sibling) == Some(group)
                        && world
                            .get::<crate::workspace::WorkspaceMember>(sibling)
                            .map_or(0, |m| m.0)
                            == workspace
                        && world
                            .get::<EventBoundary>(sibling)
                            .is_some_and(|b| b.0.iter().any(|n| n == name))
                })
            });
            if blocked {
                return Scope::Group(parent, workspace, *group);
            }
        }
        if world
            .get::<EventBoundary>(entity)
            .is_some_and(|b| b.0.iter().any(|n| n == name))
        {
            return world.get::<crate::workspace::Workspaces>(entity).map_or(Scope::Entity(entity), |spaces| Scope::Workspace(entity, workspace.unwrap_or(spaces.active)));
        }
        if let Some(spaces) = world.get::<crate::workspace::Workspaces>(entity) { return Scope::Workspace(entity, workspace.unwrap_or(spaces.active)); }
        match parent {
            Some(parent) => entity = parent,
            None => return Scope::Entity(entity),
        }
    }
}

pub fn emit(world: &mut World, source: Entity, name: &str, value: Value) {
    if world.get_entity(source).is_err() || crate::laboratory::suspended(world, source) {
        return;
    }
    let origin = scope(world, source, name);
    let listeners: Vec<_> = world
        .query::<(Entity, &EventListener)>()
        .iter(world)
        .filter(|(entity, listener)| {
            listener.0.iter().any(|n| n == name) && scope(world, *entity, name) == origin
        })
        .map(|(entity, _)| entity)
        .collect();
    for entity in listeners {
        world.trigger(SandEvent {
            entity,
            source,
            name: name.into(),
            value: value.clone(),
        });
    }
}

#[derive(Clone)]
pub(crate) struct ToggleDateBoundary;

#[cfg(test)]
mod isolation_tests {
    use super::*;

    #[test]
    fn record_selections_cannot_cross_workspaces_on_one_canvas() {
        let mut world = World::new();
        let canvas = world.spawn(crate::workspace::Workspaces::default()).id();
        let first = world.spawn((ChildOf(canvas), crate::workspace::WorkspaceMember(1))).id();
        let second = world.spawn((ChildOf(canvas), crate::workspace::WorkspaceMember(2))).id();
        let source = world.spawn(ChildOf(first)).id();
        let receiver = world.spawn(ChildOf(first)).id();
        let other = world.spawn(ChildOf(second)).id();
        assert!(scope(&world, source, "Record selected") == scope(&world, receiver, "Record selected"));
        assert!(scope(&world, source, "Record selected") != scope(&world, other, "Record selected"));
    }

    #[test]
    fn arbitrary_named_events_stay_within_each_balloon() {
        let mut world = World::new();
        let canvas = world.spawn_empty().id();
        let host = world.spawn(ChildOf(canvas)).id();
        let first = world.spawn((ChildOf(host), IsolatedEvents)).id();
        let second = world.spawn((ChildOf(host), IsolatedEvents)).id();
        let source = world.spawn(ChildOf(first)).id();
        let inside = world.spawn(ChildOf(first)).id();
        let other = world.spawn(ChildOf(second)).id();
        let outside = world.spawn(ChildOf(canvas)).id();
        for name in ["Date selected", "Custom interaction", "Another event"] {
            assert!(scope(&world, source, name) == scope(&world, inside, name));
            assert!(scope(&world, source, name) != scope(&world, outside, name));
            assert!(scope(&world, source, name) != scope(&world, other, name));
        }
    }
}

impl crate::actions::Action for ToggleDateBoundary {
    fn apply(&self, world: &mut World, target: Entity) {
        let Some(root) = world.get::<ChildOf>(target).map(ChildOf::parent) else {
            return;
        };
        if !world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|m| m.enabled)
        {
            return;
        }
        let members = crate::canvas_selection::group_members(world, root, target);
        let blocked = members.iter().any(|entity| {
            world.get::<EventBoundary>(*entity).is_some_and(|b| {
                b.0.iter()
                    .any(|name| name == crate::calendar::DATE_SELECTED)
            })
        });
        for entity in members {
            let mut boundary = world
                .get::<EventBoundary>(entity)
                .cloned()
                .unwrap_or_default();
            boundary
                .0
                .retain(|name| name != crate::calendar::DATE_SELECTED);
            if !blocked {
                boundary.0.push(crate::calendar::DATE_SELECTED.into());
            }
            world.entity_mut(entity).insert(boundary);
        }
    }
}
