use super::protocol;
use crate::StoreError;
use nucleus::social::{Delegation, PostState, PostingAuthority, Profile};
use sqlx::{Sqlite, Transaction, query, query_as, query_scalar};
use std::collections::HashSet;

const AUTHORITY_CAPACITY: i64 = 100_000;
const IDENTITY_LEDGER_LIMIT: usize = 1024;

/// Anchors an anonymous editing authority and revokes entries below its generation.
///
/// # Errors
///
/// Returns an error for an invalid generation, conflicting or revoked authority,
/// a full authority ledger, or a failed storage operation.
pub async fn anchor_posting_authority_on(
    tx: &mut Transaction<'_, Sqlite>,
    authority: &PostingAuthority,
) -> Result<(), StoreError> {
    let generation = authority
        .generation
        .parse::<i64>()
        .map_err(|_| protocol("Invalid anonymous authority generation"))?;
    let held: Option<(i64, String)> =
        query_as("SELECT generation,editor FROM social_posting_authority WHERE owner=?")
            .bind(&authority.owner_key)
            .fetch_optional(&mut **tx)
            .await?;
    if held.as_ref().is_some_and(|(floor, editor)| {
        *floor > generation || *floor == generation && editor != &authority.editor_key
    }) {
        return Err(protocol(
            "This anonymous editing key was revoked or conflicts with retained authority",
        ));
    }
    if held.is_none() {
        let count: i64 = query_scalar("SELECT COUNT(*) FROM social_posting_authority")
            .fetch_one(&mut **tx)
            .await?;
        if count >= AUTHORITY_CAPACITY {
            return Err(protocol("The anonymous authority ledger is full"));
        }
    }
    if held.is_some_and(|(floor, _)| floor < generation) {
        query("DELETE FROM social_search WHERE id IN (SELECT id FROM social_document WHERE kind='snippet' AND json_extract(body,'$.anonymous.owner_key')=? AND generation<?)")
            .bind(&authority.owner_key).bind(generation).execute(&mut **tx).await?;
        query("UPDATE social_document SET state='revoked' WHERE kind='snippet' AND json_extract(body,'$.anonymous.owner_key')=? AND generation<? AND state NOT IN ('withdrawn','fulfilled')")
            .bind(&authority.owner_key).bind(generation).execute(&mut **tx).await?;
    }
    query("INSERT INTO social_posting_authority(owner,editor,generation,expires_at,body) VALUES(?,?,?,?,?) ON CONFLICT(owner) DO UPDATE SET editor=excluded.editor,generation=excluded.generation,body=CASE WHEN excluded.generation>generation OR excluded.expires_at>expires_at THEN excluded.body ELSE body END,expires_at=CASE WHEN excluded.generation>generation THEN excluded.expires_at ELSE MAX(expires_at,excluded.expires_at) END")
        .bind(&authority.owner_key).bind(&authority.editor_key).bind(generation).bind(authority.expires_at)
        .bind(serde_json::to_string(authority).map_err(protocol)?).execute(&mut **tx).await?;
    Ok(())
}

