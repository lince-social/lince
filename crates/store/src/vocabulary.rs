mod features;
mod preferences;
mod publications;
mod translations;
mod versions;
mod references;

pub use features::*;
pub use preferences::*;
pub use publications::*;
pub use translations::*;
pub use versions::*;
pub use references::*;

use crate::StoreError;
use nucleus::vocabulary::{SAVED_LOCATION_ROLE_UID, Source, Term};
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::collections::{HashMap, HashSet};
use unicode_normalization::UnicodeNormalization;

fn invalid(message: &str) -> StoreError {
    StoreError::Protocol(message.into())
}

pub fn term_text(text: &str) -> Result<String, StoreError> {
    let text: String = text.trim().nfc().collect();
    if text.is_empty() || text.chars().count() > 256 || text.chars().any(char::is_control) {
        return Err(invalid(
            "Enter a term of 1–256 characters without control characters",
        ));
    }
    Ok(text)
}

pub fn language_tag(language: &str) -> Result<String, StoreError> {
    let language = language.trim().to_ascii_lowercase();
    if !language.is_empty()
        && (language.len() > 63
            || !language.split('-').enumerate().all(|(index, part)| {
                !part.is_empty()
                    && part.len() <= 8
                    && part.bytes().all(|byte| {
                        byte.is_ascii_alphabetic() || index > 0 && byte.is_ascii_digit()
                    })
            }))
    {
        return Err(invalid(
            "Choose a language tag such as en, pt-BR or el-Latn, or leave it empty for all languages",
        ));
    }
    Ok(language)
}

fn term(row: sqlx::sqlite::SqliteRow) -> Term {
    Term {
        uid: row.get("uid"),
        concept_uid: row.get("concept_uid"),
        language: row.get("language"),
        name: row.get("name"),
        owner_uid: row.get("owner_uid"),
        preferred: row.get("preferred"),
        retired: row.get("retired"),
        revision: row.get("revision"),
    }
}

pub async fn terms(pool: &SqlitePool, person: Option<&str>) -> Result<Vec<Term>, StoreError> {
    Ok(sqlx::query("SELECT * FROM concept_term WHERE retired=0 AND (owner_uid IS NULL OR owner_uid=?) ORDER BY concept_uid, language, name, uid")
        .bind(person).fetch_all(pool).await?.into_iter().map(term).collect())
}

pub async fn terms_on(
    connection: &mut SqliteConnection,
    person: Option<&str>,
) -> Result<Vec<Term>, StoreError> {
    Ok(sqlx::query("SELECT * FROM concept_term WHERE retired=0 AND (owner_uid IS NULL OR owner_uid=?) ORDER BY concept_uid,language,name,uid")
        .bind(person).fetch_all(connection).await?.into_iter().map(term).collect())
}

pub async fn get_term(pool: &SqlitePool, uid: &str) -> Result<Option<Term>, StoreError> {
    Ok(sqlx::query("SELECT * FROM concept_term WHERE uid=?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(term))
}

pub async fn sources(pool: &SqlitePool) -> Result<HashMap<String, Source>, StoreError> {
    Ok(sqlx::query("SELECT * FROM concept_source")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            let source = Source {
                concept_uid: row.get("concept_uid"),
                vocabulary_uid: row.get("lingua_uid"),
                original_name: row.get("original_name"),
                original_language: row.get("original_language"),
                definition: row.get("definition"),
                derived_from: row.get("derived_from"),
            };
            (source.concept_uid.clone(), source)
        })
        .collect())
}

