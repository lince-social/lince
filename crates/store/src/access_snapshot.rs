use std::collections::{BTreeMap, BTreeSet};

use nucleus::{DecimalValue, RecordKind};
use serde_json::{Map, Value};
use sqlx::{Row, SqliteConnection};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessSnapshotLimits {
    pub records: usize,
    pub concepts: usize,
    pub concept_parents: usize,
    pub assertions: usize,
    pub places: usize,
    pub roles: usize,
    pub visibility_rules: usize,
    pub linguas: usize,
    pub lingua_concepts: usize,
    pub message_drafts: usize,
    pub targets: usize,
    pub extensions: usize,
    pub row_bytes: usize,
    pub metadata_bytes: usize,
    pub content_bytes: usize,
}

impl Default for AccessSnapshotLimits {
    fn default() -> Self {
        Self {
            records: 4096,
            concepts: 4096,
            concept_parents: 65536,
            assertions: 32768,
            places: 4096,
            roles: 4096,
            visibility_rules: 32768,
            linguas: 4096,
            lingua_concepts: 65536,
            message_drafts: 4096,
            targets: 256,
            extensions: 4096,
            row_bytes: 1024 * 1024,
            metadata_bytes: 16 * 1024 * 1024,
            content_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordMetadata {
    pub uid: String,
    pub kind: RecordKind,
    pub organ_uid: Option<String>,
    pub deleted: bool,
    pub replica_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptMetadata {
    pub uid: String,
    pub name: String,
    pub origin_organ: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptParentMetadata {
    pub concept_uid: String,
    pub parent_uid: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionRole {
    Ordinary,
    Identity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssertionMetadata {
    pub uid: String,
    pub subject_uid: String,
    pub predicate_uid: String,
    pub object_uid: Option<String>,
    pub role: AssertionRole,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinguaVisibility {
    Private,
    Shared,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinguaMetadata {
    pub uid: String,
    pub name: String,
    pub owner_organ: Option<String>,
    pub visibility: LinguaVisibility,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinguaConceptMetadata {
    pub lingua_uid: String,
    pub concept_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilitySubject {
    Public,
    Actor(String),
    Organ(String),
    Role(i64),
    Unsupported { kind: String, uid: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityTarget {
    Record(String),
    Concept(String),
    Place(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityGrant {
    Visible,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibilityRuleMetadata {
    pub uid: String,
    pub subject: VisibilitySubject,
    pub target: VisibilityTarget,
    pub field: Option<String>,
    pub grant: VisibilityGrant,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageDraftMetadata {
    pub record_uid: String,
    pub author_uid: String,
    pub operator_uid: String,
    pub thread_uid: Option<String>,
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AccessMetadataSnapshot {
    pub records: Vec<RecordMetadata>,
    pub concepts: Vec<ConceptMetadata>,
    pub concept_parents: Vec<ConceptParentMetadata>,
    pub assertions: Vec<AssertionMetadata>,
    pub places: BTreeSet<String>,
    pub role_ids: BTreeSet<i64>,
    pub visibility_rules: Vec<VisibilityRuleMetadata>,
    pub linguas: Vec<LinguaMetadata>,
    pub lingua_concepts: Vec<LinguaConceptMetadata>,
    pub message_drafts: Vec<MessageDraftMetadata>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionContent {
    pub version: i64,
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetRecordContent {
    pub uid: String,
    pub kind: RecordKind,
    pub organ_uid: Option<String>,
    pub deleted: bool,
    pub replica_root: Option<String>,
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub quantity: DecimalValue,
    pub unit_uid: Option<String>,
    pub place_uid: Option<String>,
    pub extensions: BTreeMap<String, ExtensionContent>,
}

#[derive(Debug, Clone, Copy)]
struct TableStats {
    rows: usize,
    bytes: usize,
    largest: usize,
}

struct ByteBudget {
    used: usize,
    limit: usize,
}

impl ByteBudget {
    fn new(limit: usize) -> Self {
        Self { used: 0, limit }
    }

    fn add(&mut self, bytes: usize, label: &str) -> Result<(), StoreError> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| protocol(format!("{label} byte count overflow")))?;
        if self.used > self.limit {
            return Err(protocol(format!("{label} exceeds its total byte limit")));
        }
        Ok(())
    }
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn sqlite_limit(value: usize, label: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol(format!("{label} does not fit SQLite")))
}

fn checked_stat(value: i64, label: &str) -> Result<usize, StoreError> {
    usize::try_from(value).map_err(|_| protocol(format!("{label} is invalid")))
}

async fn table_stats(
    connection: &mut SqliteConnection,
    query: &str,
    row_limit: usize,
    row_bytes: usize,
    label: &str,
) -> Result<TableStats, StoreError> {
    let row = sqlx::query(query).fetch_one(&mut *connection).await?;
    let stats = TableStats {
        rows: checked_stat(row.try_get("row_count")?, &format!("{label} row count"))?,
        bytes: checked_stat(row.try_get("total_bytes")?, &format!("{label} byte count"))?,
        largest: checked_stat(
            row.try_get("largest_bytes")?,
            &format!("{label} row byte count"),
        )?,
    };
    if stats.rows > row_limit {
        return Err(protocol(format!("{label} exceeds its row limit")));
    }
    if stats.largest > row_bytes {
        return Err(protocol(format!("{label} contains an oversized row")));
    }
    Ok(stats)
}

async fn require_storage_types(
    connection: &mut SqliteConnection,
    query: &str,
    label: &str,
) -> Result<(), StoreError> {
    let invalid: i64 = sqlx::query_scalar(query)
        .fetch_one(&mut *connection)
        .await?;
    if invalid != 0 {
        return Err(protocol(format!("{label} has an invalid storage type")));
    }
    Ok(())
}

fn require_complete(actual: usize, expected: usize, label: &str) -> Result<(), StoreError> {
    if actual != expected {
        return Err(protocol(format!("{label} bounded read is incomplete")));
    }
    Ok(())
}

fn record_kind(value: &str) -> Result<RecordKind, StoreError> {
    RecordKind::parse(value)
        .ok_or_else(|| protocol(format!("invalid stored Record kind `{value}`")))
}

fn assertion_role(value: &str) -> Result<AssertionRole, StoreError> {
    match value {
        "ordinary" => Ok(AssertionRole::Ordinary),
        "identity" => Ok(AssertionRole::Identity),
        _ => Err(protocol(format!("invalid stored Assertion role `{value}`"))),
    }
}

fn lingua_visibility(value: &str) -> Result<LinguaVisibility, StoreError> {
    match value {
        "private" => Ok(LinguaVisibility::Private),
        "shared" => Ok(LinguaVisibility::Shared),
        "public" => Ok(LinguaVisibility::Public),
        _ => Err(protocol(format!(
            "invalid stored Lingua visibility `{value}`"
        ))),
    }
}

fn visibility_grant(value: &str) -> Result<VisibilityGrant, StoreError> {
    match value {
        "visible" => Ok(VisibilityGrant::Visible),
        "hidden" => Ok(VisibilityGrant::Hidden),
        _ => Err(protocol(format!(
            "invalid stored visibility grant `{value}`"
        ))),
    }
}

fn require_uid(uid: &str, prefix: &str, label: &str) -> Result<(), StoreError> {
    if !nucleus::valid_uid(uid, prefix) {
        return Err(protocol(format!("invalid stored {label} identity `{uid}`")));
    }
    Ok(())
}

fn require_lingua_uid(uid: &str) -> Result<(), StoreError> {
    if uid != crate::linguas::LOCAL_UID && !nucleus::valid_uid(uid, "g") {
        return Err(protocol(format!("invalid stored Lingua identity `{uid}`")));
    }
    Ok(())
}

fn require_record_kind(
    records: &BTreeMap<String, RecordKind>,
    uid: &str,
    accepted: &[RecordKind],
    label: &str,
) -> Result<(), StoreError> {
    let kind = records
        .get(uid)
        .ok_or_else(|| protocol(format!("{label} references a missing Record `{uid}`")))?;
    if !accepted.contains(kind) {
        return Err(protocol(format!(
            "{label} references a Record of the wrong kind"
        )));
    }
    Ok(())
}

fn parse_role_identity(value: &str) -> Result<i64, StoreError> {
    let role_id = value
        .parse::<i64>()
        .map_err(|_| protocol("visibility Role identity is not a positive canonical integer"))?;
    if role_id <= 0 || role_id.to_string() != value {
        return Err(protocol(
            "visibility Role identity is not a positive canonical integer",
        ));
    }
    Ok(role_id)
}

fn object(value: &str, label: &str) -> Result<Map<String, Value>, StoreError> {
    serde_json::from_str::<Value>(value)
        .map_err(|error| protocol(format!("{label} is not valid JSON: {error}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| protocol(format!("{label} is not a JSON object")))
}

fn read_decimal(row: &sqlx::sqlite::SqliteRow, prefix: &str) -> Result<DecimalValue, StoreError> {
    let mantissa: String = row.try_get(format!("{prefix}_mantissa").as_str())?;
    let scale: i64 = row.try_get(format!("{prefix}_scale").as_str())?;
    let digits = mantissa.strip_prefix('-').unwrap_or(&mantissa);
    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
        || mantissa == "-0"
    {
        return Err(protocol(format!(
            "stored {prefix} mantissa is not canonical"
        )));
    }
    crate::exact::parse_decimal(&mantissa, scale)
}

pub async fn metadata_on(
    connection: &mut SqliteConnection,
    limits: &AccessSnapshotLimits,
) -> Result<AccessMetadataSnapshot, StoreError> {
    let record_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count,
                COALESCE(SUM(row_bytes), 0) AS total_bytes,
                COALESCE(MAX(row_bytes), 0) AS largest_bytes
           FROM (
             SELECT COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(kind AS BLOB)), 0)
                  + COALESCE(length(CAST(organ_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(replica_root AS BLOB)), 0) + 8 AS row_bytes
               FROM record
           )",
        limits.records,
        limits.row_bytes,
        "Record metadata",
    )
    .await?;
    let concept_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count,
                COALESCE(SUM(row_bytes), 0) AS total_bytes,
                COALESCE(MAX(row_bytes), 0) AS largest_bytes
           FROM (
             SELECT COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(canonical_name AS BLOB)), 0)
                  + COALESCE(length(CAST(origin_organ AS BLOB)), 0) AS row_bytes
               FROM concept
           )",
        limits.concepts,
        limits.row_bytes,
        "Concept metadata",
    )
    .await?;
    let parent_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count,
                COALESCE(SUM(row_bytes), 0) AS total_bytes,
                COALESCE(MAX(row_bytes), 0) AS largest_bytes
           FROM (
             SELECT COALESCE(length(CAST(concept_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(parent_uid AS BLOB)), 0) AS row_bytes
               FROM concept_parent
           )",
        limits.concept_parents,
        limits.row_bytes,
        "Concept parent metadata",
    )
    .await?;
    let assertion_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count,
                COALESCE(SUM(row_bytes), 0) AS total_bytes,
                COALESCE(MAX(row_bytes), 0) AS largest_bytes
           FROM (
             SELECT COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(subject_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(predicate_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(object_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(role AS BLOB)), 0)
                  + COALESCE(length(CAST(quantity_mantissa AS BLOB)), 0)
                  + COALESCE(length(CAST(unit_uid AS BLOB)), 0) + 8 AS row_bytes
               FROM record_assertion
              WHERE retracted_at IS NULL
           )",
        limits.assertions,
        limits.row_bytes,
        "Assertion metadata",
    )
    .await?;
    let place_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count,
                COALESCE(SUM(COALESCE(length(CAST(uid AS BLOB)), 0)), 0) AS total_bytes,
                COALESCE(MAX(COALESCE(length(CAST(uid AS BLOB)), 0)), 0) AS largest_bytes
           FROM place",
        limits.places,
        limits.row_bytes,
        "Place metadata",
    )
    .await?;
    let role_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count, COUNT(*) * 8 AS total_bytes,
                CASE WHEN COUNT(*) = 0 THEN 0 ELSE 8 END AS largest_bytes
           FROM role",
        limits.roles,
        limits.row_bytes,
        "Role metadata",
    )
    .await?;
    let visibility_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count,
                COALESCE(SUM(row_bytes), 0) AS total_bytes,
                COALESCE(MAX(row_bytes), 0) AS largest_bytes
           FROM (
             SELECT COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(subject_kind AS BLOB)), 0)
                  + COALESCE(length(CAST(subject_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(target_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(field AS BLOB)), 0)
                  + COALESCE(length(CAST(grant_level AS BLOB)), 0) AS row_bytes
               FROM visibility_rule
           )",
        limits.visibility_rules,
        limits.row_bytes,
        "Visibility rule metadata",
    )
    .await?;
    let lingua_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count,
                COALESCE(SUM(row_bytes), 0) AS total_bytes,
                COALESCE(MAX(row_bytes), 0) AS largest_bytes
           FROM (
             SELECT COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(name AS BLOB)), 0)
                  + COALESCE(length(CAST(owner_organ AS BLOB)), 0)
                  + COALESCE(length(CAST(visibility AS BLOB)), 0) AS row_bytes
               FROM lingua
           )",
        limits.linguas,
        limits.row_bytes,
        "Lingua metadata",
    )
    .await?;
    let membership_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count,
                COALESCE(SUM(row_bytes), 0) AS total_bytes,
                COALESCE(MAX(row_bytes), 0) AS largest_bytes
           FROM (
             SELECT COALESCE(length(CAST(lingua_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(concept_uid AS BLOB)), 0) AS row_bytes
               FROM lingua_concept
           )",
        limits.lingua_concepts,
        limits.row_bytes,
        "Lingua membership metadata",
    )
    .await?;
    let draft_stats = table_stats(
        connection,
        "SELECT COUNT(*) AS row_count,
                COALESCE(SUM(row_bytes), 0) AS total_bytes,
                COALESCE(MAX(row_bytes), 0) AS largest_bytes
           FROM (
             SELECT COALESCE(length(CAST(r.uid AS BLOB)), 0)
                  + COALESCE(length(CAST(e.fds AS BLOB)), 0) AS row_bytes
               FROM record AS r
               LEFT JOIN record_extension AS e
                 ON e.record_uid = r.uid AND e.namespace = 'lince.message-draft'
              WHERE r.kind = 'message_draft'
           )",
        limits.message_drafts,
        limits.row_bytes,
        "MessageDraft metadata",
    )
    .await?;
    require_storage_types(
        connection,
        "SELECT COUNT(*) FROM record_assertion
          WHERE retracted_at IS NULL
            AND typeof(quantity_scale) NOT IN ('null', 'integer')",
        "Assertion quantity scale",
    )
    .await?;

    let mut budget = ByteBudget::new(limits.metadata_bytes);
    for (stats, label) in [
        (record_stats, "Record metadata"),
        (concept_stats, "Concept metadata"),
        (parent_stats, "Concept parent metadata"),
        (assertion_stats, "Assertion metadata"),
        (place_stats, "Place metadata"),
        (role_stats, "Role metadata"),
        (visibility_stats, "Visibility rule metadata"),
        (lingua_stats, "Lingua metadata"),
        (membership_stats, "Lingua membership metadata"),
        (draft_stats, "MessageDraft metadata"),
    ] {
        budget.add(stats.bytes, label)?;
    }

    let row_limit = sqlite_limit(limits.row_bytes, "snapshot row byte limit")?;
    let record_rows = sqlx::query(
        "WITH bounded AS (
             SELECT uid, kind, organ_uid, deleted_at IS NOT NULL AS deleted, replica_root,
                    COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(kind AS BLOB)), 0)
                  + COALESCE(length(CAST(organ_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(replica_root AS BLOB)), 0) + 8 AS row_bytes
               FROM record
         )
         SELECT uid, kind, organ_uid, deleted, replica_root
           FROM bounded WHERE row_bytes <= ? ORDER BY uid",
    )
    .bind(row_limit)
    .fetch_all(&mut *connection)
    .await?;
    require_complete(record_rows.len(), record_stats.rows, "Record metadata")?;
    let mut records = Vec::with_capacity(record_rows.len());
    let mut record_kinds = BTreeMap::new();
    for row in record_rows {
        let uid: String = row.try_get("uid")?;
        require_uid(&uid, "r", "Record")?;
        let kind_value: String = row.try_get("kind")?;
        let kind = record_kind(&kind_value)?;
        let organ_uid: Option<String> = row.try_get("organ_uid")?;
        let replica_root: Option<String> = row.try_get("replica_root")?;
        if let Some(value) = &organ_uid {
            require_uid(value, "r", "Record Organ")?;
        }
        if let Some(value) = &replica_root {
            require_uid(value, "r", "replica root")?;
        }
        if record_kinds.insert(uid.clone(), kind).is_some() {
            return Err(protocol("duplicate Record identity in snapshot"));
        }
        records.push(RecordMetadata {
            uid,
            kind,
            organ_uid,
            deleted: row.try_get::<i64, _>("deleted")? != 0,
            replica_root,
        });
    }

    let concept_rows = sqlx::query(
        "WITH bounded AS (
             SELECT uid, canonical_name, origin_organ,
                    COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(canonical_name AS BLOB)), 0)
                  + COALESCE(length(CAST(origin_organ AS BLOB)), 0) AS row_bytes
               FROM concept
         )
         SELECT uid, canonical_name, origin_organ
           FROM bounded WHERE row_bytes <= ? ORDER BY uid",
    )
    .bind(row_limit)
    .fetch_all(&mut *connection)
    .await?;
    require_complete(concept_rows.len(), concept_stats.rows, "Concept metadata")?;
    let mut concepts = Vec::with_capacity(concept_rows.len());
    let mut concept_ids = BTreeSet::new();
    for row in concept_rows {
        let uid: String = row.try_get("uid")?;
        require_uid(&uid, "c", "Concept")?;
        let name: String = row.try_get("canonical_name")?;
        if name.is_empty() {
            return Err(protocol("stored Concept name is empty"));
        }
        let origin_organ: Option<String> = row.try_get("origin_organ")?;
        if let Some(value) = &origin_organ {
            require_uid(value, "r", "Concept origin Organ")?;
        }
        if !concept_ids.insert(uid.clone()) {
            return Err(protocol("duplicate Concept identity in snapshot"));
        }
        concepts.push(ConceptMetadata {
            uid,
            name,
            origin_organ,
        });
    }

    let parent_rows = sqlx::query(
        "WITH bounded AS (
             SELECT concept_uid, parent_uid,
                    COALESCE(length(CAST(concept_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(parent_uid AS BLOB)), 0) AS row_bytes
               FROM concept_parent
         )
         SELECT concept_uid, parent_uid
           FROM bounded WHERE row_bytes <= ? ORDER BY concept_uid, parent_uid",
    )
    .bind(row_limit)
    .fetch_all(&mut *connection)
    .await?;
    require_complete(
        parent_rows.len(),
        parent_stats.rows,
        "Concept parent metadata",
    )?;
    let mut concept_parents = Vec::with_capacity(parent_rows.len());
    for row in parent_rows {
        let concept_uid: String = row.try_get("concept_uid")?;
        let parent_uid: String = row.try_get("parent_uid")?;
        require_uid(&concept_uid, "c", "Concept parent child")?;
        require_uid(&parent_uid, "c", "Concept parent")?;
        if !concept_ids.contains(&concept_uid) || !concept_ids.contains(&parent_uid) {
            return Err(protocol("Concept parent edge has a missing endpoint"));
        }
        concept_parents.push(ConceptParentMetadata {
            concept_uid,
            parent_uid,
        });
    }

    let assertion_rows = sqlx::query(
        "WITH bounded AS (
             SELECT uid, subject_uid, predicate_uid, object_uid, role,
                    quantity_mantissa, quantity_scale, unit_uid,
                    COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(subject_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(predicate_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(object_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(role AS BLOB)), 0)
                  + COALESCE(length(CAST(quantity_mantissa AS BLOB)), 0)
                  + COALESCE(length(CAST(unit_uid AS BLOB)), 0) + 8 AS row_bytes
               FROM record_assertion
              WHERE retracted_at IS NULL
         )
         SELECT uid, subject_uid, predicate_uid, object_uid, role,
                quantity_mantissa, quantity_scale, unit_uid
           FROM bounded WHERE row_bytes <= ? ORDER BY uid",
    )
    .bind(row_limit)
    .fetch_all(&mut *connection)
    .await?;
    require_complete(
        assertion_rows.len(),
        assertion_stats.rows,
        "Assertion metadata",
    )?;
    let mut assertions = Vec::with_capacity(assertion_rows.len());
    let mut assertion_ids = BTreeSet::new();
    for row in assertion_rows {
        let uid: String = row.try_get("uid")?;
        let subject_uid: String = row.try_get("subject_uid")?;
        let predicate_uid: String = row.try_get("predicate_uid")?;
        let object_uid: Option<String> = row.try_get("object_uid")?;
        let unit_uid: Option<String> = row.try_get("unit_uid")?;
        require_uid(&uid, "a", "Assertion")?;
        require_uid(&subject_uid, "r", "Assertion subject")?;
        require_uid(&predicate_uid, "c", "Assertion predicate")?;
        if let Some(value) = &object_uid {
            require_uid(value, "r", "Assertion object")?;
        }
        if let Some(value) = &unit_uid {
            require_uid(value, "c", "Assertion unit")?;
        }
        if !record_kinds.contains_key(&subject_uid)
            || object_uid
                .as_ref()
                .is_some_and(|value| !record_kinds.contains_key(value))
            || !concept_ids.contains(&predicate_uid)
            || unit_uid
                .as_ref()
                .is_some_and(|value| !concept_ids.contains(value))
        {
            return Err(protocol("Assertion has a missing dependency"));
        }
        if !assertion_ids.insert(uid.clone()) {
            return Err(protocol("duplicate Assertion identity in snapshot"));
        }
        let has_mantissa = row
            .try_get::<Option<String>, _>("quantity_mantissa")?
            .is_some();
        let has_scale = row.try_get::<Option<i64>, _>("quantity_scale")?.is_some();
        if has_mantissa != has_scale {
            return Err(protocol("Assertion has an incomplete exact quantity"));
        }
        let quantity = has_mantissa
            .then(|| read_decimal(&row, "quantity"))
            .transpose()?;
        let role_value: String = row.try_get("role")?;
        let role = assertion_role(&role_value)?;
        if role == AssertionRole::Identity
            && (object_uid.is_some() || quantity.is_some() || unit_uid.is_some())
        {
            return Err(protocol("identity Assertion has an invalid stored shape"));
        }
        assertions.push(AssertionMetadata {
            uid,
            subject_uid,
            predicate_uid,
            object_uid,
            role,
            quantity,
            unit_uid,
        });
    }

    let place_rows = sqlx::query(
        "SELECT uid FROM place
          WHERE COALESCE(length(CAST(uid AS BLOB)), 0) <= ? ORDER BY uid",
    )
    .bind(row_limit)
    .fetch_all(&mut *connection)
    .await?;
    require_complete(place_rows.len(), place_stats.rows, "Place metadata")?;
    let mut places = BTreeSet::new();
    for row in place_rows {
        let uid: String = row.try_get("uid")?;
        require_uid(&uid, "pl", "Place")?;
        if !places.insert(uid) {
            return Err(protocol("duplicate Place identity in snapshot"));
        }
    }

    let role_rows = sqlx::query("SELECT id FROM role ORDER BY id")
        .fetch_all(&mut *connection)
        .await?;
    require_complete(role_rows.len(), role_stats.rows, "Role metadata")?;
    let mut role_ids = BTreeSet::new();
    for row in role_rows {
        let role_id: i64 = row.try_get("id")?;
        if role_id <= 0 || !role_ids.insert(role_id) {
            return Err(protocol("stored Role has an invalid identity"));
        }
    }

    let visibility_rows = sqlx::query(
        "WITH bounded AS (
             SELECT uid, subject_kind, subject_uid, target_uid, field, grant_level,
                    COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(subject_kind AS BLOB)), 0)
                  + COALESCE(length(CAST(subject_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(target_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(field AS BLOB)), 0)
                  + COALESCE(length(CAST(grant_level AS BLOB)), 0) AS row_bytes
               FROM visibility_rule
         )
         SELECT uid, subject_kind, subject_uid, target_uid, field, grant_level
           FROM bounded WHERE row_bytes <= ? ORDER BY uid",
    )
    .bind(row_limit)
    .fetch_all(&mut *connection)
    .await?;
    require_complete(
        visibility_rows.len(),
        visibility_stats.rows,
        "Visibility rule metadata",
    )?;
    let mut visibility_rules = Vec::with_capacity(visibility_rows.len());
    let mut visibility_ids = BTreeSet::new();
    for row in visibility_rows {
        let uid: String = row.try_get("uid")?;
        let subject_kind: String = row.try_get("subject_kind")?;
        let subject_uid: Option<String> = row.try_get("subject_uid")?;
        let target_uid: String = row.try_get("target_uid")?;
        let field: Option<String> = row.try_get("field")?;
        require_uid(&uid, "v", "visibility rule")?;
        if subject_kind.is_empty() {
            return Err(protocol("visibility rule has an empty subject kind"));
        }
        if !visibility_ids.insert(uid.clone()) {
            return Err(protocol("duplicate visibility rule identity in snapshot"));
        }
        if field.as_ref().is_some_and(|value| value.is_empty()) {
            return Err(protocol("visibility rule has an empty field"));
        }
        let subject = match subject_kind.as_str() {
            "public" => {
                if subject_uid.is_some() {
                    return Err(protocol("public visibility rule has a subject identity"));
                }
                VisibilitySubject::Public
            }
            "actor" => {
                let value = subject_uid
                    .as_deref()
                    .ok_or_else(|| protocol("actor visibility rule has no subject identity"))?;
                require_uid(value, "r", "visibility actor")?;
                require_record_kind(
                    &record_kinds,
                    value,
                    &[RecordKind::Person, RecordKind::Organ],
                    "visibility actor",
                )?;
                VisibilitySubject::Actor(value.to_string())
            }
            "organ" => {
                let value = subject_uid
                    .as_deref()
                    .ok_or_else(|| protocol("Organ visibility rule has no subject identity"))?;
                require_uid(value, "r", "visibility Organ")?;
                require_record_kind(
                    &record_kinds,
                    value,
                    &[RecordKind::Organ],
                    "visibility Organ",
                )?;
                VisibilitySubject::Organ(value.to_string())
            }
            "role" => {
                let value = subject_uid
                    .as_deref()
                    .ok_or_else(|| protocol("Role visibility rule has no subject identity"))?;
                let role_id = parse_role_identity(value)?;
                if !role_ids.contains(&role_id) {
                    return Err(protocol("visibility rule references a missing Role"));
                }
                VisibilitySubject::Role(role_id)
            }
            _ => VisibilitySubject::Unsupported {
                kind: subject_kind,
                uid: subject_uid,
            },
        };
        let target = if nucleus::valid_uid(&target_uid, "r") {
            if !record_kinds.contains_key(&target_uid) {
                return Err(protocol("visibility rule references a missing Record"));
            }
            VisibilityTarget::Record(target_uid)
        } else if nucleus::valid_uid(&target_uid, "c") {
            if !concept_ids.contains(&target_uid) {
                return Err(protocol("visibility rule references a missing Concept"));
            }
            VisibilityTarget::Concept(target_uid)
        } else if nucleus::valid_uid(&target_uid, "pl") {
            if !places.contains(&target_uid) {
                return Err(protocol("visibility rule references a missing Place"));
            }
            VisibilityTarget::Place(target_uid)
        } else {
            return Err(protocol("visibility rule has an invalid target identity"));
        };
        visibility_rules.push(VisibilityRuleMetadata {
            uid,
            subject,
            target,
            field,
            grant: {
                let value: String = row.try_get("grant_level")?;
                visibility_grant(&value)?
            },
        });
    }

    let lingua_rows = sqlx::query(
        "WITH bounded AS (
             SELECT uid, name, owner_organ, visibility,
                    COALESCE(length(CAST(uid AS BLOB)), 0)
                  + COALESCE(length(CAST(name AS BLOB)), 0)
                  + COALESCE(length(CAST(owner_organ AS BLOB)), 0)
                  + COALESCE(length(CAST(visibility AS BLOB)), 0) AS row_bytes
               FROM lingua
         )
         SELECT uid, name, owner_organ, visibility
           FROM bounded WHERE row_bytes <= ? ORDER BY uid",
    )
    .bind(row_limit)
    .fetch_all(&mut *connection)
    .await?;
    require_complete(lingua_rows.len(), lingua_stats.rows, "Lingua metadata")?;
    let mut linguas = Vec::with_capacity(lingua_rows.len());
    let mut lingua_ids = BTreeSet::new();
    for row in lingua_rows {
        let uid: String = row.try_get("uid")?;
        let name: String = row.try_get("name")?;
        let owner_organ: Option<String> = row.try_get("owner_organ")?;
        require_lingua_uid(&uid)?;
        if name.is_empty() {
            return Err(protocol("stored Lingua name is empty"));
        }
        if let Some(value) = &owner_organ {
            require_uid(value, "r", "Lingua owner Organ")?;
            require_record_kind(
                &record_kinds,
                value,
                &[RecordKind::Organ],
                "Lingua owner Organ",
            )?;
        }
        if !lingua_ids.insert(uid.clone()) {
            return Err(protocol("duplicate Lingua identity in snapshot"));
        }
        let visibility_value: String = row.try_get("visibility")?;
        linguas.push(LinguaMetadata {
            uid,
            name,
            owner_organ,
            visibility: lingua_visibility(&visibility_value)?,
        });
    }

    let membership_rows = sqlx::query(
        "WITH bounded AS (
             SELECT lingua_uid, concept_uid,
                    COALESCE(length(CAST(lingua_uid AS BLOB)), 0)
                  + COALESCE(length(CAST(concept_uid AS BLOB)), 0) AS row_bytes
               FROM lingua_concept
         )
         SELECT lingua_uid, concept_uid
           FROM bounded WHERE row_bytes <= ? ORDER BY lingua_uid, concept_uid",
    )
    .bind(row_limit)
    .fetch_all(&mut *connection)
    .await?;
    require_complete(
        membership_rows.len(),
        membership_stats.rows,
        "Lingua membership metadata",
    )?;
    let mut lingua_concepts = Vec::with_capacity(membership_rows.len());
    for row in membership_rows {
        let lingua_uid: String = row.try_get("lingua_uid")?;
        let concept_uid: String = row.try_get("concept_uid")?;
        require_lingua_uid(&lingua_uid)?;
        require_uid(&concept_uid, "c", "Lingua member Concept")?;
        if !lingua_ids.contains(&lingua_uid) || !concept_ids.contains(&concept_uid) {
            return Err(protocol("Lingua membership has a missing dependency"));
        }
        lingua_concepts.push(LinguaConceptMetadata {
            lingua_uid,
            concept_uid,
        });
    }

    let draft_rows = sqlx::query(
        "WITH bounded AS (
             SELECT r.uid, e.fds,
                    COALESCE(length(CAST(r.uid AS BLOB)), 0)
                  + COALESCE(length(CAST(e.fds AS BLOB)), 0) AS row_bytes
               FROM record AS r
               LEFT JOIN record_extension AS e
                 ON e.record_uid = r.uid AND e.namespace = 'lince.message-draft'
              WHERE r.kind = 'message_draft'
         )
         SELECT uid, fds FROM bounded WHERE row_bytes <= ? ORDER BY uid",
    )
    .bind(row_limit)
    .fetch_all(&mut *connection)
    .await?;
    require_complete(draft_rows.len(), draft_stats.rows, "MessageDraft metadata")?;
    let mut message_drafts = Vec::with_capacity(draft_rows.len());
    for row in draft_rows {
        let record_uid: String = row.try_get("uid")?;
        let encoded: Option<String> = row.try_get("fds")?;
        let metadata = object(
            encoded
                .as_deref()
                .ok_or_else(|| protocol("MessageDraft has no ownership metadata"))?,
            "MessageDraft ownership metadata",
        )?;
        let author_uid = metadata
            .get("author")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("MessageDraft has no author identity"))?
            .to_string();
        let operator_uid = metadata
            .get("operator")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("MessageDraft has no operator identity"))?
            .to_string();
        require_uid(&author_uid, "r", "MessageDraft author")?;
        require_uid(&operator_uid, "r", "MessageDraft operator")?;
        require_record_kind(
            &record_kinds,
            &author_uid,
            &[RecordKind::Person, RecordKind::Organ],
            "MessageDraft author",
        )?;
        require_record_kind(
            &record_kinds,
            &operator_uid,
            &[RecordKind::Person, RecordKind::Organ],
            "MessageDraft operator",
        )?;
        let thread_uid = match metadata.get("thread") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => {
                require_uid(value, "r", "MessageDraft Thread")?;
                require_record_kind(
                    &record_kinds,
                    value,
                    &[RecordKind::Thread],
                    "MessageDraft Thread",
                )?;
                Some(value.clone())
            }
            Some(_) => return Err(protocol("MessageDraft Thread identity is not a string")),
        };
        message_drafts.push(MessageDraftMetadata {
            record_uid,
            author_uid,
            operator_uid,
            thread_uid,
            metadata,
        });
    }

    for record in &records {
        if let Some(organ_uid) = &record.organ_uid {
            require_record_kind(
                &record_kinds,
                organ_uid,
                &[RecordKind::Organ],
                "Record Organ origin",
            )?;
        }
        if let Some(root_uid) = &record.replica_root
            && !record_kinds.contains_key(root_uid)
        {
            return Err(protocol("Record references a missing replica root"));
        }
    }
    for concept in &concepts {
        if let Some(origin_uid) = &concept.origin_organ {
            require_record_kind(
                &record_kinds,
                origin_uid,
                &[RecordKind::Organ],
                "Concept origin Organ",
            )?;
        }
    }

    Ok(AccessMetadataSnapshot {
        records,
        concepts,
        concept_parents,
        assertions,
        places,
        role_ids,
        visibility_rules,
        linguas,
        lingua_concepts,
        message_drafts,
    })
}

