use crate::StoreError;
use nucleus::social::{Search, Snippet};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

pub async fn anchor_posting_authority_on(
    tx: &mut Transaction<'_, Sqlite>,
    authority: &nucleus::social::PostingAuthority,
) -> Result<(), StoreError> {
    let generation = authority
        .generation
        .parse::<i64>()
        .map_err(|_| StoreError::Protocol("Invalid anonymous authority generation".into()))?;
    let held: Option<(i64, String)> =
        sqlx::query_as("SELECT generation,editor FROM social_posting_authority WHERE owner=?")
            .bind(&authority.owner_key)
            .fetch_optional(&mut **tx)
            .await?;
    if held.as_ref().is_some_and(|(floor, editor)| {
        *floor > generation || *floor == generation && editor != &authority.editor_key
    }) {
        return Err(StoreError::Protocol(
            "This anonymous editing key was revoked or conflicts with retained authority".into(),
        ));
    }
    if held.is_none() {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM social_posting_authority")
            .fetch_one(&mut **tx)
            .await?;
        if count >= 100_000 {
            return Err(StoreError::Protocol(
                "The anonymous authority ledger is full".into(),
            ));
        }
    }
    if held.is_some_and(|(floor, _)| floor < generation) {
        sqlx::query("DELETE FROM social_search WHERE id IN (SELECT id FROM social_document WHERE kind='snippet' AND json_extract(body,'$.anonymous.owner_key')=? AND generation<?)")
            .bind(&authority.owner_key).bind(generation).execute(&mut **tx).await?;
        sqlx::query("UPDATE social_document SET state='revoked' WHERE kind='snippet' AND json_extract(body,'$.anonymous.owner_key')=? AND generation<? AND state NOT IN ('withdrawn','fulfilled')")
            .bind(&authority.owner_key).bind(generation).execute(&mut **tx).await?;
    }
    sqlx::query("INSERT INTO social_posting_authority(owner,editor,generation,expires_at,body) VALUES(?,?,?,?,?) ON CONFLICT(owner) DO UPDATE SET editor=excluded.editor,generation=excluded.generation,body=CASE WHEN excluded.generation>generation OR excluded.expires_at>expires_at THEN excluded.body ELSE body END,expires_at=CASE WHEN excluded.generation>generation THEN excluded.expires_at ELSE MAX(expires_at,excluded.expires_at) END")
        .bind(&authority.owner_key).bind(&authority.editor_key).bind(generation).bind(authority.expires_at)
        .bind(serde_json::to_string(authority).map_err(|error| StoreError::Protocol(error.to_string()))?).execute(&mut **tx).await?;
    Ok(())
}