pub async fn validate_source_parent_on(
    connection: &mut SqliteConnection,
    concept: &str,
    parent: Option<&str>,
) -> Result<(), StoreError> {
    if let Some(parent) = parent {
        let invalid_ancestry: bool = sqlx::query_scalar("WITH RECURSIVE ancestry(uid,depth) AS (SELECT ?,0 UNION ALL SELECT s.derived_from,a.depth+1 FROM concept_source s JOIN ancestry a ON s.concept_uid=a.uid WHERE s.derived_from IS NOT NULL AND a.depth<64) SELECT EXISTS(SELECT 1 FROM ancestry WHERE uid=? OR depth=64)")
            .bind(parent).bind(concept).fetch_one(connection).await?;
        if invalid_ancestry {
            return Err(invalid(
                "Source ancestry must be acyclic and have at most 64 links",
            ));
        }
    }
    Ok(())
}

pub async fn resolve(
    pool: &SqlitePool,
    token: &str,
    person: Option<&str>,
) -> Result<Option<String>, StoreError> {
    let token = token.trim().trim_start_matches('#').trim_start_matches('^');
    if let Some(feature) = token.strip_prefix("feature:") {
        return feature_resolve(pool, feature).await;
    }
    if let Some(uid) = sqlx::query_scalar("SELECT uid FROM concept WHERE uid=?")
        .bind(token)
        .fetch_optional(pool)
        .await?
    {
        return Ok(Some(uid));
    }
    let name: String = token.nfc().collect();
    let owner = terminology_owner(person);
    let mut candidates: Vec<String> = sqlx::query_scalar("SELECT DISTINCT t.concept_uid FROM concept_term t WHERE t.name=? AND t.retired=0 AND (t.owner_uid IS NULL OR t.owner_uid=?) AND NOT EXISTS(SELECT 1 FROM terminology_hidden h WHERE h.term_uid=t.uid AND h.owner_key=? AND h.context=COALESCE((SELECT context FROM terminology_context WHERE owner_key=?),'General')) UNION SELECT p.concept_uid FROM terminology_choice p JOIN concept_source s ON s.concept_uid=p.concept_uid WHERE p.owner_key=? AND p.context=COALESCE((SELECT context FROM terminology_context WHERE owner_key=?),'General') AND COALESCE(p.wording,s.original_name)=?")
        .bind(&name).bind(person).bind(owner).bind(owner).bind(owner).bind(owner).bind(&name).fetch_all(pool).await?;
    candidates.extend(sqlx::query_scalar::<_,String>("SELECT uid FROM concept c WHERE canonical_name=? AND NOT EXISTS(SELECT 1 FROM concept_source s WHERE s.concept_uid=c.uid)").bind(&name).fetch_all(pool).await?);
    candidates.sort();
    candidates.dedup();
    if candidates.len() > 1 {
        return Err(StoreError::Protocol(format!(
            "`{token}` names several concepts ({}); choose a Concept identity",
            candidates.join(", ")
        )));
    }
    Ok(candidates.pop())
}

