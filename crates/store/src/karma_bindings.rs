//! Resolves authored Karma references to stable Store identities.

use std::collections::HashSet;

/// Stored link from an authored reference to its typed identity.
pub use nucleus::karma::ConditionBinding as ReferenceBinding;
use nucleus::{
    expr::{ASSERTION, HAS_ASSERTION, assertion_binding, binding_reading},
    karma::{Condition, ConditionError, ReferenceKind, TypedUid},
    transfer::karma::{is_agreement_reading, is_reading},
};
use serde_json::{from_str, to_string};
use sqlx::{Row, Sqlite, SqlitePool, Transaction, query, query_scalar};

use crate::StoreError;

/// Parses an authored condition and substitutes the stored reference identities.
///
/// # Errors
/// Returns an error for unreadable source or a reference missing from a nonempty binding list.
pub fn apply(source: &str, bindings: &[ReferenceBinding]) -> Result<Condition, ConditionError> {
    let condition = Condition::parse(source)?;
    if bindings.is_empty() {
        return Ok(condition);
    }
    condition.map_references(|reading, authored| {
        bindings
            .iter()
            .find(|binding| binding.reading == reading && binding.authored == authored)
            .map(|binding| binding.target.as_str().to_owned())
            .ok_or_else(|| {
                ConditionError::UnknownReference(format!("unbound {reading}(@{authored})"))
            })
    })
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}

/// Resolves references to typed identities while honoring prior bindings.
///
/// # Errors
/// Returns an error for invalid source, ambiguous or unavailable references, or failed reads.
pub async fn resolve(
    pool: &SqlitePool,
    source: &str,
    previous: &[ReferenceBinding],
) -> Result<Vec<ReferenceBinding>, StoreError> {
    let mut tx = pool.begin().await?;
    let bindings = bind(&mut tx, Some(source), previous).await?;
    tx.rollback().await?;
    Ok(bindings)
}

async fn bind(
    tx: &mut Transaction<'_, Sqlite>,
    source: Option<&str>,
    previous: &[ReferenceBinding],
) -> Result<Vec<ReferenceBinding>, StoreError> {
    let mut bindings = Vec::<ReferenceBinding>::new();
    if let Some(source) = source {
        let condition = Condition::parse(source).map_err(|error| invalid(error.to_string()))?;
        let mut seen = HashSet::new();
        for token in condition.reads_ref() {
            for (position, authored) in token.slug.split('|').enumerate() {
                let reading = if token.func == HAS_ASSERTION {
                    assertion_binding(position)
                } else {
                    binding_reading(&token.func)
                };
                if !seen.insert((reading, authored)) {
                    continue;
                }
                let bound = previous
                    .iter()
                    .find(|binding| binding.reading == reading && binding.authored == authored);
                let kind = match token.func.as_str() {
                    HAS_ASSERTION if position == 1 => ReferenceKind::Concept,
                    ASSERTION | "demand" => ReferenceKind::Concept,
                    "freq" => ReferenceKind::Frequency,
                    "promise_state" | "confidence" => ReferenceKind::Promise,
                    name if is_reading(name) && position == 0 => ReferenceKind::Transfer,
                    name if is_agreement_reading(name) => ReferenceKind::Person,
                    _ => ReferenceKind::Record,
                };
                let name = bound.map_or(authored, |binding| binding.target.as_str());
                let uid = lookup_target(tx, kind, name).await?.ok_or_else(|| {
                    invalid(format!(
                        "karma_binding_unavailable:{reading}(@{authored})",
                        reading = token.func
                    ))
                })?;
                if bound.is_some_and(|binding| binding.target.as_str() != uid) {
                    return Err(invalid("karma_binding_changed"));
                }
                bindings.push(ReferenceBinding {
                    reading: reading.into(),
                    authored: authored.into(),
                    target: TypedUid::new(kind, uid).map_err(|error| invalid(error.to_string()))?,
                });
            }
        }
    }
    bindings.extend(
        previous
            .iter()
            .filter(|binding| binding.reading.starts_with("consequence."))
            .cloned(),
    );
    Ok(bindings)
}

