// non-lean-yet
// Scope: struct Held.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 26, 31, 60, 65.

use bevy::input::keyboard::{Key, KeyCode};
use std::{
    collections::{BTreeMap, btree_map::Entry},
    ops::Deref,
    sync::{Arc, OnceLock},
};

#[derive(Default)]
pub(super) struct Held {
    logical: BTreeMap<KeyCode, Key>,
    codes: Option<Arc<[KeyCode]>>,
}

impl Deref for Held {
    type Target = BTreeMap<KeyCode, Key>;

    fn deref(&self) -> &Self::Target {
        &self.logical
    }
}

impl Held {
    pub(super) fn press(&mut self, code: KeyCode, key: &Key) {
        match self.logical.entry(code) {
            Entry::Vacant(entry) => {
                entry.insert(key.clone());
                self.codes = None;
            }
            Entry::Occupied(mut entry) => {
                if entry.get() != key {
                    entry.insert(key.clone());
                }
            }
        }
    }

    pub(super) fn remove(&mut self, code: &KeyCode) {
        if self.logical.remove(code).is_some() {
            self.codes = None;
        }
    }

    pub(super) fn clear(&mut self) {
        if !self.logical.is_empty() {
            self.logical.clear();
            self.codes = None;
        }
    }

    pub(super) fn snapshot(&mut self) -> Arc<[KeyCode]> {
        self.codes
            .get_or_insert_with(|| {
                if self.logical.is_empty() {
                    static EMPTY: OnceLock<Arc<[KeyCode]>> = OnceLock::new();
                    return EMPTY
                        .get_or_init(|| {
                            engine::performance::add("keyboard-held-snapshots", 1);
                            Arc::default()
                        })
                        .clone();
                }
                engine::performance::add("keyboard-held-snapshots", 1);
                self.logical.keys().copied().collect()
            })
            .clone()
    }
}
