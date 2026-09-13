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
    Group(Entity, u64, crate::canvas_selection::SandGroup),
}

fn scope(world: &World, mut entity: Entity, name: &str) -> Scope {
    loop {
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
            return Scope::Entity(entity);
        }
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
