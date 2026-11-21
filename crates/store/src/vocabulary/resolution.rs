use super::{feature_resolve, terminology_owner};
use crate::StoreError;
use nucleus::valid_uid;
use sqlx::{SqlitePool, query_scalar};
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

/// Resolves a Concept identity, feature, or visible alias in the supplied Person context.
///
/// # Errors
///
/// Returns an error for an ambiguous name or a failed lookup.
pub async fn resolve(
    pool: &SqlitePool,
    token: &str,
    person: Option<&str>,
) -> Result<Option<String>, StoreError> {
    let token = token.trim().trim_start_matches('#').trim_start_matches('^');
    if let Some(feature) = token.strip_prefix("feature:") {
        return feature_resolve(pool, feature).await;
    }
    if let Some(uid) = query_scalar("SELECT uid FROM concept WHERE uid=?")
        .bind(token)
        .fetch_optional(pool)
        .await?
    {
        return Ok(Some(uid));
    }
    let name: String = token.nfc().collect();
    let owner = terminology_owner(person);
    let mut candidates: Vec<String> = query_scalar("SELECT DISTINCT t.concept_uid FROM concept_term t WHERE t.name=? AND t.retired=0 AND (t.owner_uid IS NULL OR t.owner_uid=?) AND NOT EXISTS(SELECT 1 FROM terminology_hidden h WHERE h.term_uid=t.uid AND h.owner_key=? AND h.context=COALESCE((SELECT context FROM terminology_context WHERE owner_key=?),'General')) UNION SELECT p.concept_uid FROM terminology_choice p JOIN concept_source s ON s.concept_uid=p.concept_uid WHERE p.owner_key=? AND p.context=COALESCE((SELECT context FROM terminology_context WHERE owner_key=?),'General') AND COALESCE(p.wording,s.original_name)=?")
        .bind(&name).bind(person).bind(owner).bind(owner).bind(owner).bind(owner).bind(&name).fetch_all(pool).await?;
    candidates.extend(query_scalar::<_,String>("SELECT uid FROM concept c WHERE canonical_name=? AND NOT EXISTS(SELECT 1 FROM concept_source s WHERE s.concept_uid=c.uid)").bind(&name).fetch_all(pool).await?);
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

/// Resolves a stored file spelling using its subject’s historical meaning bindings.
///
/// # Errors
///
/// Returns an error if a spelling remains ambiguous or a lookup fails.
pub async fn resolve_file_meaning(
    pool: &SqlitePool,
    token: &str,
    subject: &str,
) -> Result<Option<String>, StoreError> {
    let name = token.trim().trim_start_matches('#');
    if valid_uid(name, "c") {
        return Ok(Some(name.into()));
    }
    let name: String = name.nfc().collect();
    let candidates:Vec<String>=query_scalar("SELECT uid FROM concept WHERE canonical_name=? UNION SELECT concept_uid FROM concept_source WHERE original_name=? UNION SELECT t.concept_uid FROM concept_term t JOIN concept_term_history h ON h.term_uid=t.uid WHERE t.owner_uid IS NULL AND h.name=? ORDER BY 1")
        .bind(&name).bind(&name).bind(&name).fetch_all(pool).await?;
    if candidates.len() < 2 {
        return Ok(candidates.into_iter().next());
    }
    let bound:HashSet<String>=query_scalar("SELECT predicate_uid FROM record_assertion WHERE subject_uid=? AND retracted_at IS NULL UNION SELECT unit_uid FROM record_assertion WHERE subject_uid=? AND retracted_at IS NULL AND unit_uid IS NOT NULL UNION SELECT unit_uid FROM record WHERE uid=? AND unit_uid IS NOT NULL")
        .bind(subject).bind(subject).bind(subject).fetch_all(pool).await?.into_iter().collect();
    let mut selected = candidates.iter().filter(|uid| bound.contains(*uid));
    if let Some(uid) = selected.next()
        && selected.next().is_none()
    {
        return Ok(Some(uid.clone()));
    }
    Err(StoreError::Protocol(format!(
        "`{token}` names several meanings ({}); choose a Concept identity in the file",
        candidates.join(", ")
    )))
}
