use std::collections::{BTreeMap, BTreeSet};

use nucleus::{RecordKind, valid_slug};
use sqlx::{Row, SqliteConnection, query, query_scalar};

use super::{
    AccessSnapshotLimits, ExtensionContent, TargetRecordContent, protocol,
    sql::{
        ByteBudget, TableStats, bounded_sql, checked_stat, require_complete, row_bytes,
        sqlite_limit,
    },
    validate::{object, read_quantity, record_kind, require_uid},
};
use crate::StoreError;

const EXTENSIONS: super::sql::BoundedSql = bounded_sql!(
    "record_extension WHERE record_uid = ?",
    row_bytes!("namespace", "fds"; 8),
    "namespace, version, fds",
    " ORDER BY namespace",
);

const TARGET: super::sql::BoundedSql = bounded_sql!(
    "record WHERE uid = ?",
    row_bytes!("uid", "kind", "organ_uid", "replica_root", "slug", "head", "body", "quantity_mantissa", "unit_uid", "place_uid"; 16),
    "uid, kind, organ_uid, deleted_at IS NOT NULL AS deleted, replica_root, slug, head, body, quantity_mantissa, quantity_scale, unit_uid, place_uid" =>
    "uid, kind, organ_uid, deleted, replica_root, slug, head, body, quantity_mantissa, quantity_scale, unit_uid, place_uid",
    "",
);

async fn record_reference_kind_on(
    connection: &mut SqliteConnection,
    uid: &str,
    row_bytes: usize,
) -> Result<RecordKind, StoreError> {
    let row = query(
        "SELECT CASE WHEN length(CAST(kind AS BLOB)) <= ? THEN kind END AS kind,
                length(CAST(kind AS BLOB)) AS kind_bytes
           FROM record WHERE uid = ?",
    )
    .bind(sqlite_limit(row_bytes, "snapshot row byte limit")?)
    .bind(uid)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| protocol(format!("missing referenced Record `{uid}`")))?;
    let bytes = checked_stat(
        row.try_get("kind_bytes")?,
        "referenced Record kind byte count",
    )?;
    if bytes > row_bytes {
        return Err(protocol("referenced Record kind exceeds its byte limit"));
    }
    let kind: Option<&str> = row.try_get("kind")?;
    record_kind(kind.ok_or_else(|| protocol("referenced Record kind bounded read is incomplete"))?)
}

async fn reference_exists_on(
    connection: &mut SqliteConnection,
    sql: &'static str,
    uid: &str,
) -> Result<bool, StoreError> {
    Ok(query_scalar::<_, i64>(sql)
        .bind(uid)
        .fetch_optional(&mut *connection)
        .await?
        .is_some())
}

