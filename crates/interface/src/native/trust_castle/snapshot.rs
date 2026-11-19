use serde::{Serialize, Serializer, de::DeserializeOwned};
use serde_json::Value;
use std::{
    borrow::Cow,
    ops::Deref,
    sync::{Arc, OnceLock},
};

pub(super) const PAGE_CHARACTERS: usize = 4096;

#[derive(Clone, Copy)]
#[repr(usize)]
pub(super) enum JsonPart {
    Whole,
    Document,
    Artifact,
    ReviewedArtifact,
}

impl JsonPart {
    fn value(self, root: &Value) -> &Value {
        match self {
            Self::Whole => root,
            Self::Document => &root["document"],
            Self::Artifact => &root["artifact"],
            Self::ReviewedArtifact => &root["review"]["artifact"],
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Snapshot(Option<Arc<Payload>>);

struct Payload {
    value: Value,
    text: [OnceLock<JsonText>; 4],
}

pub(super) struct JsonText {
    text: String,
    characters: usize,
    pages: Vec<usize>,
}

impl JsonText {
    pub(super) fn as_str(&self) -> &str {
        &self.text
    }

    pub(super) fn characters(&self) -> usize {
        self.characters
    }

    pub(super) fn page(&self, page: usize) -> &str {
        let Some(&start) = self.pages.get(page) else {
            return "";
        };
        let end = self.pages.get(page + 1).copied().unwrap_or(self.text.len());
        &self.text[start..end]
    }
}

impl Snapshot {
    pub(super) fn capture(value: &Value) -> Self {
        if value.is_null() {
            return Self::default();
        }
        Self::from(Cow::Borrowed(value))
    }

    pub(super) fn text(&self, part: JsonPart) -> Option<&JsonText> {
        let payload = self.0.as_ref()?;
        Some(payload.text[part as usize].get_or_init(|| {
            let _prepare = nucleus::performance::Span::new("trust-json-presentation");
            nucleus::performance::add("trust-json-presentations-prepared", 1);
            let text = serde_json::to_string_pretty(part.value(&payload.value)).unwrap_or_default();
            let mut pages = Vec::new();
            let mut characters = 0;
            for (index, (byte, _)) in text.char_indices().enumerate() {
                if index % PAGE_CHARACTERS == 0 {
                    pages.push(byte);
                }
                characters += 1;
            }
            JsonText {
                text,
                characters,
                pages,
            }
        }))
    }
}

impl From<Cow<'_, Value>> for Snapshot {
    fn from(value: Cow<'_, Value>) -> Self {
        let _prepare = nucleus::performance::Span::new("trust-payload-preparation");
        Self::from(value.into_owned())
    }
}

impl From<Value> for Snapshot {
    fn from(value: Value) -> Self {
        if value.is_null() {
            return Self::default();
        }
        nucleus::performance::add("trust-payload-snapshots-prepared", 1);
        Self(Some(Arc::new(Payload {
            value,
            text: std::array::from_fn(|_| OnceLock::new()),
        })))
    }
}

impl Deref for Snapshot {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .map_or(&Value::Null, |payload| &payload.value)
    }
}

impl Serialize for Snapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.deref().serialize(serializer)
    }
}

pub(super) fn decode<T: DeserializeOwned>(value: &Value) -> serde_json::Result<T> {
    serde::Deserialize::deserialize(value)
}
