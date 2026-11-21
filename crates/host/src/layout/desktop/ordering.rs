// non-lean-yet

#[cfg(feature = "layout-telemetry")]
use lince_instrumentation::add;

use super::PlacementId;

#[derive(Default)]
pub(super) struct InfluenceOrder {
    members: Selected<(i64, PlacementId)>,
    groups: Selected<i64>,
    examined: usize,
    prepared: usize,
    swaps: usize,
}

impl InfluenceOrder {
    pub(super) fn members<T>(
        &mut self,
        items: &mut [T],
        key: impl Fn(&T) -> Option<(i64, PlacementId)>,
    ) {
        self.examined += items.len();
        self.swaps += self.members.apply(items, key);
        self.prepared += self.members.slots.len();
    }

    pub(super) fn groups<T>(&mut self, items: &mut [T], key: impl Fn(&T) -> Option<i64>) {
        self.examined += items.len();
        self.swaps += self.groups.apply(items, key);
        self.prepared += self.groups.slots.len();
    }

    pub(super) fn record(&self) {
        #[cfg(feature = "layout-telemetry")]
        {
            add("desktop-order-entries-examined", self.examined as u64);
            add("desktop-order-keys-prepared", self.prepared as u64);
            add("desktop-order-payload-swaps", self.swaps as u64);
        }
    }
}

struct Ranked<K> {
    key: K,
    source: usize,
}

struct Selected<K> {
    slots: Vec<usize>,
    ordered: Vec<Ranked<K>>,
}

impl<K> Default for Selected<K> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            ordered: Vec::new(),
        }
    }
}

impl<K: Ord> Selected<K> {
    fn apply<T>(&mut self, items: &mut [T], key: impl Fn(&T) -> Option<K>) -> usize {
        self.slots.clear();
        self.ordered.clear();
        for (slot, item) in items.iter().enumerate() {
            if let Some(key) = key(item) {
                self.ordered.push(Ranked {
                    key,
                    source: self.slots.len(),
                });
                self.slots.push(slot);
            }
        }
        if self
            .ordered
            .windows(2)
            .all(|pair| pair[0].key <= pair[1].key)
        {
            return 0;
        }
        self.ordered
            .sort_unstable_by(|a, b| a.key.cmp(&b.key).then_with(|| a.source.cmp(&b.source)));
        let mut swaps = 0;
        for start in 0..self.ordered.len() {
            let mut current = start;
            let mut next = self.ordered[current].source;
            while next != start {
                items.swap(self.slots[current], self.slots[next]);
                swaps += 1;
                self.ordered[current].source = current;
                current = next;
                next = self.ordered[current].source;
            }
            self.ordered[current].source = current;
        }
        swaps
    }
}
