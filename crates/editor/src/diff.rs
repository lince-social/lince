use similar::{ChangeTag, TextDiff};
use std::{ops::Range, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

impl Edit {
    pub fn between(before: &str, after: &str, offset: usize) -> Option<Self> {
        if before == after {
            return None;
        }
        let prefix = before
            .chars()
            .zip(after.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let old_len = before.chars().count();
        let new_len = after.chars().count();
        let suffix = before
            .chars()
            .rev()
            .zip(after.chars().rev())
            .take((old_len - prefix).min(new_len - prefix))
            .take_while(|(a, b)| a == b)
            .count();
        Some(Self {
            range: offset + prefix..offset + old_len - suffix,
            text: after
                .chars()
                .skip(prefix)
                .take(new_len - prefix - suffix)
                .collect(),
        })
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        if self.range.is_empty() || other.range.is_empty() {
            self.range.start <= other.range.end && other.range.start <= self.range.end
        } else {
            self.range.start < other.range.end && other.range.start < self.range.end
        }
    }
}

pub(crate) fn changes(before: &str, after: &str) -> Vec<Edit> {
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(100))
        .diff_lines(before, after);
    let mut result = Vec::new();
    let mut offset = 0;
    let mut removed = String::new();
    let mut inserted = String::new();
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if let Some(edit) = Edit::between(&removed, &inserted, offset) {
                    result.push(edit);
                }
                offset += removed.chars().count() + change.value().chars().count();
                removed.clear();
                inserted.clear();
            }
            ChangeTag::Delete => removed.push_str(change.value()),
            ChangeTag::Insert => inserted.push_str(change.value()),
        }
    }
    if let Some(edit) = Edit::between(&removed, &inserted, offset) {
        result.push(edit);
    }
    result
}