pub async fn display_names(
    pool: &SqlitePool,
    person: Option<&str>,
) -> Result<HashMap<String, String>, StoreError> {
    let personal_language: Option<String> =
        sqlx::query_scalar("SELECT language FROM person_terminology WHERE person_uid=?")
            .bind(person)
            .fetch_optional(pool)
            .await?;
    let language = match personal_language {
        Some(language) => language,
        None => crate::config::get(pool)
            .await?
            .map_or_else(|| "en".into(), |config| config.language),
    };
    let language = language_tag(&language).unwrap_or_else(|_| "en".into());
    let mut locales = vec![language.clone()];
    let mut current = language.as_str();
    while let Some((parent, _)) = current.rsplit_once('-') {
        locales.push(parent.to_string());
        current = parent;
    }
    let mut grouped = HashMap::<String, Vec<Term>>::new();
    for term in terms(pool, person).await? {
        grouped
            .entry(term.concept_uid.clone())
            .or_default()
            .push(term);
    }
    let sources = sources(pool).await?;
    let (choices, hidden) = preferences::preferences(pool, person).await?;
    let mut out = HashMap::new();
    for concept in crate::concepts::list_all(pool).await? {
        let source = sources.get(&concept.uid);
        if let Some(wording) = choices.get(&concept.uid) {
            out.insert(
                concept.uid,
                wording
                    .clone()
                    .or_else(|| source.map(|source| source.original_name.clone()))
                    .unwrap_or(concept.canonical_name),
            );
            continue;
        }
        let selected = grouped.get(&concept.uid).and_then(|terms| {
            terms
                .iter()
                .filter(|term| {
                    !hidden.contains(&term.uid) && (term.preferred || term.owner_uid.is_none())
                })
                .min_by_key(|term| {
                    let personal = term.owner_uid.is_some();
                    let rank = if personal && term.language.is_empty() {
                        0
                    } else if let Some(index) = locales
                        .iter()
                        .position(|locale| locale.eq_ignore_ascii_case(&term.language))
                    {
                        1 + index * 2 + usize::from(!personal)
                    } else if term.language.is_empty() {
                        100
                    } else if source.is_some_and(|source| {
                        source.original_name == term.name
                            && source
                                .original_language
                                .eq_ignore_ascii_case(&term.language)
                    }) {
                        101
                    } else {
                        102
                    };
                    (rank, usize::from(!term.preferred), term.uid.as_str())
                })
        });
        out.insert(
            concept.uid.clone(),
            selected.map(|term| term.name.clone()).unwrap_or_else(|| {
                if grouped
                    .get(&concept.uid)
                    .is_some_and(|terms| terms.iter().any(|term| hidden.contains(&term.uid)))
                {
                    concept.uid.clone()
                } else {
                    source
                        .map(|source| source.original_name.clone())
                        .unwrap_or(concept.canonical_name)
                }
            }),
        );
    }
    Ok(out)
}

pub async fn resolve_file_meaning(
    pool: &SqlitePool,
    token: &str,
    subject: &str,
) -> Result<Option<String>, StoreError> {
    let name = token.trim().trim_start_matches('#');
    if nucleus::valid_uid(name, "c") {
        return Ok(Some(name.into()));
    }
    let name: String = name.nfc().collect();
    let candidates:Vec<String>=sqlx::query_scalar("SELECT uid FROM concept WHERE canonical_name=? UNION SELECT concept_uid FROM concept_source WHERE original_name=? UNION SELECT t.concept_uid FROM concept_term t JOIN concept_term_history h ON h.term_uid=t.uid WHERE t.owner_uid IS NULL AND h.name=? ORDER BY 1")
        .bind(&name).bind(&name).bind(&name).fetch_all(pool).await?;
    if candidates.len() < 2 {
        return Ok(candidates.into_iter().next());
    }
    let bound:HashSet<String>=sqlx::query_scalar("SELECT predicate_uid FROM record_assertion WHERE subject_uid=? AND retracted_at IS NULL UNION SELECT unit_uid FROM record_assertion WHERE subject_uid=? AND retracted_at IS NULL AND unit_uid IS NOT NULL UNION SELECT unit_uid FROM record WHERE uid=? AND unit_uid IS NOT NULL")
        .bind(subject).bind(subject).bind(subject).fetch_all(pool).await?.into_iter().collect();
    let selected: Vec<_> = candidates
        .iter()
        .filter(|uid| bound.contains(*uid))
        .cloned()
        .collect();
    if selected.len() == 1 {
        return Ok(selected.into_iter().next());
    }
    Err(invalid(&format!(
        "`{token}` names several meanings ({}); choose a Concept identity in the file",
        candidates.join(", ")
    )))
}

