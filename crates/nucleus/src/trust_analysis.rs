// non-lean-yet
// Scope: mod contacts, mod paths, mod commands, mod query.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 31.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 50, 109.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 1, 2,
//   3, 4, 5, 6, 7, 8, 9, 10, 17, 18, 19, 20, 31, 32, 33, 37, 44, 50, 60, 61, 62, 63, 64, 103, 104,
//   105, 109, 119, 133, 134, 135, 136, 137.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites validate:
//   String (50, 109).

pub mod claims;
pub mod commands;
pub mod contacts;
pub mod custody;
pub mod discovery;
pub mod evidence;
pub mod paths;
pub mod query;
pub mod remote;
pub mod serving;

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::visibility::Bound;

pub const MAX_IDENTITIES: usize = 1024;
pub const MAX_DEPTH: u8 = 6;
pub const MAX_DISCLOSURES: usize = 5000;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(
    tag = "kind",
    content = "uid",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Identity {
    Person(String),
    Organ(String),
}

impl Identity {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Person(_) => "person",
            Self::Organ(_) => "organ",
        }
    }

    pub fn uid(&self) -> &str {
        match self {
            Self::Person(uid) | Self::Organ(uid) => uid,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !crate::valid_uid(self.uid(), "r") {
            return Err("Choose a canonical Person or Organ Record identity".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Condition {
    pub identities: Vec<Identity>,
    pub known: Option<bool>,
    pub lower: Option<Bound>,
    pub upper: Option<Bound>,
}

impl Condition {
    fn validate(&self) -> Result<(), String> {
        if self.identities.len() > 64
            || self.identities.iter().collect::<BTreeSet<_>>().len() != self.identities.len()
        {
            return Err("Choose at most 64 distinct identities per condition".into());
        }
        for identity in &self.identities {
            identity.validate()?;
        }
        crate::visibility::Policy {
            include: vec![crate::visibility::Condition {
                lower: self.lower,
                upper: self.upper,
                ..Default::default()
            }],
            exclude: Vec::new(),
        }
        .validate()
        .map_err(str::to_owned)
    }

    fn matches(&self, identity: &Identity, context: &IdentityContext) -> bool {
        (self.identities.is_empty() || self.identities.contains(identity))
            && self.known.is_none_or(|known| known == context.known)
            && crate::visibility::Condition {
                lower: self.lower,
                upper: self.upper,
                ..Default::default()
            }
            .matches(identity.uid(), context.proximity)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Filter {
    pub include: Vec<Condition>,
    pub exclude: Vec<Condition>,
}

impl Filter {
    pub fn validate(&self) -> Result<(), String> {
        if self.include.len() + self.exclude.len() > 64 {
            return Err("Choose at most 64 include/exclude conditions".into());
        }
        for condition in self.include.iter().chain(&self.exclude) {
            condition.validate()?;
        }
        Ok(())
    }

    pub fn allows(&self, identity: &Identity, context: &IdentityContext) -> bool {
        (self.include.is_empty()
            || self
                .include
                .iter()
                .any(|condition| condition.matches(identity, context)))
            && !self
                .exclude
                .iter()
                .any(|condition| condition.matches(identity, context))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdentityContext {
    pub readable: bool,
    pub blocked: bool,
    pub known: bool,
    pub proximity: Option<u32>,
}

#[cfg(test)]
mod tests;