pub async fn device_state(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<(String, i64)>, StoreError> {
    Ok(
        sqlx::query_as("SELECT body,version FROM social_device_state WHERE id=?")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn put_device_state_on(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
    kind: &str,
    context: &str,
    body: &str,
    previous: Option<i64>,
    now: i64,
) -> Result<(), StoreError> {
    let (count, bytes): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM social_device_state WHERE id<>?",
    ).bind(id).fetch_one(&mut **tx).await?;
    if count >= 4096 || bytes.saturating_add(body.len() as i64) > 64 * 1024 * 1024 {
        return Err(StoreError::Protocol(
            "The device's private transport storage is full".into(),
        ));
    }
    if let Some(version) = previous {
        let next = version
            .checked_add(1)
            .ok_or_else(|| StoreError::Protocol("Private transport version exhausted".into()))?;
        let changed = sqlx::query("UPDATE social_device_state SET body=?,version=?,updated_at=? WHERE id=? AND version=? AND kind=? AND context=?")
            .bind(body).bind(next).bind(now).bind(id).bind(version).bind(kind).bind(context).execute(&mut **tx).await?;
        if changed.rows_affected() != 1 {
            return Err(StoreError::Protocol(
                "Private transport state changed; retry without advancing the saved session".into(),
            ));
        }
    } else {
        sqlx::query(
            "INSERT INTO social_device_state(id,kind,context,body,updated_at) VALUES(?,?,?,?,?)",
        )
        .bind(id)
        .bind(kind)
        .bind(context)
        .bind(body)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn put_snippet_on(
    tx: &mut Transaction<'_, Sqlite>,
    doc: &Snippet,
    hash: &str,
    source: &str,
    now: i64,
) -> Result<bool, StoreError> {
    if let Some(authority) = &doc.anonymous {
        anchor_posting_authority_on(tx, authority).await?;
    }
    if let Some(authority) = &doc.profile {
        anchor_profile_authority_on(
            tx,
            authority,
            doc.state == nucleus::social::PostState::Withdrawn,
        )
        .await?;
    }
    let revision: i64 = doc
        .revision
        .parse()
        .map_err(|_| StoreError::Protocol("Invalid public revision".into()))?;
    let generation = doc
        .anonymous
        .as_ref()
        .map(|authority| authority.generation.as_str())
        .or_else(|| {
            doc.profile
                .as_ref()
                .map(|authority| authority.generation.as_str())
        })
        .unwrap_or("1")
        .parse::<i64>()
        .map_err(|_| StoreError::Protocol("Invalid posting authority generation".into()))?;
    let authority = doc
        .anonymous
        .as_ref()
        .map_or(doc.signing_key.as_str(), |authority| {
            authority.owner_key.as_str()
        });
    let body = serde_json::to_string(doc).map_err(|e| StoreError::Protocol(e.to_string()))?;
    let ended: Option<(String, i64, String, i64)> = sqlx::query_as(
        "SELECT authority,revision,hash,generation FROM social_ended_post WHERE id=?",
    )
    .bind(&doc.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((held_authority, floor, held_hash, held_generation)) = &ended {
        if held_authority != authority {
            return Err(StoreError::Protocol(
                "A withdrawn public post cannot be reopened or taken over".into(),
            ));
        }
        if (generation, revision) < (*held_generation, *floor) {
            return Ok(false);
        }
        if doc.state != nucleus::social::PostState::Withdrawn
            || (generation, revision) == (*held_generation, *floor) && held_hash != hash
        {
            return Err(StoreError::Protocol(
                "A withdrawn public post cannot be reopened or changed at the same revision".into(),
            ));
        }
    }
    let current: Option<(String, i64, String, i64)> = sqlx::query_as(
        "SELECT authority,revision,hash,generation FROM social_document WHERE kind='snippet' AND id=?",
    )
    .bind(&doc.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((held_authority, held, held_hash, held_generation)) = &current {
        if held_authority != authority
            || (*held_generation, *held) == (generation, revision) && held_hash != hash
        {
            return Err(StoreError::Protocol(
                "Conflicting publication authority or revision".into(),
            ));
        }
        if (*held_generation, *held) >= (generation, revision) {
            return Ok(false);
        }
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM social_document")
        .fetch_one(&mut **tx)
        .await?;
    if current.is_none() && count >= 100_000 {
        return Err(StoreError::Protocol("The public cache is full".into()));
    }
    if current.is_none() && ended.is_none() {
        let retained: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM social_ended_post)+(SELECT COUNT(*) FROM social_document WHERE kind='snippet' AND state<>'withdrawn')")
            .fetch_one(&mut **tx).await?;
        if retained >= 100_000 {
            return Err(StoreError::Protocol(
                "The public post and ending ledger is full".into(),
            ));
        }
    }
    if doc.state == nucleus::social::PostState::Withdrawn {
        sqlx::query("DELETE FROM social_revision WHERE kind='snippet' AND id=?")
            .bind(&doc.id)
            .execute(&mut **tx)
            .await?;
        sqlx::query("INSERT INTO social_ended_post(id,authority,revision,hash,generation) VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,hash=excluded.hash,generation=excluded.generation")
            .bind(&doc.id).bind(authority).bind(revision).bind(hash).bind(generation).execute(&mut **tx).await?;
    }
    let state = serde_json::to_value(doc.state).map_err(|e| StoreError::Protocol(e.to_string()))?;
    let direction =
        serde_json::to_value(doc.direction).map_err(|e| StoreError::Protocol(e.to_string()))?;
    sqlx::query("INSERT INTO social_revision(kind,id,hash,body,expires_at) VALUES ('snippet',?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(&doc.id).bind(hash).bind(&body).bind(doc.expires_at).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO social_document(kind,id,authority,revision,hash,body,expires_at,state,title,text,direction,language,area,concept,unit,source) VALUES ('snippet',?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(kind,id) DO UPDATE SET revision=excluded.revision,hash=excluded.hash,body=excluded.body,expires_at=excluded.expires_at,state=excluded.state,title=excluded.title,text=excluded.text,direction=excluded.direction,language=excluded.language,area=excluded.area,concept=excluded.concept,unit=excluded.unit,source=excluded.source")
        .bind(&doc.id).bind(authority).bind(revision).bind(hash).bind(&body).bind(doc.expires_at).bind(state.as_str().unwrap_or_default())
        .bind(&doc.title).bind(&doc.text).bind(direction.as_str().unwrap_or_default()).bind(&doc.language).bind(&doc.area)
        .bind(doc.concept.as_deref().unwrap_or_default()).bind(doc.unit.as_deref().unwrap_or_default()).bind(source).execute(&mut **tx).await?;
    sqlx::query("UPDATE social_document SET generation=? WHERE kind='snippet' AND id=?")
        .bind(generation)
        .bind(&doc.id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM social_search WHERE id=?")
        .bind(&doc.id)
        .execute(&mut **tx)
        .await?;
    if doc.state == nucleus::social::PostState::Active && doc.expires_at > now {
        sqlx::query("INSERT INTO social_search(id,title,text) VALUES (?,?,?)")
            .bind(&doc.id)
            .bind(&doc.title)
            .bind(&doc.text)
            .execute(&mut **tx)
            .await?;
    }
    Ok(true)
}

pub async fn enqueue_on(
    tx: &mut Transaction<'_, Sqlite>,
    kind: &str,
    hash: &str,
    body: &str,
    destinations: &[String],
    expiry: i64,
) -> Result<(), StoreError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| StoreError::Protocol(e.to_string()))?;
    let transport_id;
    let id = if kind == "reply-admission" {
        transport_id = format!(
            "{}:{}",
            value["document"]["mailbox"]
                .as_str()
                .ok_or_else(|| StoreError::Protocol("Invalid reply mailbox".into()))?,
            value["document"]["sender_owner"]
                .as_str()
                .ok_or_else(|| StoreError::Protocol("Invalid reply sender".into()))?
        );
        Some(transport_id.as_str())
    } else if kind == "reply-route" {
        value["document"]["route"]["mailbox"].as_str()
    } else if kind == "reply-control" {
        value["document"]["owner_key"].as_str()
    } else if kind == "reply-ending" {
        value["document"]["id"].as_str()
    } else if matches!(kind, "snippet" | "image") {
        if kind == "image" {
            value["hash"].as_str()
        } else {
            value["id"].as_str()
        }
    } else if kind == "posting-authority" {
        value["authority"]["owner_key"].as_str()
    } else {
        value["authority"]["organ"].as_str()
    }
    .ok_or_else(|| StoreError::Protocol("Invalid publication identity".into()))?;
    for destination in destinations {
        let existing: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM social_publication_job WHERE hash=? AND destination=?)",
        )
        .bind(hash)
        .bind(destination)
        .fetch_one(&mut **tx)
        .await?;
        if existing {
            continue;
        }
        sqlx::query("UPDATE social_publication_job SET state='cancelled',body='' WHERE kind=? AND destination=? AND hash<>? AND state='pending' AND CASE WHEN kind='snippet' THEN json_extract(body,'$.id') WHEN kind='image' THEN json_extract(body,'$.hash') WHEN kind='posting-authority' THEN json_extract(body,'$.authority.owner_key') WHEN kind='reply-route' THEN json_extract(body,'$.document.route.mailbox') WHEN kind='reply-control' THEN json_extract(body,'$.document.owner_key') WHEN kind='reply-ending' THEN json_extract(body,'$.document.id') WHEN kind='reply-admission' THEN json_extract(body,'$.document.mailbox') || ':' || json_extract(body,'$.document.sender_owner') ELSE json_extract(body,'$.authority.organ') END=?")
            .bind(kind).bind(destination).bind(hash).bind(id).execute(&mut **tx).await?;
        let (count, bytes): (i64, i64) = sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))+512),0) FROM social_publication_job")
            .fetch_one(&mut **tx).await?;
        if count >= 10_000
            || bytes.saturating_add(body.len() as i64).saturating_add(512) > 64 * 1024 * 1024
        {
            return Err(StoreError::Protocol(
                "The publication queue is full; remove expired work before publishing".into(),
            ));
        }
        sqlx::query("INSERT INTO social_publication_job(hash,destination,body,kind,expires_at) VALUES (?,?,?,?,?) ON CONFLICT DO NOTHING")
            .bind(hash).bind(destination).bind(body).bind(kind).bind(expiry).execute(&mut **tx).await?;
    }
    Ok(())
}

