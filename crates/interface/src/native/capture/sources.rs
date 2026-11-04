use super::*;
use bevy::ecs::{
    entity_disabling::Disabled,
    query::{Allow, QueryData},
    system::SystemParam,
};

mod storage;
#[cfg(test)]
mod tests;
use storage::Path;
pub(crate) use storage::Scope;

#[derive(Clone, Copy)]
struct Links {
    boundary: bool,
    owners: [Option<Entity>; 5],
    parent: Option<Entity>,
}

#[derive(QueryData)]
struct Nodes {
    entity: Entity,
    layout: Has<LayoutBox>,
    workspace: Has<Workspaces>,
    windows: Has<WindowSources>,
    metadata: Has<crate::native::full_record::window::Metadata>,
    application: Has<crate::native::layout::desktop::ApplicationWindow>,
    visual: Option<&'static crate::native::topology::presentation::VisualOwner>,
    target: Option<&'static render::TargetSource>,
    shader: crate::native::shader_effects::CaptureOwners,
    parent: Option<&'static ChildOf>,
}

impl NodesItem<'_, '_> {
    fn links(&self) -> Links {
        let [floating, mesh, sphere] = self.shader.entities();
        Links {
            boundary: self.layout
                || self.workspace
                || self.windows
                || self.metadata
                || self.application,
            owners: [
                self.visual.map(|owner| owner.0),
                self.target.map(|owner| owner.0),
                floating,
                mesh,
                sphere,
            ],
            parent: self.parent.map(ChildOf::parent),
        }
    }
}

#[derive(SystemParam)]
pub(crate) struct Sources<'w, 's> {
    nodes: Query<'w, 's, Nodes, Allow<Disabled>>,
}

impl Sources<'_, '_> {
    #[cfg(test)]
    pub(crate) fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.nodes.iter().map(|node| node.entity)
    }

    #[cfg(test)]
    pub(crate) fn compact(&self, entity: Entity) -> Scope {
        resolve(entity, |entity| {
            self.nodes.get(entity).ok().map(|node| node.links())
        })
    }

    #[cfg(test)]
    pub(crate) fn resolve(&self, entity: Entity) -> BTreeSet<Entity> {
        self.compact(entity).into_btree()
    }

    pub(crate) fn snapshot(&self, limit: usize) -> Snapshot {
        let mut live = self.nodes.iter();
        let mut nodes: Vec<_> = live
            .by_ref()
            .take(limit)
            .map(|node| (node.entity, node.links()))
            .collect();
        let complete = live.next().is_none();
        nodes.sort_unstable_by_key(|(entity, _)| *entity);
        Snapshot { nodes, complete }
    }
}

pub(crate) struct Snapshot {
    nodes: Vec<(Entity, Links)>,
    complete: bool,
}

impl Snapshot {
    fn links(&self, entity: Entity) -> Option<Links> {
        self.nodes
            .binary_search_by_key(&entity, |(entity, _)| *entity)
            .ok()
            .map(|index| self.nodes[index].1)
    }

    pub(crate) fn resolve(&self, entity: Entity) -> Option<Scope> {
        if !self.complete {
            return None;
        }
        self.links(entity)?;
        Some(resolve(entity, |entity| self.links(entity)))
    }
}

pub fn semantic_sources(world: &World, entity: Entity) -> BTreeSet<Entity> {
    resolve(entity, |entity| {
        world.get_entity(entity).ok()?;
        let boundary = world.get::<LayoutBox>(entity).is_some()
            || world.get::<Workspaces>(entity).is_some()
            || world.get::<WindowSources>(entity).is_some()
            || world
                .get::<crate::native::full_record::window::Metadata>(entity)
                .is_some()
            || world
                .get::<crate::native::layout::desktop::ApplicationWindow>(entity)
                .is_some();
        let mut owners = [
            world
                .get::<crate::native::topology::presentation::VisualOwner>(entity)
                .map(|owner| owner.0),
            world
                .get::<render::TargetSource>(entity)
                .map(|owner| owner.0),
            None,
            None,
            None,
        ];
        for (slot, owner) in
            owners
                .iter_mut()
                .skip(2)
                .zip(crate::native::shader_effects::capture_sources(
                    world, entity,
                ))
        {
            *slot = Some(owner);
        }
        Some(Links {
            boundary,
            owners,
            parent: if boundary {
                None
            } else {
                world.get::<ChildOf>(entity).map(ChildOf::parent)
            },
        })
    })
    .into_btree()
}

fn resolve(entity: Entity, read: impl Fn(Entity) -> Option<Links>) -> Scope {
    fn visit(
        entity: Entity,
        read: &impl Fn(Entity) -> Option<Links>,
        path: &mut Path,
        budget: &mut usize,
        sources: &mut Scope,
    ) {
        if *budget == 0 || path.len() >= MAX_DEPTH || !path.insert(entity) {
            sources.insert(entity);
            return;
        }
        let Some(links) = read(entity) else {
            sources.insert(entity);
            return;
        };
        *budget -= 1;
        if links.boundary {
            sources.insert(entity);
        }
        let linked = links.owners.iter().any(Option::is_some);
        for owner in links.owners.into_iter().flatten() {
            visit(owner, read, path, budget, sources);
        }
        if !links.boundary {
            if let Some(parent) = links.parent {
                visit(parent, read, path, budget, sources);
            } else if !linked {
                sources.insert(entity);
            }
        }
        path.remove(&entity);
    }
    let mut sources = Scope::default();
    visit(entity, &read, &mut Path::default(), &mut 1024, &mut sources);
    sources
}
