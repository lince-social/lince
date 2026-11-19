use crate::EngineError;
use futures::TryStreamExt;
use nucleus::karma::Consequences;
use serde_json::{Value, json};
use store::sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

const BATCH: usize = 128;

pub(super) async fn read(
    pool: &SqlitePool,
    event: &str,
    rule: &str,
    revision: i64,
    consequences: &Consequences,
) -> Result<Vec<Value>, EngineError> {
    let _read = nucleus::performance::Span::new("karma-history-effect-read");
    let mut effects = Vec::new();
    for (batch, consequences) in consequences.as_slice().chunks(BATCH).enumerate() {
        let offset = batch * BATCH;
        let mut query = QueryBuilder::<Sqlite>::new("WITH requested(position, request_id) AS (");
        query.push_values(consequences.iter().enumerate(), |mut row, (position, _)| {
            row.push_bind(position as i64)
                .push_bind(format!("{event}:{rule}:{revision}:{}", offset + position));
        });
        query.push(
            ") SELECT r.position, 0 AS pending, o.attempt, o.status, o.result, o.at
             FROM requested r
             JOIN effect_queue q ON q.request_id = r.request_id
             JOIN karma_effect_outcome o ON o.effect_uid = q.uid
             UNION ALL
             SELECT r.position, 1 AS pending, q.attempts, q.status, NULL, NULL
             FROM requested r
             JOIN effect_queue q ON q.request_id = r.request_id
             WHERE q.status IN ('queued', 'running', 'uncertain')
             ORDER BY position, pending, attempt",
        );
        nucleus::performance::add("karma-history-effect-batches", 1);
        nucleus::performance::add(
            "karma-history-consequences-requested",
            consequences.len() as u64,
        );
        let mut rows = query.build().fetch(pool);
        while let Some(row) = rows.try_next().await? {
            let index = row.try_get::<i64, _>("position")? as usize;
            let position = offset + index;
            let effect = if row.try_get::<i64, _>("pending")? == 0 {
                let reason = if consequences[index].transfer_target().is_some() {
                    Some(row.try_get::<&str, _>("result")?)
                } else {
                    None
                };
                json!({
                    "position": position,
                    "attempt": row.try_get::<i64, _>("attempt")?,
                    "status": row.try_get::<&str, _>("status")?,
                    "at": row.try_get::<&str, _>("at")?,
                    "reason": reason,
                })
            } else {
                json!({
                    "position": position,
                    "attempt": row.try_get::<i64, _>("attempt")?,
                    "status": row.try_get::<&str, _>("status")?,
                })
            };
            nucleus::performance::add("karma-history-effect-rows", 1);
            effects.push(effect);
        }
    }
    Ok(effects)
}
