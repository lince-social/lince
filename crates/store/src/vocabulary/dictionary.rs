use super::{
    definitions_on, invalid, language_tag, term_text,
    terms::decode_term,
    versions::{save_definitions_on, validate_definitions},
};
use crate::{
    StoreError,
    linguas::LOCAL_UID,
    sync_ops::{OpKind, log_local_tx},
    write_tx,
};
use nucleus::{
    execution::now,
    valid_uid,
    vocabulary::{Dictionary, Source},
};
use sqlx::{
    Row, Sqlite, SqliteConnection, SqlitePool, Transaction, query, query_as, query_scalar,
    sqlite::SqliteRow,
};
use std::collections::{HashMap, HashSet};

const MAX_SHARED_TERMS: usize = 512;
const MAX_DEFINITION_BYTES: usize = 16_384;

fn decode_source(row: &SqliteRow) -> Result<Source, StoreError> {
    Ok(Source {
        concept_uid: row.try_get("concept_uid")?,
        vocabulary_uid: row.try_get("lingua_uid")?,
        original_name: row.try_get("original_name")?,
        original_language: row.try_get("original_language")?,
        definition: row.try_get("definition")?,
        derived_from: row.try_get("derived_from")?,
    })
}

/// Reads Concept source definitions keyed by Concept UID.
///
/// # Errors
///
/// Returns an error if the read or source decoding fails.
pub async fn sources(pool: &SqlitePool) -> Result<HashMap<String, Source>, StoreError> {
    query("SELECT * FROM concept_source")
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| {
            let source = decode_source(row)?;
            Ok((source.concept_uid.clone(), source))
        })
        .collect()
}

/// Checks that a proposed source parent keeps the ancestry acyclic and bounded.
///
/// # Errors
///
/// Returns an error for a cycle, 64 or more ancestors beyond the proposed parent, or a failed lookup.
pub async fn validate_source_parent_on(
    connection: &mut SqliteConnection,
    concept: &str,
    parent: Option<&str>,
) -> Result<(), StoreError> {
    if let Some(parent) = parent {
        let invalid_ancestry: bool = query_scalar("WITH RECURSIVE ancestry(uid,depth) AS (SELECT ?,0 UNION ALL SELECT s.derived_from,a.depth+1 FROM concept_source s JOIN ancestry a ON s.concept_uid=a.uid WHERE s.derived_from IS NOT NULL AND a.depth<64) SELECT EXISTS(SELECT 1 FROM ancestry WHERE uid=? OR depth=64)")
            .bind(parent).bind(concept).fetch_one(connection).await?;
        if invalid_ancestry {
            return Err(invalid(
                "Source ancestry must be acyclic and have at most 64 links",
            ));
        }
    }
    Ok(())
}

/// Reads a Concept’s source, shared terms, and definition versions.
///
/// # Errors
///
/// Returns an error if its source is missing or a query or decoding step fails.
pub async fn dictionary_on(
    connection: &mut SqliteConnection,
    concept: &str,
) -> Result<Dictionary, StoreError> {
    let row = query("SELECT * FROM concept_source WHERE concept_uid=?")
        .bind(concept)
        .fetch_optional(&mut *connection)
        .await?;
    let Some(row) = row else {
        return Err(invalid("Concept definition unavailable"));
    };
    let source = decode_source(&row)?;
    let terms =
        query("SELECT * FROM concept_term WHERE concept_uid=? AND owner_uid IS NULL ORDER BY uid")
            .bind(concept)
            .fetch_all(&mut *connection)
            .await?
            .iter()
            .map(decode_term)
            .collect::<Result<_, _>>()?;
    let definitions = definitions_on(connection, concept).await?;
    Ok(Dictionary {
        source,
        terms,
        definitions,
    })
}

/// Advances the Vocabulary revision and records the shared dictionary in the sync log.
///
/// # Errors
///
/// Returns an error if storage or dictionary encoding fails.
pub async fn log_shared_on(
    tx: &mut Transaction<'_, Sqlite>,
    concept: &str,
) -> Result<(), StoreError> {
    query("UPDATE lingua SET revision=revision+1 WHERE uid IN (SELECT lingua_uid FROM lingua_concept WHERE concept_uid=?)")
        .bind(concept).execute(&mut **tx).await?;
    let dictionary = dictionary_on(tx, concept).await?;
    log_local_tx(
        tx,
        "concept",
        concept,
        "vocabulary",
        OpKind::Set,
        Some(
            serde_json::to_string(&dictionary)
                .map_err(|error| StoreError::Protocol(error.to_string()))?,
        ),
    )
    .await?;
    Ok(())
}

