use super::{anchor_posting_authority_on, anchor_profile_authority_on, direction_label, protocol};
use crate::StoreError;
use nucleus::social::{PostState, Snippet};
use sqlx::{Sqlite, Transaction, query, query_as, query_scalar};

const CACHE_CAPACITY: i64 = 100_000;

/// Stores a public announcement and refreshes any active search entry.
///
/// Returns false for a revision already covered by a newer retained revision.
///
/// # Errors
///
/// Returns an error for invalid authority or revision, a full cache, serialization
/// failures, or a failed storage operation.
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
        anchor_profile_authority_on(tx, authority, doc.state == PostState::Withdrawn).await?;
    }
    let revision: i64 = doc
        .revision
        .parse()
        .map_err(|_| protocol("Invalid public revision"))?;
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
        .map_err(|_| protocol("Invalid posting authority generation"))?;
    let authority = doc
        .anonymous
        .as_ref()
        .map_or(doc.signing_key.as_str(), |authority| {
            authority.owner_key.as_str()
        });
    let body = serde_json::to_string(doc).map_err(protocol)?;
    let ended: Option<(String, i64, String, i64)> =
        query_as("SELECT authority,revision,hash,generation FROM social_ended_post WHERE id=?")
            .bind(&doc.id)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some((held_authority, floor, held_hash, held_generation)) = &ended {
        if held_authority != authority {
            return Err(protocol(
                "A withdrawn public post cannot be reopened or taken over",
            ));
        }
        if (generation, revision) < (*held_generation, *floor) {
            return Ok(false);
        }
        if doc.state != PostState::Withdrawn
            || (generation, revision) == (*held_generation, *floor) && held_hash != hash
        {
            return Err(protocol(
                "A withdrawn public post cannot be reopened or changed at the same revision",
            ));
        }
    }
    let current: Option<(String, i64, String, i64)> = query_as(
        "SELECT authority,revision,hash,generation FROM social_document WHERE kind='snippet' AND id=?",
    )
    .bind(&doc.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((held_authority, held, held_hash, held_generation)) = &current {
        if held_authority != authority
            || (*held_generation, *held) == (generation, revision) && held_hash != hash
        {
            return Err(protocol("Conflicting publication authority or revision"));
        }
        if (*held_generation, *held) >= (generation, revision) {
            return Ok(false);
        }
    }
    let count: i64 = query_scalar("SELECT COUNT(*) FROM social_document")
        .fetch_one(&mut **tx)
        .await?;
    if current.is_none() && count >= CACHE_CAPACITY {
        return Err(protocol("The public cache is full"));
    }
    if current.is_none() && ended.is_none() {
        let retained: i64 = query_scalar("SELECT (SELECT COUNT(*) FROM social_ended_post)+(SELECT COUNT(*) FROM social_document WHERE kind='snippet' AND state<>'withdrawn')")
            .fetch_one(&mut **tx).await?;
        if retained >= CACHE_CAPACITY {
            return Err(protocol("The public post and ending ledger is full"));
        }
    }
    if doc.state == PostState::Withdrawn {
        query("DELETE FROM social_revision WHERE kind='snippet' AND id=?")
            .bind(&doc.id)
            .execute(&mut **tx)
            .await?;
        query("INSERT INTO social_ended_post(id,authority,revision,hash,generation) VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,hash=excluded.hash,generation=excluded.generation")
            .bind(&doc.id).bind(authority).bind(revision).bind(hash).bind(generation).execute(&mut **tx).await?;
    }
    let state = match doc.state {
        PostState::Active => "active",
        PostState::Paused => "paused",
        PostState::Fulfilled => "fulfilled",
        PostState::Withdrawn => "withdrawn",
    };
    query("INSERT INTO social_revision(kind,id,hash,body,expires_at) VALUES ('snippet',?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(&doc.id).bind(hash).bind(&body).bind(doc.expires_at).execute(&mut **tx).await?;
    query("INSERT INTO social_document(kind,id,authority,revision,hash,body,expires_at,state,title,text,direction,language,area,concept,unit,source) VALUES ('snippet',?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(kind,id) DO UPDATE SET revision=excluded.revision,hash=excluded.hash,body=excluded.body,expires_at=excluded.expires_at,state=excluded.state,title=excluded.title,text=excluded.text,direction=excluded.direction,language=excluded.language,area=excluded.area,concept=excluded.concept,unit=excluded.unit,source=excluded.source")
        .bind(&doc.id).bind(authority).bind(revision).bind(hash).bind(&body).bind(doc.expires_at).bind(state)
        .bind(&doc.title).bind(&doc.text).bind(direction_label(doc.direction)).bind(&doc.language).bind(&doc.area)
        .bind(doc.concept.as_deref().unwrap_or_default()).bind(doc.unit.as_deref().unwrap_or_default()).bind(source).execute(&mut **tx).await?;
    query("UPDATE social_document SET generation=? WHERE kind='snippet' AND id=?")
        .bind(generation)
        .bind(&doc.id)
        .execute(&mut **tx)
        .await?;
    query("DELETE FROM social_search WHERE id=?")
        .bind(&doc.id)
        .execute(&mut **tx)
        .await?;
    if doc.state == PostState::Active && doc.expires_at > now {
        query("INSERT INTO social_search(id,title,text) VALUES (?,?,?)")
            .bind(&doc.id)
            .bind(&doc.title)
            .bind(&doc.text)
            .execute(&mut **tx)
            .await?;
    }
    Ok(true)
}
