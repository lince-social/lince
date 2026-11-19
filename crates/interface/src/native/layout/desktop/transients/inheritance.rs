use super::*;

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
            let request = self
                .owners
                .get_mut(&root)
                .unwrap()
                .requests
                .get_mut(&id)
                .unwrap();
            request.needs_model |= matches!(request.excluding(entity), Owner::Missing);
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
        engine::performance::add("desktop-restored-owner-roots", self.owners.len() as u64);
        engine::performance::add("desktop-restored-children-examined", children_examined);
        engine::performance::add(
            "desktop-restored-model-items-examined",
            model_items_examined,
        );
    }

    fn apply_restored(&self, world: &mut World) {
        for &(entity, id, root) in &self.restored {
            let owners = &self.owners[&root];
            let request = &owners.requests[&id];
            match request.excluding(entity) {
                Owner::Unique(parent) => {
                    world
                        .entity_mut(entity)
                        .remove::<RestoredTransient>()
                        .insert(TransientParent(parent));
                }
                Owner::Missing if owners.known && !request.retained => {
                    world.entity_mut(entity).remove::<RestoredTransient>();
                    crate::native::notifications::report(
                        world,
                        "Item owner",
                        "The saved owner is no longer available. Its retained item was restored independently.",
                    );
                }
                _ => {}
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
            if entity == parent || world.get_entity(parent).is_err() {
                world.entity_mut(entity).remove::<TransientParent>();
                continue;
            }
            if let Some(member) = world.get::<WorkspaceMember>(parent).copied()
                && world.get::<WorkspaceMember>(entity).map(|member| member.0) != Some(member.0)
            {
                world.entity_mut(entity).insert(member);
                engine::performance::add("desktop-transient-memberships-written", 1);
            }
        }
    }
}

pub(super) fn inherit(world: &mut World) {
    let _inheritance = engine::performance::Span::new("desktop-transient-inheritance");
    world.init_resource::<Inheritance>();
    world.resource_scope(|world, mut inheritance: Mut<Inheritance>| {
        inheritance.prepare_restored(world);
        inheritance.apply_restored(world);
        inheritance.inherit_current(world);
    });
}