async fn record_reference_kind_on(
    connection: &mut SqliteConnection,
    uid: &str,
    row_bytes: usize,
) -> Result<RecordKind, StoreError> {
    let row = sqlx::query(
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
    let kind: Option<String> = row.try_get("kind")?;
    record_kind(
        kind.as_deref()
            .ok_or_else(|| protocol("referenced Record kind bounded read is incomplete"))?,
    )
}

async fn reference_exists_on(
    connection: &mut SqliteConnection,
    table: &str,
    uid: &str,
) -> Result<bool, StoreError> {
    let query = match table {
        "concept" => "SELECT 1 FROM concept WHERE uid = ?",
        "place" => "SELECT 1 FROM place WHERE uid = ?",
        _ => return Err(protocol("unsupported snapshot reference table")),
    };
    Ok(sqlx::query_scalar::<_, i64>(query)
        .bind(uid)
        .fetch_optional(&mut *connection)
        .await?
        .is_some())
}

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
        if !targets.insert(uid.clone()) {
            return Err(protocol(
                "target content request contains a duplicate Record",
            ));
        }
    }

    let mut budget = ByteBudget::new(limits.content_bytes);
    let mut extension_count = 0usize;
    let mut expected_extensions = BTreeMap::new();
    for uid in &targets {
        let invalid_record_scale: i64 = sqlx::query_scalar(
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
        let invalid_extension_versions: i64 = sqlx::query_scalar(
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
        let record_stats = sqlx::query(
            "SELECT COUNT(*) AS row_count,
                    COALESCE(SUM(row_bytes), 0) AS total_bytes,
                    COALESCE(MAX(row_bytes), 0) AS largest_bytes
               FROM (
                 SELECT COALESCE(length(CAST(uid AS BLOB)), 0)
                      + COALESCE(length(CAST(kind AS BLOB)), 0)
                      + COALESCE(length(CAST(organ_uid AS BLOB)), 0)
                      + COALESCE(length(CAST(replica_root AS BLOB)), 0)
                      + COALESCE(length(CAST(slug AS BLOB)), 0)
                      + COALESCE(length(CAST(head AS BLOB)), 0)
                      + COALESCE(length(CAST(body AS BLOB)), 0)
                      + COALESCE(length(CAST(quantity_mantissa AS BLOB)), 0)
                      + COALESCE(length(CAST(unit_uid AS BLOB)), 0)
                      + COALESCE(length(CAST(place_uid AS BLOB)), 0) + 16 AS row_bytes
                   FROM record WHERE uid = ?
               )",
        )
        .bind(uid)
        .fetch_one(&mut *connection)
        .await?;
        let record_stats = TableStats {
            rows: checked_stat(
                record_stats.try_get("row_count")?,
                "target Record row count",
            )?,
            bytes: checked_stat(
                record_stats.try_get("total_bytes")?,
                "target Record byte count",
            )?,
            largest: checked_stat(
                record_stats.try_get("largest_bytes")?,
                "target Record row byte count",
            )?,
        };
        if record_stats.rows != 1 {
            return Err(protocol(format!("target Record `{uid}` does not exist")));
        }
        if record_stats.largest > limits.row_bytes {
            return Err(protocol("target content contains an oversized Record row"));
        }
        budget.add(record_stats.bytes, "target content")?;

        let extension_stats = sqlx::query(
            "SELECT COUNT(*) AS row_count,
                    COALESCE(SUM(row_bytes), 0) AS total_bytes,
                    COALESCE(MAX(row_bytes), 0) AS largest_bytes
               FROM (
                 SELECT COALESCE(length(CAST(namespace AS BLOB)), 0)
                      + COALESCE(length(CAST(fds AS BLOB)), 0) + 8 AS row_bytes
                   FROM record_extension WHERE record_uid = ?
               )",
        )
        .bind(uid)
        .fetch_one(&mut *connection)
        .await?;
        let extension_stats = TableStats {
            rows: checked_stat(
                extension_stats.try_get("row_count")?,
                "target extension row count",
            )?,
            bytes: checked_stat(
                extension_stats.try_get("total_bytes")?,
                "target extension byte count",
            )?,
            largest: checked_stat(
                extension_stats.try_get("largest_bytes")?,
                "target extension row byte count",
            )?,
        };
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
        expected_extensions.insert(uid.clone(), extension_stats.rows);
    }

    let row_limit = sqlite_limit(limits.row_bytes, "snapshot row byte limit")?;
    let mut content = BTreeMap::new();
    for uid in targets {
        let row = sqlx::query(
            "WITH bounded AS (
                 SELECT uid, kind, organ_uid, deleted_at IS NOT NULL AS deleted, replica_root,
                        slug, head, body, quantity_mantissa, quantity_scale, unit_uid, place_uid,
                        COALESCE(length(CAST(uid AS BLOB)), 0)
                      + COALESCE(length(CAST(kind AS BLOB)), 0)
                      + COALESCE(length(CAST(organ_uid AS BLOB)), 0)
                      + COALESCE(length(CAST(replica_root AS BLOB)), 0)
                      + COALESCE(length(CAST(slug AS BLOB)), 0)
                      + COALESCE(length(CAST(head AS BLOB)), 0)
                      + COALESCE(length(CAST(body AS BLOB)), 0)
                      + COALESCE(length(CAST(quantity_mantissa AS BLOB)), 0)
                      + COALESCE(length(CAST(unit_uid AS BLOB)), 0)
                      + COALESCE(length(CAST(place_uid AS BLOB)), 0) + 16 AS row_bytes
                   FROM record WHERE uid = ?
             )
             SELECT uid, kind, organ_uid, deleted, replica_root, slug, head, body,
                    quantity_mantissa, quantity_scale, unit_uid, place_uid
               FROM bounded WHERE row_bytes <= ?",
        )
        .bind(&uid)
        .bind(row_limit)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| protocol("target Record bounded read is incomplete"))?;
        let kind_value: String = row.try_get("kind")?;
        let kind = record_kind(&kind_value)?;
        let organ_uid: Option<String> = row.try_get("organ_uid")?;
        let replica_root: Option<String> = row.try_get("replica_root")?;
        let slug: Option<String> = row.try_get("slug")?;
        let unit_uid: Option<String> = row.try_get("unit_uid")?;
        let place_uid: Option<String> = row.try_get("place_uid")?;
        if let Some(value) = &slug
            && !nucleus::valid_slug(value)
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
            if !reference_exists_on(connection, "concept", value).await? {
                return Err(protocol("target Record references a missing unit Concept"));
            }
        }
        if let Some(value) = &place_uid {
            require_uid(value, "pl", "target Record Place")?;
            if !reference_exists_on(connection, "place", value).await? {
                return Err(protocol("target Record references a missing Place"));
            }
        }

        let extension_rows = sqlx::query(
            "WITH bounded AS (
                 SELECT namespace, version, fds,
                        COALESCE(length(CAST(namespace AS BLOB)), 0)
                      + COALESCE(length(CAST(fds AS BLOB)), 0) + 8 AS row_bytes
                   FROM record_extension WHERE record_uid = ?
             )
             SELECT namespace, version, fds
               FROM bounded WHERE row_bytes <= ? ORDER BY namespace",
        )
        .bind(&uid)
        .bind(row_limit)
        .fetch_all(&mut *connection)
        .await?;
        require_complete(
            extension_rows.len(),
            *expected_extensions
                .get(&uid)
                .ok_or_else(|| protocol("target extension preflight is missing"))?,
            "target extensions",
        )?;
        let mut extensions = BTreeMap::new();
        for extension in extension_rows {
            let namespace: String = extension.try_get("namespace")?;
            let version: i64 = extension.try_get("version")?;
            let encoded: String = extension.try_get("fds")?;
            if namespace.is_empty() {
                return Err(protocol("target extension has an empty namespace"));
            }
            if version <= 0 {
                return Err(protocol("target extension has an invalid version"));
            }
            let fields = object(&encoded, "target extension")?;
            if extensions
                .insert(namespace, ExtensionContent { version, fields })
                .is_some()
            {
                return Err(protocol("target Record has duplicate extension namespaces"));
            }
        }
        let target = TargetRecordContent {
            uid: uid.clone(),
            kind,
            organ_uid,
            deleted: row.try_get::<i64, _>("deleted")? != 0,
            replica_root,
            slug,
            head: row.try_get("head")?,
            body: row.try_get("body")?,
            quantity: read_decimal(&row, "quantity")?,
            unit_uid,
            place_uid,
            extensions,
        };
        if content.insert(uid, target).is_some() {
            return Err(protocol("duplicate target Record content"));
        }
    }
    Ok(content)
}
