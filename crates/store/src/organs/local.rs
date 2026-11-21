use std::borrow::Cow;

use nucleus::{
    RecordKind, execution::now, new_uid, performance::Span, vocabulary::DEFAULT_VOCABULARY_UID,
};
use serde_json::{Value, json};
use sqlx::{
    Sqlite, SqlitePool, query, query::Query, query_as, query_scalar, sqlite::SqliteArguments,
};

use crate::{
    StoreError,
    cells::{self, LOCAL_CELL_SLUG},
    write_tx,
};

/// The Record slug identifying the Organ hosted by this Cell.
pub const LOCAL_ORGAN_SLUG: &str = "local-organ";

const CELL_SURFACE_CONFIG: &str = "lince.cell.surface";

/// The local Organ Record and its Cell surface configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct OrganRecord {
    pub uid: String,
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub base_url: String,
    pub local: bool,
}

/// Ensures the Cell has a local Organ and keeps its configured URL when omitted.
///
/// # Errors
/// Propagates Organ, Cell, and vocabulary setup failures.
pub async fn ensure_local(pool: &SqlitePool, base_url: &str) -> Result<OrganRecord, StoreError> {
    let mut base_url = Cow::Borrowed(normalize_base_url(base_url));
    if base_url.is_empty()
        && let Some(existing) = local(pool).await?
    {
        base_url = Cow::Owned(existing.base_url);
    }
    let now = now().to_rfc3339();
    let existing_uid = query_scalar::<_, String>("SELECT uid FROM record WHERE slug = ?")
        .bind(LOCAL_ORGAN_SLUG)
        .fetch_optional(pool)
        .await?;
    let uid = match existing_uid {
        Some(uid) => {
            query(
                "UPDATE record
                    SET kind = ?,
                        updated_at = ?
                  WHERE uid = ? AND kind <> ?",
            )
            .bind(RecordKind::Organ.as_str())
            .bind(&now)
            .bind(&uid)
            .bind(RecordKind::Organ.as_str())
            .execute(pool)
            .await?;
            uid
        }
        None => {
            let uid = new_uid("r");
            insert_organ(&uid, Some(LOCAL_ORGAN_SLUG), "Local Lince", "", &now)
                .execute(pool)
                .await?;
            uid
        }
    };

    cells::ensure_local(pool, &uid, "this cell").await?;
    cells::set_config(pool, CELL_SURFACE_CONFIG, &surface(&base_url)).await?;
    query("INSERT INTO organ_vocabulary(organ_uid,vocabulary_uid,adopted_at) SELECT ?,uid,created_at FROM lingua WHERE uid IN ('g_local',?) OR owner_organ=? ON CONFLICT DO NOTHING")
        .bind(&uid)
        .bind(DEFAULT_VOCABULARY_UID)
        .bind(&uid)
        .execute(pool)
        .await?;
    local(pool).await?.ok_or(StoreError::RowNotFound)
}

/// Checks whether this Cell already holds Records or vocabulary that prevent joining.
pub async fn holds_own_records(pool: &SqlitePool) -> Result<bool, StoreError> {
    let _presence = Span::new("organ-own-record-presence-await");
    let records: bool = query_scalar(
        "SELECT EXISTS(SELECT 1 FROM record WHERE slug IS NULL OR slug NOT IN (?, ?))",
    )
    .bind(LOCAL_ORGAN_SLUG)
    .bind(LOCAL_CELL_SLUG)
    .fetch_one(pool)
    .await?;
    let meanings: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM concept_source WHERE lingua_uid='g_local') OR EXISTS(SELECT 1 FROM organ_vocabulary v JOIN record o ON o.uid=v.organ_uid WHERE o.slug=? AND (v.publication_uid IS NOT NULL OR v.vocabulary_uid NOT IN ('g_local',?)))")
        .bind(LOCAL_ORGAN_SLUG)
        .bind(DEFAULT_VOCABULARY_UID)
        .fetch_one(pool)
        .await?;
    Ok(records || meanings)
}

