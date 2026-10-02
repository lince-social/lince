use super::*;
use store::sqlx::{Sqlite, Transaction};

fn version(document: &Snippet) -> (i64, i64) {
    (
        document
            .anonymous
            .as_ref()
            .map(|authority| authority.generation.as_str())
            .or_else(|| {
                document
                    .profile
                    .as_ref()
                    .map(|authority| authority.generation.as_str())
            })
            .unwrap_or("1")
            .parse()
            .unwrap_or(0),
        document.revision.parse().unwrap_or(0),
    )
}

async fn fits_on(
    tx: &mut Transaction<'_, Sqlite>,
    additional: i64,
    settings: &ServiceSettings,
) -> Result<bool, EngineError> {
    let held:i64 = store::sqlx::query_scalar("SELECT (SELECT COALESCE(SUM(length(CAST(body AS BLOB))),0)*4 FROM social_revision)+(SELECT COUNT(*)*8192 FROM social_profile_authority)+(SELECT COUNT(*)*16384 FROM social_posting_authority)+(SELECT COUNT(*)*512 FROM social_ended_post)+(SELECT COUNT(*)*1024 FROM social_discovery_source)+(SELECT COALESCE(SUM(length(CAST(first_body AS BLOB))+length(CAST(second_body AS BLOB))),0)*4 FROM social_discovery_conflict)").fetch_one(&mut **tx).await?;
    Ok(held.saturating_add(additional).max(0) as u64 <= settings.storage_bytes / 2)
}