/// Applies a dictionary in a write transaction with the supplied origin.
///
/// # Errors
///
/// Returns an error for invalid data, conflicting source or term identity, or failed storage.
pub async fn apply_dictionary(
    pool: &SqlitePool,
    dictionary: &Dictionary,
    origin: &str,
    local: bool,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    apply_dictionary_on(&mut tx, dictionary, origin, local).await?;
    tx.commit().await
}

/// Applies a dictionary within the supplied connection, enforcing remote origin ownership.
///
/// # Errors
///
/// Returns an error for invalid data, conflicting source or term identity, or failed storage.
pub async fn apply_dictionary_on(
    connection: &mut SqliteConnection,
    dictionary: &Dictionary,
    origin: &str,
    local: bool,
) -> Result<(), StoreError> {
    validate_dictionary(dictionary)?;
    let source = &dictionary.source;
    let terms_seen: HashSet<_> = dictionary
        .terms
        .iter()
        .map(|term| term.uid.as_str())
        .collect();
    let existing: Option<(Option<String>, bool)> = query_as("SELECT origin_organ, EXISTS(SELECT 1 FROM concept_source s WHERE s.concept_uid=c.uid) FROM concept c WHERE uid=?").bind(&source.concept_uid).fetch_optional(&mut *connection).await?;
    if !local
        && existing
            .as_ref()
            .is_some_and(|(owner, defined)| *defined && owner.as_deref() != Some(origin))
    {
        return Ok(());
    }
    if let Some(original) = query("SELECT * FROM concept_source WHERE concept_uid=?")
        .bind(&source.concept_uid)
        .fetch_optional(&mut *connection)
        .await?
        && decode_source(&original)? != *source
    {
        return Err(invalid(
            "A Vocabulary update cannot replace an existing source meaning",
        ));
    }
    query(
        "INSERT OR IGNORE INTO concept(uid,canonical_name,origin_organ,created_at) VALUES(?,?,?,?)",
    )
    .bind(&source.concept_uid)
    .bind(&source.original_name)
    .bind(origin)
    .bind(now().to_rfc3339())
    .execute(&mut *connection)
    .await?;
    query("UPDATE concept SET canonical_name=?,origin_organ=? WHERE uid=? AND NOT EXISTS(SELECT 1 FROM concept_source s WHERE s.concept_uid=concept.uid)")
        .bind(&source.original_name).bind(origin).bind(&source.concept_uid).execute(&mut *connection).await?;
    query("INSERT OR IGNORE INTO lingua(uid,name,visibility,created_at) VALUES(?,?,'shared',?)")
        .bind(&source.vocabulary_uid)
        .bind(&source.vocabulary_uid)
        .bind(now().to_rfc3339())
        .execute(&mut *connection)
        .await?;
    if let Some(parent) = &source.derived_from {
        validate_source_parent_on(connection, &source.concept_uid, Some(parent)).await?;
        let known: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM concept WHERE uid=?)")
            .bind(parent)
            .fetch_one(&mut *connection)
            .await?;
        if !valid_uid(parent, "c") {
            return Err(invalid("Invalid source Concept identity"));
        }
        if !known {
            query("INSERT INTO concept(uid,canonical_name,created_at) VALUES(?,?,?)")
                .bind(parent)
                .bind(format!("concept:{parent}"))
                .bind(now().to_rfc3339())
                .execute(&mut *connection)
                .await?;
        }
    }
    query("INSERT OR IGNORE INTO concept_source(concept_uid,lingua_uid,original_name,original_language,definition,derived_from) VALUES(?,?,?,?,?,?)")
        .bind(&source.concept_uid).bind(&source.vocabulary_uid).bind(&source.original_name).bind(&source.original_language).bind(&source.definition).bind(&source.derived_from).execute(&mut *connection).await?;
    query("INSERT INTO lingua_concept(lingua_uid,concept_uid,adopted_at) VALUES(?,?,?) ON CONFLICT DO NOTHING")
        .bind(&source.vocabulary_uid).bind(&source.concept_uid).bind(now().to_rfc3339()).execute(&mut *connection).await?;
    for incoming in &dictionary.terms {
        let belongs: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM concept_term WHERE uid=? AND (concept_uid!=? OR owner_uid IS NOT NULL))")
            .bind(&incoming.uid).bind(&source.concept_uid).fetch_one(&mut *connection).await?;
        if belongs {
            return Err(invalid(
                "A term identity cannot move to another Concept or owner",
            ));
        }
    }
    let existing_terms: Vec<String> =
        query_scalar("SELECT uid FROM concept_term WHERE concept_uid=? AND owner_uid IS NULL")
            .bind(&source.concept_uid)
            .fetch_all(&mut *connection)
            .await?;
    let existing_terms: HashSet<_> = existing_terms.into_iter().collect();
    if existing_terms.len()
        + dictionary
            .terms
            .iter()
            .filter(|term| !existing_terms.contains(&term.uid))
            .count()
        > MAX_SHARED_TERMS
    {
        return Err(invalid("A Concept can retain at most 512 shared terms"));
    }
    for uid in existing_terms {
        if terms_seen.contains(uid.as_str()) {
            continue;
        }
        query("UPDATE concept_term SET retired=1,preferred=0,revision=revision+1 WHERE uid=? AND retired=0")
            .bind(uid).execute(&mut *connection).await?;
    }
    query("UPDATE concept_term SET preferred=0,revision=revision+1 WHERE concept_uid=? AND owner_uid IS NULL AND preferred=1 AND retired=0")
        .bind(&source.concept_uid).execute(&mut *connection).await?;
    for incoming in &dictionary.terms {
        let language = language_tag(&incoming.language)?;
        let name = term_text(&incoming.name)?;
        query("UPDATE concept_term SET retired=1,preferred=0,revision=revision+1 WHERE concept_uid=? AND language=? AND name=? AND owner_uid IS NULL AND retired=0 AND uid!=?")
            .bind(&source.concept_uid).bind(&language).bind(&name).bind(&incoming.uid).execute(&mut *connection).await?;
        query("INSERT INTO concept_term(uid,concept_uid,language,name,preferred,retired,revision) VALUES(?,?,?,?,?,?,?) ON CONFLICT(uid) DO UPDATE SET language=excluded.language,name=excluded.name,preferred=excluded.preferred,retired=excluded.retired,revision=MAX(concept_term.revision+1,excluded.revision)")
            .bind(&incoming.uid).bind(&incoming.concept_uid).bind(language).bind(name).bind(incoming.preferred).bind(incoming.retired).bind(incoming.revision).execute(&mut *connection).await?;
    }
    save_definitions_on(connection, dictionary).await?;
    Ok(())
}