/// Adopts another Organ identity on a Cell with no independent records.
///
/// # Errors
/// Refuses replacing an Organ with retained records or vocabulary and propagates
/// transaction failures.
pub async fn adopt_identity(
    pool: &SqlitePool,
    joined_organ_uid: &str,
    base_url: &str,
) -> Result<OrganRecord, StoreError> {
    let Some(current) = local(pool).await? else {
        return Err(sqlx::Error::Protocol(
            "this Cell has no Organ to replace".into(),
        ));
    };
    if current.uid == joined_organ_uid {
        return Ok(current);
    }
    if holds_own_records(pool).await? {
        return Err(sqlx::Error::Protocol(
            "this Cell already holds Records of its own, so joining another \
                      Organ would have to merge two identities. Enrol a device that \
                      has not been used yet."
                .into(),
        ));
    }
    let cell = cells::local(pool)
        .await?
        .ok_or_else(|| sqlx::Error::Protocol("this Cell has no Cell Record".into()))?;

    let mut tx = write_tx(pool).await?;
    query("DELETE FROM sync_op").execute(&mut *tx).await?;
    query("DELETE FROM record_extension WHERE record_uid = ?")
        .bind(&current.uid)
        .execute(&mut *tx)
        .await?;
    query("UPDATE record SET organ_uid = ? WHERE slug = ?")
        .bind(joined_organ_uid)
        .bind(LOCAL_CELL_SLUG)
        .execute(&mut *tx)
        .await?;
    query("DELETE FROM organ_vocabulary WHERE organ_uid=?")
        .bind(&current.uid)
        .execute(&mut *tx)
        .await?;
    query("DELETE FROM record WHERE uid = ?")
        .bind(&current.uid)
        .execute(&mut *tx)
        .await?;
    let now = now().to_rfc3339();
    insert_organ(
        joined_organ_uid,
        Some(LOCAL_ORGAN_SLUG),
        &current.head,
        "",
        &now,
    )
    .execute(&mut *tx)
    .await?;
    query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace)
         DO UPDATE SET fds = excluded.fds, version = version + 1",
    )
    .bind(&cell.uid)
    .bind(CELL_SURFACE_CONFIG)
    .bind(surface(normalize_base_url(base_url)).to_string())
    .execute(&mut *tx)
    .await?;
    query("INSERT INTO organ_vocabulary(organ_uid,vocabulary_uid,adopted_at) SELECT ?,uid,created_at FROM lingua WHERE uid IN ('g_local',?) ON CONFLICT DO NOTHING")
        .bind(joined_organ_uid)
        .bind(DEFAULT_VOCABULARY_UID)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    local(pool).await?.ok_or(StoreError::RowNotFound)
}

pub(super) fn insert_organ(
    uid: &str,
    slug: Option<&str>,
    head: &str,
    body: &str,
    time: &str,
) -> Query<'static, Sqlite, SqliteArguments> {
    query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, '1', 0, ?, ?, ?)",
    )
    .bind(uid)
    .bind(slug)
    .bind(RecordKind::Organ.as_str())
    .bind(head)
    .bind(body)
    .bind(uid)
    .bind(time)
    .bind(time)
}

macro_rules! local_select {
    ($columns:literal) => {
        concat!(
            "SELECT ",
            $columns,
            " FROM record r WHERE r.slug = ? AND r.kind = ? LIMIT 1"
        )
    };
}

/// Reads just the local Organ's title.
pub async fn local_head(pool: &SqlitePool) -> Result<Option<String>, StoreError> {
    let _head = Span::new("organ-local-head-await");
    query_scalar(local_select!("r.head"))
        .bind(LOCAL_ORGAN_SLUG)
        .bind(RecordKind::Organ.as_str())
        .fetch_optional(pool)
        .await
}

/// Reads the local Organ and its advertised surface, if one is configured.
pub async fn local(pool: &SqlitePool) -> Result<Option<OrganRecord>, StoreError> {
    let Some((uid, slug, head, body)) = query_as(local_select!("r.uid, r.slug, r.head, r.body"))
        .bind(LOCAL_ORGAN_SLUG)
        .bind(RecordKind::Organ.as_str())
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };
    let fds = cells::config(pool, CELL_SURFACE_CONFIG)
        .await?
        .unwrap_or_default();
    Ok(Some(OrganRecord {
        uid,
        slug,
        head,
        body,
        base_url: fds
            .get("baseUrl")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        local: fds.get("local").and_then(Value::as_bool).unwrap_or(true),
    }))
}

fn surface(base_url: &str) -> Value {
    json!({
        "baseUrl": base_url,
        "aliases": ["http://127.0.0.1", "http://localhost"],
        "local": true
    })
}

pub(super) fn normalize_base_url(base_url: &str) -> &str {
    base_url.trim().trim_end_matches('/')
}