async fn lookup_target(
    tx: &mut Transaction<'_, Sqlite>,
    kind: ReferenceKind,
    name: &str,
) -> Result<Option<String>, StoreError> {
    let sql = match kind {
        ReferenceKind::Concept => "SELECT uid FROM concept WHERE uid = ?1 OR canonical_name = ?1",
        ReferenceKind::Frequency => {
            "SELECT r.uid FROM record r JOIN karma_frequency f ON f.record_uid = r.uid WHERE r.deleted_at IS NULL AND (r.uid = ?1 OR r.slug = ?1)"
        }
        ReferenceKind::Transfer => {
            "SELECT uid FROM record WHERE kind = 'transfer' AND deleted_at IS NULL AND (uid = ?1 OR slug = ?1) UNION SELECT transfer_uid FROM transfer_remote_reference WHERE (transfer_uid = ?1 OR uid = ?1) AND state = 'active' AND projection IS NOT NULL AND recipient_organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ' AND deleted_at IS NULL) LIMIT 1"
        }
        ReferenceKind::Person => {
            "SELECT uid FROM record WHERE kind = 'person' AND deleted_at IS NULL AND (uid = ?1 OR slug = ?1)"
        }
        ReferenceKind::Promise => "SELECT uid FROM promise WHERE uid = ?1",
        _ => "SELECT uid FROM record WHERE deleted_at IS NULL AND (uid = ?1 OR slug = ?1)",
    };
    let direct = query_scalar(sql)
        .bind(name)
        .fetch_optional(&mut **tx)
        .await?;
    if kind != ReferenceKind::Concept || direct.is_some() {
        return Ok(direct);
    }
    let aliases: Vec<String> = query_scalar(
        "SELECT DISTINCT concept_uid FROM concept_name WHERE name = ? ORDER BY concept_uid LIMIT 2",
    )
    .bind(name)
    .fetch_all(&mut **tx)
    .await?;
    if aliases.len() > 1 {
        return Err(invalid(format!("karma_binding_ambiguous:{name}")));
    }
    Ok(aliases.into_iter().next())
}

/// Stores the resolved bindings on the rule and its current revision in the caller's transaction.
///
/// # Errors
/// Returns an error if references cannot be resolved or the bindings cannot be encoded or saved.
pub async fn save(
    tx: &mut Transaction<'_, Sqlite>,
    rule: &str,
    source: Option<&str>,
    previous: &[ReferenceBinding],
) -> Result<(), StoreError> {
    let bindings = bind(tx, source, previous).await?;
    let json = to_string(&bindings).map_err(|error| invalid(error.to_string()))?;
    query("UPDATE recurrence SET bindings_json = ? WHERE uid = ?")
        .bind(&json)
        .bind(rule)
        .execute(&mut **tx)
        .await?;
    query("UPDATE recurrence_revision SET bindings_json = ? WHERE recurrence_uid = ? AND revision = (SELECT revision FROM recurrence WHERE uid = ?)")
        .bind(json).bind(rule).bind(rule).execute(&mut **tx).await?;
    Ok(())
}

/// Reads the current bindings for a rule, returning an empty list when it is absent.
///
/// # Errors
/// Returns an error if the bindings query or decoding fails.
pub async fn previous(
    tx: &mut Transaction<'_, Sqlite>,
    rule: &str,
) -> Result<Vec<ReferenceBinding>, StoreError> {
    query("SELECT bindings_json FROM recurrence WHERE uid = ?")
        .bind(rule)
        .try_map(|row| {
            from_str(row.try_get("bindings_json")?).map_err(|error| invalid(error.to_string()))
        })
        .fetch_optional(&mut **tx)
        .await
        .map(Option::unwrap_or_default)
}

#[cfg(test)]
mod tests {
    use sqlx::query;

    use super::{apply, resolve};
    use crate::{
        Store,
        concepts::create,
        exact::zero,
        records::{NewRecord, create as create_record},
    };

    #[tokio::test]
    async fn a_prior_binding_survives_new_ambiguity_in_its_authored_alias() {
        let store = Store::open_memory().await.unwrap();
        let pool = &store.pool;
        let record = create_record(
            pool,
            NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Response",
                body: "",
                quantity: zero(),
            },
        )
        .await
        .unwrap();
        let first = create(pool, "first meaning", &[]).await.unwrap();
        let second = create(pool, "second meaning", &[]).await.unwrap();
        let alias =
            "INSERT INTO concept_name (concept_uid, lang, name) VALUES (?, '', 'shared-alias')";
        query(alias).bind(&first).execute(pool).await.unwrap();
        let source = format!(
            "has_assertion(@{}, #shared-alias, @{})",
            record.uid, record.uid
        );
        let previous = resolve(pool, &source, &[]).await.unwrap();
        query(alias).bind(&second).execute(pool).await.unwrap();
        let error = resolve(pool, &source, &[]).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("karma_binding_ambiguous:shared-alias")
        );

        let retained = resolve(pool, &source, &previous).await.unwrap();
        let applied = apply(&source, &retained).unwrap();
        let reading = applied.reads_ref().first().unwrap();
        assert_eq!(
            reading.slug,
            format!("{}|{first}|{}", record.uid, record.uid)
        );
    }
}