pub async fn search(
    pool: &SqlitePool,
    query: &Search,
    now: i64,
) -> Result<Vec<serde_json::Value>, StoreError> {
    search_scoped(pool, query, now, None).await
}

pub async fn search_public(
    pool: &SqlitePool,
    query: &Search,
    now: i64,
    host: &str,
) -> Result<Vec<serde_json::Value>, StoreError> {
    search_scoped(pool, query, now, Some(host)).await
}

async fn search_scoped(
    pool: &SqlitePool,
    query: &Search,
    now: i64,
    host: Option<&str>,
) -> Result<Vec<serde_json::Value>, StoreError> {
    let text = query
        .text
        .split_whitespace()
        .take(12)
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    let direction = query
        .direction
        .map(|d| {
            serde_json::to_value(d)
                .unwrap_or_default()
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .unwrap_or_default();
    let sql = if text.is_empty() {
        "SELECT d.body,d.source,d.hash FROM social_document d WHERE d.kind='snippet' AND d.state='active' AND d.expires_at>? AND NOT EXISTS(SELECT 1 FROM social_listing_removal WHERE post=d.id) AND (?='' OR d.direction=?) AND (?='' OR d.language=?) AND (?='' OR d.area=?) AND (?='' OR d.concept=?) AND (?='' OR d.unit=?) AND d.id>? AND (? IS NULL OR EXISTS(SELECT 1 FROM json_each(d.body,'$.destinations') WHERE value=?) OR (json_extract(d.body,'$.redistribute')=1 AND json_array_length(d.body,'$.destinations')>0)) ORDER BY d.id LIMIT 50"
    } else {
        "SELECT d.body,d.source,d.hash FROM social_document d WHERE d.id IN (SELECT id FROM social_search WHERE social_search MATCH ?) AND d.kind='snippet' AND d.state='active' AND d.expires_at>? AND NOT EXISTS(SELECT 1 FROM social_listing_removal WHERE post=d.id) AND (?='' OR d.direction=?) AND (?='' OR d.language=?) AND (?='' OR d.area=?) AND (?='' OR d.concept=?) AND (?='' OR d.unit=?) AND d.id>? AND (? IS NULL OR EXISTS(SELECT 1 FROM json_each(d.body,'$.destinations') WHERE value=?) OR (json_extract(d.body,'$.redistribute')=1 AND json_array_length(d.body,'$.destinations')>0)) ORDER BY d.id LIMIT 50"
    };
    let mut request = sqlx::query(sql);
    if !text.is_empty() {
        request = request.bind(&text);
    }
    request = request
        .bind(now)
        .bind(&direction)
        .bind(&direction)
        .bind(&query.language)
        .bind(&query.language)
        .bind(&query.area)
        .bind(&query.area)
        .bind(&query.concept)
        .bind(&query.concept)
        .bind(&query.unit)
        .bind(&query.unit)
        .bind(query.after.as_deref().unwrap_or_default())
        .bind(host)
        .bind(host);
    let mut result = Vec::new();
    let mut bytes = 0;
    for row in request.fetch_all(pool).await? {
        let body: String = row.get("body");
        bytes += body.len() + 256;
        if bytes > nucleus::social::MAX_FRAME_BYTES - 1024 {
            break;
        }
        let document: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| StoreError::Protocol(e.to_string()))?;
        let source = host
            .map(str::to_owned)
            .unwrap_or_else(|| row.get::<String, _>("source"));
        result.push(serde_json::json!({"document":document,"source":source,"hash":row.get::<String,_>("hash")}));
    }
    Ok(result)
}

pub async fn jobs(pool: &SqlitePool) -> Result<Vec<serde_json::Value>, StoreError> {
    Ok(sqlx::query("SELECT hash,destination,kind,state,error,attempts,receipt FROM social_publication_job ORDER BY rowid DESC LIMIT 100").fetch_all(pool).await?.into_iter().map(|r|
        serde_json::json!({"hash":r.get::<String,_>("hash"),"destination":r.get::<String,_>("destination"),"kind":r.get::<String,_>("kind"),"state":r.get::<String,_>("state"),"error":r.get::<Option<String>,_>("error"),"attempts":r.get::<i64,_>("attempts"),"receipt":r.get::<Option<String>,_>("receipt").and_then(|body|serde_json::from_str::<serde_json::Value>(&body).ok())})).collect())
}

pub async fn spend(
    pool: &SqlitePool,
    source: &str,
    direction: &str,
    bytes: usize,
    limit: u64,
    now: i64,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    spend_on(&mut tx, source, direction, bytes, limit, now).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn spend_on(
    tx: &mut Transaction<'_, Sqlite>,
    source: &str,
    direction: &str,
    bytes: usize,
    limit: u64,
    now: i64,
) -> Result<(), StoreError> {
    if source.len() > 128 || !matches!(direction, "in" | "out") {
        return Err(StoreError::Protocol(
            "Invalid service accounting scope".into(),
        ));
    }
    let window = now.div_euclid(60);
    sqlx::query("DELETE FROM social_service_budget WHERE window<?")
        .bind(window - 1)
        .execute(&mut **tx)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM social_service_budget")
        .fetch_one(&mut **tx)
        .await?;
    if count >= 8192 {
        return Err(StoreError::Protocol(
            "The service source budget is full".into(),
        ));
    }
    for (key, ceiling, work_limit) in [("*", limit, 2048u64), (source, limit / 4, 256)] {
        let row: Option<(i64, i64, i64)> = sqlx::query_as(
            "SELECT window,bytes,work FROM social_service_budget WHERE source=? AND direction=?",
        )
        .bind(key)
        .bind(direction)
        .fetch_optional(&mut **tx)
        .await?;
        let (old_bytes, old_work) = row
            .filter(|(held, _, _)| *held == window)
            .map(|(_, b, w)| (b as u64, w as u64))
            .unwrap_or((0, 0));
        let next = old_bytes.saturating_add(bytes as u64);
        if next > ceiling || old_work >= work_limit {
            return Err(StoreError::Protocol(
                "The service limited requests; try later".into(),
            ));
        }
        sqlx::query("INSERT INTO social_service_budget(source,direction,window,bytes,work) VALUES (?,?,?,?,?) ON CONFLICT(source,direction) DO UPDATE SET window=excluded.window,bytes=excluded.bytes,work=excluded.work")
            .bind(key).bind(direction).bind(window).bind(next as i64).bind((old_work+1) as i64).execute(&mut **tx).await?;
    }
    Ok(())
}

pub async fn anchor_profile_authority_on(
    tx: &mut Transaction<'_, Sqlite>,
    authority: &nucleus::social::Delegation,
    ending: bool,
) -> Result<(), StoreError> {
    let known: Option<String> = sqlx::query_scalar(
        "SELECT public_key FROM identity_key WHERE actor_uid=? AND key_id='ed25519:root:v1'",
    )
    .bind(&authority.organ)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(known) = known {
        let revoked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)",
        )
        .bind(&authority.organ)
        .bind(&authority.root_key)
        .fetch_one(&mut **tx)
        .await?;
        let mut keys = std::collections::HashSet::from([known]);
        let edges: Vec<(String, String)> = sqlx::query_as(
            "SELECT old_key,new_key FROM identity_succession WHERE organ_uid=? LIMIT 1025",
        )
        .bind(&authority.organ)
        .fetch_all(&mut **tx)
        .await?;
        if edges.len() > 1024 {
            return Err(StoreError::Protocol(
                "Refresh the bounded trusted identity chain before accepting public profiles"
                    .into(),
            ));
        }
        let revoked_keys: Vec<String> = sqlx::query_scalar(
            "SELECT revoked_key FROM identity_revocation WHERE organ_uid=? LIMIT 1025",
        )
        .bind(&authority.organ)
        .fetch_all(&mut **tx)
        .await?;
        if revoked_keys.len() > 1024 {
            return Err(StoreError::Protocol(
                "Refresh the bounded trusted identity revocation ledger".into(),
            ));
        }
        let edges = edges
            .into_iter()
            .chain(
                authority
                    .successions
                    .iter()
                    .filter(|edge| !revoked_keys.contains(&edge.old_key))
                    .map(|edge| (edge.old_key.clone(), edge.new_key.clone())),
            )
            .collect::<Vec<_>>();
        for _ in 0..edges.len() {
            let before = keys.len();
            for (old, new) in &edges {
                if keys.contains(old) {
                    keys.insert(new.clone());
                }
            }
            if keys.len() == before {
                break;
            }
        }
        if revoked || !keys.contains(&authority.root_key) {
            return Err(StoreError::Protocol(
                "This public profile conflicts with the Organ's already trusted identity".into(),
            ));
        }
    }
    let generation: i64 = authority
        .generation
        .parse()
        .map_err(|_| StoreError::Protocol("Invalid profile authority generation".into()))?;
    let held: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT root_key,editor_key,generation FROM social_profile_authority WHERE organ=?",
    )
    .bind(&authority.organ)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((root, editor, floor)) = &held {
        if root != &authority.root_key
            && !authority
                .successions
                .iter()
                .any(|change| &change.old_key == root)
        {
            return Err(StoreError::Protocol(
                "The public Organ identity key changed without verified succession".into(),
            ));
        }
        if ending {
            return Ok(());
        }
        if generation < *floor && !ending || generation == *floor && editor != &authority.editor_key
        {
            return Err(StoreError::Protocol(
                "The public-profile editing authority was revoked".into(),
            ));
        }
        if generation < *floor {
            return Ok(());
        }
        if generation > *floor {
            sqlx::query("UPDATE social_document SET state='revoked' WHERE state NOT IN ('withdrawn','fulfilled') AND ((kind='profile' AND id=? AND CAST(json_extract(body,'$.authority.generation') AS INTEGER)<?) OR (kind='snippet' AND json_extract(body,'$.profile.organ')=? AND CAST(json_extract(body,'$.profile.generation') AS INTEGER)<?))")
                .bind(&authority.organ).bind(generation).bind(&authority.organ).bind(generation).execute(&mut **tx).await?;
            sqlx::query("DELETE FROM social_search WHERE id IN (SELECT id FROM social_document WHERE state='revoked')").execute(&mut **tx).await?;
        }
    } else {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM social_profile_authority")
            .fetch_one(&mut **tx)
            .await?;
        if count >= 100_000 {
            return Err(StoreError::Protocol(
                "The profile identity ledger is full".into(),
            ));
        }
        if ending {
            sqlx::query("INSERT INTO social_profile_authority(organ,root_key,editor_key,generation,body,expires_at) VALUES(?,?,?,0,?,?)")
                .bind(&authority.organ).bind(&authority.root_key).bind("")
                .bind(serde_json::to_string(authority).map_err(|error| StoreError::Protocol(error.to_string()))?)
                .bind(authority.expires_at).execute(&mut **tx).await?;
            return Ok(());
        }
    }
    let body = serde_json::to_string(authority)
        .map_err(|error| StoreError::Protocol(error.to_string()))?;
    sqlx::query("INSERT INTO social_profile_authority(organ,root_key,editor_key,generation,body,expires_at) VALUES(?,?,?,?,?,?) ON CONFLICT(organ) DO UPDATE SET root_key=excluded.root_key,editor_key=excluded.editor_key,profile_revision=CASE WHEN excluded.generation>generation THEN 0 ELSE profile_revision END,profile_hash=CASE WHEN excluded.generation>generation THEN NULL ELSE profile_hash END,generation=excluded.generation,body=excluded.body,expires_at=MAX(expires_at,excluded.expires_at)")
        .bind(&authority.organ).bind(&authority.root_key).bind(&authority.editor_key).bind(generation).bind(body).bind(authority.expires_at).execute(&mut **tx).await?;
    Ok(())
}

