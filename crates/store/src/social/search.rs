use super::{direction_label, protocol};
use crate::StoreError;
use nucleus::social::{MAX_FRAME_BYTES, Search, transport::Announcement};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, SqlitePool, query_as, query_scalar};

const PAGE_SIZE: usize = 50;
const TRANSPORT_SCAN_PAGES: usize = 100;
const RESPONSE_PAYLOAD_LIMIT: usize = MAX_FRAME_BYTES - 1024;

/// Searches active announcements in the local cache.
///
/// # Errors
///
/// Returns an error for invalid transport filters, a scan past the search budget,
/// or a read or decoding failure.
pub async fn search(pool: &SqlitePool, query: &Search, now: i64) -> Result<Vec<Value>, StoreError> {
    search_scoped(pool, query, now, None).await
}

/// Searches announcements visible to a public host and identifies that host as their source.
///
/// # Errors
///
/// Returns an error for invalid transport filters, a scan past the search budget,
/// or a read or decoding failure.
pub async fn search_public(
    pool: &SqlitePool,
    query: &Search,
    now: i64,
    host: &str,
) -> Result<Vec<Value>, StoreError> {
    search_scoped(pool, query, now, Some(host)).await
}

async fn search_scoped(
    pool: &SqlitePool,
    query: &Search,
    now: i64,
    host: Option<&str>,
) -> Result<Vec<Value>, StoreError> {
    let Some(filter) = &query.transport else {
        return search_page(pool, query, query.after.as_deref(), now, host).await;
    };
    filter.validate().map_err(StoreError::Protocol)?;
    let mut after = None;
    let mut result = Vec::new();
    let mut bytes = 0;
    for _ in 0..TRANSPORT_SCAN_PAGES {
        let after_id = after.as_deref().or(query.after.as_deref());
        let rows = search_page(pool, query, after_id, now, host).await?;
        let last = rows
            .last()
            .and_then(|row| row.pointer("/document/id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let exhausted = rows.len() < PAGE_SIZE;
        for row in rows {
            let announcement = row
                .pointer("/document/transport")
                .and_then(|value| Announcement::deserialize(value).ok());
            if filter.matches(announcement.as_ref()) {
                bytes += nucleus::json::encoded_len(&row).map_err(protocol)?;
                if bytes > RESPONSE_PAYLOAD_LIMIT {
                    return Ok(result);
                }
                result.push(row);
                if result.len() == PAGE_SIZE {
                    return Ok(result);
                }
            }
        }
        if exhausted || last.is_none() || last.as_deref() == after_id {
            return Ok(result);
        }
        after = last;
    }
    Err(protocol(
        "Narrow the transport search by area, words or time; its 5000-announcement scan limit was reached",
    ))
}

async fn search_page(
    pool: &SqlitePool,
    query: &Search,
    after: Option<&str>,
    now: i64,
    host: Option<&str>,
) -> Result<Vec<Value>, StoreError> {
    let words = query
        .text
        .split_whitespace()
        .take(12)
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    let text = if words.is_empty() {
        String::new()
    } else {
        format!("{{title text}} : ({words})")
    };
    let direction = query.direction.map_or("", direction_label);
    let filters = "d.kind='snippet' AND d.state='active' AND d.expires_at>?2 AND NOT EXISTS(SELECT 1 FROM social_listing_removal WHERE post=d.id) AND (?3='' OR d.direction=?3) AND (?4='' OR d.language=?4) AND (?5='' OR d.area=?5) AND (?6='' OR d.concept=?6) AND (?7='' OR d.unit=?7) AND d.id>?8";
    let visibility = "(?9 IS NULL OR EXISTS(SELECT 1 FROM json_each(d.body,'$.destinations') WHERE value=?9) OR (json_extract(d.body,'$.redistribute')=1 AND json_array_length(d.body,'$.destinations')>0))";
    let probe = !text.is_empty()
        && query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM social_document INDEXED BY social_discovery_conflict_page WHERE kind='snippet' AND state='active' LIMIT 1 OFFSET 10000)")
            .fetch_one(pool)
            .await?;
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for bounded in [true, false] {
        if bounded && !probe {
            continue;
        }
        let sql = if bounded {
            format!(
                "WITH candidates AS MATERIALIZED (SELECT d.id FROM social_document d INDEXED BY social_discovery_conflict_page WHERE d.kind='snippet' AND d.state='active' AND d.id>?8 ORDER BY d.id LIMIT 256) SELECT d.body,d.source,d.hash FROM social_document d INDEXED BY social_discovery_conflict_page WHERE {filters} AND d.id<=(SELECT MAX(id) FROM candidates) AND {visibility} AND EXISTS(SELECT 1 FROM social_search WHERE social_search MATCH ?1 AND id MATCH ('\"' || replace(substr(d.id,6),'\"','\"\"') || '\"')) ORDER BY d.id LIMIT {PAGE_SIZE}"
            )
        } else {
            let matching = if text.is_empty() {
                "?1=''"
            } else {
                "d.id IN (SELECT id FROM social_search WHERE social_search MATCH ?1)"
            };
            format!(
                "SELECT d.body,d.source,d.hash FROM social_document d WHERE {filters} AND {visibility} AND {matching} ORDER BY d.id LIMIT {PAGE_SIZE}"
            )
        };
        rows = query_as(AssertSqlSafe(sql.as_str()))
            .bind(&text)
            .bind(now)
            .bind(direction)
            .bind(&query.language)
            .bind(&query.area)
            .bind(&query.concept)
            .bind(&query.unit)
            .bind(after.unwrap_or_default())
            .bind(host)
            .fetch_all(pool)
            .await?;
        if rows.len() == PAGE_SIZE {
            break;
        }
    }

    let mut result = Vec::new();
    let mut bytes = 0;
    for (body, source, hash) in rows {
        bytes += body.len() + 256;
        if bytes > RESPONSE_PAYLOAD_LIMIT {
            break;
        }
        let document: Value = serde_json::from_str(&body).map_err(protocol)?;
        let source = host.unwrap_or(&source);
        result.push(json!({"document":document,"source":source,"hash":hash}));
    }
    Ok(result)
}
