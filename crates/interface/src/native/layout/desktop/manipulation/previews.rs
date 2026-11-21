// non-lean-yet
// Scope: struct Members, struct Previews, fn remove.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;
use std::collections::HashSet;

#[derive(Default)]
struct Members {
    ordered: Vec<Entity>,
    lookup: HashSet<Entity>,
}

impl Members {
    fn clear(&mut self) {
        self.ordered.clear();
        self.lookup.clear();
    }
}

#[derive(Default)]
pub(super) struct Previews {
    current: Members,
    next: Members,
}

fn remove(world: &mut World, entity: Entity) {
    if let Ok(mut entity) = world.get_entity_mut(entity) {
        entity.remove::<ItemPreview>();
        entity.remove::<CompactViewportPreview>();
    }
}

impl Previews {
    pub(super) fn contains(&self, entity: &Entity) -> bool {
        self.current.lookup.contains(entity)
    }

    pub(super) fn begin(&mut self) {
        self.next.clear();
    }

    pub(super) fn push(&mut self, entity: Entity) {
        self.next.ordered.push(entity);
        self.next.lookup.insert(entity);
    }

    pub(super) fn finish(&mut self, world: &mut World) {
        let _membership = engine::performance::Span::new("desktop-manipulation-preview-membership");
        let mut retired = 0;
        for entity in self.current.ordered.drain(..) {
            if !self.next.lookup.contains(&entity) {
                remove(world, entity);
                retired += 1;
            }
        }
        engine::performance::add(
            "desktop-manipulation-preview-items",
            self.next.ordered.len() as u64,
        );
        engine::performance::add("desktop-manipulation-preview-retirements", retired);
        std::mem::swap(&mut self.current, &mut self.next);
        self.next.clear();
    }

    pub(super) fn clear(&mut self, world: &mut World) {
        for entity in self.current.ordered.drain(..) {
            remove(world, entity);
        }
        self.current.lookup.clear();
        self.next.clear();
    }
}
