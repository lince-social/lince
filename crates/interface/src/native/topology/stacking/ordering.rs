// non-lean-yet
// Scope: type Key, type Phase, struct Group, struct Preparation.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   53, 70, 82, 88, 141.

use super::*;
use bevy::render::render_phase::SortedRenderPhase;

type Key = (Entity, MainEntity);
type Phase = SortedRenderPhase<Transparent3d>;

#[derive(Default)]
struct Group {
    entries: Vec<(usize, Key, Order)>,
    disordered: bool,
}

#[derive(Default)]
pub(super) struct Preparation {
    pub orders: HashMap<Entity, (Order, Plane)>,
    pub anchors: HashMap<Entity, (Entity, i8)>,
    planes: HashMap<Plane, usize>,
    groups: Vec<Group>,
    used_groups: usize,
    slots: Vec<usize>,
    ranks: HashMap<Key, usize>,
    decorations: Decorations,
}

impl Preparation {
    pub fn apply(&mut self, phase: &mut Phase) {
        if phase.items.len() < 2 {
            return;
        }
        self.coplanar(phase);
        self.decorations.apply(phase, &self.anchors);
        self.clear_phase();
    }

    fn coplanar(&mut self, phase: &mut Phase) {
        let _preparation = nucleus::performance::Span::new("native-coplanar-order");
        let mut disordered = false;
        for (index, (entity, item)) in phase.items.iter().enumerate() {
            if !item.distance.is_finite() {
                continue;
            }
            let Some((order, plane)) = self.orders.get(&entity.1.id()) else {
                continue;
            };
            let next = self.used_groups;
            let group = *self.planes.entry(*plane).or_insert(next);
            if group == next {
                if self.groups.len() == next {
                    self.groups.push(Group::default());
                }
                self.used_groups += 1;
            }
            let group = &mut self.groups[group];
            if group.entries.last().is_some_and(|entry| entry.2 > *order) {
                group.disordered = true;
                disordered = true;
            }
            group.entries.push((index, *entity, *order));
        }
        if !disordered {
            return;
        }
        self.ranks.extend(
            phase
                .items
                .keys()
                .enumerate()
                .map(|(index, entity)| (*entity, index)),
        );
        for group in self.groups[..self.used_groups]
            .iter_mut()
            .filter(|group| group.disordered)
        {
            self.slots.clear();
            self.slots.extend(group.entries.iter().map(|entry| entry.0));
            group.entries.sort_by_key(|entry| entry.2);
            for (slot, (_, entity, _)) in self.slots.iter().zip(&group.entries) {
                self.ranks.insert(*entity, *slot);
            }
            nucleus::performance::add("native-stacking-plane-sorts", 1);
        }
        phase.items.sort_by_key(|entity, _| self.ranks[entity]);
        nucleus::performance::add("native-stacking-phase-reorders", 1);
    }

    fn clear_phase(&mut self) {
        self.planes.clear();
        for group in &mut self.groups[..self.used_groups] {
            group.entries.clear();
            group.disordered = false;
        }
        self.used_groups = 0;
        self.slots.clear();
        self.ranks.clear();
    }

    pub fn clear_frame(&mut self) {
        self.clear_phase();
        self.orders.clear();
        self.anchors.clear();
    }
}

#[derive(Default)]
pub(super) struct Decorations {
    original: HashMap<Entity, usize>,
    ranks: HashMap<Key, (usize, i8)>,
}

impl Decorations {
    pub fn apply(&mut self, phase: &mut Phase, anchors: &HashMap<Entity, (Entity, i8)>) {
        if anchors.is_empty()
            || phase.items.len() < 2
            || !phase
                .items
                .keys()
                .any(|entity| anchors.contains_key(&entity.1.id()))
        {
            return;
        }
        let _preparation = nucleus::performance::Span::new("native-decoration-order");
        self.original.extend(
            phase
                .items
                .keys()
                .enumerate()
                .map(|(index, key)| (key.1.id(), index)),
        );
        let mut previous = None;
        let mut disordered = false;
        for (index, key) in phase.items.keys().enumerate() {
            let rank = anchors
                .get(&key.1.id())
                .and_then(|(face, part)| self.original.get(face).map(|index| (*index, *part)))
                .unwrap_or((index, 0));
            disordered |= previous.is_some_and(|previous| previous > rank);
            previous = Some(rank);
            self.ranks.insert(*key, rank);
        }
        if disordered {
            phase.items.sort_by_key(|entity, _| self.ranks[entity]);
            nucleus::performance::add("native-stacking-decoration-reorders", 1);
        }
        self.original.clear();
        self.ranks.clear();
    }
}