/// Checks a profile delegation against the trusted identity and retained authority.
///
/// Set `ending` for a withdrawal, which can retain the root without granting
/// permission to publish a live profile.
///
/// # Errors
///
/// Returns an error for invalid or conflicting authority, a full ledger, or a
/// failed storage operation.
pub async fn anchor_profile_authority_on(
    tx: &mut Transaction<'_, Sqlite>,
    authority: &Delegation,
    ending: bool,
) -> Result<(), StoreError> {
    let known: Option<String> = query_scalar(
        "SELECT public_key FROM identity_key WHERE actor_uid=? AND key_id='ed25519:root:v1'",
    )
    .bind(&authority.organ)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(known) = known {
        let revoked: bool = query_scalar(
            "SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)",
        )
        .bind(&authority.organ)
        .bind(&authority.root_key)
        .fetch_one(&mut **tx)
        .await?;
        let mut keys = HashSet::from([known.as_str()]);
        let edges: Vec<(String, String)> = query_as(
            "SELECT old_key,new_key FROM identity_succession WHERE organ_uid=? LIMIT 1025",
        )
        .bind(&authority.organ)
        .fetch_all(&mut **tx)
        .await?;
        if edges.len() > IDENTITY_LEDGER_LIMIT {
            return Err(protocol(
                "Refresh the bounded trusted identity chain before accepting public profiles",
            ));
        }
        let revoked_keys: Vec<String> = query_scalar(
            "SELECT revoked_key FROM identity_revocation WHERE organ_uid=? LIMIT 1025",
        )
        .bind(&authority.organ)
        .fetch_all(&mut **tx)
        .await?;
        if revoked_keys.len() > IDENTITY_LEDGER_LIMIT {
            return Err(protocol(
                "Refresh the bounded trusted identity revocation ledger",
            ));
        }
        let edges = edges
            .iter()
            .map(|(old, new)| (old.as_str(), new.as_str()))
            .chain(
                authority
                    .successions
                    .iter()
                    .filter(|edge| !revoked_keys.contains(&edge.old_key))
                    .map(|edge| (edge.old_key.as_str(), edge.new_key.as_str())),
            );
        while !keys.contains(authority.root_key.as_str()) {
            let before = keys.len();
            for (old, new) in edges.clone() {
                if keys.contains(old) {
                    keys.insert(new);
                }
            }
            if keys.len() == before {
                break;
            }
        }
        if revoked || !keys.contains(authority.root_key.as_str()) {
            return Err(protocol(
                "This public profile conflicts with the Organ's already trusted identity",
            ));
        }
    }
    let generation: i64 = authority
        .generation
        .parse()
        .map_err(|_| protocol("Invalid profile authority generation"))?;
    let held: Option<(String, String, i64)> = query_as(
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
            return Err(protocol(
                "The public Organ identity key changed without verified succession",
            ));
        }
        if ending {
            return Ok(());
        }
        if generation < *floor || generation == *floor && editor != &authority.editor_key {
            return Err(protocol("The public-profile editing authority was revoked"));
        }
        if generation > *floor {
            query("UPDATE social_document SET state='revoked' WHERE state NOT IN ('withdrawn','fulfilled') AND ((kind='profile' AND id=? AND CAST(json_extract(body,'$.authority.generation') AS INTEGER)<?) OR (kind='snippet' AND json_extract(body,'$.profile.organ')=? AND CAST(json_extract(body,'$.profile.generation') AS INTEGER)<?))")
                .bind(&authority.organ).bind(generation).bind(&authority.organ).bind(generation).execute(&mut **tx).await?;
            query("DELETE FROM social_search WHERE id IN (SELECT id FROM social_document WHERE state='revoked')").execute(&mut **tx).await?;
        }
    } else {
        let count: i64 = query_scalar("SELECT COUNT(*) FROM social_profile_authority")
            .fetch_one(&mut **tx)
            .await?;
        if count >= AUTHORITY_CAPACITY {
            return Err(protocol("The profile identity ledger is full"));
        }
        if ending {
            query("INSERT INTO social_profile_authority(organ,root_key,editor_key,generation,body,expires_at) VALUES(?,?,?,0,?,?)")
                .bind(&authority.organ).bind(&authority.root_key).bind("")
                .bind(serde_json::to_string(authority).map_err(protocol)?)
                .bind(authority.expires_at).execute(&mut **tx).await?;
            return Ok(());
        }
    }
    let body = serde_json::to_string(authority).map_err(protocol)?;
    query("INSERT INTO social_profile_authority(organ,root_key,editor_key,generation,body,expires_at) VALUES(?,?,?,?,?,?) ON CONFLICT(organ) DO UPDATE SET root_key=excluded.root_key,editor_key=excluded.editor_key,profile_revision=CASE WHEN excluded.generation>generation THEN 0 ELSE profile_revision END,profile_hash=CASE WHEN excluded.generation>generation THEN NULL ELSE profile_hash END,generation=excluded.generation,body=excluded.body,expires_at=MAX(expires_at,excluded.expires_at)")
        .bind(&authority.organ).bind(&authority.root_key).bind(&authority.editor_key).bind(generation).bind(body).bind(authority.expires_at).execute(&mut **tx).await?;
    Ok(())
}

