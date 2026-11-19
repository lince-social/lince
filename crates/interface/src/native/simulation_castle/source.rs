use nucleus::karma::CanonicalHash;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use simulation::scenario::Scenario;
use std::{
    fmt,
    ops::Deref,
    sync::{Arc, OnceLock},
};

#[derive(Clone)]
pub struct Source(Arc<Snapshot>);

struct Snapshot {
    text: String,
    parsed: OnceLock<Result<Arc<Scenario>, String>>,
    base: OnceLock<Result<Option<CanonicalHash>, String>>,
}

impl From<String> for Source {
    fn from(text: String) -> Self {
        Self(Arc::new(Snapshot {
            text,
            parsed: OnceLock::new(),
            base: OnceLock::new(),
        }))
    }
}

impl Deref for Source {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0.text
    }
}

impl AsRef<str> for Source {
    fn as_ref(&self) -> &str {
        self
    }
}

impl fmt::Debug for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0.text, formatter)
    }
}

impl PartialEq for Source {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.text == other.0.text
    }
}

impl Eq for Source {}

impl Serialize for Source {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self)
    }
}

impl<'de> Deserialize<'de> for Source {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from)
    }
}

impl Source {
    pub(super) fn set(&mut self, text: String) {
        if self.0.text != text {
            *self = text.into();
        }
    }

    pub(crate) fn parsed(&self) -> Result<Arc<Scenario>, String> {
        self.0
            .parsed
            .get_or_init(|| {
                let _prepare = nucleus::performance::Span::new("simulation-scenario-parse");
                nucleus::performance::add("simulation-scenario-parses", 1);
                serde_json::from_str(&self.0.text)
                    .map(Arc::new)
                    .map_err(|error| error.to_string())
            })
            .clone()
    }

    pub(super) fn draft(&self) -> Result<Scenario, String> {
        self.parsed().map(|scenario| scenario.as_ref().clone())
    }

    pub(super) fn base_stamp(&self) -> Result<Option<CanonicalHash>, String> {
        self.0
            .base
            .get_or_init(|| {
                let scenario = self.parsed()?;
                let _prepare = nucleus::performance::Span::new("simulation-base-stamp");
                scenario
                    .proposal
                    .as_ref()
                    .map(|proposal| {
                        nucleus::performance::add("simulation-base-stamps-prepared", 1);
                        simulation::world::digest(&(&proposal.source, &proposal.records))
                            .map_err(|error| error.to_string())
                    })
                    .transpose()
            })
            .clone()
    }
}
