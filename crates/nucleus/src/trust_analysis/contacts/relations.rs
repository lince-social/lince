//! Resolve current observed contact edges and their proof identifiers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    super::{Identity, IdentityContext, MAX_BYTES, MAX_DISCLOSURES},
    Document, MAX_DOCUMENT_BYTES, Statement,
};
use crate::{json::encoded_len, karma::CanonicalHash};

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

struct Revision<'a> {
    statement: &'a Statement,
    usable: bool,
    conflicted: bool,
}

/// Resolve observed edges without using older revisions as a fallback.
///
/// # Errors
///
/// A disclosure or viewer is invalid, or the input exceeds its count or byte budget.
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
    let mut pairs = BTreeMap::<_, [Option<Revision<'_>>; 2]>::new();
    for checked in disclosures {
        let document = &checked.document;
        let size = encoded_len(document).map_err(|error| error.to_string())?;
        if size > MAX_DOCUMENT_BYTES {
            return Err("Use a bounded authenticated contact-disclosure document".into());
        }
        bytes = bytes.saturating_add(size);
        if bytes > MAX_BYTES {
            return Err("Contact-proof processing exceeds 8 MiB".into());
        }
        document.validate()?;
        let statement = &document.statement;
        if statement.issued_at > now {
            continue;
        }
        let pair = statement.endpoints();
        let [forward, reverse] = pairs.entry(pair).or_default();
        let direction = if &statement.issuer == pair.0 {
            forward
        } else {
            reverse
        };
        let usable = checked.readable && checked.authority_current;
        match direction {
            Some(current) if current.statement.revision > statement.revision => {}
            Some(current) if current.statement.revision == statement.revision => {
                current.conflicted |= current.statement != statement;
                current.usable |= usable;
            }
            _ => {
                *direction = Some(Revision {
                    statement,
                    usable,
                    conflicted: false,
                })
            }
        }
    }
    let visible = |identity: &Identity| {
        identities
            .get(identity)
            .is_some_and(|context| context.readable && !context.blocked)
    };
    let mut edges = Vec::new();
    for directions in pairs.into_values() {
        if directions
            .iter()
            .flatten()
            .any(|current| current.conflicted)
        {
            continue;
        }
        let mut active = directions.into_iter().flatten().filter_map(|revision| {
            let statement = revision.statement;
            (revision.usable
                && statement.audience.allows(viewer, &statement.issuer)
                && visible(&statement.issuer)
                && visible(&statement.contact)
                && !statement.withdrawn
                && statement.expires_at > now)
                .then_some(statement)
        });
        let Some(first) = active.next() else { continue };
        let mut proofs = vec![first.id()?];
        let mut kind = RelationKind::Unilateral;
        if let Some(second) = active.next() {
            proofs.push(second.id()?);
            proofs.sort_unstable();
            kind = RelationKind::Mutual;
            edges.push(edge(second, kind, proofs.clone()));
        }
        edges.push(edge(first, kind, proofs));
    }
    edges.sort_unstable_by(|left, right| (&left.from, &left.to).cmp(&(&right.from, &right.to)));
    Ok(edges)
}

fn edge(statement: &Statement, kind: RelationKind, proofs: Vec<CanonicalHash>) -> Edge {
    Edge {
        from: statement.issuer.clone(),
        to: statement.contact.clone(),
        kind,
        proofs,
    }
}
