//! Bounded three-way merging and explicit conflict choices for settings review.

use serde_json::{Map, Value};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Choice {
    Local,
    File,
}

pub(super) struct Conflict {
    pub path: Vec<String>,
    pub local: Option<Value>,
    pub file: Option<Value>,
    pub choice: Option<Choice>,
}

pub(super) struct Proposal {
    pub value: Option<Value>,
    pub conflicts: Vec<Conflict>,
}

fn merge_node<'a>(
    base: Option<&'a Value>,
    local: Option<&'a Value>,
    file: Option<&'a Value>,
    path: &mut Vec<&'a str>,
    conflicts: &mut Vec<Conflict>,
) -> Option<Value> {
    if local == file || file == base {
        return local.cloned();
    }
    if local == base {
        return file.cloned();
    }
    if let (Some(Value::Object(local)), Some(Value::Object(file))) = (local, file)
        && base.is_none_or(Value::is_object)
    {
        let base = base.and_then(Value::as_object);
        let keys: BTreeSet<_> = base
            .into_iter()
            .flat_map(Map::keys)
            .chain(local.keys())
            .chain(file.keys())
            .map(String::as_str)
            .collect();
        let mut merged = Map::new();
        for key in keys {
            path.push(key);
            if let Some(value) = merge_node(
                base.and_then(|base| base.get(key)),
                local.get(key),
                file.get(key),
                path,
                conflicts,
            ) {
                merged.insert(key.into(), value);
            }
            path.pop();
        }
        return Some(Value::Object(merged));
    }
    conflicts.push(Conflict {
        path: path.iter().map(|&key| key.into()).collect(),
        local: local.cloned(),
        file: file.cloned(),
        choice: None,
    });
    local.cloned()
}

fn bounded(value: &Value, depth: usize, count: &mut usize) -> bool {
    *count += 1;
    if depth > 16 || *count > 2048 {
        return false;
    }
    match value {
        Value::Object(values) => values
            .values()
            .all(|value| bounded(value, depth + 1, count)),
        Value::Array(values) => values.iter().all(|value| bounded(value, depth + 1, count)),
        _ => true,
    }
}

impl Proposal {
    pub(super) fn new(base: &Value, local: &Value, file: &Value) -> Result<Self, String> {
        for value in [base, local, file] {
            let mut count = 0;
            if !bounded(value, 0, &mut count)
                || nucleus::json::encoded_len(value).map_err(|error| error.to_string())?
                    > super::super::LIMIT
            {
                return Err(
                    "This settings draft exceeds the bounded configuration review size.".into(),
                );
            }
        }
        let mut conflicts = Vec::new();
        let value = merge_node(
            Some(base),
            Some(local),
            Some(file),
            &mut Vec::new(),
            &mut conflicts,
        );
        if conflicts.len() > 512 {
            return Err("This settings review has more than 512 conflicting fields.".into());
        }
        Ok(Self { value, conflicts })
    }

    pub(super) fn choose(&mut self, index: usize, choice: Choice) -> Result<(), String> {
        let conflict = self
            .conflicts
            .get_mut(index)
            .ok_or("The reviewed field is unavailable.")?;
        let value = match choice {
            Choice::Local => &conflict.local,
            Choice::File => &conflict.file,
        };
        if let Some((key, parents)) = conflict.path.split_last() {
            let mut parent = self
                .value
                .as_mut()
                .ok_or("The settings draft is unavailable.")?;
            for key in parents {
                parent = parent.get_mut(key).ok_or("The reviewed field changed.")?;
            }
            let parent = parent
                .as_object_mut()
                .ok_or("The reviewed field is not an object.")?;
            if let Some(value) = value {
                parent.insert(key.clone(), value.clone());
            } else {
                parent.remove(key);
            }
        } else {
            self.value.clone_from(value);
        }
        conflict.choice = Some(choice);
        Ok(())
    }

    pub(super) fn resolved(&self) -> bool {
        self.value.is_some()
            && self
                .conflicts
                .iter()
                .all(|conflict| conflict.choice.is_some())
    }
}

#[cfg(test)]
mod tests;
