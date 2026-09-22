use nucleus::DecimalValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{cmp::Ordering, collections::BTreeMap};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub predicate: String,
    pub name: String,
}

impl Selection {
    pub(super) fn valid(&self) -> bool {
        nucleus::valid_uid(&self.predicate, "c") && self.name.len() <= 1024
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Item {
    pub uid: String,
    pub head: String,
    pub quantity: Option<DecimalValue>,
}

pub(super) fn choices(rows: &[Value]) -> Vec<Selection> {
    let mut choices = BTreeMap::new();
    for assertion in rows
        .iter()
        .flat_map(|row| row["assertions"].as_array().into_iter().flatten())
    {
        if !assertion["object"].is_null()
            || !assertion["unit"].is_null()
            || assertion["role"] == "identity"
        {
            continue;
        }
        if let (Some(predicate), Some(name)) = (
            assertion["predicate_uid"].as_str(),
            assertion["predicate"].as_str(),
        ) {
            let selection = Selection {
                predicate: predicate.into(),
                name: name.into(),
            };
            if selection.valid() {
                choices.insert((name.to_owned(), predicate.to_owned()), selection);
            }
        }
    }
    choices.into_values().collect()
}

pub(super) fn ordered(rows: &[Value], selection: Option<&Selection>) -> Result<Vec<Item>, String> {
    let mut items = Vec::with_capacity(rows.len());
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let uid = row["uid"]
            .as_str()
            .filter(|uid| nucleus::valid_uid(uid, "r"))
            .ok_or("Invalid Record in Protein results")?;
        if !seen.insert(uid) {
            return Err("Protein returned the same Record more than once".into());
        }
        let mut quantity = None;
        if let Some(selection) = selection {
            for assertion in
                row["assertions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|assertion| {
                        assertion["predicate_uid"].as_str() == Some(&selection.predicate)
                            && assertion["object"].is_null()
                    })
            {
                if assertion["role"] == "identity" {
                    return Err("Identity assertions cannot be numbered".into());
                }
                if !assertion["unit"].is_null() {
                    return Err("Choose an assertion without units for numbering".into());
                }
                if assertion["quantity"].is_null() {
                    continue;
                }
                if quantity.is_some() {
                    return Err("A Record has multiple quantities for this assertion; resolve them before numbering".into());
                }
                quantity = Some(
                    DecimalValue::parse_inferred(
                        assertion["quantity"]
                            .as_str()
                            .ok_or("Invalid assertion quantity")?,
                    )
                    .map_err(|_| "Invalid assertion quantity")?,
                );
            }
        }
        items.push(Item {
            uid: uid.into(),
            head: row["head"].as_str().unwrap_or(uid).into(),
            quantity,
        });
    }
    if selection.is_some() {
        items.sort_by(|left, right| match (left.quantity, right.quantity) {
            (Some(left), Some(right)) => left.exact_numeric_cmp(right),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        });
    }
    Ok(items)
}

pub(super) fn apply_order(rows: &mut [Item], order: &[String]) {
    if order.is_empty() {
        return;
    }
    let positions: std::collections::HashMap<_, _> = order
        .iter()
        .enumerate()
        .map(|(index, uid)| (uid.as_str(), index))
        .collect();
    rows.sort_by_key(|row| {
        positions
            .get(row.uid.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
}
