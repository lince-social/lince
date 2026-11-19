use super::{Count, Key, RecordBinding, Source, performance};
use bevy::platform::collections::Equivalent;
use std::hash::{Hash, Hasher};

pub(super) type Documents<V> =
    bevy::platform::collections::HashMap<Key, V, std::collections::hash_map::RandomState>;

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