pub async fn put_profile_on(
    tx: &mut Transaction<'_, Sqlite>,
    doc: &nucleus::social::Profile,
    hash: &str,
    source: &str,
) -> Result<bool, StoreError> {
    anchor_profile_authority_on(tx, &doc.authority, false).await?;
    let body = serde_json::to_string(doc).map_err(|e| StoreError::Protocol(e.to_string()))?;
    let revision: i64 = doc
        .revision
        .parse()
        .map_err(|_| StoreError::Protocol("Invalid profile revision".into()))?;
    let current: Option<(i64, i64, String)> = sqlx::query_as(
        "SELECT CAST(json_extract(body,'$.authority.generation') AS INTEGER),revision,hash FROM social_document WHERE kind='profile' AND id=?",
    )
    .bind(&doc.authority.organ)
    .fetch_optional(&mut **tx)
    .await?;
    let floor: (i64, Option<String>) = sqlx::query_as(
        "SELECT profile_revision,profile_hash FROM social_profile_authority WHERE organ=?",
    )
    .bind(&doc.authority.organ)
    .fetch_one(&mut **tx)
    .await?;
    if revision < floor.0 {
        return Ok(false);
    }
    if let Some((generation, held, held_hash)) = &current {
        if held_hash == hash {
            return Ok(false);
        }
        if doc.authority.generation.parse::<i64>().unwrap_or(0) <= *generation && revision < *held {
            return Ok(false);
        }
    }
    sqlx::query("INSERT INTO social_revision(kind,id,hash,body,expires_at) VALUES ('profile',?,?,?,?) ON CONFLICT DO NOTHING").bind(&doc.authority.organ).bind(hash).bind(&body).bind(doc.expires_at).execute(&mut **tx).await?;
    let conflict = current.as_ref().is_some_and(|(generation, held, head)| {
        doc.authority.generation.parse::<i64>().unwrap_or(0) <= *generation
            && (revision == *held || !doc.parents.contains(head))
    }) || current.is_none()
        && floor.1.as_ref().is_some_and(|head| {
            revision == floor.0 && hash != head || revision > floor.0 && !doc.parents.contains(head)
        });
    if conflict && current.is_some() {
        sqlx::query("UPDATE social_document SET state='conflict' WHERE kind='profile' AND id=?")
            .bind(&doc.authority.organ)
            .execute(&mut **tx)
            .await?;
        return Ok(false);
    }
    sqlx::query("INSERT INTO social_document(kind,id,authority,revision,hash,body,expires_at,state,title,text,direction,language,area,concept,unit,source) VALUES ('profile',?,?,?,?,?,?,?, ?,?,'','',?,'','',?) ON CONFLICT(kind,id) DO UPDATE SET authority=excluded.authority,revision=excluded.revision,hash=excluded.hash,body=excluded.body,expires_at=excluded.expires_at,state=excluded.state,title=excluded.title,text=excluded.text,area=excluded.area,source=excluded.source")
        .bind(&doc.authority.organ).bind(&doc.authority.root_key).bind(revision).bind(hash).bind(&body).bind(doc.expires_at).bind(if conflict { "conflict" } else if doc.state == nucleus::social::PostState::Withdrawn { "withdrawn" } else { "active" }).bind(&doc.fields.name).bind(&doc.fields.description).bind(&doc.fields.area).bind(source).execute(&mut **tx).await?;
    if conflict {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE social_profile_authority SET profile_revision=?,profile_hash=? WHERE organ=?",
    )
    .bind(revision)
    .bind(hash)
    .bind(&doc.authority.organ)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct PublicationJob {
    pub hash: String,
    pub destination: String,
    pub body: String,
    pub kind: String,
    pub expires_at: i64,
}

pub async fn due_publications(
    pool: &SqlitePool,
    now: i64,
) -> Result<Vec<PublicationJob>, StoreError> {
    sqlx::query("UPDATE social_publication_job SET state='expired',body='' WHERE expires_at<=? AND state='pending'").bind(now).execute(pool).await?;
    Ok(sqlx::query("SELECT hash,destination,body,kind,expires_at FROM social_publication_job WHERE state='pending' AND expires_at>? AND next_attempt<=? ORDER BY next_attempt,CASE kind WHEN 'reply-route' THEN 0 WHEN 'reply-control' THEN 1 WHEN 'reply-ending' THEN 2 WHEN 'reply-admission' THEN 3 ELSE 4 END,rowid LIMIT 8")
        .bind(now).bind(now).fetch_all(pool).await?.into_iter().map(|row|PublicationJob {hash:row.get("hash"),destination:row.get("destination"),body:row.get("body"),kind:row.get("kind"),expires_at:row.get("expires_at")}).collect())
}

pub async fn publication_result(
    pool: &SqlitePool,
    job: &PublicationJob,
    error: Option<&str>,
    now: i64,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    publication_result_on(&mut tx, job, error, now).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn publication_result_on(
    tx: &mut Transaction<'_, Sqlite>,
    job: &PublicationJob,
    error: Option<&str>,
    now: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE social_publication_job SET state=CASE WHEN ? IS NULL THEN 'accepted' ELSE 'pending' END,error=?,attempts=MIN(attempts+1,1000000),next_attempt=?+MIN(3600,5*(1<<MIN(attempts,10))),body=CASE WHEN ? IS NULL THEN '' ELSE body END WHERE hash=? AND destination=? AND state='pending'")
        .bind(error).bind(error).bind(now).bind(error).bind(&job.hash).bind(&job.destination).execute(&mut **tx).await?;
    Ok(())
}

pub async fn publication_receipt_on(
    tx: &mut Transaction<'_, Sqlite>,
    job: &PublicationJob,
    accepted_hash: &str,
) -> Result<(), StoreError> {
    if accepted_hash.len() != 64 || !accepted_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StoreError::Protocol(
            "Invalid publication acknowledgement hash".into(),
        ));
    }
    let receipt = serde_json::json!({
        "accepted":true,"service":job.destination,"hash":accepted_hash,
        "expires_at":job.expires_at
    });
    sqlx::query("UPDATE social_publication_job SET receipt=? WHERE hash=? AND destination=?")
        .bind(receipt.to_string())
        .bind(&job.hash)
        .bind(&job.destination)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn fail_publication(
    pool: &SqlitePool,
    job: &PublicationJob,
    reason: &str,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    fail_publication_on(&mut tx, job, reason).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn fail_publication_on(
    tx: &mut Transaction<'_, Sqlite>,
    job: &PublicationJob,
    reason: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE social_publication_job SET state='failed',error=?,body='' WHERE hash=? AND destination=? AND state='pending'")
        .bind(reason).bind(&job.hash).bind(&job.destination).execute(&mut **tx).await?;
    Ok(())
}

pub async fn prune(pool: &SqlitePool, now: i64) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("DELETE FROM social_search WHERE id IN (SELECT id FROM social_document WHERE kind='snippet' AND expires_at<=?)")
        .bind(now).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM social_revision WHERE expires_at<? AND hash NOT IN (SELECT hash FROM social_document)")
        .bind(now - 600).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM social_document WHERE expires_at<?")
        .bind(now - 600)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM social_discovery_source WHERE NOT EXISTS(SELECT 1 FROM social_document WHERE kind='snippet' AND id=social_discovery_source.post)").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM social_discovery_conflict WHERE NOT EXISTS(SELECT 1 FROM social_document WHERE kind='snippet' AND id=social_discovery_conflict.post)").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM social_publication_job WHERE expires_at<?")
        .bind(now - 600)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
