use super::{Identity, IdentityContext, MAX_BYTES, MAX_DISCLOSURES};
use crate::karma::{CanonicalHash, canonical_hash, canonical_json_bytes};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const NAMESPACE: &str = "lince.contact-disclosure";
pub const MAX_DOCUMENT_BYTES: usize = 32768;
pub const MAX_TERM_SECONDS: i64 = 366 * 24 * 60 * 60;

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
            if viewers.len() > 64 || viewers.iter().collect::<BTreeSet<_>>().len() != viewers.len()
            {
                return Err("Choose at most 64 distinct disclosure viewers".into());
            }
            for identity in viewers {
                identity.validate()?;
            }
            if !viewers.windows(2).all(|pair| pair[0] < pair[1]) {
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

    pub fn relation(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        let (left, right) = if self.issuer < self.contact {
            (&self.issuer, &self.contact)
        } else {
            (&self.contact, &self.issuer)
        };
        canonical_hash("lince.contact-disclosure.relation.v1", &(left, right))
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub statement: Statement,
    pub key_id: String,
    pub public_key: String,
    pub signature: String,
}

impl Document {
    pub fn id(&self) -> Result<CanonicalHash, String> {
        self.validate()?;
        canonical_hash("lince.contact-disclosure.document.v1", self)
            .map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.signing_bytes()?;
        if self.signature.len() != 88 {
            return Err("Use a canonical Ed25519 signature".into());
        }
        let signature = STANDARD
            .decode(&self.signature)
            .map_err(|_| "Use a canonical Ed25519 signature".to_owned())?;
        if signature.len() != 64 || STANDARD.encode(signature) != self.signature {
            return Err("Use a canonical Ed25519 signature".into());
        }
        Ok(())
    }

    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.statement.validate()?;
        if self.key_id.is_empty()
            || self.key_id.len() > 160
            || !self.key_id.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err("Choose a bounded signing-key identifier".into());
        }
        if self.public_key.len() != 44 {
            return Err("Use a canonical Ed25519 public key".into());
        }
        let key = STANDARD
            .decode(&self.public_key)
            .map_err(|_| "Use a canonical Ed25519 public key".to_owned())?;
        if key.len() != 32 || STANDARD.encode(key) != self.public_key {
            return Err("Use a canonical Ed25519 public key".into());
        }
        canonical_json_bytes(&(
            "lince.contact-disclosure.signature.v1",
            &self.statement,
            &self.key_id,
            &self.public_key,
        ))
        .map_err(|error| error.to_string())
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err("Contact disclosure exceeds 32 KiB".into());
        }
        let document: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        document.validate()?;
        Ok(document)
    }
}

#[derive(Debug, Clone)]
pub struct CheckedDisclosure {
    pub document: Document,
    pub readable: bool,
    pub authority_current: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Mutual,
    Unilateral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: Identity,
    pub to: Identity,
    pub kind: RelationKind,
    pub proofs: Vec<CanonicalHash>,
}

pub fn permitted_edges(
    disclosures: &[CheckedDisclosure],
    identities: &BTreeMap<Identity, IdentityContext>,
    viewer: &Identity,
    now: i64,
) -> Result<Vec<Edge>, String> {
    viewer.validate()?;
    if disclosures.len() > MAX_DISCLOSURES {
        return Err("Contact-proof processing exceeds 5000 disclosures".into());
    }
    let mut bytes = 0usize;
    let mut grouped: BTreeMap<CanonicalHash, BTreeMap<Identity, Vec<&CheckedDisclosure>>> =
        BTreeMap::new();
    for checked in disclosures {
        let document = &checked.document;
        let size = serde_json::to_vec(document)
            .map_err(|error| error.to_string())?
            .len();
        if size > MAX_DOCUMENT_BYTES {
            return Err("Use a bounded authenticated contact-disclosure document".into());
        }
        bytes = bytes.saturating_add(size);
        if bytes > MAX_BYTES {
            return Err("Contact-proof processing exceeds 8 MiB".into());
        }
        document.validate()?;
        if document.statement.issued_at > now {
            continue;
        }
        grouped
            .entry(document.statement.relation()?)
            .or_default()
            .entry(document.statement.issuer.clone())
            .or_default()
            .push(checked);
    }
    let visible = |identity: &Identity| {
        identities
            .get(identity)
            .is_some_and(|context| context.readable && !context.blocked)
    };
    let mut edges = Vec::new();
    for by_issuer in grouped.values() {
        let mut active = Vec::new();
        let mut conflict = false;
        for versions in by_issuer.values() {
            let revision = versions
                .iter()
                .map(|checked| checked.document.statement.revision)
                .max()
                .unwrap_or(0);
            let current = versions
                .iter()
                .filter(|checked| checked.document.statement.revision == revision)
                .collect::<Vec<_>>();
            let ids = current
                .iter()
                .map(|checked| checked.document.statement.id())
                .collect::<Result<BTreeSet<_>, _>>()?;
            if ids.len() != 1 {
                conflict = true;
                break;
            }
            if let Some(checked) = current.iter().find(|checked| {
                let statement = &checked.document.statement;
                checked.readable
                    && checked.authority_current
                    && statement.audience.allows(viewer, &statement.issuer)
                    && visible(&statement.issuer)
                    && visible(&statement.contact)
                    && !statement.withdrawn
                    && statement.expires_at > now
            }) {
                active.push((*checked, ids.first().unwrap().clone()));
            }
        }
        if conflict {
            continue;
        }
        let kind = if active.len() == 2 {
            RelationKind::Mutual
        } else {
            RelationKind::Unilateral
        };
        let mut proofs = active.iter().map(|(_, id)| id.clone()).collect::<Vec<_>>();
        proofs.sort();
        for (checked, _) in active {
            edges.push(Edge {
                from: checked.document.statement.issuer.clone(),
                to: checked.document.statement.contact.clone(),
                kind,
                proofs: proofs.clone(),
            });
        }
    }
    edges.sort_by(|left, right| (&left.from, &left.to).cmp(&(&right.from, &right.to)));
    Ok(edges)
}
