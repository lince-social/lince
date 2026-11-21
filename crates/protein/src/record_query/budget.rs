use serde_json::Value;

use super::{QueryError, QueryLimits};

pub(super) struct Budget<'a> {
    pub(super) limits: &'a QueryLimits,
    steps: usize,
    bytes: usize,
    edges: usize,
    predicates: usize,
}

impl<'a> Budget<'a> {
    pub(super) fn new(limits: &'a QueryLimits) -> Self {
        Self {
            limits,
            steps: 0,
            bytes: 0,
            edges: 0,
            predicates: 0,
        }
    }

    pub(super) fn steps(&mut self, count: usize) -> Result<(), QueryError> {
        charge(&mut self.steps, count, self.limits.steps)
    }

    pub(super) fn step(&mut self) -> Result<(), QueryError> {
        self.steps(1)
    }

    pub(super) fn edge(&mut self) -> Result<(), QueryError> {
        charge(&mut self.edges, 1, self.limits.graph_edges)
    }

    pub(super) fn text(&mut self, text: &str) -> Result<(), QueryError> {
        within(text.len(), self.limits.string_bytes)?;
        self.add_bytes(text.len())
    }

    pub(super) fn value(&mut self, value: &Value) -> Result<(), QueryError> {
        let mut pending = vec![value];
        while let Some(value) = pending.pop() {
            self.step()?;
            match value {
                Value::Null | Value::Bool(true) => self.add_bytes(4)?,
                Value::Bool(false) => self.add_bytes(5)?,
                Value::Number(number) => self.text(&number.to_string())?,
                Value::String(text) => self.text(text)?,
                Value::Array(values) => {
                    within(values.len(), self.limits.steps.saturating_sub(self.steps))?;
                    pending.extend(values);
                }
                Value::Object(values) => {
                    within(values.len(), self.limits.steps.saturating_sub(self.steps))?;
                    for (key, value) in values {
                        self.text(key)?;
                        pending.push(value);
                    }
                }
            }
        }
        Ok(())
    }

    fn add_bytes(&mut self, count: usize) -> Result<(), QueryError> {
        charge(&mut self.bytes, count, self.limits.bytes)
    }

    pub(super) fn predicate(&mut self, depth: usize) -> Result<(), QueryError> {
        self.step()?;
        within(depth, self.limits.predicate_depth)?;
        charge(&mut self.predicates, 1, self.limits.predicate_nodes)
    }

    pub(super) fn remaining_predicates(&self) -> usize {
        self.limits.predicate_nodes - self.predicates
    }
}

fn charge(used: &mut usize, count: usize, limit: usize) -> Result<(), QueryError> {
    *used = used.checked_add(count).ok_or(QueryError::LimitExceeded)?;
    within(*used, limit)
}

pub(super) fn within(value: usize, limit: usize) -> Result<(), QueryError> {
    if value > limit {
        Err(QueryError::LimitExceeded)
    } else {
        Ok(())
    }
}