/// Reads complete content for the selected Records, including soft-deleted Records.
/// Use the same transaction for metadata and content to keep their snapshots consistent.
///
/// # Errors
/// Returns an error for a duplicate or missing target, excessive row or byte
/// usage, invalid stored content or references, or a failed SQLite read.
pub async fn content_on(
    connection: &mut SqliteConnection,
    target_uids: &[String],
    limits: &AccessSnapshotLimits,
) -> Result<BTreeMap<String, TargetRecordContent>, StoreError> {
    if target_uids.len() > limits.targets {
        return Err(protocol("target content request exceeds its target limit"));
    }
    let mut targets = BTreeSet::new();
    for uid in target_uids {
        require_uid(uid, "r", "target Record")?;
        if !targets.insert(uid.as_str()) {
            return Err(protocol(
                "target content request contains a duplicate Record",
            ));
        }
    }

    let mut budget = ByteBudget::new(limits.content_bytes);
    let mut extension_count = 0usize;
    let mut measured_targets = Vec::with_capacity(targets.len());
    for uid in targets {
        let invalid_record_scale: i64 = query_scalar(
            "SELECT COUNT(*) FROM record
              WHERE uid = ? AND typeof(quantity_scale) != 'integer'",
        )
        .bind(uid)
        .fetch_one(&mut *connection)
        .await?;
        if invalid_record_scale != 0 {
            return Err(protocol(
                "target Record quantity scale has an invalid storage type",
            ));
        }
        let invalid_extension_versions: i64 = query_scalar(
            "SELECT COUNT(*) FROM record_extension
              WHERE record_uid = ? AND typeof(version) != 'integer'",
        )
        .bind(uid)
        .fetch_one(&mut *connection)
        .await?;
        if invalid_extension_versions != 0 {
            return Err(protocol(
                "target extension version has an invalid storage type",
            ));
        }
        let record_stats = query(TARGET.measure)
            .bind(uid)
            .fetch_one(&mut *connection)
            .await?;
        let record_stats = TableStats::decode(&record_stats, "target Record")?;
        if record_stats.rows != 1 {
            return Err(protocol(format!("target Record `{uid}` does not exist")));
        }
        if record_stats.largest > limits.row_bytes {
            return Err(protocol("target content contains an oversized Record row"));
        }
        budget.add(record_stats.bytes, "target content")?;

        let extension_stats = query(EXTENSIONS.measure)
            .bind(uid)
            .fetch_one(&mut *connection)
            .await?;
        let extension_stats = TableStats::decode(&extension_stats, "target extension")?;
        extension_count = extension_count
            .checked_add(extension_stats.rows)
            .ok_or_else(|| protocol("target extension row count overflow"))?;
        if extension_count > limits.extensions {
            return Err(protocol("target content exceeds its extension row limit"));
        }
        if extension_stats.largest > limits.row_bytes {
            return Err(protocol(
                "target content contains an oversized extension row",
            ));
        }
        budget.add(extension_stats.bytes, "target content")?;
        measured_targets.push((uid, extension_stats.rows));
    }

    let row_limit = sqlite_limit(limits.row_bytes, "snapshot row byte limit")?;
    let mut content = BTreeMap::new();
    for (uid, expected_extensions) in measured_targets {
        let row = query(TARGET.read)
            .bind(uid)
            .bind(row_limit)
            .fetch_optional(&mut *connection)
            .await?
            .ok_or_else(|| protocol("target Record bounded read is incomplete"))?;
        let kind_value: &str = row.try_get("kind")?;
        let kind = record_kind(kind_value)?;
        let organ_uid: Option<String> = row.try_get("organ_uid")?;
        let replica_root: Option<String> = row.try_get("replica_root")?;
        let slug: Option<String> = row.try_get("slug")?;
        let unit_uid: Option<String> = row.try_get("unit_uid")?;
        let place_uid: Option<String> = row.try_get("place_uid")?;
        if let Some(value) = &slug
            && !valid_slug(value)
        {
            return Err(protocol("target Record has an invalid slug"));
        }
        if let Some(value) = &organ_uid {
            require_uid(value, "r", "target Record Organ")?;
            if record_reference_kind_on(connection, value, limits.row_bytes).await?
                != RecordKind::Organ
            {
                return Err(protocol("target Record has a non-Organ origin"));
            }
        }
        if let Some(value) = &replica_root {
            require_uid(value, "r", "target Record replica root")?;
            record_reference_kind_on(connection, value, limits.row_bytes).await?;
        }
        if let Some(value) = &unit_uid {
            require_uid(value, "c", "target Record unit")?;
            if !reference_exists_on(connection, "SELECT 1 FROM concept WHERE uid = ?", value)
                .await?
            {
                return Err(protocol("target Record references a missing unit Concept"));
            }
        }
        if let Some(value) = &place_uid {
            require_uid(value, "pl", "target Record Place")?;
            if !reference_exists_on(connection, "SELECT 1 FROM place WHERE uid = ?", value).await? {
                return Err(protocol("target Record references a missing Place"));
            }
        }

        let extension_rows = query(EXTENSIONS.read)
            .bind(uid)
            .bind(row_limit)
            .fetch_all(&mut *connection)
            .await?;
        require_complete(
            extension_rows.len(),
            expected_extensions,
            "target extensions",
        )?;
        let mut extensions = BTreeMap::new();
        for extension in extension_rows {
            let namespace: String = extension.try_get("namespace")?;
            let version: i64 = extension.try_get("version")?;
            let encoded: &str = extension.try_get("fds")?;
            if namespace.is_empty() {
                return Err(protocol("target extension has an empty namespace"));
            }
            if version <= 0 {
                return Err(protocol("target extension has an invalid version"));
            }
            let fields = object(encoded, "target extension")?;
            if extensions
                .insert(namespace, ExtensionContent { version, fields })
                .is_some()
            {
                return Err(protocol("target Record has duplicate extension namespaces"));
            }
        }
        let target = TargetRecordContent {
            uid: uid.to_owned(),
            kind,
            organ_uid,
            deleted: row.try_get::<i64, _>("deleted")? != 0,
            replica_root,
            slug,
            head: row.try_get("head")?,
            body: row.try_get("body")?,
            quantity: read_quantity(&row)?,
            unit_uid,
            place_uid,
            extensions,
        };
        if content.insert(uid.to_owned(), target).is_some() {
            return Err(protocol("duplicate target Record content"));
        }
    }
    Ok(content)
}
