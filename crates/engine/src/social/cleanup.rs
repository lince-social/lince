use super::*;
use nucleus::social::requests::*;
use store::sqlx::{Row, Sqlite, Transaction};

pub(super) async fn inactive_on(
    tx: &mut Transaction<'_, Sqlite>,
    context: &str,
    organ: &str,
    deleted: bool,
    now: i64,
) -> Result<(), EngineError> {
    let cache_record = if deleted { organ } else { context };
    let after: Option<String> = store::sqlx::query_scalar(
        "SELECT metadata_after FROM social_context_retention WHERE context=?",
    )
    .bind(context)
    .fetch_optional(&mut **tx)
    .await?;
    let maps = store::sqlx::query("SELECT namespace,fds FROM record_extension WHERE record_uid=? AND namespace LIKE 'lince.social.admission-%' AND namespace>? ORDER BY namespace LIMIT 8")
        .bind(cache_record).bind(after.unwrap_or_default()).fetch_all(&mut **tx).await?;
    let next: String = maps
        .last()
        .map(|row| row.get("namespace"))
        .unwrap_or_default();
    for row in maps {
        let namespace: String = row.get("namespace");
        let mut map: Value = serde_json::from_str(&row.get::<String, _>("fds"))?;
        let object = map
            .as_object_mut()
            .ok_or_else(|| invalid("Review malformed inactive admission metadata"))?;
        let mut removed = Vec::new();
        for (field, value) in object.iter() {
            if deleted && !field.starts_with(&format!("{context}:")) {
                continue;
            }
            if value.is_null() {
                removed.push(field.clone());
                continue;
            }
            let admission: SenderAdmission = serde_json::from_value(value.clone())?;
            if admission.expires_at.saturating_add(30 * 86400) <= now
                && admission.certificate.expires_at.saturating_add(30 * 86400) <= now
            {
                removed.push(field.clone());
            }
        }
        if !removed.is_empty() {
            for field in removed {
                object.remove(&field);
            }
            store::records::set_extension_on(tx, cache_record, &namespace, &map).await?;
        }
    }
    if !deleted {
        let mut map = owner::extension_on(tx, context, BLOCK_NAMESPACE).await?;
        let object = map
            .as_object_mut()
            .ok_or_else(|| invalid("Review malformed inactive block metadata"))?;
        let before = object.len();
        object.retain(|_, entry| {
            entry["blocked"] != false
                || entry["window"].as_i64().is_none_or(|at| {
                    at <= 0 || at.saturating_add(AUTHORITY_LIFETIME + 30 * 86400) > now
                })
        });
        if before != object.len() {
            store::records::set_extension_on(tx, context, BLOCK_NAMESPACE, &map).await?;
        }
    }
    store::sqlx::query("INSERT INTO social_context_retention(context,state,checked_at,metadata_after) VALUES(?,'retired',?,?) ON CONFLICT(context) DO UPDATE SET metadata_after=excluded.metadata_after")
        .bind(context).bind(now).bind(next).execute(&mut **tx).await?;
    Ok(())
}