pub(super) async fn observe_on(
    tx: &mut Transaction<'_, Sqlite>,
    document: &Snippet,
    hash: &str,
    source: &str,
    settings: &ServiceSettings,
    now: i64,
) -> Result<(), EngineError> {
    if source.len() > 128 || source.chars().any(char::is_control) {
        return Err(invalid("The immediate discovery source label is invalid"));
    }
    let current: Option<(String, String, String)> = store::sqlx::query_as(
        "SELECT hash,state,body FROM social_document WHERE kind='snippet' AND id=?",
    )
    .bind(&document.id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((current_hash, state, body)) = current else {
        return Ok(());
    };
    let held: Snippet = serde_json::from_str(&body)?;
    if current_hash != hash && (state != "conflict" || version(&held) != version(document)) {
        return Ok(());
    }
    let known: bool = store::sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM social_discovery_source WHERE post=? AND hash=? AND source=?)",
    )
    .bind(&document.id)
    .bind(hash)
    .bind(source)
    .fetch_one(&mut **tx)
    .await?;
    if !known {
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_discovery_source")
            .fetch_one(&mut **tx)
            .await?;
        if count >= 8192 {
            store::sqlx::query("DELETE FROM social_discovery_source WHERE (post,hash,source) IN (SELECT post,hash,source FROM social_discovery_source ORDER BY checked_at,post,hash,source LIMIT 1)").execute(&mut **tx).await?;
        }
        let count: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM social_discovery_source WHERE post=?")
                .bind(&document.id)
                .fetch_one(&mut **tx)
                .await?;
        if count >= 8 {
            store::sqlx::query("DELETE FROM social_discovery_source WHERE (post,hash,source) IN (SELECT post,hash,source FROM social_discovery_source WHERE post=? ORDER BY checked_at,hash,source LIMIT 1)").bind(&document.id).execute(&mut **tx).await?;
        }
        if !fits_on(tx, 1024, settings).await? {
            return Ok(());
        }
    }
    store::sqlx::query("INSERT INTO social_discovery_source(post,hash,source,checked_at) VALUES(?,?,?,?) ON CONFLICT(post,hash,source) DO UPDATE SET checked_at=MAX(checked_at,excluded.checked_at)")
        .bind(&document.id).bind(hash).bind(source).bind(now).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn quarantine_on(
    tx: &mut Transaction<'_, Sqlite>,
    document: &Snippet,
    hash: &str,
    settings: &ServiceSettings,
    now: i64,
) -> Result<bool, EngineError> {
    validate_snippet(document, now)?;
    if document_hash("snippet", document)? != hash {
        return Err(invalid(
            "The discovery document hash differs from its signature",
        ));
    }
    let current: Option<(String, String, String)> = store::sqlx::query_as(
        "SELECT body,hash,state FROM social_document WHERE kind='snippet' AND id=?",
    )
    .bind(&document.id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((body, held_hash, state)) = current else {
        return Ok(false);
    };
    let held: Snippet = serde_json::from_str(&body)?;
    if version(&held) != version(document)
        || held.signing_key != document.signing_key
        || held.id != document.id
    {
        return Ok(false);
    }
    if held_hash == hash {
        return Ok(state == "conflict");
    }
    validate_snippet(&held, held.issued_at)?;
    if document_hash("snippet", &held)? != held_hash {
        return Err(invalid(
            "The cached conflict evidence has an invalid content hash",
        ));
    }
    if let Some(authority) = &document.anonymous {
        store::social::anchor_posting_authority_on(tx, authority).await?;
    }
    if let Some(authority) = &document.profile {
        store::social::anchor_profile_authority_on(
            tx,
            authority,
            document.state == PostState::Withdrawn,
        )
        .await?;
    }
    store::sqlx::query("UPDATE social_document SET state=CASE WHEN state='withdrawn' THEN 'withdrawn' ELSE 'conflict' END WHERE kind='snippet' AND id=?")
        .bind(&document.id)
        .execute(&mut **tx)
        .await?;
    store::sqlx::query("DELETE FROM social_search WHERE id=?")
        .bind(&document.id)
        .execute(&mut **tx)
        .await?;
    let known: bool = store::sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM social_discovery_conflict WHERE post=?)",
    )
    .bind(&document.id)
    .fetch_one(&mut **tx)
    .await?;
    if !known {
        let second = serde_json::to_string(document)?;
        let added = body.len().saturating_add(second.len()) as i64;
        let (count,bytes):(i64,i64) = store::sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(first_body AS BLOB))+length(CAST(second_body AS BLOB))),0) FROM social_discovery_conflict").fetch_one(&mut **tx).await?;
        if count < 256
            && bytes.saturating_add(added) <= 8 * 1024 * 1024
            && fits_on(tx, added.saturating_mul(4), settings).await?
        {
            let (generation, revision) = version(document);
            store::sqlx::query("INSERT INTO social_discovery_conflict(post,generation,revision,first_hash,first_body,second_hash,second_body,observed_at) VALUES(?,?,?,?,?,?,?,?)")
                .bind(&document.id).bind(generation).bind(revision).bind(held_hash).bind(body).bind(hash).bind(second).bind(now).execute(&mut **tx).await?;
        }
    }
    Ok(true)
}

