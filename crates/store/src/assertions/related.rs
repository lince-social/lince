use std::collections::HashMap;

use nucleus::performance::{Span, add};
use serde_json::to_string;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, query};

use crate::{
    StoreError,
    records::{METADATA_COLUMNS, RecordBody, RecordRow, map_row},
};

/// Lists live subjects pointing to a Record through a predicate or feature.
pub async fn subjects_pointing_to(
    pool: &SqlitePool,
    predicate_uid: &str,
    object_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    query(
        "SELECT DISTINCT r.*,
                (SELECT predicate_uid FROM record_assertion identity
                  WHERE identity.subject_uid = r.uid AND identity.role = 'identity'
                    AND identity.retracted_at IS NULL) AS identity_predicate_uid
           FROM record_assertion_link a JOIN record r ON r.uid = a.subject_uid
          WHERE (a.predicate_uid = ? OR a.predicate_uid IN (SELECT c.concept_uid FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid=f.role_uid WHERE f.feature=?)) AND a.target_uid = ?
            AND a.retracted_at IS NULL AND r.deleted_at IS NULL
          ORDER BY a.created_at, a.uid",
    )
    .bind(predicate_uid)
    .bind(predicate_uid.strip_prefix("feature:"))
    .bind(object_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_row)
    .collect()
}

/// Batches incoming links and groups their live subjects by target.
pub async fn subjects_pointing_to_many(
    pool: &SqlitePool,
    predicate_uid: &str,
    object_uids: &[&str],
) -> Result<HashMap<String, Vec<RecordRow>>, StoreError> {
    let mut subjects = HashMap::<String, Vec<_>>::new();
    if object_uids.is_empty() {
        return Ok(subjects);
    }
    let _selection = Span::new("record-related-subjects-await");
    add("record-related-subject-batches", 1);
    let ids = to_string(object_uids).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows = query(
        "SELECT DISTINCT a.target_uid AS parent_uid, r.*,
                (SELECT predicate_uid FROM record_assertion identity
                  WHERE identity.subject_uid = r.uid AND identity.role = 'identity'
                    AND identity.retracted_at IS NULL) AS identity_predicate_uid
           FROM record_assertion_link a JOIN record r ON r.uid = a.subject_uid
          WHERE (a.predicate_uid = ? OR a.predicate_uid IN (SELECT c.concept_uid FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid=f.role_uid WHERE f.feature=?))
            AND a.target_uid IN (SELECT value FROM json_each(?))
            AND a.retracted_at IS NULL AND r.deleted_at IS NULL
          ORDER BY a.created_at, a.uid",
    )
    .bind(predicate_uid)
    .bind(predicate_uid.strip_prefix("feature:"))
    .bind(ids)
    .fetch_all(pool)
    .await?;
    for row in rows {
        subjects
            .entry(row.get("parent_uid"))
            .or_default()
            .push(map_row(row)?);
    }
    Ok(subjects)
}

/// Lists the most recent live positive-quantity messages in a thread.
pub async fn recent_messages(
    pool: &SqlitePool,
    predicate_uid: &str,
    thread: &str,
    limit: usize,
) -> Result<Vec<RecordRow>, StoreError> {
    messages_before(pool, predicate_uid, thread, limit, None).await
}

/// Reads a thread page strictly before a timestamp and UID cursor, newest first.
pub async fn messages_before(
    pool: &SqlitePool,
    predicate_uid: &str,
    thread: &str,
    limit: usize,
    before: Option<(&str, &str)>,
) -> Result<Vec<RecordRow>, StoreError> {
    query(
        "SELECT r.*,
                (SELECT predicate_uid FROM record_assertion identity
                  WHERE identity.subject_uid = r.uid AND identity.role = 'identity'
                    AND identity.retracted_at IS NULL) AS identity_predicate_uid
           FROM record r
          WHERE r.kind = 'message' AND r.deleted_at IS NULL
            AND r.quantity_mantissa != '0' AND r.quantity_mantissa NOT LIKE '-%'
            AND EXISTS (SELECT 1 FROM record_assertion_link a
                WHERE a.subject_uid = r.uid AND (a.predicate_uid = ? OR a.predicate_uid IN (SELECT c.concept_uid FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid=f.role_uid WHERE f.feature=?))
                  AND a.target_uid = ? AND a.retracted_at IS NULL)
            AND (? IS NULL OR (r.created_at, r.uid) < (?, ?))
          ORDER BY r.created_at DESC, r.uid DESC LIMIT ?",
    )
    .bind(predicate_uid)
    .bind(predicate_uid.strip_prefix("feature:"))
    .bind(thread)
    .bind(before.map(|(at, _)| at))
    .bind(before.map(|(at, _)| at))
    .bind(before.map(|(_, uid)| uid))
    .bind(i64::try_from(limit).unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_row)
    .collect()
}

