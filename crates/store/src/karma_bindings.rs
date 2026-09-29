use nucleus::karma::{Condition, ConditionError, ReferenceKind, TypedUid};
use sqlx::{Row, Sqlite, Transaction};

use crate::StoreError;

pub use nucleus::karma::ConditionBinding as ReferenceBinding;

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

pub async fn resolve(
    pool: &sqlx::SqlitePool,
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
        for token in Condition::parse(source)
            .map_err(|error| invalid(error.to_string()))?
            .reads()
        {
            for authored in token.slug.split('|') {
                if bindings
                    .iter()
                    .any(|binding| binding.reading == token.func && binding.authored == authored)
                {
                    continue;
                }
                let bound = previous
                    .iter()
                    .find(|binding| binding.reading == token.func && binding.authored == authored);
                let kind = match token.func.as_str() {
                    nucleus::expr::ASSERTION | "demand" => ReferenceKind::Concept,
                    "freq" => ReferenceKind::Frequency,
                    "promise_state" | "confidence" => ReferenceKind::Promise,
                    _ => ReferenceKind::Record,
                };
                let name = bound.map_or(authored, |binding| binding.target.as_str());
                let uid: Option<String> = match kind {
                    ReferenceKind::Concept => {
                        let direct = sqlx::query_scalar("SELECT uid FROM concept WHERE uid = ? OR canonical_name = ?")
                            .bind(name).bind(name).fetch_optional(&mut **tx).await?;
                        match direct {
                            Some(uid) => Some(uid),
                            None => {
                                let aliases: Vec<String> = sqlx::query_scalar("SELECT DISTINCT concept_uid FROM concept_name WHERE name = ? ORDER BY concept_uid").bind(name).fetch_all(&mut **tx).await?;
                                if aliases.len() > 1 { return Err(invalid(format!("karma_binding_ambiguous:{name}"))); }
                                aliases.into_iter().next()
                            }
                        }
                    },
                    ReferenceKind::Frequency => sqlx::query_scalar("SELECT r.uid FROM record r JOIN karma_frequency f ON f.record_uid = r.uid WHERE r.deleted_at IS NULL AND (r.uid = ? OR r.slug = ?)")
                        .bind(name).bind(name).fetch_optional(&mut **tx).await?,
                    ReferenceKind::Promise => sqlx::query_scalar("SELECT uid FROM promise WHERE uid = ?")
                        .bind(name).fetch_optional(&mut **tx).await?,
                    _ => sqlx::query_scalar("SELECT uid FROM record WHERE deleted_at IS NULL AND (uid = ? OR slug = ?)")
                        .bind(name).bind(name).fetch_optional(&mut **tx).await?,
                };
                let uid = uid.ok_or_else(|| {
                    invalid(format!(
                        "karma_binding_unavailable:{reading}(@{authored})",
                        reading = token.func
                    ))
                })?;
                if bound.is_some_and(|binding| binding.target.as_str() != uid) {
                    return Err(invalid("karma_binding_changed"));
                }
                bindings.push(ReferenceBinding {
                    reading: token.func.clone(),
                    authored: authored.into(),
                    target: TypedUid::new(kind, uid).map_err(|error| invalid(error.to_string()))?,
                });
            }
        }
    }
    Ok(bindings)
}

pub async fn save(
    tx: &mut Transaction<'_, Sqlite>,
    rule: &str,
    source: Option<&str>,
    previous: &[ReferenceBinding],
) -> Result<(), StoreError> {
    let bindings = bind(tx, source, previous).await?;
    let json = serde_json::to_string(&bindings).map_err(|error| invalid(error.to_string()))?;
    sqlx::query("UPDATE recurrence SET bindings_json = ? WHERE uid = ?")
        .bind(&json)
        .bind(rule)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE recurrence_revision SET bindings_json = ? WHERE recurrence_uid = ? AND revision = (SELECT revision FROM recurrence WHERE uid = ?)")
        .bind(json).bind(rule).bind(rule).execute(&mut **tx).await?;
    Ok(())
}

pub async fn previous(
    tx: &mut Transaction<'_, Sqlite>,
    rule: &str,
) -> Result<Vec<ReferenceBinding>, StoreError> {
    let row = sqlx::query("SELECT bindings_json FROM recurrence WHERE uid = ?")
        .bind(rule)
        .fetch_optional(&mut **tx)
        .await?;
    row.map(|row| {
        serde_json::from_str(&row.get::<String, _>("bindings_json"))
            .map_err(|error| invalid(error.to_string()))
    })
    .transpose()
    .map(Option::unwrap_or_default)
}
