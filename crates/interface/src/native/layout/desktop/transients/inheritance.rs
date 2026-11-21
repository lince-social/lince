use super::{
    LayoutBox, RestoredTransient, TransientParent, WorkspaceMember, Workspaces, direct_children,
};
use crate::native::notifications::report;
use bevy::prelude::*;
use engine::performance::{Span, add};
use std::collections::HashMap;

#[derive(Resource)]
struct Inheritance {
    restored_query: QueryState<(Entity, &'static RestoredTransient, &'static ChildOf)>,
    transient_query: QueryState<(Entity, &'static TransientParent)>,
    restored: Vec<(Entity, [u8; 16], Entity)>,
    transients: Vec<(Entity, Entity)>,
    owners: HashMap<Entity, RootOwners>,
}

impl FromWorld for Inheritance {
    fn from_world(world: &mut World) -> Self {
        Self {
            restored_query: world.query(),
            transient_query: world.query(),
            restored: Vec::new(),
            transients: Vec::new(),
            owners: HashMap::new(),
        }
    }
}

#[derive(Default)]
struct RootOwners {
    requests: HashMap<[u8; 16], Candidates>,
    known: bool,
}

#[derive(Default)]
struct Candidates {
    // Three matches suffice to detect ambiguity after excluding the requester.
    first: [Option<Entity>; 3],
    needs_model: bool,
    retained: bool,
}

#[derive(Clone, Copy)]
enum Owner {
    Missing,
    Unique(Entity),
    Ambiguous,
}

impl Candidates {
    fn add(&mut self, entity: Entity) {
        if let Some(slot) = self.first.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(entity);
        }
    }

    fn excluding(&self, entity: Entity) -> Owner {
        let mut matches = self
            .first
            .iter()
            .flatten()
            .copied()
            .filter(|candidate| *candidate != entity);
        match (matches.next(), matches.next()) {
            (Some(parent), None) => Owner::Unique(parent),
            (Some(_), Some(_)) => Owner::Ambiguous,
            _ => Owner::Missing,
        }
    }
}

impl Inheritance {
    fn prepare_restored(&mut self, world: &World) {
        self.restored.clear();
        self.restored.extend(
            self.restored_query
                .iter(world)
                .map(|(entity, parent, root)| (entity, parent.0, root.parent())),
        );
        if self.restored.is_empty() {
            self.owners.clear();
            return;
        }
        for owners in self.owners.values_mut() {
            owners.requests.clear();
            owners.known = false;
        }
        for &(_, id, root) in &self.restored {
            self.owners
                .entry(root)
                .or_default()
                .requests
                .entry(id)
                .or_default();
        }
        self.owners.retain(|_, owners| !owners.requests.is_empty());
        let mut children_examined = 0;
        for (&root, owners) in &mut self.owners {
            for child in direct_children(world, root) {
                children_examined += 1;
                if let Some(layout) = world.get::<LayoutBox>(child)
                    && let Some(request) = owners.requests.get_mut(&layout.id)
                {
                    request.add(child);
                }
            }
        }
        for &(entity, id, root) in &self.restored {
            if let Some(request) = self
                .owners
                .get_mut(&root)
                .and_then(|owners| owners.requests.get_mut(&id))
                && !request.needs_model
            {
                request.needs_model = matches!(request.excluding(entity), Owner::Missing);
            }
        }
        let mut model_items_examined = 0;
        for (&root, owners) in &mut self.owners {
            let mut remaining = owners
                .requests
                .values()
                .filter(|request| request.needs_model)
                .count();
            if remaining == 0 {
                continue;
            }
            let Some(spaces) = world.get::<Workspaces>(root) else {
                continue;
            };
            owners.known = true;
            'models: for space in spaces
                .entries
                .iter()
                .chain(spaces.trash.iter().map(|entry| &entry.workspace))
            {
                for item in space.desktop.items() {
                    model_items_examined += 1;
                    if let Some(request) = owners.requests.get_mut(&item.id.0)
                        && request.needs_model
                        && !request.retained
                    {
                        request.retained = true;
                        remaining -= 1;
                        if remaining == 0 {
                            break 'models;
                        }
                    }
                }
            }
        }
        add("desktop-restored-owner-roots", self.owners.len() as u64);
        add("desktop-restored-children-examined", children_examined);
        add(
            "desktop-restored-model-items-examined",
            model_items_examined,
        );
    }

    fn apply_restored(&self, world: &mut World) {
        for &(entity, id, root) in &self.restored {
            let Some(owners) = self.owners.get(&root) else {
                continue;
            };
            let Some(request) = owners.requests.get(&id) else {
                continue;
            };
            let parent = match request.excluding(entity) {
                Owner::Unique(parent) => Some(parent),
                Owner::Missing if owners.known && !request.retained => None,
                _ => continue,
            };
            let Ok(mut target) = world.get_entity_mut(entity) else {
                continue;
            };
            target.remove::<RestoredTransient>();
            if let Some(parent) = parent {
                target.insert(TransientParent(parent));
            } else {
                report(
                    world,
                    "Item owner",
                    "The saved owner is no longer available. Its retained item was restored independently.",
                );
            }
        }
    }

    fn inherit_current(&mut self, world: &mut World) {
        self.transients.clear();
        self.transients.extend(
            self.transient_query
                .iter(world)
                .map(|(entity, parent)| (entity, parent.0)),
        );
        for &(entity, parent) in &self.transients {
            if entity != parent
                && let Ok(owner) = world.get_entity(parent)
            {
                if let Some(member) = owner.get::<WorkspaceMember>().copied()
                    && let Ok(mut target) = world.get_entity_mut(entity)
                    && target.get::<WorkspaceMember>() != Some(&member)
                {
                    target.insert(member);
                    add("desktop-transient-memberships-written", 1);
                }
            } else if let Ok(mut target) = world.get_entity_mut(entity) {
                target.remove::<TransientParent>();
            }
        }
    }
}

pub(super) fn inherit(world: &mut World) {
    let _inheritance = Span::new("desktop-transient-inheritance");
    world.init_resource::<Inheritance>();
    world.resource_scope(|world, mut inheritance: Mut<'_, Inheritance>| {
        inheritance.prepare_restored(world);
        inheritance.apply_restored(world);
        inheritance.inherit_current(world);
    });
}
