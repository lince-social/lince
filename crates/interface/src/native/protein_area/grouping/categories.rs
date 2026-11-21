//! Stable grouping identities, labels, and ordering for Protein Area rows.

use super::GroupAxis;
use crate::records::{display, display_list};
use serde::Serialize;
use serde_json::Value;
use std::cmp::Ordering;

#[derive(Debug, Default)]
pub(super) struct Category {
    pub(super) key: String,
    pub(super) title: String,
    order: String,
    missing: bool,
}

pub(super) fn category(row: &Value, axis: Option<&GroupAxis>) -> Category {
    let Some(axis) = axis else {
        return Category::default();
    };
    let value = &row[&axis.property];
    let (key, title) = match axis.property.as_str() {
        "assignees" => assignees(value),
        "assertions" => assertions(value),
        _ => {
            let title = display(value);
            (title.clone(), title)
        }
    };
    let missing = value.is_null() || title.trim().is_empty();
    let order = if missing {
        "unset".into()
    } else {
        title.to_lowercase()
    };
    Category {
        key: if missing {
            "null".into()
        } else {
            format!("value:{key}")
        },
        title: if missing { "Unset".into() } else { title },
        order,
        missing,
    }
}

fn assignees(value: &Value) -> (String, String) {
    let mut people: Vec<_> = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|person| {
            Some((
                person["uid"].as_str()?,
                person["head"].as_str().unwrap_or("Person"),
            ))
        })
        .collect();
    people.sort_by_key(|&(uid, _)| uid);
    people.dedup_by_key(|person| person.0);
    let key = encode(&people.iter().map(|person| person.0).collect::<Vec<_>>());
    people.sort_by_cached_key(|&(uid, name)| (name.to_lowercase(), uid));
    let title = people
        .into_iter()
        .map(|(_, name)| name)
        .collect::<Vec<_>>()
        .join(", ");
    (key, title)
}

fn assertions(value: &Value) -> (String, String) {
    let (mut keys, mut titles): (Vec<_>, Vec<_>) = value
        .as_array()
        .into_iter()
        .flatten()
        .map(|assertion| {
            let fields = ["predicate_uid", "object", "quantity", "unit"].map(|key| &assertion[key]);
            (
                encode(&fields),
                display_list(std::slice::from_ref(assertion)),
            )
        })
        .unzip();
    keys.sort();
    keys.dedup();
    titles.sort();
    titles.dedup();
    (encode(&keys), titles.join(", "))
}

fn encode(value: &impl Serialize) -> String {
    serde_json::to_string(value).expect("group keys contain only strings and existing JSON values")
}

fn decimal_order(a: &str, b: &str) -> Ordering {
    let negative_a = a.starts_with('-');
    let negative_b = b.starts_with('-');
    fn parts(value: &str) -> (&str, &str) {
        let value = value.trim_start_matches('-');
        let (integer, fraction) = value.split_once('.').unwrap_or((value, ""));
        (
            integer.trim_start_matches('0'),
            fraction.trim_end_matches('0'),
        )
    }
    let a = parts(a);
    let b = parts(b);
    let negative_a = negative_a && (!a.0.is_empty() || !a.1.is_empty());
    let negative_b = negative_b && (!b.0.is_empty() || !b.1.is_empty());
    let order =
        a.0.len()
            .cmp(&b.0.len())
            .then(a.0.cmp(b.0))
            .then(a.1.cmp(b.1));
    match (negative_a, negative_b) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => order.reverse(),
        _ => order,
    }
}

pub(super) fn compare(a: &Category, b: &Category, axis: Option<&GroupAxis>) -> Ordering {
    let Some(axis) = axis else {
        return Ordering::Equal;
    };
    if a.missing != b.missing {
        return a.missing.cmp(&b.missing);
    }
    let order = if matches!(
        axis.property.as_str(),
        "quantity" | "estimate_min" | "spent_seconds"
    ) {
        decimal_order(&a.title, &b.title)
    } else {
        a.order.cmp(&b.order)
    }
    .then(a.key.cmp(&b.key));
    if axis.descending {
        order.reverse()
    } else {
        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn category_identity_and_labels_keep_their_distinct_orders() {
        let row = json!({
            "assignees": [
                {"uid": "b", "head": "alice"},
                {"uid": "a", "head": "ALICE"},
                {"uid": "b", "head": "later duplicate"},
                {"uid": "c"},
                {"head": "missing ID"},
            ],
            "assertions": [
                {"predicate_uid": "p", "predicate": "tag", "quantity": "2"},
                {"predicate_uid": "p", "head": "Other label", "quantity": "2"},
            ],
        });
        let people = category(&row, Some(&GroupAxis::new("assignees")));
        assert_eq!(people.key, r#"value:["a","b","c"]"#);
        assert_eq!(people.title, "ALICE, alice, Person");
        let assertions = category(&row, Some(&GroupAxis::new("assertions")));
        assert_eq!(assertions.key, r#"value:["[\"p\",null,\"2\",null]"]"#);
        assert_eq!(assertions.title, "#tag: 2, Other label");
    }

    #[test]
    fn exact_decimals_sort_before_unset_in_both_directions() {
        let mut axis = GroupAxis::new("quantity");
        let large = category(&json!({"quantity": "9007199254740993"}), Some(&axis));
        let lower = category(&json!({"quantity": "9007199254740992"}), Some(&axis));
        let unset = category(&json!({}), Some(&axis));
        assert_eq!(compare(&large, &lower, Some(&axis)), Ordering::Greater);
        assert_eq!(compare(&large, &unset, Some(&axis)), Ordering::Less);
        axis.descending = true;
        assert_eq!(compare(&large, &lower, Some(&axis)), Ordering::Less);
        assert_eq!(compare(&large, &unset, Some(&axis)), Ordering::Less);
        assert_eq!(decimal_order("-0.000", "000.0"), Ordering::Equal);
    }
}
