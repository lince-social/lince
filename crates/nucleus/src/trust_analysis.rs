pub mod contacts;
pub mod paths;
pub mod commands;
pub mod query;
pub mod claims;
pub mod serving;
pub mod custody;
pub mod discovery;
pub mod remote;
pub mod evidence;

use crate::visibility::Bound;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