#[derive(Clone, Copy)]
enum RelatedSide {
    Subject,
    Object,
}

enum RelatedEndpoint<'a> {
    Record(&'a str),
    Selection(&'a str),
}

fn related_record_query(
    select: &str,
    side: RelatedSide,
    predicate_uid: &str,
    endpoint: RelatedEndpoint<'_>,
    selected_uid: Option<&str>,
) -> QueryBuilder<Sqlite> {
    let (selected, endpoint_column) = match side {
        RelatedSide::Subject => ("a.subject_uid", "a.target_uid"),
        RelatedSide::Object => ("a.target_uid", "a.subject_uid"),
    };
    let mut query = QueryBuilder::<Sqlite>::new(select);
    query
        .push(" FROM record_assertion_link a JOIN record r ON r.uid = ")
        .push(selected)
        .push(" WHERE ")
        .push(endpoint_column);
    match endpoint {
        RelatedEndpoint::Record(uid) => {
            query.push(" = ").push_bind(uid);
        }
        RelatedEndpoint::Selection(ids) => {
            query
                .push(" IN (SELECT value FROM json_each(")
                .push_bind(ids)
                .push("))");
        }
    }
    if let Some(selected_uid) = selected_uid {
        query.push(" AND r.uid = ").push_bind(selected_uid);
    }
    query
        .push(" AND (a.predicate_uid = ")
        .push_bind(predicate_uid);
    query.push(" OR a.predicate_uid IN (SELECT c.concept_uid FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid = f.role_uid WHERE f.feature = ")
        .push_bind(predicate_uid.strip_prefix("feature:"));
    query.push(")) AND a.retracted_at IS NULL AND r.deleted_at IS NULL");
    query
}

/// Checks whether a predicate links the subject to a live Record target.
pub async fn object_exists(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
    target_uid: Option<&str>,
) -> Result<bool, StoreError> {
    let _read = Span::new("assertion-object-exists-await");
    let mut query = related_record_query(
        "SELECT EXISTS(SELECT 1",
        RelatedSide::Object,
        predicate_uid,
        RelatedEndpoint::Record(subject_uid),
        target_uid,
    );
    query.push(")");
    query.build_query_scalar::<bool>().fetch_one(pool).await
}

/// Checks whether the specified live subject points to the target.
pub async fn subject_exists(
    pool: &SqlitePool,
    predicate_uid: &str,
    object_uid: &str,
    subject_uid: &str,
) -> Result<bool, StoreError> {
    let _read = Span::new("assertion-subject-exists-await");
    let mut query = related_record_query(
        "SELECT EXISTS(SELECT 1",
        RelatedSide::Subject,
        predicate_uid,
        RelatedEndpoint::Record(object_uid),
        Some(subject_uid),
    );
    query.push(")");
    query.build_query_scalar::<bool>().fetch_one(pool).await
}

/// Lists distinct live Record targets in link creation order.
pub async fn active_object_uids(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
) -> Result<Vec<String>, StoreError> {
    let _read = Span::new("assertion-related-identifiers-await");
    let mut query = related_record_query(
        "SELECT DISTINCT r.uid",
        RelatedSide::Object,
        predicate_uid,
        RelatedEndpoint::Record(subject_uid),
        None,
    );
    query.push(" ORDER BY a.created_at, a.uid");
    query.build_query_scalar::<String>().fetch_all(pool).await
}

/// Lists live Record targets and their kinds up to the requested limit.
pub async fn active_object_kinds(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
    limit: usize,
) -> Result<Vec<(String, String)>, StoreError> {
    let _read = Span::new("assertion-related-kinds-await");
    let mut query = related_record_query(
        "SELECT DISTINCT r.uid, r.kind",
        RelatedSide::Object,
        predicate_uid,
        RelatedEndpoint::Record(subject_uid),
        None,
    );
    query
        .push(" ORDER BY a.created_at, a.uid LIMIT ")
        .push_bind(i64::try_from(limit).unwrap_or(i64::MAX));
    query
        .build_query_as::<(String, String)>()
        .fetch_all(pool)
        .await
}

/// Lists distinct titles of live subjects pointing to the target.
pub async fn subject_heads(
    pool: &SqlitePool,
    predicate_uid: &str,
    object_uid: &str,
) -> Result<Vec<String>, StoreError> {
    let _read = Span::new("assertion-related-heads-await");
    let mut query = related_record_query(
        "SELECT DISTINCT r.head",
        RelatedSide::Subject,
        predicate_uid,
        RelatedEndpoint::Record(object_uid),
        None,
    );
    query.build_query_scalar::<String>().fetch_all(pool).await
}

/// Loads live Record targets and their bodies for one subject.
pub async fn objects_from_subject(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    Ok(
        objects_from_subjects(pool, &[subject_uid], predicate_uid, true)
            .await?
            .remove(subject_uid)
            .unwrap_or_default(),
    )
}

/// Groups live Record targets by subject, optionally projecting their bodies.
pub async fn objects_from_subjects<Body: RecordBody>(
    pool: &SqlitePool,
    subject_uids: &[&str],
    predicate_uid: &str,
    with_body: bool,
) -> Result<HashMap<String, Vec<RecordRow<Body>>>, StoreError> {
    let mut objects = HashMap::<String, Vec<_>>::new();
    if subject_uids.is_empty() {
        return Ok(objects);
    }
    let ids = to_string(subject_uids).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let columns = if with_body { "r.*" } else { METADATA_COLUMNS };
    let columns = format!(
        "SELECT DISTINCT a.subject_uid AS source_uid, {columns},
                (SELECT predicate_uid FROM record_assertion identity
                  WHERE identity.subject_uid = r.uid AND identity.role = 'identity'
                    AND identity.retracted_at IS NULL) AS identity_predicate_uid",
    );
    let mut query = related_record_query(
        &columns,
        RelatedSide::Object,
        predicate_uid,
        RelatedEndpoint::Selection(&ids),
        None,
    );
    query.push(" ORDER BY a.created_at, a.uid");
    let rows = query.build().fetch_all(pool).await?;
    for row in rows {
        objects
            .entry(row.get("source_uid"))
            .or_default()
            .push(map_row(row)?);
    }
    Ok(objects)
}

/// Groups the distinct live Record targets of the supplied subjects.
pub async fn active_object_uids_from_subjects(
    pool: &SqlitePool,
    subject_uids: &[&str],
    predicate_uid: &str,
) -> Result<HashMap<String, Vec<String>>, StoreError> {
    let mut objects = HashMap::<String, Vec<String>>::new();
    if subject_uids.is_empty() {
        return Ok(objects);
    }
    let _read = Span::new("assertion-related-identifiers-await");
    let ids = to_string(subject_uids).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let mut query = related_record_query(
        "SELECT DISTINCT a.subject_uid, r.uid",
        RelatedSide::Object,
        predicate_uid,
        RelatedEndpoint::Selection(&ids),
        None,
    );
    query.push(" ORDER BY a.created_at, a.uid");
    for (subject, object) in query
        .build_query_as::<(String, String)>()
        .fetch_all(pool)
        .await?
    {
        objects.entry(subject).or_default().push(object);
    }
    Ok(objects)
}

/// Lists current link targets, including targets whose Records are deleted.
pub async fn object_uids_from_subject(
    pool: &SqlitePool,
    subject_uid: &str,
    predicate_uid: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(
        object_uids_from_subjects(pool, &[subject_uid], predicate_uid)
            .await?
            .remove(subject_uid)
            .unwrap_or_default(),
    )
}

/// Groups link targets, including targets whose Records are deleted.
pub async fn object_uids_from_subjects(
    pool: &SqlitePool,
    subject_uids: &[&str],
    predicate_uid: &str,
) -> Result<HashMap<String, Vec<String>>, StoreError> {
    let mut objects = HashMap::<String, Vec<String>>::new();
    if subject_uids.is_empty() {
        return Ok(objects);
    }
    let ids = to_string(subject_uids).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows = query(
        "SELECT DISTINCT subject_uid, target_uid AS object_uid FROM record_assertion_link a
          WHERE subject_uid IN (SELECT value FROM json_each(?)) AND (a.predicate_uid = ? OR a.predicate_uid IN (SELECT c.concept_uid FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid=f.role_uid WHERE f.feature=?))
            AND target_uid IS NOT NULL AND retracted_at IS NULL
          ORDER BY created_at, uid",
    )
    .bind(ids)
    .bind(predicate_uid)
    .bind(predicate_uid.strip_prefix("feature:"))
    .fetch_all(pool)
    .await?;
    for row in rows {
        objects
            .entry(row.get("subject_uid"))
            .or_default()
            .push(row.get("object_uid"));
    }
    Ok(objects)
}
