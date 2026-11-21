mod assertions;
mod policy;
mod queries;
mod records;

use std::collections::{BTreeMap, BTreeSet};

use nucleus::RecordKind;
use sqlx::{Row, SqliteConnection, query};

use self::{
    assertions::{assertions_on, resolved_links_on},
    policy::{drafts_on, lingua_concepts_on, linguas_on, visibility_on},
    queries::{
        ASSERTIONS, CONCEPTS, DRAFTS, LINGUAS, MEMBERSHIPS, PARENTS, PLACES, RECORDS, ROLES,
        SOURCE_MEASURE, VISIBILITY,
    },
    records::{concepts_on, parents_on, records_on, validate_origins},
};
use super::{
    AccessMetadataSnapshot, AccessSnapshotLimits, protocol,
    sql::{
        ByteBudget, checked_rows, require_complete, require_storage_types, sqlite_limit,
        table_stats,
    },
    validate::require_uid,
};
use crate::StoreError;

type RecordKinds<'a> = BTreeMap<&'a str, RecordKind>;
type ConceptIds<'a> = BTreeSet<&'a str>;

/// Reads bounded authorization metadata from the caller's SQLite snapshot.
/// Record bodies and targeted extension content are read through [`super::content_on`].
///
/// # Errors
/// Returns an error when limits are exceeded, storage is incomplete or invalid,
/// dependencies are missing, or a SQLite read fails.
pub async fn metadata_on(
    connection: &mut SqliteConnection,
    limits: &AccessSnapshotLimits,
) -> Result<AccessMetadataSnapshot, StoreError> {
    let [
        record_count,
        concept_count,
        parent_count,
        assertion_count,
        place_count,
        role_count,
        visibility_count,
        lingua_count,
        membership_count,
        draft_count,
    ] = preflight(connection, limits).await?;
    let max_bytes = sqlite_limit(limits.row_bytes, "snapshot row byte limit")?;

    let records = records_on(connection, max_bytes, record_count).await?;
    let record_kinds = records
        .iter()
        .map(|row| (row.uid.as_str(), row.kind))
        .collect();
    let concepts = concepts_on(connection, max_bytes, concept_count).await?;
    let concept_ids = concepts.iter().map(|row| row.uid.as_str()).collect();
    let concept_parents = parents_on(connection, max_bytes, parent_count, &concept_ids).await?;
    let assertions = assertions_on(
        connection,
        max_bytes,
        assertion_count,
        &record_kinds,
        &concept_ids,
    )
    .await?;
    let resolved_links = resolved_links_on(connection, &assertions, &record_kinds).await?;
    let places = places_on(connection, max_bytes, place_count).await?;
    let role_ids = roles_on(connection, role_count).await?;
    let visibility_rules = visibility_on(
        connection,
        max_bytes,
        visibility_count,
        &record_kinds,
        &concept_ids,
        &places,
        &role_ids,
    )
    .await?;
    let linguas = linguas_on(connection, max_bytes, lingua_count, &record_kinds).await?;
    let lingua_concepts = lingua_concepts_on(
        connection,
        max_bytes,
        membership_count,
        &linguas,
        &concept_ids,
    )
    .await?;
    let message_drafts = drafts_on(connection, max_bytes, draft_count, &record_kinds).await?;
    validate_origins(&records, &concepts, &record_kinds)?;

    Ok(AccessMetadataSnapshot {
        records,
        concepts,
        concept_parents,
        assertions,
        resolved_links,
        places,
        role_ids,
        visibility_rules,
        linguas,
        lingua_concepts,
        message_drafts,
    })
}

async fn preflight(
    connection: &mut SqliteConnection,
    limits: &AccessSnapshotLimits,
) -> Result<[usize; 10], StoreError> {
    let tables = [
        (RECORDS.measure, limits.records, "Record metadata"),
        (CONCEPTS.measure, limits.concepts, "Concept metadata"),
        (
            PARENTS.measure,
            limits.concept_parents,
            "Concept parent metadata",
        ),
        (ASSERTIONS.measure, limits.assertions, "Assertion metadata"),
        (PLACES.measure, limits.places, "Place metadata"),
        (ROLES, limits.roles, "Role metadata"),
        (
            VISIBILITY.measure,
            limits.visibility_rules,
            "Visibility rule metadata",
        ),
        (LINGUAS.measure, limits.linguas, "Lingua metadata"),
        (
            MEMBERSHIPS.measure,
            limits.lingua_concepts,
            "Lingua membership metadata",
        ),
        (
            DRAFTS.measure,
            limits.message_drafts,
            "MessageDraft metadata",
        ),
    ];
    let mut counts = [0; 10];
    let mut bytes = [0; 10];
    for ((sql, max_rows, label), (rows, bytes)) in
        tables.into_iter().zip(counts.iter_mut().zip(&mut bytes))
    {
        let stats = table_stats(connection, sql, max_rows, limits.row_bytes, label).await?;
        *rows = stats.rows;
        *bytes = stats.bytes;
    }
    require_storage_types(
        connection,
        "SELECT COUNT(*) FROM record_assertion
          WHERE retracted_at IS NULL AND typeof(quantity_scale) NOT IN ('null', 'integer')",
        "Assertion quantity scale",
    )
    .await?;
    let sources = table_stats(
        connection,
        SOURCE_MEASURE,
        limits.records.saturating_mul(64),
        limits.row_bytes,
        "Source location metadata",
    )
    .await?;
    let mut budget = ByteBudget::new(limits.metadata_bytes);
    for ((_, _, label), bytes) in tables.into_iter().zip(bytes) {
        budget.add(bytes, label)?;
        if label == "Assertion metadata" {
            budget.add(sources.bytes, "Source location metadata")?;
        }
    }
    Ok(counts)
}

async fn places_on(
    connection: &mut SqliteConnection,
    max_bytes: i64,
    count: usize,
) -> Result<BTreeSet<String>, StoreError> {
    let rows = checked_rows(connection, PLACES.read, max_bytes, count, "Place metadata").await?;
    let mut places = BTreeSet::new();
    for row in rows {
        let uid = row.try_get::<String, _>("uid")?;
        require_uid(&uid, "pl", "Place")?;
        if !places.insert(uid) {
            return Err(protocol("duplicate Place identity in snapshot"));
        }
    }
    Ok(places)
}

async fn roles_on(
    connection: &mut SqliteConnection,
    count: usize,
) -> Result<BTreeSet<i64>, StoreError> {
    let rows = query("SELECT id FROM role ORDER BY id")
        .fetch_all(&mut *connection)
        .await?;
    require_complete(rows.len(), count, "Role metadata")?;
    let mut roles = BTreeSet::new();
    for row in rows {
        let id = row.try_get::<i64, _>("id")?;
        if id <= 0 || !roles.insert(id) {
            return Err(protocol("stored Role has an invalid identity"));
        }
    }
    Ok(roles)
}
