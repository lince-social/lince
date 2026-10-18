use super::{Filter, Identity, IdentityContext, MAX_DEPTH, MAX_IDENTITIES};
use crate::karma::CanonicalHash;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::contacts::{Edge, RelationKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub anchor: Identity,
    pub max_depth: u8,
    pub max_identities: usize,
    pub allow_unilateral: bool,
    pub targets: Filter,
    pub intermediaries: Filter,
    pub exclude_everywhere: Vec<Identity>,
}

impl Policy {
    pub fn for_anchor(anchor: Identity) -> Self {
        Self {
            anchor,
            max_depth: 3,
            max_identities: MAX_IDENTITIES,
            allow_unilateral: false,
            targets: Filter::default(),
            intermediaries: Filter::default(),
            exclude_everywhere: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.anchor.validate()?;
        self.targets.validate()?;
        self.intermediaries.validate()?;
        if self.max_depth == 0
            || self.max_depth > MAX_DEPTH
            || self.max_identities == 0
            || self.max_identities > MAX_IDENTITIES
            || self.exclude_everywhere.len() > MAX_IDENTITIES
        {
            return Err("Choose depth 1–6 and an identity budget of 1–1024".into());
        }
        for identity in &self.exclude_everywhere {
            identity.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Path {
    pub identities: Vec<Identity>,
    pub proofs: Vec<Vec<CanonicalHash>>,
    pub unilateral: bool,
}

impl Path {
    pub fn distance(&self) -> usize {
        self.identities.len().saturating_sub(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultSet {
    pub paths: Vec<Path>,
    pub observed_only: bool,
    pub identity_limit_reached: bool,
    pub depth_limit_reached: bool,
}

pub fn search(
    edges: &[Edge],
    identities: &BTreeMap<Identity, IdentityContext>,
    policy: &Policy,
) -> Result<ResultSet, String> {
    policy.validate()?;
    if edges.len() > super::MAX_DISCLOSURES * 2 {
        return Err("Contact traversal exceeds its bounded edge budget".into());
    }
    let excluded = policy.exclude_everywhere.iter().collect::<BTreeSet<_>>();
    let permitted = |identity: &Identity| {
        !excluded.contains(identity)
            && identities
                .get(identity)
                .is_some_and(|context| context.readable && !context.blocked)
    };
    let mut result = ResultSet {
        paths: Vec::new(),
        observed_only: true,
        identity_limit_reached: false,
        depth_limit_reached: false,
    };
    if !permitted(&policy.anchor) {
        return Ok(result);
    }
    let mut adjacency: BTreeMap<&Identity, BTreeMap<&Identity, &Edge>> = BTreeMap::new();
    for edge in edges {
        if permitted(&edge.from)
            && permitted(&edge.to)
            && (edge.kind == RelationKind::Mutual || policy.allow_unilateral)
        {
            adjacency
                .entry(&edge.from)
                .or_default()
                .entry(&edge.to)
                .and_modify(|current| {
                    if (edge.kind == RelationKind::Mutual
                        && current.kind == RelationKind::Unilateral)
                        || (edge.kind == current.kind && edge.proofs < current.proofs)
                    {
                        *current = edge;
                    }
                })
                .or_insert(edge);
        }
    }
    let mut visited = BTreeSet::from([policy.anchor.clone()]);
    let mut frontier = vec![Path {
        identities: vec![policy.anchor.clone()],
        proofs: Vec::new(),
        unilateral: false,
    }];
    for depth in 0..=policy.max_depth {
        let mut next = Vec::new();
        for path in frontier {
            let last = path.identities.last().unwrap();
            if depth > 0 && !policy.intermediaries.allows(last, &identities[last]) {
                continue;
            }
            let Some(neighbors) = adjacency.get(last) else {
                continue;
            };
            for (identity, edge) in neighbors {
                if visited.contains(*identity) {
                    continue;
                }
                if depth == policy.max_depth {
                    result.depth_limit_reached = true;
                    continue;
                }
                if visited.len() >= policy.max_identities {
                    result.identity_limit_reached = true;
                    return Ok(result);
                }
                visited.insert((*identity).clone());
                let mut candidate = path.clone();
                candidate.identities.push((*identity).clone());
                candidate.proofs.push(edge.proofs.clone());
                candidate.unilateral |= edge.kind == RelationKind::Unilateral;
                if policy.targets.allows(identity, &identities[*identity]) {
                    result.paths.push(candidate.clone());
                }
                next.push(candidate);
            }
        }
        next.sort_by(|left, right| left.identities.cmp(&right.identities));
        frontier = next;
    }
    Ok(result)
}
