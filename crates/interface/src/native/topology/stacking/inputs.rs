use super::Order;
use crate::{
    layout::desktop::PlacementId,
    native::{layout::desktop, workspace},
};
use bevy::prelude::*;
use std::collections::HashMap;

pub(crate) struct Keys<'w> {
    world: &'w World,
    roots: HashMap<Entity, Root<'w>>,
    owners: u64,
}

#[derive(Default)]
struct Root<'w> {
    children: Option<&'w Children>,
    siblings: HashMap<Entity, usize>,
    next_sibling: usize,
    windows: HashMap<PlacementId, Entity>,
    next_window: usize,
    focused: HashMap<u64, Option<Entity>>,
}

impl<'w> Keys<'w> {
    pub(crate) fn new(world: &'w World) -> Self {
        Self {
            world,
            roots: HashMap::new(),
            owners: 0,
        }
    }

    pub(crate) fn key(&mut self, entity: Entity) -> Order {
        let _preparation = nucleus::performance::Span::new("native-stacking-inputs");
        self.owners += 1;
        let world = self.world;
        let managed = world.get::<desktop::Managed>(entity);
        let mut order = Order {
            floating: managed.is_none_or(|managed| managed.floating),
            active: (false, 0),
            index: world.get::<ZIndex>(entity).map_or(0, |index| index.0),
            sibling: 0,
            part: 0,
        };
        let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
            return order;
        };
        let root = self.roots.entry(parent).or_insert_with(|| Root {
            children: world.get::<Children>(parent),
            ..Default::default()
        });
        order.sibling = root.sibling(entity);
        if managed.is_some_and(|managed| managed.floating)
            && world.get::<desktop::ApplicationWindow>(entity).is_some()
            && let Some(workspace) = world.get::<workspace::WorkspaceMember>(entity)
            && let Some((ancestor, depth)) = family(world, entity, parent, workspace.0)
            && root.focused(world, parent, workspace.0) == Some(ancestor)
        {
            order.active = (true, depth);
        }
        order
    }
}

impl Root<'_> {
    fn sibling(&mut self, entity: Entity) -> usize {
        if let Some(index) = self.siblings.get(&entity) {
            return *index;
        }
        if let Some(children) = self.children {
            while let Some(&child) = children.get(self.next_sibling) {
                let index = self.next_sibling;
                self.next_sibling += 1;
                self.siblings.entry(child).or_insert(index);
                if child == entity {
                    return index;
                }
            }
        }
        0
    }

    fn window(&mut self, world: &World, focused: PlacementId) -> Option<Entity> {
        if let Some(entity) = self.windows.get(&focused) {
            return Some(*entity);
        }
        let children = self.children?;
        while let Some(&entity) = children.get(self.next_window) {
            self.next_window += 1;
            if let Some(managed) = world.get::<desktop::Managed>(entity)
                && world.get::<desktop::ApplicationWindow>(entity).is_some()
            {
                self.windows.entry(managed.id).or_insert(entity);
                if managed.id == focused {
                    return Some(entity);
                }
            }
        }
        None
    }

    fn focused(&mut self, world: &World, root: Entity, workspace: u64) -> Option<Entity> {
        if let Some(ancestor) = self.focused.get(&workspace) {
            return *ancestor;
        }
        let ancestor = (|| {
            let id = world
                .get::<workspace::Workspaces>(root)?
                .entries
                .iter()
                .find(|entry| entry.id == workspace)?
                .desktop
                .focused()?;
            let target = self.window(world, id)?;
            family(world, target, root, workspace).map(|(ancestor, _)| ancestor)
        })();
        self.focused.insert(workspace, ancestor);
        ancestor
    }
}

fn family(world: &World, mut entity: Entity, root: Entity, workspace: u64) -> Option<(Entity, u8)> {
    for depth in 0..32 {
        if world.get::<ChildOf>(entity)?.parent() != root
            || world.get::<workspace::WorkspaceMember>(entity)?.0 != workspace
        {
            return None;
        }
        let Some(parent) = world.get::<desktop::TransientParent>(entity) else {
            return Some((entity, depth));
        };
        entity = parent.0;
    }
    None
}

impl Drop for Keys<'_> {
    fn drop(&mut self) {
        let (siblings, windows, focused) = self.roots.values().fold(
            (0_u64, 0_u64, 0_u64),
            |(siblings, windows, focused), root| {
                (
                    siblings + root.next_sibling as u64,
                    windows + root.next_window as u64,
                    focused + root.focused.len() as u64,
                )
            },
        );
        nucleus::performance::add("native-stacking-input-owners", self.owners);
        nucleus::performance::add("native-stacking-sibling-visits", siblings);
        nucleus::performance::add("native-stacking-window-visits", windows);
        nucleus::performance::add("native-stacking-focus-resolutions", focused);
    }
}
