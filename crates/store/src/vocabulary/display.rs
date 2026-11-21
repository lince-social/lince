// non-lean-yet
// Scope: struct DisplayTerm, struct SourceName, fn display_names, fn display_terms.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::map_unwrap_or] called `map(<f>).unwrap_or(<a>)` on an `Option` value. Sites
//   65.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 63.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 59, 73, 89.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   98.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 19.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 19.

use std::collections::{HashMap, HashSet};

use sqlx::{Row, SqlitePool};

use super::{language_tag, preferences};
use crate::StoreError;

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

pub async fn display_names(
    pool: &SqlitePool,
    person: Option<&str>,
) -> Result<HashMap<String, String>, StoreError> {
    let _display = nucleus::performance::Span::new("store-vocabulary-display-await");
    let personal_language: Option<String> =
        sqlx::query_scalar("SELECT language FROM person_terminology WHERE person_uid=?")
            .bind(person)
            .fetch_optional(pool)
            .await?;
    let language = match personal_language {
        Some(language) => language,
        None => crate::config::language(pool)
            .await?
            .unwrap_or_else(|| "en".into()),
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
    let (mut choices, hidden) = preferences::preferences(pool, person).await?;
    let concepts = crate::concepts::canonical_names(pool).await?;
    let _selection = nucleus::performance::Span::new("store-vocabulary-display-select");
    let mut out = HashMap::new();
    let mut considered = 0;
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
        considered += terms.len() as u64;
        let (selected, has_hidden) = select_name(terms, source.as_ref(), &locales, &hidden);
        let name = selected.unwrap_or_else(|| {
            if has_hidden {
                uid.clone()
            } else {
                source
                    .map(|source| source.original_name)
                    .unwrap_or(canonical_name)
            }
        });
        out.insert(uid, name);
    }
    nucleus::performance::add("store-vocabulary-display-terms-considered", considered);
    nucleus::performance::add("store-vocabulary-display-labels", out.len() as u64);
    Ok(out)
}

async fn display_terms(
    pool: &SqlitePool,
    person: Option<&str>,
) -> Result<HashMap<String, Vec<DisplayTerm>>, StoreError> {
    let rows = sqlx::query(
        "SELECT uid,concept_uid,language,name,owner_uid IS NOT NULL AS personal,preferred
         FROM concept_term WHERE retired=0 AND (owner_uid IS NULL OR owner_uid=?)
         ORDER BY concept_uid,language,name,uid",
    )
    .bind(person)
    .fetch_all(pool)
    .await?;
    nucleus::performance::add("store-vocabulary-display-terms-read", rows.len() as u64);
    let mut groups: Vec<(String, Vec<DisplayTerm>)> = Vec::new();
    for row in rows {
        let concept = row.get::<&str, _>("concept_uid");
        if groups.last().is_none_or(|(uid, _)| uid != concept) {
            groups.push((concept.to_owned(), Vec::new()));
        }
        groups
            .last_mut()
            .expect("the current concept group has been created")
            .1
            .push(DisplayTerm {
                uid: row.get("uid"),
                language: row.get("language"),
                name: row.get("name"),
                personal: row.get("personal"),
                preferred: row.get("preferred"),
            });
    }
    Ok(groups.into_iter().collect())
}

async fn source_names(pool: &SqlitePool) -> Result<HashMap<String, SourceName>, StoreError> {
    Ok(
        sqlx::query("SELECT concept_uid,original_name,original_language FROM concept_source")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| {
                (
                    row.get("concept_uid"),
                    SourceName {
                        original_name: row.get("original_name"),
                        original_language: row.get("original_language"),
                    },
                )
            })
            .collect(),
    )
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
