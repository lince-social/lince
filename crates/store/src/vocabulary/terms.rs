use super::invalid;
use crate::StoreError;
use nucleus::{new_uid, vocabulary::Term};
use sqlx::{Row, SqliteConnection, SqlitePool, query, query_scalar, sqlite::SqliteRow};
use unicode_normalization::UnicodeNormalization;

const MAX_TERM_CHARACTERS: usize = 256;
const MAX_TERMS_PER_SCOPE: i64 = 512;
const ACTIVE_TERMS: &str = "SELECT * FROM concept_term WHERE retired=0 AND (owner_uid IS NULL OR owner_uid=?) ORDER BY concept_uid,language,name,uid";

/// Trims a term and normalizes it to Unicode NFC.
///
/// # Errors
///
/// Returns an error if the text is empty, exceeds 256 characters, or has control characters.
pub fn term_text(text: &str) -> Result<String, StoreError> {
    let text: String = text.trim().nfc().collect();
    if text.is_empty()
        || text
            .chars()
            .enumerate()
            .any(|(index, character)| index >= MAX_TERM_CHARACTERS || character.is_control())
    {
        return Err(invalid(
            "Enter a term of 1–256 characters without control characters",
        ));
    }
    Ok(text)
}

/// Normalizes tag casing. An empty tag applies to all languages.
///
/// # Errors
///
/// Returns an error if a nonempty tag exceeds the allowed length or has invalid subtags.
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

pub(super) fn decode_term(row: &SqliteRow) -> Result<Term, StoreError> {
    Ok(Term {
        uid: row.try_get("uid")?,
        concept_uid: row.try_get("concept_uid")?,
        language: row.try_get("language")?,
        name: row.try_get("name")?,
        owner_uid: row.try_get("owner_uid")?,
        preferred: row.try_get("preferred")?,
        retired: row.try_get("retired")?,
        revision: row.try_get("revision")?,
    })
}

/// Reads active shared terms and the supplied Person’s active terms.
///
/// # Errors
///
/// Returns an error if the lookup or term decoding fails.
pub async fn terms(pool: &SqlitePool, person: Option<&str>) -> Result<Vec<Term>, StoreError> {
    query(ACTIVE_TERMS)
        .bind(person)
        .fetch_all(pool)
        .await?
        .iter()
        .map(decode_term)
        .collect()
}

/// Reads active terms within the caller’s connection scope.
///
/// # Errors
///
/// Returns an error if the lookup or term decoding fails.
pub async fn terms_on(
    connection: &mut SqliteConnection,
    person: Option<&str>,
) -> Result<Vec<Term>, StoreError> {
    query(ACTIVE_TERMS)
        .bind(person)
        .fetch_all(connection)
        .await?
        .iter()
        .map(decode_term)
        .collect()
}

/// Reads a term by UID, including retired terms.
///
/// # Errors
///
/// Returns an error if the lookup or term decoding fails.
pub async fn get_term(pool: &SqlitePool, uid: &str) -> Result<Option<Term>, StoreError> {
    query("SELECT * FROM concept_term WHERE uid=?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(decode_term)
        .transpose()
}

/// Adds a unique term and clears the earlier preference when the term is preferred.
///
/// # Errors
///
/// Returns an error for invalid text, a duplicate, an exceeded term limit, or a failed write.
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
    let duplicate: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM concept_term WHERE concept_uid=? AND LOWER(language)=? AND name=? AND owner_uid IS ? AND retired=0)")
        .bind(concept).bind(&language).bind(&name).bind(owner).fetch_one(&mut *connection).await?;
    if duplicate {
        return Err(invalid(
            "This meaning already has that term in this language and scope",
        ));
    }
    let count: i64 =
        query_scalar("SELECT COUNT(*) FROM concept_term WHERE concept_uid=? AND owner_uid IS ?")
            .bind(concept)
            .bind(owner)
            .fetch_one(&mut *connection)
            .await?;
    if count >= MAX_TERMS_PER_SCOPE {
        return Err(invalid(
            "This meaning already has 512 terms in this scope; edit an existing term",
        ));
    }
    if preferred {
        query("UPDATE concept_term SET preferred=0, revision=revision+1 WHERE concept_uid=? AND LOWER(language)=? AND owner_uid IS ? AND preferred=1 AND retired=0")
            .bind(concept).bind(&language).bind(owner).execute(&mut *connection).await?;
    }
    let uid = new_uid("ct");
    query("INSERT INTO concept_term(uid,concept_uid,language,name,owner_uid,preferred) VALUES(?,?,?,?,?,?)")
        .bind(&uid).bind(concept).bind(&language).bind(&name).bind(owner).bind(preferred).execute(connection).await?;
    Ok(uid)
}

/// Edits an active term at the expected revision.
///
/// # Errors
///
/// Returns an error for invalid text, a duplicate, a stale revision, or a failed write.
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
    let current = query("SELECT * FROM concept_term WHERE uid=? AND retired=0 AND revision=?")
        .bind(uid)
        .bind(expected)
        .fetch_optional(&mut *connection)
        .await?
        .as_ref()
        .map(decode_term)
        .transpose()?
        .ok_or_else(|| invalid("The term changed; refresh before editing"))?;
    let duplicate: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM concept_term WHERE concept_uid=? AND LOWER(language)=? AND name=? AND owner_uid IS ? AND retired=0 AND uid!=?)")
        .bind(&current.concept_uid).bind(&language).bind(&name).bind(&current.owner_uid).bind(uid).fetch_one(&mut *connection).await?;
    if duplicate {
        return Err(invalid(
            "This meaning already has that term in this language and scope",
        ));
    }
    if preferred {
        query("UPDATE concept_term SET preferred=0, revision=revision+1 WHERE concept_uid=? AND LOWER(language)=? AND owner_uid IS ? AND preferred=1 AND retired=0 AND uid!=?")
            .bind(&current.concept_uid).bind(&language).bind(&current.owner_uid).bind(uid).execute(&mut *connection).await?;
    }
    query("UPDATE concept_term SET name=?,language=?,preferred=?,revision=revision+1 WHERE uid=? AND revision=? AND retired=0")
        .bind(name).bind(language).bind(preferred).bind(uid).bind(expected).execute(connection).await?;
    Ok(())
}

/// Retires the active term at the expected revision and clears its preference.
///
/// # Errors
///
/// Returns an error if the revision no longer matches or the update fails.
pub async fn retire_term_on(
    connection: &mut SqliteConnection,
    uid: &str,
    expected: i64,
) -> Result<(), StoreError> {
    if query("UPDATE concept_term SET retired=1,preferred=0,revision=revision+1 WHERE uid=? AND revision=? AND retired=0").bind(uid).bind(expected).execute(connection).await?.rows_affected() != 1 {
        return Err(invalid("The term changed; refresh before removing it"));
    }
    Ok(())
}
