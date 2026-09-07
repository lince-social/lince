use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use super::failure::KarmaBoundaryError;

const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceKind {
    Record,
    Fact,
    Person,
    Organ,
    Place,
    Concept,
    Unit,
    Link,
    Promise,
    Transfer,
    Program,
    ProgramRevision,
    Node,
    Signal,
    Frequency,
    Sense,
    View,
    Model,
    Objective,
    Workflow,
    Grant,
    TrustScope,
    Run,
    Candidate,
    Decision,
    Intent,
    Receipt,
    Simulation,
}

impl ReferenceKind {
    pub const fn short(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Fact => "fact",
            Self::Person => "person",
            Self::Organ => "organ",
            Self::Place => "place",
            Self::Concept => "concept",
            Self::Unit => "unit",
            Self::Link => "link",
            Self::Promise => "promise",
            Self::Transfer => "transfer",
            Self::Program => "prog",
            Self::ProgramRevision => "rev",
            Self::Node => "node",
            Self::Signal => "sig",
            Self::Frequency => "freq",
            Self::Sense => "sense",
            Self::View => "view",
            Self::Model => "model",
            Self::Objective => "obj",
            Self::Workflow => "flow",
            Self::Grant => "grant",
            Self::TrustScope => "trust",
            Self::Run => "run",
            Self::Candidate => "cand",
            Self::Decision => "dec",
            Self::Intent => "intent",
            Self::Receipt => "receipt",
            Self::Simulation => "sim",
        }
    }

    const fn uid_prefix(self) -> &'static str {
        match self {
            Self::Fact => "f",
            Self::Promise => "p",
            Self::Place => "pl",
            Self::Concept | Self::Unit => "c",
            Self::Link => "l",
            Self::Record
            | Self::Person
            | Self::Organ
            | Self::Transfer
            | Self::Program
            | Self::ProgramRevision
            | Self::Node
            | Self::Signal
            | Self::Frequency
            | Self::Sense
            | Self::View
            | Self::Model
            | Self::Objective
            | Self::Workflow
            | Self::Grant
            | Self::TrustScope
            | Self::Run
            | Self::Candidate
            | Self::Decision
            | Self::Intent
            | Self::Receipt
            | Self::Simulation => "r",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypedUid {
    kind: ReferenceKind,
    uid: String,
}

impl TypedUid {
    pub fn new(kind: ReferenceKind, uid: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let uid = uid.into();
        if valid_prefixed_ulid(&uid, kind.uid_prefix()) {
            Ok(Self { kind, uid })
        } else {
            Err(KarmaBoundaryError::invalid_input(format!(
                "{} reference requires a {}_ Crockford ULID",
                kind.short(),
                kind.uid_prefix()
            )))
        }
    }

    pub const fn kind(&self) -> ReferenceKind {
        self.kind
    }

    pub fn as_str(&self) -> &str {
        &self.uid
    }
}

fn valid_prefixed_ulid(value: &str, prefix: &str) -> bool {
    let Some(encoded) = value
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('_'))
    else {
        return false;
    };
    encoded.len() == 26
        && encoded
            .bytes()
            .all(|byte| CROCKFORD.binary_search(&byte).is_ok())
}

#[derive(Serialize, Deserialize)]
struct TypedUidWire {
    kind: ReferenceKind,
    uid: String,
}

impl Serialize for TypedUid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TypedUidWire {
            kind: self.kind,
            uid: self.uid.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TypedUid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TypedUidWire::deserialize(deserializer)?;
        Self::new(wire.kind, wire.uid).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slug(String);

impl Slug {
    pub fn new(value: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let value = value.into();
        if crate::valid_slug(&value) {
            Ok(Self(value))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "slug must be dot-separated lowercase alphanumeric/hyphen segments",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Slug {
    type Err = KarmaBoundaryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl Serialize for Slug {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Slug {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(String);

impl LocalId {
    pub fn new(value: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let value = value.into();
        let mut chars = value.chars();
        let starts_valid = chars.next().is_some_and(|ch| ch.is_ascii_lowercase());
        if starts_valid
            && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        {
            Ok(Self(value))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "local id must be lower snake_case and start with a letter",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for LocalId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for LocalId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResolvedReference {
    pub target: TypedUid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_slug: Option<Slug>,
}