pub(super) async fn advance_on(
    tx: &mut Transaction<'_, Sqlite>,
    document: &Snippet,
    hash: &str,
) -> Result<(), EngineError> {
    store::sqlx::query("DELETE FROM social_discovery_conflict WHERE post=?")
        .bind(&document.id)
        .execute(&mut **tx)
        .await?;
    store::sqlx::query("DELETE FROM social_discovery_source WHERE post=? AND hash<>?")
        .bind(&document.id)
        .bind(hash)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

impl Engine {
    pub(super) async fn social_discovery_known_refs(
        &self,
        query: &Search,
        now: i64,
    ) -> Result<Vec<String>, EngineError> {
        let (conflicts, _) = self.social_discovery_conflicts(query).await?;
        let mut known: Vec<String> = conflicts
            .iter()
            .filter_map(|row| {
                Some(format!(
                    "{}:{}",
                    row["post"].as_str()?,
                    row["hash"].as_str()?
                ))
            })
            .collect();
        for row in store::social::search(&self.store.pool, query, now).await? {
            if known.len() >= 50 {
                break;
            }
            if let (Some(post), Some(hash)) = (row["document"]["id"].as_str(), row["hash"].as_str())
            {
                known.push(format!("{post}:{hash}"));
            }
        }
        Ok(known)
    }

    pub(super) async fn social_discovery_details(
        &self,
        results: &mut [Value],
    ) -> Result<(), EngineError> {
        for row in results {
            let observations:Vec<(String,i64)> = store::sqlx::query_as("SELECT source,checked_at FROM social_discovery_source WHERE post=? AND hash=? ORDER BY checked_at DESC,source LIMIT 8")
                .bind(row["document"]["id"].as_str().unwrap_or_default()).bind(row["hash"].as_str().unwrap_or_default()).fetch_all(&self.store.pool).await?;
            row["checked_at"] = observations
                .first()
                .map(|(_, time)| json!(time))
                .unwrap_or(Value::Null);
            row["sources"] = json!(
                observations
                    .into_iter()
                    .map(|(source, time)| json!({"source":source,"checked_at":time}))
                    .collect::<Vec<_>>()
            );
        }
        Ok(())
    }

    pub(super) async fn social_discovery_conflicts(
        &self,
        query: &Search,
    ) -> Result<(Vec<Value>, Option<String>), EngineError> {
        let candidates:Vec<(String,String,Option<String>,Option<i64>)> = store::sqlx::query_as("SELECT d.body,d.hash,c.second_hash,c.observed_at FROM social_document d LEFT JOIN social_discovery_conflict c ON c.post=d.id WHERE d.kind='snippet' AND (d.state='conflict' OR (d.state='withdrawn' AND c.post IS NOT NULL)) AND d.expires_at>? AND d.id>? AND NOT EXISTS(SELECT 1 FROM social_listing_removal WHERE post=d.id) ORDER BY d.id LIMIT 12")
            .bind(nucleus::execution::now().timestamp()).bind(query.after.as_deref().unwrap_or_default()).fetch_all(&self.store.pool).await?;
        let mut rows = Vec::new();
        let mut next = None;
        if candidates.len() == 12 {
            if let Some((body, _, _, _)) = candidates.last() {
                let document: Snippet = serde_json::from_str(body)?;
                next = Some(document.id);
            }
        }
        for (body, hash, second, observed) in candidates {
            let document: Snippet = serde_json::from_str(&body)?;
            if !matches_query(query, &document) {
                continue;
            }
            rows.push(json!({"document":document,"hash":hash,"conflicting_hash":second,"observed_at":observed,"evidence_limited":second.is_none()}));
        }
        self.social_filter_local_results(&mut rows).await?;
        Ok((rows.into_iter().map(|row|json!({"post":row["document"]["id"],"revision":row["document"]["revision"],"hash":row["hash"],"conflicting_hash":row["conflicting_hash"],"observed_at":row["observed_at"],"evidence_limited":row["evidence_limited"],"withdrawn":row["document"]["state"]=="withdrawn"})).collect(),next))
    }
}

fn matches_query(query: &Search, document: &Snippet) -> bool {
    let content = format!("{} {}", document.title, document.text).to_lowercase();
    query
        .text
        .split_whitespace()
        .take(12)
        .all(|word| content.contains(&word.to_lowercase()))
        && query
            .direction
            .is_none_or(|direction| direction == document.direction)
        && (query.language.is_empty() || query.language == document.language)
        && (query.area.is_empty() || query.area == document.area)
        && (query.concept.is_empty() || document.concept.as_deref() == Some(&query.concept))
        && (query.unit.is_empty() || document.unit.as_deref() == Some(&query.unit))
}

pub(super) fn rank_page(query: &Search, results: &mut [Value]) {
    let score = |row: &Value| {
        let title = row["document"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        let body = row["document"]["text"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        let relevance: usize = query
            .text
            .split_whitespace()
            .take(12)
            .map(|word| {
                let word = word.to_lowercase();
                usize::from(title.contains(&word)) * 2 + usize::from(body.contains(&word))
            })
            .sum();
        (
            std::cmp::Reverse(relevance),
            std::cmp::Reverse(row["document"]["issued_at"].as_i64().unwrap_or(0)),
            row["document"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        )
    };
    results.sort_by_cached_key(score);
}