pub async fn add_term_on(
    connection: &mut SqliteConnection,
    concept: &str,
    name: &str,
    language: &str,
    owner: Option<&str>,
    preferred: bool,
) -> Result<String, StoreError> {
    let name = term_text(name)?;
    let language = language_tag(language)?;
    let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept_term WHERE concept_uid=? AND LOWER(language)=? AND name=? AND owner_uid IS ? AND retired=0)")
        .bind(concept).bind(&language).bind(&name).bind(owner).fetch_one(&mut *connection).await?;
    if duplicate {
        return Err(invalid(
            "This meaning already has that term in this language and scope",
        ));
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM concept_term WHERE concept_uid=? AND owner_uid IS ?",
    )
    .bind(concept)
    .bind(owner)
    .fetch_one(&mut *connection)
    .await?;
    if count >= 512 {
        return Err(invalid(
            "This meaning already has 512 terms in this scope; edit an existing term",
        ));
    }
    if preferred {
        sqlx::query("UPDATE concept_term SET preferred=0, revision=revision+1 WHERE concept_uid=? AND LOWER(language)=? AND owner_uid IS ? AND preferred=1 AND retired=0")
            .bind(concept).bind(&language).bind(owner).execute(&mut *connection).await?;
    }
    let uid = nucleus::new_uid("ct");
    sqlx::query("INSERT INTO concept_term(uid,concept_uid,language,name,owner_uid,preferred) VALUES(?,?,?,?,?,?)")
        .bind(&uid).bind(concept).bind(&language).bind(&name).bind(owner).bind(preferred).execute(connection).await?;
    Ok(uid)
}

pub async fn edit_term_on(
    connection: &mut SqliteConnection,
    uid: &str,
    expected: i64,
    name: &str,
    language: &str,
    preferred: bool,
) -> Result<(), StoreError> {
    let name = term_text(name)?;
    let language = language_tag(language)?;
    let current =
        sqlx::query("SELECT * FROM concept_term WHERE uid=? AND retired=0 AND revision=?")
            .bind(uid)
            .bind(expected)
            .fetch_optional(&mut *connection)
            .await?
            .map(term)
            .ok_or_else(|| invalid("The term changed; refresh before editing"))?;
    let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept_term WHERE concept_uid=? AND LOWER(language)=? AND name=? AND owner_uid IS ? AND retired=0 AND uid!=?)")
        .bind(&current.concept_uid).bind(&language).bind(&name).bind(&current.owner_uid).bind(uid).fetch_one(&mut *connection).await?;
    if duplicate {
        return Err(invalid(
            "This meaning already has that term in this language and scope",
        ));
    }
    if preferred {
        sqlx::query("UPDATE concept_term SET preferred=0, revision=revision+1 WHERE concept_uid=? AND LOWER(language)=? AND owner_uid IS ? AND preferred=1 AND retired=0 AND uid!=?")
            .bind(&current.concept_uid).bind(&language).bind(&current.owner_uid).bind(uid).execute(&mut *connection).await?;
    }
    sqlx::query("UPDATE concept_term SET name=?,language=?,preferred=?,revision=revision+1 WHERE uid=? AND revision=? AND retired=0")
        .bind(name).bind(language).bind(preferred).bind(uid).bind(expected).execute(connection).await?;
    Ok(())
}

pub async fn retire_term_on(
    connection: &mut SqliteConnection,
    uid: &str,
    expected: i64,
) -> Result<(), StoreError> {
    if sqlx::query("UPDATE concept_term SET retired=1,preferred=0,revision=revision+1 WHERE uid=? AND revision=? AND retired=0").bind(uid).bind(expected).execute(connection).await?.rows_affected() != 1 {
        return Err(invalid("The term changed; refresh before removing it"));
    }
    Ok(())
}

pub async fn location_bindings(
    pool: &SqlitePool,
) -> Result<(String, i64, HashSet<String>), StoreError> {
    let (write, revision) =
        sqlx::query_as("SELECT write_concept_uid,revision FROM vocabulary_role WHERE uid=?")
            .bind(SAVED_LOCATION_ROLE_UID)
            .fetch_one(pool)
            .await?;
    let concepts = sqlx::query_scalar::<_, String>(
        "SELECT concept_uid FROM vocabulary_role_concept WHERE role_uid=?",
    )
    .bind(SAVED_LOCATION_ROLE_UID)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    Ok((write, revision, concepts))
}

