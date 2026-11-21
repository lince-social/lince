//! Composes Role read policy with a Person's saved read filter.

use crate::{Predicate, ProteinError, role_authority::policy_for};
use nucleus::valid_uid;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use store::{Store, auth::person_access, sqlx::Error::Protocol};

/// Concept conditions that admit or exclude Records.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadRules {
    /// Conditions that must match to admit a Record.
    pub allow: Predicate,
    /// Conditions that exclude a matching Record.
    pub block: Predicate,
}

impl Default for ReadRules {
    fn default() -> Self {
        Self {
            allow: Predicate::All(vec![]),
            block: Predicate::Any(vec![]),
        }
    }
}

/// Negates a condition, distributing negation over groups.
pub fn negate(predicate: &Predicate) -> Predicate {
    match predicate {
        Predicate::All(children) => Predicate::Any(children.iter().map(negate).collect()),
        Predicate::Any(children) => Predicate::All(children.iter().map(negate).collect()),
        Predicate::Not(child) => child.as_ref().clone(),
        other => Predicate::Not(Box::new(other.clone())),
    }
}

impl ReadRules {
    /// Builds the condition a Record must satisfy to be readable.
    pub fn predicate(&self) -> Predicate {
        Predicate::All(vec![self.allow.clone(), negate(&self.block)])
    }

    /// Restores the two-part filter produced by [Self::predicate].
    ///
    /// Other filter forms become an allow condition with an empty block.
    pub fn from_predicate(predicate: Predicate) -> Self {
        let allow = match predicate {
            Predicate::All(children) => match <[Predicate; 2]>::try_from(children) {
                Ok([allow, blocked]) => {
                    return Self {
                        allow,
                        block: negate(&blocked),
                    };
                }
                Err(children) => Predicate::All(children),
            },
            allow => allow,
        };
        Self {
            allow,
            ..Self::default()
        }
    }

    /// Checks the supported Concept conditions and tree size.
    ///
    /// # Errors
    /// Returns an error for invalid conditions, depth, or combined node count.
    pub fn validate(&self) -> Result<(), ProteinError> {
        const MAX_NODES: usize = 128;
        const MAX_DEPTH: usize = 8;

        fn visit(predicate: &Predicate, depth: usize, nodes: &mut usize) -> bool {
            *nodes += 1;
            if depth > MAX_DEPTH || *nodes > MAX_NODES {
                return false;
            }
            match predicate {
                Predicate::All(children) | Predicate::Any(children) => {
                    children.iter().all(|child| visit(child, depth + 1, nodes))
                }
                Predicate::Not(child) => {
                    matches!(child.as_ref(), Predicate::ConceptIn(_))
                        && visit(child, depth + 1, nodes)
                }
                Predicate::ConceptIn(uid) => valid_uid(uid, "c"),
                _ => false,
            }
        }
        let mut nodes = 0;
        if visit(&self.allow, 0, &mut nodes) && visit(&self.block, 0, &mut nodes) {
            Ok(())
        } else {
            Err(Protocol(
                "Use at most 128 tag conditions and eight group levels.".into(),
            ))
        }
    }
}

/// Combines the Person's Role read conditions and personal read filter.
///
/// # Errors
/// Returns an error if access cannot be read or a saved filter is invalid.
pub async fn effective_predicate(
    store: &Store,
    person: &str,
) -> Result<Option<Predicate>, ProteinError> {
    let Some(access) = person_access(&store.pool, person).await? else {
        return Ok(None);
    };
    let mut filters = Vec::new();
    if let Some(policy) = policy_for(store, person, None).await? {
        filters.push(policy.read);
    }
    if let Some(raw) = access.read_filter {
        filters.push(from_str(&raw).map_err(|_| Protocol("Invalid personal read filter.".into()))?);
    }
    Ok((!filters.is_empty()).then_some(Predicate::All(filters)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_rules_bound_size_depth_and_condition_types() {
        let tag = Predicate::ConceptIn(nucleus::new_uid("c"));
        let mut rules = ReadRules {
            allow: Predicate::All(vec![tag.clone(); 128]),
            block: Predicate::Any(vec![]),
        };
        assert!(rules.validate().is_err());
        let mut deep = tag.clone();
        for _ in 0..10 {
            deep = Predicate::All(vec![deep]);
        }
        rules.allow = deep;
        assert!(rules.validate().is_err());
        rules.allow = Predicate::UidEq(nucleus::new_uid("r"));
        assert!(rules.validate().is_err());
        rules.allow = Predicate::All(vec![Predicate::Any(vec![
            tag.clone(),
            Predicate::Not(Box::new(tag)),
        ])]);
        rules.validate().unwrap();
        let predicate = rules.predicate();
        let restored = ReadRules::from_predicate(predicate.clone()).predicate();
        assert_eq!(
            serde_json::to_value(predicate).unwrap(),
            serde_json::to_value(restored).unwrap()
        );
    }
}
