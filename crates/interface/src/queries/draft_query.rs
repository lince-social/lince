// non-lean-yet
// Scope: struct DraftQuery, struct Snapshot.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 21, 42, 114.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::{
    fmt,
    ops::{Deref, DerefMut},
    sync::{Arc, OnceLock},
};

#[derive(Clone)]
pub struct DraftQuery(Arc<Snapshot>);

struct Snapshot {
    value: Value,
    valid: OnceLock<bool>,
    decoded: OnceLock<Result<Arc<protein::Protein>, String>>,
}

impl Clone for Snapshot {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            valid: OnceLock::new(),
            decoded: OnceLock::new(),
        }
    }
}

impl From<Value> for DraftQuery {
    fn from(value: Value) -> Self {
        Self(Arc::new(Snapshot {
            value,
            valid: OnceLock::new(),
            decoded: OnceLock::new(),
        }))
    }
}

impl From<DraftQuery> for Value {
    fn from(query: DraftQuery) -> Self {
        match Arc::try_unwrap(query.0) {
            Ok(snapshot) => snapshot.value,
            Err(snapshot) => snapshot.value.clone(),
        }
    }
}

impl Deref for DraftQuery {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.0.value
    }
}

impl DerefMut for DraftQuery {
    fn deref_mut(&mut self) -> &mut Value {
        let snapshot = Arc::make_mut(&mut self.0);
        snapshot.valid.take();
        snapshot.decoded.take();
        &mut snapshot.value
    }
}

impl PartialEq for DraftQuery {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.value == other.0.value
    }
}

impl fmt::Debug for DraftQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0.value, formatter)
    }
}

impl Serialize for DraftQuery {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DraftQuery {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Value::deserialize(deserializer).map(Self::from)
    }
}

impl DraftQuery {
    pub(super) fn same_encoding(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || nucleus::json::same_value_encoding(&self.0.value, &other.0.value)
    }

    pub(super) fn valid_storage(&self) -> bool {
        *self.0.valid.get_or_init(|| {
            let _preparation = nucleus::performance::Span::new("protein-draft-storage-preparation");
            nucleus::performance::add("protein-draft-storage-preparations", 1);
            let value = &self.0.value;
            nucleus::json::encoded_len(value).is_ok_and(|bytes| bytes <= 131_072)
                && super::bounded(value, 0, &mut 0)
                && value.is_object()
                && value["source"].is_string()
                && value["where"].is_array()
        })
    }

    pub(super) fn decoded(&self) -> Result<Arc<protein::Protein>, String> {
        self.0
            .decoded
            .get_or_init(|| {
                let _preparation = nucleus::performance::Span::new("protein-draft-decoding");
                nucleus::performance::add("protein-drafts-decoded", 1);
                super::decode_query(&self.0.value).map(Arc::new)
            })
            .clone()
    }
}
