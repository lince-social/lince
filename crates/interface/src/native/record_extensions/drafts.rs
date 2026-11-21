//! Shared baselines, current edits, and write payloads for Record extensions.

use nucleus::performance::add;
use serde_json::Value;
use std::{collections::BTreeMap, ops::Deref, ptr, sync::Arc};

type Values = BTreeMap<String, Value>;

#[derive(Clone)]
pub(super) struct DraftData<T> {
    original: Arc<T>,
    current: Arc<T>,
}

impl<T> DraftData<T> {
    pub(super) fn new(value: T) -> Self {
        let original = Arc::new(value);
        Self {
            current: Arc::clone(&original),
            original,
        }
    }

    pub(super) fn original(&self) -> &T {
        &self.original
    }

    pub(super) fn snapshot(&self) -> Arc<T> {
        Arc::clone(&self.current)
    }
}

impl<T: Eq> DraftData<T> {
    pub(super) fn changed(&self) -> bool {
        !Arc::ptr_eq(&self.current, &self.original) && self.current != self.original
    }
}

impl<T: Clone> DraftData<T> {
    pub(super) fn edit(&mut self) -> &mut T {
        edit(&mut self.current)
    }
}

impl<T> Deref for DraftData<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.current
    }
}

impl DraftData<Values> {
    pub(super) fn apply_values(&self) -> Values {
        let mut fields = Values::clone(&self.current);
        fields.extend(tombstones(self.original(), self));
        fields
    }

    pub(super) fn undo_values(&self) -> Arc<Values> {
        let mut fields = Arc::clone(&self.original);
        let mut added = tombstones(self, self.original()).peekable();
        if added.peek().is_some() {
            edit(&mut fields).extend(added);
        }
        fields
    }
}

fn tombstones<'a>(from: &'a Values, to: &'a Values) -> impl Iterator<Item = (String, Value)> + 'a {
    (!ptr::eq(from, to))
        .then(|| from.keys())
        .into_iter()
        .flatten()
        .filter(|key| !to.contains_key(*key))
        .map(|key| (key.clone(), Value::Null))
}

fn edit<T: Clone>(snapshot: &mut Arc<T>) -> &mut T {
    if Arc::strong_count(snapshot) > 1 {
        add("extension-draft-data-copies", 1);
    }
    Arc::make_mut(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merges_apply_and_undo_tombstones_without_editing_snapshots() {
        let baseline = Values::from([
            ("existing".into(), json!(-0.0)),
            ("removed".into(), json!("previous")),
            ("null".into(), Value::Null),
        ]);
        let mut draft = DraftData::new(baseline.clone());
        let original = draft.snapshot();
        assert_eq!(draft.apply_values(), baseline);
        assert!(Arc::ptr_eq(&original, &draft.undo_values()));

        let current = draft.edit();
        current.remove("removed");
        current.insert("existing".into(), json!("edited"));
        current.insert("added".into(), json!(["new"]));
        let current = draft.snapshot();
        assert!(draft.changed());
        assert_eq!(draft.apply_values()["removed"], Value::Null);
        assert_eq!(draft.apply_values()["added"], json!(["new"]));
        assert_eq!(draft.apply_values()["null"], Value::Null);
        assert_eq!(draft.undo_values()["added"], Value::Null);
        assert_eq!(draft.undo_values()["removed"], json!("previous"));
        assert_eq!(draft.undo_values()["existing"].to_string(), "-0.0");
        assert_eq!(*original, baseline);
        assert!(!current.contains_key("removed"));
        assert!(!original.contains_key("added"));
    }

    #[test]
    fn undo_shares_the_baseline_when_no_keys_were_added() {
        let mut draft = DraftData::new(Values::from([("removed".into(), json!(1))]));
        let original = draft.snapshot();
        draft.edit().remove("removed");
        assert!(draft.changed());
        assert_eq!(draft.apply_values()["removed"], Value::Null);
        assert!(Arc::ptr_eq(&original, &draft.undo_values()));
        *draft.edit() = (*original).clone();
        assert!(!draft.changed());
        assert!(Arc::ptr_eq(&original, &draft.undo_values()));
    }
}
