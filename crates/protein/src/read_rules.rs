use serde::{Deserialize, Serialize};

use crate::{Predicate, ProteinError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadRules {
    pub allow: Predicate,
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

pub fn negate(predicate: &Predicate) -> Predicate {
    match predicate {
        Predicate::All(children) => Predicate::Any(children.iter().map(negate).collect()),
        Predicate::Any(children) => Predicate::All(children.iter().map(negate).collect()),
        Predicate::Not(child) => *child.clone(),
        other => Predicate::Not(Box::new(other.clone())),
    }
}

impl ReadRules {
    pub fn predicate(&self) -> Predicate {
        Predicate::All(vec![self.allow.clone(), negate(&self.block)])
    }

    pub fn from_predicate(predicate: Predicate) -> Self {
        if let Predicate::All(children) = &predicate {
            if let [allow, blocked] = children.as_slice() {
                return Self {
                    allow: allow.clone(),
                    block: negate(blocked),
                };
            }
        }
        Self {
            allow: predicate,
            block: Predicate::Any(vec![]),
        }
    }

    pub fn validate(&self) -> Result<(), ProteinError> {
        fn visit(predicate: &Predicate, depth: usize, nodes: &mut usize) -> bool {
            *nodes += 1;
            if depth > 8 || *nodes > 128 {
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
                Predicate::ConceptIn(uid) => nucleus::valid_uid(uid, "c"),
                _ => false,
            }
        }
        let mut nodes = 0;
        if visit(&self.allow, 0, &mut nodes) && visit(&self.block, 0, &mut nodes) {
            Ok(())
        } else {
            Err(store::sqlx::Error::Protocol(
                "Use at most 128 tag conditions and eight group levels.".into(),
            ))
        }
    }
}

pub async fn effective_predicate(
    store: &store::Store,
    person: &str,
) -> Result<Option<Predicate>, ProteinError> {
    let Some(access) = store::auth::person_access(&store.pool, person).await? else {
        return Ok(None);
    };
    let mut filters = Vec::new();
    if let Some(role) = access.role_id {
        if let Some(row) = store::role_policies::get(&store.pool, role).await? {
            if let Some(value) = row.policy {
                let policy: crate::authority::RolePolicy =
                    serde_json::from_value(value).map_err(|_| {
                        store::sqlx::Error::Protocol("Invalid role read policy.".into())
                    })?;
                filters.push(policy.read);
            }
        }
    }
    if let Some(raw) = access.read_filter {
        filters.push(
            serde_json::from_str(&raw).map_err(|_| {
                store::sqlx::Error::Protocol("Invalid personal read filter.".into())
            })?,
        );
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
