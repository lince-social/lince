use super::*;

pub(super) async fn find(
    pool: &store::sqlx::SqlitePool,
    definition: &Definition,
) -> Result<Option<Saved>, EngineError> {
    if let Some(saved) = load(pool, &definition.uid).await? {
        return Ok(Some(saved));
    }
    let mut connection = pool.acquire().await?;
    let uid = find_uid_on(&mut connection, definition).await?;
    drop(connection);
    match uid {
        Some(uid) => load(pool, &uid).await,
        None => Ok(None),
    }
}

pub(super) async fn find_uid_on(
    connection: &mut store::sqlx::SqliteConnection,
    definition: &Definition,
) -> Result<Option<String>, EngineError> {
    let (kind, transfer, promise) = match &definition.recipe {
        Recipe::Offer {
            transfer_uid,
            promise_uid,
            ..
        } => ("offer", transfer_uid.as_str(), promise_uid.as_str()),
        Recipe::Replenish { .. } => ("replenish", "", ""),
    };
    let rows:Vec<String>=store::sqlx::query_scalar("SELECT e.record_uid FROM record_extension e JOIN record r ON r.uid=e.record_uid AND r.deleted_at IS NULL WHERE e.namespace=? AND json_extract(e.fds,'$.review.definition.controller_uid')=? AND json_extract(e.fds,'$.review.definition.stock_uid')=? AND json_extract(e.fds,'$.review.definition.recipe.kind')=? AND (?='replenish' OR (json_extract(e.fds,'$.review.definition.recipe.transfer_uid')=? AND json_extract(e.fds,'$.review.definition.recipe.promise_uid')=?)) LIMIT 2")
        .bind(NAMESPACE).bind(&definition.controller_uid).bind(&definition.stock_uid).bind(kind).bind(kind).bind(transfer).bind(promise).fetch_all(connection).await?;
    if rows.len() > 1 {
        return Err(invalid("Conflicting stock policies need review"));
    }
    Ok(rows.into_iter().next())
}

pub(super) async fn load(
    pool: &store::sqlx::SqlitePool,
    uid: &str,
) -> Result<Option<Saved>, EngineError> {
    let raw:Option<Option<String>>=store::sqlx::query_scalar("SELECT CASE WHEN length(CAST(e.fds AS BLOB))<=131072 THEN e.fds END FROM record_extension e JOIN record r ON r.uid=e.record_uid AND r.deleted_at IS NULL WHERE e.record_uid=? AND e.namespace=?")
        .bind(uid).bind(NAMESPACE).fetch_optional(pool).await?;
    raw.map(|raw| {
        let saved: Saved = serde_json::from_str(
            &raw.ok_or_else(|| invalid("Stock policy exceeds its byte limit"))?,
        )?;
        if saved.review.definition.uid != uid
            || saved.review.commitment().map_err(invalid)? != saved.commitment
        {
            return Err(invalid("Stock policy differs from its reviewed definition"));
        }
        Ok(saved)
    })
    .transpose()
}

pub(super) async fn save(
    engine: &Engine,
    saved: &mut Saved,
    expected: u64,
) -> Result<(), EngineError> {
    let mut tx = store::write_tx(&engine.store.pool).await?;
    let revision:Option<i64>=store::sqlx::query_scalar("SELECT CAST(json_extract(fds,'$.revision') AS INTEGER) FROM record_extension WHERE record_uid=? AND namespace=?")
        .bind(&saved.review.definition.uid).bind(NAMESPACE).fetch_optional(&mut *tx).await?;
    if revision != Some(expected as i64) {
        return Err(invalid("Stock policy changed; inspect it again"));
    }
    saved.revision = expected
        .checked_add(1)
        .filter(|value| *value <= i64::MAX as u64)
        .ok_or_else(|| invalid("Stock policy revision limit reached"))?;
    store::records::set_stock_policy_on(&mut tx, &saved.review.definition.uid, saved).await?;
    tx.commit().await?;
    Ok(())
}
