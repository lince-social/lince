//! Validated slugs, local names, typed IDs, and bindings for Karma programs.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::failure::KarmaBoundaryError;

const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
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
    TrustScope,
    Run,
    Candidate,
    Decision,
    Receipt,
    Simulation,
}

impl ReferenceKind {
    /// Returns the stable abbreviation used in the Karma DSL.
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
            Self::TrustScope => "trust",
            Self::Run => "run",
            Self::Candidate => "cand",
            Self::Decision => "dec",
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
            | Self::TrustScope
            | Self::Run
            | Self::Candidate
            | Self::Decision
            | Self::Receipt
            | Self::Simulation => "r",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema)]
pub struct TypedUid {
    kind: ReferenceKind,
    uid: String,
}

impl TypedUid {
    /// Pairs a reference kind with a prefixed Crockford ULID.
    ///
    /// # Errors
    ///
    /// Returns an invalid input error if the prefix or ULID is malformed.
    pub fn new(kind: ReferenceKind, uid: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let uid = uid.into();
        Self::validate(kind, &uid)?;
        Ok(Self { kind, uid })
    }

    pub(crate) fn validate(kind: ReferenceKind, uid: &str) -> Result<(), KarmaBoundaryError> {
        let prefix = kind.uid_prefix();
        if valid_prefixed_ulid(uid, prefix) {
            Ok(())
        } else {
            Err(KarmaBoundaryError::invalid_input(format!(
                "{} reference requires a {prefix}_ Crockford ULID",
                kind.short()
            )))
        }
    }

    /// Returns the domain kind attached to the identifier.
    pub const fn kind(&self) -> ReferenceKind {
        self.kind
    }

    /// Borrows the UID, including its prefix.
    pub fn as_str(&self) -> &str {
        &self.uid
    }
}

fn valid_prefixed_ulid(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('_'))
        .is_some_and(|encoded| {
            encoded.len() == 26
                && encoded
                    .bytes()
                    .all(|byte| CROCKFORD.binary_search(&byte).is_ok())
        })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TypedUidWire {
    kind: ReferenceKind,
    uid: String,
}

impl Serialize for TypedUid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct as _;
        let mut wire = serializer.serialize_struct("TypedUidWire", 2)?;
        wire.serialize_field("kind", &self.kind)?;
        wire.serialize_field("uid", &self.uid)?;
        wire.end()
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema)]
pub struct Slug(String);

impl Slug {
    /// Validates a name against [`valid_slug`].
    ///
    /// # Errors
    ///
    /// Returns an invalid input error if a segment is empty or contains invalid characters.
    pub fn new(value: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let value = value.into();
        if valid_slug(&value) {
            Ok(Self(value))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "slug must be dot-separated lowercase alphanumeric/hyphen segments",
            ))
        }
    }

    /// Borrows the validated name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Checks dot-separated ASCII segments that begin with a lowercase letter or digit.
pub fn valid_slug(slug: &str) -> bool {
    let alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    slug.split('.').all(|segment| {
        let mut bytes = segment.bytes();
        bytes.next().is_some_and(alphanumeric)
            && bytes.all(|byte| alphanumeric(byte) || byte == b'-')
    })
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, schemars::JsonSchema)]
pub struct LocalId(String);

impl LocalId {
    /// Validates a local name beginning with an ASCII lowercase letter.
    ///
    /// # Errors
    ///
    /// Returns an invalid input error for an empty name or invalid characters.
    pub fn new(value: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let value = value.into();
        let mut bytes = value.bytes();
        if bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
            && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            Ok(Self(value))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "local id must be lower snake_case and start with a letter",
            ))
        }
    }

    /// Borrows the validated local name.
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

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct ResolvedReference {
    pub target: TypedUid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_slug: Option<Slug>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConditionBinding {
    pub reading: String,
    pub authored: String,
    pub target: TypedUid,
}
