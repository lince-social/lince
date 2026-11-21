//! Contact disclosure terms and their public identifiers.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{super::Identity, MAX_TERM_SECONDS};
use crate::karma::{CanonicalHash, canonical_hash};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Audience {
    Public,
    Selected { viewers: Vec<Identity> },
}

impl Default for Audience {
    fn default() -> Self {
        Self::Selected {
            viewers: Vec::new(),
        }
    }
}

impl Audience {
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Selected { viewers } = self {
            let ordered = viewers.len() <= 64 && viewers.windows(2).all(|pair| pair[0] < pair[1]);
            if !ordered
                && (viewers.len() > 64
                    || viewers.iter().collect::<BTreeSet<_>>().len() != viewers.len())
            {
                return Err("Choose at most 64 distinct disclosure viewers".into());
            }
            for identity in viewers {
                identity.validate()?;
            }
            if !ordered {
                return Err("Disclosure viewers must use canonical identity order".into());
            }
        }
        Ok(())
    }

    pub fn allows(&self, viewer: &Identity, issuer: &Identity) -> bool {
        viewer == issuer
            || match self {
                Self::Public => true,
                Self::Selected { viewers } => viewers.contains(viewer),
            }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Statement {
    pub issuer: Identity,
    pub contact: Identity,
    pub revision: u64,
    pub issued_at: i64,
    pub expires_at: i64,
    pub withdrawn: bool,
    pub audience: Audience,
}

impl Statement {
    pub fn validate(&self) -> Result<(), String> {
        self.issuer.validate()?;
        self.contact.validate()?;
        self.audience.validate()?;
        if self.issuer.uid() == self.contact.uid()
            || self.revision == 0
            || self.revision > i64::MAX as u64
            || self.issued_at <= 0
            || self.expires_at <= self.issued_at
            || self.expires_at.saturating_sub(self.issued_at) > MAX_TERM_SECONDS
        {
            return Err(
                "Use distinct identities, a positive revision and a bounded disclosure term".into(),
            );
        }
        Ok(())
    }

    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.contact-disclosure.statement.v1", self)
            .map_err(|error| error.to_string())
    }

    /// Identify the unordered pair without collapsing Person and Organ identities.
    ///
    /// # Errors
    ///
    /// The disclosure is invalid or its endpoint identities cannot be encoded.
    pub fn relation(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.contact-disclosure.relation.v1", &self.endpoints())
            .map_err(|error| error.to_string())
    }

    pub(super) fn endpoints(&self) -> (&Identity, &Identity) {
        if self.issuer < self.contact {
            (&self.issuer, &self.contact)
        } else {
            (&self.contact, &self.issuer)
        }
    }
}
