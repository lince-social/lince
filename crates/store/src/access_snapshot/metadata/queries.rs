use crate::access_snapshot::sql::{BoundedSql, bounded_sql, row_bytes};

pub(super) const RECORDS: BoundedSql = bounded_sql!(
    "record",
    row_bytes!("uid", "kind", "organ_uid", "replica_root"; 8),
    "uid, kind, organ_uid, deleted_at IS NOT NULL AS deleted, replica_root" =>
    "uid, kind, organ_uid, deleted, replica_root",
    " ORDER BY uid",
);

pub(super) const CONCEPTS: BoundedSql = bounded_sql!(
    "concept",
    row_bytes!("uid", "canonical_name", "origin_organ"),
    "uid, canonical_name, origin_organ",
    " ORDER BY uid",
);

pub(super) const PARENTS: BoundedSql = bounded_sql!(
    "concept_parent",
    row_bytes!("concept_uid", "parent_uid"),
    "concept_uid, parent_uid",
    " ORDER BY concept_uid, parent_uid",
);

pub(super) const ASSERTIONS: BoundedSql = bounded_sql!(
    "record_assertion
     WHERE retracted_at IS NULL",
    row_bytes!("uid", "subject_uid", "predicate_uid", "object_uid", "object_uri", "role", "quantity_mantissa", "unit_uid"; 8),
    "uid, subject_uid, predicate_uid, object_uid, object_uri, role, quantity_mantissa, quantity_scale, unit_uid",
    " ORDER BY uid",
);

pub(super) const VISIBILITY: BoundedSql = bounded_sql!(
    "visibility_rule",
    row_bytes!(
        "uid",
        "subject_kind",
        "subject_uid",
        "target_uid",
        "field",
        "grant_level"
    ),
    "uid, subject_kind, subject_uid, target_uid, field, grant_level",
    " ORDER BY uid",
);

pub(super) const LINGUAS: BoundedSql = bounded_sql!(
    "lingua",
    row_bytes!("uid", "name", "owner_organ", "visibility"),
    "uid, name, owner_organ, visibility",
    " ORDER BY uid",
);

pub(super) const MEMBERSHIPS: BoundedSql = bounded_sql!(
    "lingua_concept",
    row_bytes!("lingua_uid", "concept_uid"),
    "lingua_uid, concept_uid",
    " ORDER BY lingua_uid, concept_uid",
);

pub(super) const DRAFTS: BoundedSql = bounded_sql!(
    "record AS r
     LEFT JOIN record_extension AS e
     ON e.record_uid = r.uid AND e.namespace = 'lince.message-draft'
     WHERE r.kind = 'message_draft'",
    row_bytes!("r.uid", "e.fds"),
    "r.uid, e.fds" =>
    "uid, fds",
    " ORDER BY uid",
);

pub(super) const PLACES: BoundedSql =
    bounded_sql!("place", row_bytes!("uid"), "uid", " ORDER BY uid",);
pub(super) const ROLES: &str = "SELECT COUNT(*) AS row_count, COUNT(*) * 8 AS total_bytes,
            CASE WHEN COUNT(*) = 0 THEN 0 ELSE 8 END AS largest_bytes FROM role";
pub(super) const SOURCE_MEASURE: &str =
    "SELECT COUNT(*) AS row_count, COALESCE(SUM(row_bytes), 0) AS total_bytes,
            COALESCE(MAX(row_bytes), 0) AS largest_bytes
       FROM (SELECT length(CAST(record_uid AS BLOB)) + length(CAST(source_uid AS BLOB))
                  + length(CAST(uri AS BLOB)) + length(CAST(hash AS BLOB)) AS row_bytes
               FROM record_source_location)";
