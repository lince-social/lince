//! Bounded contact traversal with deterministic routes and retained proofs.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    Filter, Identity, IdentityContext, MAX_DEPTH, MAX_DISCLOSURES, MAX_IDENTITIES,
    contacts::{Edge, RelationKind},
};
use crate::karma::CanonicalHash;

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

/// Find the first shortest observed route to each eligible identity.
///
/// # Errors
///
/// The policy is invalid or the observed edges exceed the traversal budget.
pub fn search(
    edges: &[Edge],
    identities: &BTreeMap<Identity, IdentityContext>,
    policy: &Policy,
) -> Result<ResultSet, String> {
    policy.validate()?;
    if edges.len() > MAX_DISCLOSURES * 2 {
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
    let mut visited = BTreeSet::from([&policy.anchor]);
    let mut trails = vec![Step {
        identity: &policy.anchor,
        incoming: None,
        unilateral: false,
    }];
    let mut frontier = 0..1;
    for depth in 0..=policy.max_depth {
        let next = trails.len();
        // The frontier holds the appended range from the preceding level.
        // Ordered parent routes and ordered neighbors produce ordered children.
        for parent in frontier {
            let step = trails[parent];
            let last = step.identity;
            if depth > 0
                && !identities
                    .get(last)
                    .is_some_and(|context| policy.intermediaries.allows(last, context))
            {
                continue;
            }
            let Some(neighbors) = adjacency.get(last) else {
                continue;
            };
            for (&identity, &edge) in neighbors {
                if visited.contains(identity) {
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
                visited.insert(identity);
                let candidate = Step {
                    identity,
                    incoming: Some((parent, edge)),
                    unilateral: step.unilateral || edge.kind == RelationKind::Unilateral,
                };
                if identities
                    .get(identity)
                    .is_some_and(|context| policy.targets.allows(identity, context))
                {
                    result
                        .paths
                        .push(candidate.path(&trails, usize::from(depth) + 1));
                }
                trails.push(candidate);
            }
        }
        frontier = next..trails.len();
        if frontier.is_empty() {
            break;
        }
    }
    Ok(result)
}

#[derive(Clone, Copy)]
struct Step<'a> {
    identity: &'a Identity,
    incoming: Option<(usize, &'a Edge)>,
    unilateral: bool,
}

impl Step<'_> {
    fn path(mut self, trail: &[Self], distance: usize) -> Path {
        let mut path = Path {
            identities: Vec::with_capacity(distance + 1),
            proofs: Vec::with_capacity(distance),
            unilateral: self.unilateral,
        };
        path.identities.push(self.identity.clone());
        // Every parent index was assigned before its child was appended.
        while let Some((parent, edge)) = self.incoming {
            self = trail[parent];
            path.identities.push(self.identity.clone());
            path.proofs.push(edge.proofs.clone());
        }
        path.identities.reverse();
        path.proofs.reverse();
        path
    }
}
