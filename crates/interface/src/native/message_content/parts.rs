// non-lean-yet
// Scope: struct Parts, struct Content.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 24, 25, 82.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   68, 69.

use nucleus::message::MessagePart;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt,
    ops::Deref,
    sync::{Arc, OnceLock},
};

#[derive(Clone, Default)]
pub(crate) struct Parts(Arc<Content>);

#[derive(Default)]
struct Content {
    parts: Vec<MessagePart>,
    keys: Vec<u64>,
    next_key: u64,
    validation: OnceLock<Result<(), String>>,
}

impl Clone for Content {
    fn clone(&self) -> Self {
        nucleus::performance::add("message-draft-content-copies", 1);
        Self {
            parts: self.parts.clone(),
            keys: self.keys.clone(),
            next_key: self.next_key,
            validation: OnceLock::new(),
        }
    }
}

impl Parts {
    fn change(&mut self) -> &mut Content {
        let content = Arc::make_mut(&mut self.0);
        content.validation = OnceLock::new();
        content
    }

    pub(super) fn rows(&self) -> impl Iterator<Item = (u64, &MessagePart)> {
        self.0.keys.iter().copied().zip(self.0.parts.iter())
    }

    pub(crate) fn push(&mut self, part: MessagePart) {
        self.extend(std::iter::once(part));
    }

    pub(crate) fn extend(&mut self, parts: impl IntoIterator<Item = MessagePart>) {
        let mut parts = parts.into_iter().peekable();
        if parts.peek().is_none() {
            return;
        }
        let content = self.change();
        for part in parts {
            content.keys.push(content.next_key);
            content.next_key += 1;
            content.parts.push(part);
        }
    }

    pub(crate) fn remove(&mut self, index: usize) {
        let content = self.change();
        content.parts.remove(index);
        content.keys.remove(index);
    }

    pub(super) fn replace(&mut self, index: usize, part: MessagePart) {
        let content = self.change();
        content.parts[index] = part;
        content.keys[index] = content.next_key;
        content.next_key += 1;
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        self.0
            .validation
            .get_or_init(|| {
                let _validation =
                    nucleus::performance::Span::new("message-draft-content-validation");
                nucleus::performance::add("message-draft-content-validations", 1);
                nucleus::message::validate(&self.0.parts)
            })
            .clone()
    }

    pub(crate) fn clear(&mut self) {
        if !self.is_empty() {
            *self = Self(Arc::new(Content {
                next_key: self.0.next_key,
                ..Content::default()
            }));
        }
    }
}

impl From<Vec<MessagePart>> for Parts {
    fn from(parts: Vec<MessagePart>) -> Self {
        let next_key = parts.len() as u64;
        Self(Arc::new(Content {
            parts,
            keys: (0..next_key).collect(),
            next_key,
            validation: OnceLock::new(),
        }))
    }
}

impl Deref for Parts {
    type Target = Vec<MessagePart>;

    fn deref(&self) -> &Self::Target {
        &self.0.parts
    }
}

impl PartialEq for Parts {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.parts == other.0.parts
    }
}

impl fmt::Debug for Parts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.parts.fmt(formatter)
    }
}

impl Serialize for Parts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.parts.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Parts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<MessagePart>::deserialize(deserializer).map(Self::from)
    }
}
