use super::*;
use crate::actions::Action;

#[derive(Component, Clone)]
struct AuthoredEvents {
    scopes: BTreeMap<Entity, String>,
    original: Vec<String>,
}

#[derive(Component, Clone)]
struct Routes {
    edits: Edits,
    entities: BTreeMap<String, Entity>,
}

pub(super) fn install(
    world: &mut World,
    host: Entity,
    lookup: &BTreeMap<String, Entity>,
    edits: &Edits,
) {
    let changed = world
        .get::<Routes>(host)
        .is_none_or(|routes| routes.edits != *edits || routes.entities != *lookup);
    if !changed {
        return;
    }
    world.entity_mut(host).insert(Routes {
        edits: edits.clone(),
        entities: lookup.clone(),
    });
    for (id, entity) in lookup {
        let original = world
            .get::<AuthoredEvents>(*entity)
            .map(|events| events.original.clone())
            .unwrap_or_else(|| {
                world
                    .get::<crate::scoped_events::EventListener>(*entity)
                    .map(|listener| listener.0.clone())
                    .unwrap_or_default()
            });
        let mut names: BTreeSet<_> = original.iter().cloned().collect();
        if let Some(events) = world.get::<AuthoredEvents>(*entity) {
            for (scope, target) in &events.scopes {
                if *scope == host {
                    continue;
                }
                if let Some(routes) = world.get::<Routes>(*scope) {
                    names.extend(
                        routes
                            .edits
                            .behaviors
                            .values()
                            .filter(|behavior| behavior.enabled && &behavior.part == target)
                            .map(|behavior| behavior.event.clone()),
                    );
                    names.extend(
                        routes
                            .edits
                            .links
                            .values()
                            .filter(|link| link.enabled && &link.from == target)
                            .map(|link| link.event.clone()),
                    );
                }
            }
        }
        names.extend(
            edits
                .behaviors
                .values()
                .filter(|behavior| behavior.enabled && &behavior.part == id)
                .map(|behavior| behavior.event.clone()),
        );
        names.extend(
            edits
                .links
                .values()
                .filter(|link| link.enabled && &link.from == id)
                .map(|link| link.event.clone()),
        );
        world
            .entity_mut(*entity)
            .insert(crate::scoped_events::EventListener(
                names.into_iter().collect(),
            ));
        if let Some(mut events) = world.get_mut::<AuthoredEvents>(*entity) {
            events.scopes.insert(host, id.clone());
        } else {
            world
                .entity_mut(*entity)
                .insert(AuthoredEvents {
                    scopes: BTreeMap::from([(host, id.clone())]),
                    original,
                })
                .observe(event)
                .observe(activate);
        }
    }
}

fn event(
    event: On<crate::scoped_events::SandEvent>,
    parts: Query<&AuthoredEvents>,
    mut commands: Commands,
) {
    let Ok(part) = parts.get(event.entity) else {
        return;
    };
    let scopes = part.scopes.clone();
    let name = event.name.clone();
    commands.queue(move |world: &mut World| {
        for (host, id) in scopes {
            fire(world, host, &id, &name);
        }
    });
}

fn activate(
    event: On<bevy::ui_widgets::Activate>,
    parts: Query<&AuthoredEvents>,
    mut commands: Commands,
) {
    let Ok(part) = parts.get(event.entity) else {
        return;
    };
    let scopes = part.scopes.clone();
    commands.queue(move |world: &mut World| {
        for (host, id) in scopes {
            fire(world, host, &id, "activate");
        }
    });
}

pub(super) fn fire(world: &mut World, host: Entity, part: &str, event: &str) {
    if crate::laboratory::suspended(world, host) {
        return;
    }
    let Some(routes) = world.get::<Routes>(host).cloned() else {
        return;
    };
    let mut pending = std::collections::VecDeque::from([(part.to_string(), event.to_string())]);
    let mut visited = BTreeSet::new();
    while let Some((part, event)) = pending.pop_front() {
        if !visited.insert((part.clone(), event.clone())) || visited.len() > 256 {
            continue;
        }
        let Some(entity) = routes.entities.get(&part).copied() else {
            continue;
        };
        if world.get_entity(entity).is_err()
            || routes
                .edits
                .parts
                .get(&part)
                .is_some_and(|edit| edit.removed)
        {
            continue;
        }
        for behavior in
            routes.edits.behaviors.values().filter(|behavior| {
                behavior.enabled && behavior.part == part && behavior.event == event
            })
        {
            if let Ok(action) = serde_json::from_value(behavior.action.clone()) {
                crate::component_push::composition::Invoke(action).apply(world, entity);
            }
        }
        for link in routes
            .edits
            .links
            .values()
            .filter(|link| link.enabled && link.from == part && link.event == event)
        {
            pending.push_back((link.to.clone(), link.receive.clone()));
        }
    }
}