pub async fn apply_location_binding(
    pool: &SqlitePool,
    binding: &nucleus::vocabulary::LocationBinding,
) -> Result<(), StoreError> {
    let concepts: HashSet<_> = binding.concept_uids.iter().collect();
    if binding.revision < 1
        || concepts.is_empty()
        || concepts.len() > 128
        || concepts.len() != binding.concept_uids.len()
        || !concepts.contains(&binding.write_concept_uid)
        || concepts.iter().any(|uid| !nucleus::valid_uid(uid, "c"))
    {
        return Err(invalid("Invalid saved location bindings"));
    }
    let mut tx = crate::write_tx(pool).await?;
    for uid in &binding.concept_uids {
        sqlx::query("INSERT OR IGNORE INTO concept(uid,canonical_name,created_at) VALUES(?,?,?)")
            .bind(uid)
            .bind(format!("concept:{uid}"))
            .bind(nucleus::execution::now().to_rfc3339())
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO vocabulary_role_concept(role_uid,concept_uid) VALUES(?,?) ON CONFLICT DO NOTHING")
            .bind(SAVED_LOCATION_ROLE_UID).bind(uid).execute(&mut *tx).await?;
    }
    sqlx::query(
        "UPDATE vocabulary_role SET write_concept_uid=?,revision=MAX(revision+1,?) WHERE uid=?",
    )
    .bind(&binding.write_concept_uid)
    .bind(binding.revision)
    .bind(SAVED_LOCATION_ROLE_UID)
    .execute(&mut *tx)
    .await?;
    let previous: Vec<String> =
        sqlx::query_scalar("SELECT concept_uid FROM vocabulary_role_concept WHERE role_uid=?")
            .bind(SAVED_LOCATION_ROLE_UID)
            .fetch_all(&mut *tx)
            .await?;
    for uid in previous {
        if !concepts.contains(&uid) {
            sqlx::query("DELETE FROM vocabulary_role_concept WHERE role_uid=? AND concept_uid=?")
                .bind(SAVED_LOCATION_ROLE_UID)
                .bind(uid)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await
}

pub async fn dictionary_on(
    connection: &mut SqliteConnection,
    concept: &str,
) -> Result<nucleus::vocabulary::Dictionary, StoreError> {
    let row = sqlx::query("SELECT * FROM concept_source WHERE concept_uid=?")
        .bind(concept)
        .fetch_optional(&mut *connection)
        .await?;
    let Some(row) = row else {
        return Err(invalid("Concept definition unavailable"));
    };
    let source = Source {
        concept_uid: concept.into(),
        vocabulary_uid: row.get("lingua_uid"),
        original_name: row.get("original_name"),
        original_language: row.get("original_language"),
        definition: row.get("definition"),
        derived_from: row.get("derived_from"),
    };
    let terms = sqlx::query(
        "SELECT * FROM concept_term WHERE concept_uid=? AND owner_uid IS NULL ORDER BY uid",
    )
    .bind(concept)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(term)
    .collect();
    let definitions = definitions_on(connection, concept).await?;
    Ok(nucleus::vocabulary::Dictionary {
        source,
        terms,
        definitions,
    })
}

pub async fn log_shared_on(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    concept: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE lingua SET revision=revision+1 WHERE uid IN (SELECT lingua_uid FROM lingua_concept WHERE concept_uid=?)")
        .bind(concept).execute(&mut **tx).await?;
    let dictionary = dictionary_on(tx, concept).await?;
    crate::sync_ops::log_local_tx(
        tx,
        "concept",
        concept,
        "vocabulary",
        crate::sync_ops::OpKind::Set,
        Some(
            serde_json::to_string(&dictionary)
                .map_err(|error| StoreError::Protocol(error.to_string()))?,
        ),
    )
    .await?;
    Ok(())
}

pub async fn apply_dictionary(
    pool: &SqlitePool,
    dictionary: &nucleus::vocabulary::Dictionary,
    origin: &str,
    local: bool,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    apply_dictionary_on(&mut tx, dictionary, origin, local).await?;
    tx.commit().await
}

pub async fn apply_dictionary_on(
    connection: &mut SqliteConnection,
    dictionary: &nucleus::vocabulary::Dictionary,
    origin: &str,
    local: bool,
) -> Result<(), StoreError> {
    validate_dictionary(dictionary)?;
    let source = &dictionary.source;
    let terms_seen: HashSet<String> = dictionary
        .terms
        .iter()
        .map(|term| term.uid.clone())
        .collect();
    let existing: Option<(Option<String>, bool)> = sqlx::query_as("SELECT origin_organ, EXISTS(SELECT 1 FROM concept_source s WHERE s.concept_uid=c.uid) FROM concept c WHERE uid=?").bind(&source.concept_uid).fetch_optional(&mut *connection).await?;
    if !local
        && existing
            .as_ref()
            .is_some_and(|(owner, defined)| *defined && owner.as_deref() != Some(origin))
    {
        return Ok(());
    }
    if let Some(original) = sqlx::query("SELECT * FROM concept_source WHERE concept_uid=?")
        .bind(&source.concept_uid)
        .fetch_optional(&mut *connection)
        .await?
    {
        if original.get::<String, _>("original_name") != source.original_name
            || original.get::<String, _>("original_language") != source.original_language
            || original.get::<String, _>("definition") != source.definition
            || original.get::<String, _>("lingua_uid") != source.vocabulary_uid
            || original.get::<Option<String>, _>("derived_from") != source.derived_from
        {
            return Err(invalid(
                "A Vocabulary update cannot replace an existing source meaning",
            ));
        }
    }
    sqlx::query(
        "INSERT OR IGNORE INTO concept(uid,canonical_name,origin_organ,created_at) VALUES(?,?,?,?)",
    )
    .bind(&source.concept_uid)
    .bind(&source.original_name)
    .bind(origin)
    .bind(nucleus::execution::now().to_rfc3339())
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE concept SET canonical_name=?,origin_organ=? WHERE uid=? AND NOT EXISTS(SELECT 1 FROM concept_source s WHERE s.concept_uid=concept.uid)")
        .bind(&source.original_name).bind(origin).bind(&source.concept_uid).execute(&mut *connection).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO lingua(uid,name,visibility,created_at) VALUES(?,?,'shared',?)",
    )
    .bind(&source.vocabulary_uid)
    .bind(&source.vocabulary_uid)
    .bind(nucleus::execution::now().to_rfc3339())
    .execute(&mut *connection)
    .await?;
    if let Some(parent) = &source.derived_from {
        validate_source_parent_on(connection, &source.concept_uid, Some(parent)).await?;
        let known: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept WHERE uid=?)")
            .bind(parent)
            .fetch_one(&mut *connection)
            .await?;
        if !nucleus::valid_uid(parent, "c") {
            return Err(invalid("Invalid source Concept identity"));
        }
        if !known {
            sqlx::query("INSERT INTO concept(uid,canonical_name,created_at) VALUES(?,?,?)")
                .bind(parent)
                .bind(format!("concept:{parent}"))
                .bind(nucleus::execution::now().to_rfc3339())
                .execute(&mut *connection)
                .await?;
        }
    }
    sqlx::query("INSERT OR IGNORE INTO concept_source(concept_uid,lingua_uid,original_name,original_language,definition,derived_from) VALUES(?,?,?,?,?,?)")
        .bind(&source.concept_uid).bind(&source.vocabulary_uid).bind(&source.original_name).bind(&source.original_language).bind(&source.definition).bind(&source.derived_from).execute(&mut *connection).await?;
    sqlx::query("INSERT INTO lingua_concept(lingua_uid,concept_uid,adopted_at) VALUES(?,?,?) ON CONFLICT DO NOTHING")
        .bind(&source.vocabulary_uid).bind(&source.concept_uid).bind(nucleus::execution::now().to_rfc3339()).execute(&mut *connection).await?;
    for incoming in &dictionary.terms {
        let belongs: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept_term WHERE uid=? AND (concept_uid!=? OR owner_uid IS NOT NULL))")
            .bind(&incoming.uid).bind(&source.concept_uid).fetch_one(&mut *connection).await?;
        if belongs {
            return Err(invalid(
                "A term identity cannot move to another Concept or owner",
            ));
        }
    }
    let existing_terms: Vec<String> = sqlx::query_scalar(
        "SELECT uid FROM concept_term WHERE concept_uid=? AND owner_uid IS NULL",
    )
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
        > 512
    {
        return Err(invalid("A Concept can retain at most 512 shared terms"));
    }
    for uid in existing_terms.difference(&terms_seen) {
        sqlx::query("UPDATE concept_term SET retired=1,preferred=0,revision=revision+1 WHERE uid=? AND retired=0")
            .bind(uid).execute(&mut *connection).await?;
    }
    sqlx::query("UPDATE concept_term SET preferred=0,revision=revision+1 WHERE concept_uid=? AND owner_uid IS NULL AND preferred=1 AND retired=0")
        .bind(&source.concept_uid).execute(&mut *connection).await?;
    for incoming in &dictionary.terms {
        sqlx::query("UPDATE concept_term SET retired=1,preferred=0,revision=revision+1 WHERE concept_uid=? AND language=? AND name=? AND owner_uid IS NULL AND retired=0 AND uid!=?")
            .bind(&source.concept_uid).bind(language_tag(&incoming.language)?).bind(term_text(&incoming.name)?).bind(&incoming.uid).execute(&mut *connection).await?;
        sqlx::query("INSERT INTO concept_term(uid,concept_uid,language,name,preferred,retired,revision) VALUES(?,?,?,?,?,?,?) ON CONFLICT(uid) DO UPDATE SET language=excluded.language,name=excluded.name,preferred=excluded.preferred,retired=excluded.retired,revision=MAX(concept_term.revision+1,excluded.revision)")
            .bind(&incoming.uid).bind(&incoming.concept_uid).bind(language_tag(&incoming.language)?).bind(term_text(&incoming.name)?).bind(incoming.preferred).bind(incoming.retired).bind(incoming.revision).execute(&mut *connection).await?;
    }
    save_definitions_on(connection, dictionary).await?;
    Ok(())
}

pub fn validate_dictionary(dictionary: &nucleus::vocabulary::Dictionary) -> Result<(), StoreError> {
    validate_definitions(dictionary)?;
    let source = &dictionary.source;
    if !nucleus::valid_uid(&source.concept_uid, "c")
        || (!nucleus::valid_uid(&source.vocabulary_uid, "g")
            && source.vocabulary_uid != crate::linguas::LOCAL_UID)
        || dictionary.terms.len() > 512
        || source.definition.len() > 16384
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
            || !nucleus::valid_uid(&term.uid, "ct")
            || term.revision < 1
            || !terms_seen.insert(term.uid.clone())
        {
            return Err(invalid("Invalid public Vocabulary term identity"));
        }
        term_text(&term.name)?;
        language_tag(&term.language)?;
        if !term.retired
            && !names_seen.insert((term.language.to_ascii_lowercase(), term_text(&term.name)?))
        {
            return Err(invalid("Duplicate Vocabulary terms"));
        }
        if term.preferred && !term.retired && !preferred.insert(term.language.to_ascii_lowercase())
        {
            return Err(invalid("Several preferred terms in one language"));
        }
    }
    Ok(())
}
