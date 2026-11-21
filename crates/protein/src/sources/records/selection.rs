use std::collections::HashSet;

use nucleus::{performance::Span, sync::PERSONAL_FILE_ORIGIN};
use store::{
    Store,
    cells::LOCAL_CELL_SLUG,
    records::{RecordBody, RecordRow, RecordSelection, read},
    sqlx::Error::Protocol,
};

use crate::{Predicate, Protein, ProteinError, record_order, record_predicates, record_schema};

/// Finds readable Records matching the query, with their full bodies.
///
/// # Errors
///
/// Returns an error when the query uses an unsupported predicate or a read fails.
pub async fn matching_records(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<RecordRow>, ProteinError> {
    matching_records_for::<String>(store, protein, visible, None, true).await
}

pub(crate) async fn matching_records_for<Body: RecordBody>(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
    with_body: bool,
) -> Result<Vec<RecordRow<Body>>, ProteinError> {
    record_predicates::validate(&protein.filter)?;
    let predicates = protein.filter.as_slice();
    let uids = if matches!(predicates, [Predicate::UidEq(_) | Predicate::SlugEq(_)]) {
        None
    } else {
        predicates.iter().find_map(required_uids)
    };
    let selection = match (predicates, uids.as_deref()) {
        ([Predicate::UidEq(uid)], _) => RecordSelection::Uid(uid),
        ([Predicate::SlugEq(slug)], _) => RecordSelection::Token(slug),
        (_, Some(uids)) => RecordSelection::Uids(uids),
        _ => predicates
            .iter()
            .find_map(|predicate| match predicate {
                Predicate::FileOriginEq(origin) => Some(RecordSelection::FileOrigin(
                    (origin != PERSONAL_FILE_ORIGIN).then_some(origin.as_str()),
                )),
                _ => None,
            })
            .unwrap_or(RecordSelection::All),
    };
    let all = read(&store.pool, selection, with_body).await?;
    let ctx = record_predicates::Context::prepare(store, &protein.filter, visible, subject).await?;
    let _matching = Span::new("protein-record-predicate-matching");
    let mut rows = Vec::new();
    for r in all {
        if visible.is_some_and(|v| !v.contains(&r.uid)) {
            continue;
        }
        if ctx.matches_record(&r)? {
            rows.push(r);
        }
    }
    Ok(rows)
}

/// Finds the matching Record UIDs within the supplied set.
///
/// # Errors
///
/// Returns an error when predicate preparation, matching, or storage fails.
pub async fn matching_among(
    store: &Store,
    protein: &Protein,
    among: &HashSet<String>,
) -> Result<HashSet<String>, ProteinError> {
    if among.is_empty() {
        return Ok(HashSet::new());
    }
    Ok(matching_records_for::<Option<String>>(
        store,
        protein,
        Some(among),
        None,
        record_schema::predicates_require_body(&protein.filter),
    )
    .await?
    .into_iter()
    .map(|row| row.uid)
    .collect())
}

/// Selects complete Records in query order and applies the page cursor and limit.
///
/// # Errors
///
/// Returns an error if matching or ordering fails, or if the page cursor is stale.
pub async fn select_records(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<RecordRow>, ProteinError> {
    select_records_for::<String>(store, protein, visible, None, true).await
}

pub(super) async fn select_records_for<Body: RecordBody>(
    store: &Store,
    protein: &Protein,
    visible: Option<&HashSet<String>>,
    subject: Option<&str>,
    with_body: bool,
) -> Result<Vec<RecordRow<Body>>, ProteinError> {
    let mut rows =
        matching_records_for::<Body>(store, protein, visible, subject, with_body).await?;
    rows.retain(|row| row.slug.as_deref() != Some(LOCAL_CELL_SLUG));
    if protein.aggregate.is_none() {
        rows = record_order::records(store, rows, &protein.order).await?;
        if let Some(after) = &protein.include.record_after {
            let position = rows
                .iter()
                .position(|row| &row.uid == after)
                .ok_or_else(|| {
                    Protocol("This page changed. Press Refresh to return to the first page.".into())
                })?;
            rows.drain(..=position);
        }
        if let Some(limit) = protein.limit {
            rows.truncate(limit);
        }
    }
    Ok(rows)
}

fn required_uids(predicate: &Predicate) -> Option<Vec<&str>> {
    match predicate {
        Predicate::UidEq(uid) => Some(vec![uid]),
        Predicate::Any(children) => children
            .iter()
            .map(|child| {
                let Predicate::UidEq(uid) = child else {
                    return None;
                };
                Some(uid.as_str())
            })
            .collect::<Option<HashSet<_>>>()
            .map(|uids| uids.into_iter().collect()),
        _ => None,
    }
}
