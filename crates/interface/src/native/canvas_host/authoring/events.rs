// non-lean-yet
// Scope: struct AuthoredEvents, struct Routes, struct EventRoute, struct Routing.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 48, 50, 53, 59,
//   61, 64, 68, 69, 73, 75, 101, 105, 109, 135, 141, 158, 159, 175, 188.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   215, 223.

use super::*;
use crate::native::actions::Action;

#[derive(Component, Clone)]
struct AuthoredEvents {
    scopes: BTreeMap<Entity, String>,
    original: Vec<String>,
}

#[derive(Component, Clone)]
struct Routes(Arc<Routing>);

#[derive(Default)]
struct EventRoute {
    behaviors: Vec<String>,
    links: Vec<String>,
}

struct Routing {
    behaviors: BTreeMap<String, api::authoring::Behavior>,
    links: BTreeMap<String, api::authoring::Link>,
    removed: BTreeSet<String>,
    entities: BTreeMap<String, Entity>,
    events: BTreeMap<String, BTreeMap<String, EventRoute>>,
}

impl Routing {
    fn matches(&self, lookup: &BTreeMap<String, Entity>, edits: &Edits) -> bool {
        self.entities == *lookup
            && self.behaviors == edits.behaviors
            && self.links == edits.links
            && self.removed.iter().map(String::as_str).eq(edits
                .parts
                .iter()
                .filter_map(|(id, edit)| edit.removed.then_some(id.as_str())))
    }

    fn prepare(lookup: &BTreeMap<String, Entity>, edits: &Edits) -> Self {
        let _preparation =
            engine::performance::Span::new("native-canvas-authoring-route-preparation");
        crate::host::performance::count(
            crate::host::performance::Count::CanvasAuthoringRouteRebuilds,
        );
        let mut events = BTreeMap::<String, BTreeMap<String, EventRoute>>::new();
        for (id, behavior) in &edits.behaviors {
            if behavior.enabled {
                events
                    .entry(behavior.part.clone())
                    .or_default()
                    .entry(behavior.event.clone())
                    .or_default()
                    .behaviors
                    .push(id.clone());
            }
        }
        for (id, link) in &edits.links {
            if link.enabled {
                events
                    .entry(link.from.clone())
                    .or_default()
                    .entry(link.event.clone())
                    .or_default()
                    .links
                    .push(id.clone());
            }
        }
        Self {
            behaviors: edits.behaviors.clone(),
            links: edits.links.clone(),
            removed: edits
                .parts
                .iter()
                .filter_map(|(id, edit)| edit.removed.then(|| id.clone()))
                .collect(),
            entities: lookup.clone(),
            events,
        }
    }

    fn names(&self, part: &str) -> impl Iterator<Item = &String> {
        self.events
            .get(part)
            .into_iter()
            .flat_map(|events| events.keys())
    }
}

pub(super) fn install(
    world: &mut World,
    host: Entity,
    lookup: &BTreeMap<String, Entity>,
    edits: &Edits,
) {
    let changed = world
        .get::<Routes>(host)
        .is_none_or(|routes| !routes.0.matches(lookup, edits));
    if !changed {
        return;
    }
    let routes = Arc::new(Routing::prepare(lookup, edits));
    world.entity_mut(host).insert(Routes(routes.clone()));
    for (id, entity) in lookup {
        let original = world
            .get::<AuthoredEvents>(*entity)
            .map(|events| events.original.clone())
            .unwrap_or_else(|| {
                world
                    .get::<crate::native::scoped_events::EventListener>(*entity)
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
                    names.extend(routes.0.names(target).cloned());
                }
            }
        }
        names.extend(routes.names(id).cloned());
        let names: Vec<_> = names.into_iter().collect();
        if world
            .get::<crate::native::scoped_events::EventListener>(*entity)
            .is_none_or(|listener| listener.0 != names)
        {
            world
                .entity_mut(*entity)
                .insert(crate::native::scoped_events::EventListener(names));
        }
        if let Some(mut events) = world.get_mut::<AuthoredEvents>(*entity) {
            if events.scopes.get(&host) != Some(id) {
                events.scopes.insert(host, id.clone());
            }
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
    event: On<crate::native::scoped_events::SandEvent>,
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
    let _event = engine::performance::Span::new("native-canvas-authoring-event");
    if crate::native::laboratory::suspended(world, host) {
        return;
    }
    let Some(routes) = world.get::<Routes>(host).map(|routes| routes.0.clone()) else {
        return;
    };
    if routes.removed.contains(part)
        || routes
            .events
            .get(part)
            .is_none_or(|events| !events.contains_key(event))
    {
        return;
    }
    let mut pending = std::collections::VecDeque::from([(part, event)]);
    let mut visited = BTreeSet::new();
    while let Some((part, event)) = pending.pop_front() {
        if !visited.insert((part, event)) || visited.len() > 256 {
            continue;
        }
        let Some(entity) = routes.entities.get(part).copied() else {
            continue;
        };
        if world.get_entity(entity).is_err() || routes.removed.contains(part) {
            continue;
        }
        let Some(selected) = routes.events.get(part).and_then(|events| events.get(event)) else {
            continue;
        };
        for id in &selected.behaviors {
            let behavior = &routes.behaviors[id];
            if let Ok(action) =
                <engine::actions::Action as serde::Deserialize>::deserialize(&behavior.action)
            {
                crate::native::component_push::composition::Invoke(action).apply(world, entity);
            }
        }
        for id in &selected.links {
            let link = &routes.links[id];
            pending.push_back((link.to.as_str(), link.receive.as_str()));
        }
    }
}
