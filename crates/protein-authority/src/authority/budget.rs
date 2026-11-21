use nucleus::valid_uid;
use serde_json::Value;

use super::{AuthorityError, ExtensionProperty, Limits};

#[derive(Clone, Copy)]
pub(super) struct Budget<'a> {
    pub(super) limits: &'a Limits,
    steps: usize,
    bytes: usize,
    predicates: usize,
    edges: usize,
}

impl<'a> Budget<'a> {
    pub(super) fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            steps: 0,
            bytes: 0,
            predicates: 0,
            edges: 0,
        }
    }

    pub(super) fn step(&mut self) -> Result<(), AuthorityError> {
        charge(&mut self.steps, 1, self.limits.steps)
    }

    pub(super) fn text(&mut self, text: &str) -> Result<(), AuthorityError> {
        charge(&mut self.bytes, text.len(), self.limits.bytes)
    }

    pub(super) fn edge(&mut self) -> Result<(), AuthorityError> {
        charge(&mut self.edges, 1, self.limits.graph_edges)
    }

    pub(super) fn predicate(&mut self, depth: usize) -> Result<(), AuthorityError> {
        self.step()?;
        within(depth, self.limits.predicate_depth)?;
        charge(&mut self.predicates, 1, self.limits.predicate_nodes)
    }

    pub(super) fn remaining_predicates(&self) -> usize {
        self.limits.predicate_nodes - self.predicates
    }

    pub(super) fn property(&mut self, property: &ExtensionProperty) -> Result<(), AuthorityError> {
        if property.namespace.is_empty() || property.field.is_empty() {
            return Err(AuthorityError::InvalidPolicy);
        }
        self.text(&property.namespace)?;
        self.text(&property.field)
    }

    pub(super) fn value(&mut self, value: &Value) -> Result<(), AuthorityError> {
        let mut pending = vec![(value, 0)];
        while let Some((value, depth)) = pending.pop() {
            self.step()?;
            within(depth, 32)?;
            match value {
                Value::String(text) => self.text(text)?,
                Value::Array(values) => {
                    within(values.len(), self.limits.steps.saturating_sub(self.steps))?;
                    pending.extend(values.iter().map(|value| (value, depth + 1)));
                }
                Value::Object(values) => {
                    within(values.len(), self.limits.steps.saturating_sub(self.steps))?;
                    for (key, value) in values {
                        self.text(key)?;
                        pending.push((value, depth + 1));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn charge(used: &mut usize, increment: usize, limit: usize) -> Result<(), AuthorityError> {
    *used = used
        .checked_add(increment)
        .ok_or(AuthorityError::LimitExceeded)?;
    within(*used, limit)
}

pub(super) fn within(value: usize, limit: usize) -> Result<(), AuthorityError> {
    if value > limit {
        Err(AuthorityError::LimitExceeded)
    } else {
        Ok(())
    }
}

pub(super) fn identity(uid: &str, prefix: &str) -> Result<(), AuthorityError> {
    if valid_uid(uid, prefix) {
        Ok(())
    } else {
        Err(AuthorityError::InvalidIdentity)
    }
}
