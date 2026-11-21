//! Borrowed document keys and pending request identities for Record bindings.

use super::{Count, Key, RecordBinding, Source, performance};
use bevy::platform::collections::{Equivalent, HashMap};
use std::{
    collections::{HashSet, hash_map::RandomState},
    hash::{Hash, Hasher},
};

pub(super) type Documents<V> = HashMap<Key, V, RandomState>;

pub(super) struct Borrowed<'a> {
    source: &'a Source,
    uid: &'a str,
}

impl Hash for Borrowed<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.source, self.uid).hash(state);
    }
}

impl Equivalent<Key> for Borrowed<'_> {
    fn equivalent(&self, key: &Key) -> bool {
        self.source == &key.0 && self.uid == key.1.as_str()
    }
}

impl From<&Borrowed<'_>> for Key {
    fn from(key: &Borrowed<'_>) -> Self {
        (key.source.clone(), key.uid.to_owned())
    }
}

pub(super) fn key_ref(record: &RecordBinding) -> Borrowed<'_> {
    key_parts(&record.source, &record.uid)
}

pub(super) fn key_parts<'a>(source: &'a Source, uid: &'a str) -> Borrowed<'a> {
    performance::count(Count::RecordBindingBorrowedKeys);
    Borrowed { source, uid }
}

enum Lookup<'a> {
    Fresh,
    Scanned,
    Indexed(HashSet<&'a str>),
}

pub(super) struct PendingIds<'a, I> {
    values: I,
    lookup: Lookup<'a>,
}

impl<'a, I: Iterator<Item = &'a str> + Clone> PendingIds<'a, I> {
    pub(super) fn new(values: I) -> Self {
        Self {
            values,
            lookup: Lookup::Fresh,
        }
    }

    pub(super) fn contains(&mut self, id: &str) -> bool {
        match &self.lookup {
            Lookup::Indexed(index) => index.contains(id),
            Lookup::Scanned if self.values.size_hint().1.is_none_or(|count| count > 1) => {
                let index: HashSet<_> = self.values.clone().collect();
                let found = index.contains(id);
                self.lookup = Lookup::Indexed(index);
                performance::count(Count::RecordEditPendingIndexes);
                found
            }
            Lookup::Fresh | Lookup::Scanned => {
                self.lookup = Lookup::Scanned;
                self.values.clone().any(|candidate| candidate == id)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Entity;

    #[test]
    fn borrowed_keys_preserve_source_identity_for_the_same_uid() {
        let record = RecordBinding {
            area: Entity::PLACEHOLDER,
            uid: "same".into(),
            source: Source::Local,
        };
        let other = Source::Organ("office".into());
        let local = key_ref(&record);
        let remote = key_parts(&other, "same");
        let mut documents = Documents::default();
        documents.entry_ref(&local).or_insert(1);
        documents.entry_ref(&remote).or_insert(2);
        assert_eq!(documents.get(&local), Some(&1));
        assert_eq!(documents.get(&remote), Some(&2));
        assert_eq!(documents.remove(&local), Some(1));
        assert_eq!(documents.get(&remote), Some(&2));
    }

    #[test]
    fn pending_ids_index_only_after_a_scan_with_an_uncertain_or_larger_size() {
        let mut many = PendingIds::new(["one", "two", "one"].into_iter());
        assert!(many.contains("two"));
        assert!(matches!(many.lookup, Lookup::Scanned));
        assert!(!many.contains("missing"));
        assert!(matches!(&many.lookup, Lookup::Indexed(index) if index.len() == 2));
        assert!(many.contains("one"));

        let mut single = PendingIds::new(std::iter::once("one"));
        assert!(!single.contains("missing"));
        assert!(single.contains("one"));
        assert!(matches!(single.lookup, Lookup::Scanned));

        let mut next = Some("one");
        let mut unknown = PendingIds::new(std::iter::from_fn(move || next.take()));
        assert!(unknown.contains("one"));
        assert!(!unknown.contains("missing"));
        assert!(matches!(unknown.lookup, Lookup::Indexed(_)));
    }
}
