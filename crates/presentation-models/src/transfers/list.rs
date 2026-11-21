// non-lean-yet

use super::{display_ref, facet, status_ref, title_ref};
use serde_json::Value;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

pub fn filtered(rows: &[Value], mine: bool, filter: &str, query: &str, sort: &str) -> Vec<Value> {
    filtered_ref(rows, mine, filter, query, sort)
        .into_iter()
        .cloned()
        .collect()
}

pub fn filtered_ref<'a>(
    rows: &'a [Value],
    mine: bool,
    filter: &str,
    query: &str,
    sort: &str,
) -> Vec<&'a Value> {
    let _prepare = nucleus::performance::Span::new("transfer-list-order-preparation");
    let query = query.trim().to_lowercase();
    let mut result: Vec<_> = rows
        .iter()
        .filter(|row| {
            (!mine || facet(row, "mine"))
                && facet(row, filter)
                && (query.is_empty() || row.to_string().to_lowercase().contains(&query))
        })
        .collect();
    nucleus::performance::add("transfer-list-admitted-rows", result.len() as u64);
    result.sort_by_cached_key(|&row| {
        let priority = match sort {
            "name" => 0,
            _ if sort == "attention" && facet(row, "awaiting_me") => 0,
            _ if sort == "attention" && facet(row, "awaiting_others") => 1,
            _ => match status_ref(row).as_ref() {
                "system_disputed" | "participant_disputed" | "broken" => 2,
                "active" | "partially_settled" => 3,
                "draft" | "open" => 4,
                "completed" | "settled" | "kept" => 5,
                _ => 6,
            },
        };
        (
            priority,
            title_ref(row).to_lowercase(),
            display_ref(&row["uid"]),
        )
    });
    result
}

pub struct Hierarchy<'a> {
    first: HashMap<Cow<'a, str>, &'a Value>,
}

impl<'a> Hierarchy<'a> {
    pub fn new(rows: &'a [Value]) -> Self {
        let mut first = HashMap::with_capacity(rows.len());
        for row in rows {
            first.entry(display_ref(&row["uid"])).or_insert(row);
        }
        nucleus::performance::add("transfer-tree-index-entries", first.len() as u64);
        Self { first }
    }

    pub fn find(&self, uid: &str) -> Option<&'a Value> {
        self.first.get(uid).copied()
    }

    fn visit(&self, row: &'a Value, mut visit: impl FnMut(&'a Value)) {
        let mut parent = display_ref(&row["parent"]);
        let mut seen = HashSet::from([display_ref(&row["uid"])]);
        while !parent.is_empty() && seen.insert(parent.clone()) {
            let Some(ancestor) = self.find(&parent) else {
                break;
            };
            visit(ancestor);
            parent = display_ref(&ancestor["parent"]);
        }
    }

    pub fn path(&self, row: &'a Value) -> Vec<Cow<'a, str>> {
        let mut path = vec![title_ref(row)];
        self.visit(row, |ancestor| path.push(title_ref(ancestor)));
        path.reverse();
        path
    }

    pub fn depth(&self, row: &'a Value) -> usize {
        let mut depth = 0;
        self.visit(row, |_| depth += 1);
        depth
    }
}

pub fn depth(row: &Value, rows: &[Value]) -> usize {
    Hierarchy::new(rows).depth(row)
}
