use std::collections::BTreeSet;

use serde_json::{Value, from_str};
use sqlx::SqlitePool;

use super::OpRow;
use crate::{StoreError, concepts::resolve};

/// Prefix for an Assertion link selector.
pub const LINK_PREFIX: &str = "link:";
/// Scope selector for all Assertion links.
pub const LINK_ALL: &str = "link:*";

/// The resolved Assertion predicates a scope admits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkScope {
    pub all: bool,
    pub predicates: BTreeSet<String>,
}

impl LinkScope {
    /// Reports whether the scope includes any Assertion links.
    pub fn wanted(&self) -> bool {
        self.all || !self.predicates.is_empty()
    }

    fn admits(&self, payload: Option<&str>) -> bool {
        self.all
            || payload
                .and_then(|value| from_str::<Value>(value).ok())
                .is_some_and(|value| {
                    value
                        .get("predicate_uid")
                        .and_then(Value::as_str)
                        .is_some_and(|uid| self.predicates.contains(uid))
                })
    }
}

fn named_links(scope: &[String]) -> impl Iterator<Item = &str> {
    scope
        .iter()
        .filter_map(|entry| entry.strip_prefix(LINK_PREFIX))
        .filter(|name| *name != "*")
}

/// Lists explicit predicate names from a scope, excluding the all-links selector.
pub fn link_names(scope: &[String]) -> Vec<String> {
    named_links(scope).map(str::to_owned).collect()
}

/// Checks whether the scope contains an Assertion link selector.
pub fn names_links(scope: &[String]) -> bool {
    scope.iter().any(|entry| entry.starts_with(LINK_PREFIX))
}

/// Resolves named link selectors against the Concept catalog. Absent scope
/// allows all links.
pub async fn resolve_link_scope(
    pool: &SqlitePool,
    scope: Option<&[String]>,
) -> Result<LinkScope, StoreError> {
    let Some(scope) = scope else {
        return Ok(LinkScope {
            all: true,
            predicates: Default::default(),
        });
    };
    let mut resolved = LinkScope {
        all: scope.iter().any(|entry| entry == LINK_ALL),
        predicates: Default::default(),
    };
    for name in named_links(scope) {
        if let Some(uid) = resolve(pool, name).await? {
            resolved.predicates.insert(uid);
        }
    }
    Ok(resolved)
}

/// Filters operations by field and predicate while preserving their order.
pub fn narrow_ops_to_scope(
    ops: Vec<OpRow>,
    scope: Option<&[String]>,
    links: &LinkScope,
) -> Vec<OpRow> {
    let Some(scope) = scope else {
        return ops;
    };
    ops.into_iter()
        .filter(|op| {
            op_in_scope_with_links(
                &op.tbl,
                &op.kind,
                &op.field,
                Some(scope),
                op.value.as_deref(),
                links,
            )
        })
        .collect()
}

/// Checks scope at the operation-class level, for example when queuing tips.
/// Use the link-aware check when a predicate payload is available.
pub fn op_in_scope(tbl: &str, kind: &str, field: &str, scope: Option<&[String]>) -> bool {
    let Some(scope) = scope else {
        return true;
    };
    let links = LinkScope {
        all: names_links(scope),
        ..LinkScope::default()
    };
    op_in_scope_with_links(tbl, kind, field, Some(scope), None, &links)
}

/// Checks both the property scope and resolved Assertion predicate selection.
pub fn op_in_scope_with_links(
    tbl: &str,
    kind: &str,
    field: &str,
    scope: Option<&[String]>,
    value: Option<&str>,
    links: &LinkScope,
) -> bool {
    let Some(scope) = scope else {
        return true;
    };
    let names = |wanted: &str| scope.iter().any(|s| s == wanted);
    match tbl {
        "record" => match kind {
            "tombstone" => true,
            "crdt" | "snapshot" => names("head") || names("body"),
            "set" => {
                let property = field.strip_prefix("property:").unwrap_or(field);
                !property.is_empty()
                    && (names(property)
                        || (property.starts_with("work.log:") && names("work.logs")))
            }
            _ => false,
        },
        "fact" => names("quantity"),
        "record_assertion" => match kind {
            "tombstone" => links.wanted(),
            "set" => links.admits(value),
            _ => false,
        },
        "record_extension" => !field.is_empty() && names(field),
        "concept" => links.wanted(),
        _ => false,
    }
}
