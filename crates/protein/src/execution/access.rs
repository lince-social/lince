//! Source permissions and the scope of readable Records for a query.

use crate::{
    Protein, ProteinError, Source, matching_records_for, read_rules, record_schema, role_authority,
};
use serde_json::to_string;
use std::collections::HashSet;
use store::{
    Store,
    auth::principal,
    sqlx::{Error::Protocol, query_scalar},
    vocabulary::feature_read,
};

pub(super) async fn readable_records(
    store: &Store,
    subject: Option<&str>,
) -> Result<Option<HashSet<String>>, ProteinError> {
    let Some(subject) = subject else {
        return Ok(None);
    };
    let mut allowed = role_authority::readable(store, subject).await?;
    if let Some(narrowed) = read_filter_targets(store, subject, &allowed).await? {
        allowed.retain(|uid| narrowed.contains(uid));
    }
    Ok(Some(allowed))
}

async fn read_filter_targets(
    store: &Store,
    person_uid: &str,
    ceiling: &HashSet<String>,
) -> Result<Option<HashSet<String>>, ProteinError> {
    let Some(predicate) = read_rules::effective_predicate(store, person_uid).await? else {
        return Ok(None);
    };
    let narrowed = Protein {
        source: Source::Record,
        filter: vec![predicate],
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let roots = matching_records_for::<Option<String>>(
        store,
        &narrowed,
        Some(ceiling),
        Some(person_uid),
        record_schema::predicates_require_body(&narrowed.filter),
    )
    .await?
    .into_iter()
    .map(|row| row.uid)
    .filter(|uid| ceiling.contains(uid))
    .collect::<Vec<_>>();
    let threads = feature_read(&store.pool, "thread").await?;
    let messages = feature_read(&store.pool, "message").await?;
    let roots = to_string(&roots).map_err(|error| Protocol(error.to_string()))?;
    let ceiling = to_string(ceiling).map_err(|error| Protocol(error.to_string()))?;
    let visible = query_scalar::<_, String>(
        "WITH RECURSIVE admitted(uid) AS (
             SELECT value FROM json_each(?)
             UNION
             SELECT a.subject_uid FROM record_assertion a JOIN admitted parent ON a.object_uid = parent.uid
             JOIN record child ON child.uid = a.subject_uid
             WHERE a.retracted_at IS NULL AND child.deleted_at IS NULL
               AND a.subject_uid IN (SELECT value FROM json_each(?))
               AND ((child.kind = 'thread' AND a.predicate_uid = ?) OR (child.kind = 'message' AND a.predicate_uid = ?))
         ) SELECT uid FROM admitted",
    )
    .bind(roots)
    .bind(ceiling)
    .bind(threads)
    .bind(messages)
    .fetch_all(&store.pool)
    .await?;
    Ok(Some(visible.into_iter().collect()))
}

fn source_permission(source: Source) -> Option<&'static str> {
    match source {
        Source::Record
        | Source::Fact
        | Source::Timeline
        | Source::Calendar
        | Source::Schedule
        | Source::Entry
        | Source::Assertion => Some("record:read"),
        Source::Promise
        | Source::Transfer
        | Source::TransferSettlementPreview
        | Source::TransferBulkCompletionPreview => Some("transfer:read"),
        Source::Frequency | Source::Recurrence | Source::KarmaRule => Some("frequency:read"),
        Source::Karma => Some("karma:read"),
        Source::Decision | Source::Concept | Source::Lingua | Source::Nearby | Source::Auth => None,
    }
}

pub(super) async fn can_read_source(
    store: &Store,
    subject: Option<&str>,
    source: Source,
) -> Result<bool, ProteinError> {
    let (Some(subject), Some(permission)) = (subject, source_permission(source)) else {
        return Ok(true);
    };
    Ok(principal(&store.pool, subject)
        .await?
        .is_some_and(|user| user.permits(permission)))
}
