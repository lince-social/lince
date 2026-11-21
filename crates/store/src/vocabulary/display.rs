//! Selection of displayed vocabulary terms and source labels.

use std::collections::{HashMap, HashSet};

use nucleus::performance::{Span, add};
use sqlx::{Row, SqlitePool, query, query_as, query_scalar};

use super::{language_tag, preferences::preferences};
use crate::{StoreError, concepts::canonical_names, config};

struct DisplayTerm {
    uid: String,
    language: String,
    name: String,
    personal: bool,
    preferred: bool,
}

struct SourceName {
    original_name: String,
    original_language: String,
}

/// Selects display labels for all concepts using the optional person's terminology and language.
///
/// # Errors
/// Returns an error if the terms, definitions, or preferences cannot be read or decoded.
pub async fn display_names(
    pool: &SqlitePool,
    person: Option<&str>,
) -> Result<HashMap<String, String>, StoreError> {
    let _display = Span::new("store-vocabulary-display-await");
    let personal_language: Option<String> =
        query_scalar("SELECT language FROM person_terminology WHERE person_uid=?")
            .bind(person)
            .fetch_optional(pool)
            .await?;
    let language = match personal_language {
        Some(language) => language,
        None => config::language(pool).await?.unwrap_or_else(|| "en".into()),
    };
    let language = language_tag(&language).unwrap_or_else(|_| "en".into());
    let mut locales = vec![language.as_str()];
    let mut current = language.as_str();
    while let Some((parent, _)) = current.rsplit_once('-') {
        locales.push(parent);
        current = parent;
    }
    let mut grouped = display_terms(pool, person).await?;
    let mut sources = source_names(pool).await?;
    let (mut choices, hidden) = preferences(pool, person).await?;
    let concepts = canonical_names(pool).await?;
    let _selection = Span::new("store-vocabulary-display-select");
    let mut out = HashMap::with_capacity(concepts.len());
    let mut considered = 0_u64;
    for (uid, canonical_name) in concepts {
        let source = sources.remove(&uid);
        if let Some(wording) = choices.remove(&uid) {
            let name = wording
                .or_else(|| source.map(|source| source.original_name))
                .unwrap_or(canonical_name);
            out.insert(uid, name);
            continue;
        }
        let terms = grouped.remove(&uid).unwrap_or_default();
        considered = considered.saturating_add(count(terms.len()));
        let (selected, has_hidden) = select_name(terms, source.as_ref(), &locales, &hidden);
        let name = selected.unwrap_or_else(|| {
            if has_hidden {
                uid.clone()
            } else {
                source.map_or(canonical_name, |source| source.original_name)
            }
        });
        out.insert(uid, name);
    }
    add("store-vocabulary-display-terms-considered", considered);
    add("store-vocabulary-display-labels", count(out.len()));
    Ok(out)
}

async fn display_terms(
    pool: &SqlitePool,
    person: Option<&str>,
) -> Result<HashMap<String, Vec<DisplayTerm>>, StoreError> {
    let rows = query(
        "SELECT uid,concept_uid,language,name,owner_uid IS NOT NULL AS personal,preferred
         FROM concept_term WHERE retired=0 AND (owner_uid IS NULL OR owner_uid=?)
         ORDER BY concept_uid,language,name,uid",
    )
    .bind(person)
    .fetch_all(pool)
    .await?;
    add("store-vocabulary-display-terms-read", count(rows.len()));
    let mut groups: Vec<(String, Vec<DisplayTerm>)> = Vec::new();
    for row in rows {
        let concept: &str = row.try_get("concept_uid")?;
        let term = DisplayTerm {
            uid: row.try_get("uid")?,
            language: row.try_get("language")?,
            name: row.try_get("name")?,
            personal: row.try_get("personal")?,
            preferred: row.try_get("preferred")?,
        };
        match groups.last_mut() {
            Some((uid, terms)) if uid.as_str() == concept => terms.push(term),
            _ => groups.push((concept.to_owned(), vec![term])),
        }
    }
    Ok(groups.into_iter().collect())
}

async fn source_names(pool: &SqlitePool) -> Result<HashMap<String, SourceName>, StoreError> {
    Ok(
        query_as("SELECT concept_uid,original_name,original_language FROM concept_source")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|(uid, original_name, original_language)| {
                (
                    uid,
                    SourceName {
                        original_name,
                        original_language,
                    },
                )
            })
            .collect(),
    )
}

fn count(length: usize) -> u64 {
    u64::try_from(length).unwrap_or(u64::MAX)
}

fn select_name(
    terms: Vec<DisplayTerm>,
    source: Option<&SourceName>,
    locales: &[&str],
    hidden: &HashSet<String>,
) -> (Option<String>, bool) {
    let mut best: Option<((usize, usize), DisplayTerm)> = None;
    let mut has_hidden = false;
    for term in terms {
        let is_hidden = hidden.contains(&term.uid);
        has_hidden |= is_hidden;
        if is_hidden || (!term.preferred && term.personal) {
            continue;
        }
        let rank = if term.personal && term.language.is_empty() {
            0
        } else if let Some(index) = locales
            .iter()
            .position(|locale| locale.eq_ignore_ascii_case(&term.language))
        {
            1 + index * 2 + usize::from(!term.personal)
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
        let rank = (rank, usize::from(!term.preferred));
        if best.as_ref().is_none_or(|(current, selected)| {
            (rank, term.uid.as_str()) < (*current, selected.uid.as_str())
        }) {
            best = Some((rank, term));
        }
    }
    (best.map(|(_, term)| term.name), has_hidden)
}