/// Stores a public profile and its revision, returning whether it became current.
///
/// # Errors
///
/// Returns an error for invalid authority or revision, serialization failures,
/// or a failed storage operation.
pub async fn put_profile_on(
    tx: &mut Transaction<'_, Sqlite>,
    doc: &Profile,
    hash: &str,
    source: &str,
) -> Result<bool, StoreError> {
    anchor_profile_authority_on(tx, &doc.authority, false).await?;
    let authority_generation = doc
        .authority
        .generation
        .parse::<i64>()
        .map_err(|_| protocol("Invalid profile authority generation"))?;
    let body = serde_json::to_string(doc).map_err(protocol)?;
    let revision: i64 = doc
        .revision
        .parse()
        .map_err(|_| protocol("Invalid profile revision"))?;
    let current: Option<(i64, i64, String)> = query_as(
        "SELECT CAST(json_extract(body,'$.authority.generation') AS INTEGER),revision,hash FROM social_document WHERE kind='profile' AND id=?",
    )
    .bind(&doc.authority.organ)
    .fetch_optional(&mut **tx)
    .await?;
    let floor: (i64, Option<String>) = query_as(
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
        if authority_generation <= *generation && revision < *held {
            return Ok(false);
        }
    }
    query("INSERT INTO social_revision(kind,id,hash,body,expires_at) VALUES ('profile',?,?,?,?) ON CONFLICT DO NOTHING").bind(&doc.authority.organ).bind(hash).bind(&body).bind(doc.expires_at).execute(&mut **tx).await?;
    let conflict = current.as_ref().is_some_and(|(generation, held, head)| {
        authority_generation <= *generation && (revision == *held || !doc.parents.contains(head))
    }) || current.is_none()
        && floor.1.as_ref().is_some_and(|head| {
            revision == floor.0 && hash != head || revision > floor.0 && !doc.parents.contains(head)
        });
    if conflict && current.is_some() {
        query("UPDATE social_document SET state='conflict' WHERE kind='profile' AND id=?")
            .bind(&doc.authority.organ)
            .execute(&mut **tx)
            .await?;
        return Ok(false);
    }
    query("INSERT INTO social_document(kind,id,authority,revision,hash,body,expires_at,state,title,text,direction,language,area,concept,unit,source) VALUES ('profile',?,?,?,?,?,?,?, ?,?,'','',?,'','',?) ON CONFLICT(kind,id) DO UPDATE SET authority=excluded.authority,revision=excluded.revision,hash=excluded.hash,body=excluded.body,expires_at=excluded.expires_at,state=excluded.state,title=excluded.title,text=excluded.text,area=excluded.area,source=excluded.source")
        .bind(&doc.authority.organ).bind(&doc.authority.root_key).bind(revision).bind(hash).bind(&body).bind(doc.expires_at).bind(if conflict { "conflict" } else if doc.state == PostState::Withdrawn { "withdrawn" } else { "active" }).bind(&doc.fields.name).bind(&doc.fields.description).bind(&doc.fields.area).bind(source).execute(&mut **tx).await?;
    if conflict {
        return Ok(false);
    }
    query("UPDATE social_profile_authority SET profile_revision=?,profile_hash=? WHERE organ=?")
        .bind(revision)
        .bind(hash)
        .bind(&doc.authority.organ)
        .execute(&mut **tx)
        .await?;
    Ok(true)
}
