//! Determine which observed statement controls each disclosed claim series.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::super::{Identity, MAX_BYTES, MAX_DISCLOSURES, claims::Document};
use crate::{json::encoded_len, transfer::receipt_history::Provenance};

/// A claim observation and the authority verdict supplied by its caller.
#[derive(Debug)]
pub struct Claim<'a> {
    pub document: &'a Document,
    pub provenance: Option<Provenance>,
    pub authority_current: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadState {
    CurrentObserved,
    Withdrawn,
    Expired,
    Conflicted,
    Withheld,
}

#[derive(Debug, Serialize)]
pub struct ClaimHead {
    pub issuer: Identity,
    pub claim: String,
    pub revision: Option<u64>,
    pub state: HeadState,
    pub originals: Vec<Provenance>,
}

/// Resolve the latest known revision for each claim with observed provenance.
///
/// # Errors
///
/// An input statement fails validation or encoding, or the batch exceeds its budget.
pub fn claim_heads(inputs: &[Claim<'_>], now: i64) -> Result<Vec<ClaimHead>, String> {
    if inputs.len() > MAX_DISCLOSURES {
        return Err("Claim controls exceed their work budget".into());
    }
    let mut bytes = 0usize;
    let mut series: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for input in inputs {
        input.document.validate()?;
        bytes =
            bytes.saturating_add(encoded_len(input.document).map_err(|error| error.to_string())?);
        if bytes > MAX_BYTES {
            return Err("Claim controls exceed their byte budget".into());
        }
        let statement = &input.document.statement;
        if statement.issued_at <= now {
            series
                .entry((&statement.issuer, &statement.claim))
                .or_default()
                .push(input);
        }
    }
    let mut result = Vec::new();
    for ((issuer, claim), versions) in series {
        if !versions.iter().any(|input| input.provenance.is_some()) {
            continue;
        }
        let Some(revision) = versions
            .iter()
            .map(|input| input.document.statement.revision)
            .max()
        else {
            continue;
        };
        let mut current = versions
            .into_iter()
            .filter(|input| input.document.statement.revision == revision)
            .peekable();
        let Some(first) = current.peek() else {
            continue;
        };
        let statement = &first.document.statement;
        let mut first_id = None;
        let mut conflicted = false;
        let mut authorized = false;
        let mut originals = BTreeSet::new();
        for input in current {
            let id = input.document.statement.id()?;
            match &first_id {
                Some(first) => conflicted |= *first != id,
                None => first_id = Some(id),
            }
            if let Some(provenance) = &input.provenance {
                originals.insert(provenance);
                authorized |= input.authority_current;
            }
        }
        let state = if !authorized {
            HeadState::Withheld
        } else if conflicted {
            HeadState::Conflicted
        } else if statement.withdrawn {
            HeadState::Withdrawn
        } else if statement.expires_at <= now {
            HeadState::Expired
        } else {
            HeadState::CurrentObserved
        };
        result.push(ClaimHead {
            issuer: issuer.clone(),
            claim: claim.clone(),
            revision: (!originals.is_empty()).then_some(revision),
            state,
            originals: originals.into_iter().cloned().collect(),
        });
    }
    Ok(result)
}
