use serde_json::{Map, Value};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Choice { Local, File }

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

fn node(base: Option<&Value>, local: Option<&Value>, file: Option<&Value>, path: &mut Vec<String>, conflicts: &mut Vec<Conflict>) -> Option<Value> {
    if local == file || file == base { return local.cloned(); }
    if local == base { return file.cloned(); }
    if let (Some(Value::Object(local)), Some(Value::Object(file))) = (local, file)
        && base.is_none_or(Value::is_object) {
        let base = base.and_then(Value::as_object);
        let keys: BTreeSet<_> = base.into_iter().flat_map(|base| base.keys()).chain(local.keys()).chain(file.keys()).cloned().collect();
        let mut merged = Map::new();
        for key in keys {
            path.push(key.clone());
            if let Some(value) = node(base.and_then(|base| base.get(&key)), local.get(&key), file.get(&key), path, conflicts) { merged.insert(key, value); }
            path.pop();
        }
        return Some(Value::Object(merged));
    }
    conflicts.push(Conflict { path: path.clone(), local: local.cloned(), file: file.cloned(), choice: None });
    local.cloned()
}

fn bounded(value: &Value, depth: usize, count: &mut usize) -> bool {
    *count += 1;
    if depth > 16 || *count > 2048 { return false; }
    match value {
        Value::Object(values) => values.values().all(|value| bounded(value, depth + 1, count)),
        Value::Array(values) => values.iter().all(|value| bounded(value, depth + 1, count)),
        _ => true,
    }
}

impl Proposal {
    pub(super) fn new(base: &Value, local: &Value, file: &Value) -> Result<Self, String> {
        for value in [base, local, file] {
            let mut count = 0;
            if !bounded(value, 0, &mut count) || serde_json::to_vec(value).map_err(|error| error.to_string())?.len() > super::super::LIMIT {
                return Err("This settings draft exceeds the bounded configuration review size.".into());
            }
        }
        let mut conflicts = Vec::new();
        let value = node(Some(base), Some(local), Some(file), &mut Vec::new(), &mut conflicts);
        if conflicts.len() > 512 { return Err("This settings review has more than 512 conflicting fields.".into()); }
        Ok(Self { value, conflicts })
    }

    pub(super) fn choose(&mut self, index: usize, choice: Choice) -> Result<(), String> {
        let conflict = self.conflicts.get_mut(index).ok_or("The reviewed field is unavailable.")?;
        let value = match choice { Choice::Local => &conflict.local, Choice::File => &conflict.file }.clone();
        if conflict.path.is_empty() { self.value = value; }
        else {
            let mut parent = self.value.as_mut().ok_or("The settings draft is unavailable.")?;
            for key in &conflict.path[..conflict.path.len() - 1] { parent = parent.get_mut(key).ok_or("The reviewed field changed.")?; }
            let parent = parent.as_object_mut().ok_or("The reviewed field is not an object.")?;
            let key = conflict.path.last().unwrap();
            if let Some(value) = value { parent.insert(key.clone(), value); } else { parent.remove(key); }
        }
        conflict.choice = Some(choice);
        Ok(())
    }

    pub(super) fn resolved(&self) -> bool { self.value.is_some() && self.conflicts.iter().all(|conflict| conflict.choice.is_some()) }
}

#[cfg(test)]
mod tests;