/// Validates shared dictionary sources, terms, and definition versions.
///
/// # Errors
///
/// Returns an error for malformed entries or conflicting term identities, names, or preferences.
pub fn validate_dictionary(dictionary: &Dictionary) -> Result<(), StoreError> {
    validate_definitions(dictionary)?;
    let source = &dictionary.source;
    if !valid_uid(&source.concept_uid, "c")
        || (!valid_uid(&source.vocabulary_uid, "g") && source.vocabulary_uid != LOCAL_UID)
        || dictionary.terms.len() > MAX_SHARED_TERMS
        || source.definition.len() > MAX_DEFINITION_BYTES
    {
        return Err(invalid("Invalid Vocabulary dictionary"));
    }
    term_text(&source.original_name)?;
    language_tag(&source.original_language)?;
    let mut terms_seen = HashSet::new();
    let mut preferred = HashSet::new();
    let mut names_seen = HashSet::new();
    for term in &dictionary.terms {
        if term.concept_uid != source.concept_uid
            || term.owner_uid.is_some()
            || !valid_uid(&term.uid, "ct")
            || term.revision < 1
            || !terms_seen.insert(term.uid.as_str())
        {
            return Err(invalid("Invalid public Vocabulary term identity"));
        }
        let name = term_text(&term.name)?;
        language_tag(&term.language)?;
        if !term.retired && !names_seen.insert((term.language.to_ascii_lowercase(), name)) {
            return Err(invalid("Duplicate Vocabulary terms"));
        }
        if term.preferred && !term.retired && !preferred.insert(term.language.to_ascii_lowercase())
        {
            return Err(invalid("Several preferred terms in one language"));
        }
    }
    Ok(())
}
