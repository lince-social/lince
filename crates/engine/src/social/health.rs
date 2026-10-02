use super::*;
use store::sqlx::{Column, Row, Sqlite, Transaction};

async fn aggregate(
    tx: &mut Transaction<'_, Sqlite>,
    sql: &str,
    now: i64,
) -> Result<Value, EngineError> {
    let row = store::sqlx::query(sql)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;
    let mut value = serde_json::Map::new();
    for column in row.columns() {
        value.insert(
            column.name().into(),
            json!(row.try_get::<Option<i64>, _>(column.name())?),
        );
    }
    if let Some(issued) = value.remove("oldest_at") {
        value.insert(
            "oldest_age_seconds".into(),
            json!(issued.as_i64().map(|at| now.saturating_sub(at))),
        );
    }
    if let Some(retry) = value.remove("next_attempt") {
        value.insert(
            "next_retry_seconds".into(),
            json!(retry.as_i64().map(|at| at.saturating_sub(now).max(0))),
        );
    }
    Ok(Value::Object(value))
}

pub(super) async fn snapshot(
    tx: &mut Transaction<'_, Sqlite>,
    now: i64,
) -> Result<Value, EngineError> {
    let preparation = aggregate(tx, "WITH clock(now) AS (VALUES (?)), jobs AS (
        SELECT w.*, CASE WHEN json_valid(e.fds) THEN e.fds ELSE '{}' END AS metadata
        FROM social_message_work w LEFT JOIN record_extension e ON e.record_uid=w.record_uid AND e.namespace='lince.social.message'
    ), dated AS (SELECT *, CASE WHEN json_type(metadata,'$.content.issued_at')='integer' THEN json_extract(metadata,'$.content.issued_at') END AS issued FROM jobs)
    SELECT COUNT(*) AS messages,
        COUNT(CASE WHEN expires_at>now AND next_attempt<=now THEN 1 END) AS due,
        COUNT(CASE WHEN expires_at>now AND next_attempt>now THEN 1 END) AS delayed,
        COUNT(CASE WHEN expires_at<=now THEN 1 END) AS elapsed,
        COUNT(error) AS errors,
        COUNT(CASE WHEN issued IS NULL OR issued<=0 THEN 1 END) AS unknown_age,
        COUNT(CASE WHEN issued>now THEN 1 END) AS future_age,
        MIN(CASE WHEN issued>0 AND issued<=now THEN issued END) AS oldest_at,
        MIN(CASE WHEN expires_at>now THEN next_attempt END) AS next_attempt FROM dated,clock", now).await?;
    let copies = aggregate(tx, "WITH clock(now) AS (VALUES (?)), jobs AS (
        SELECT *, CASE WHEN json_valid(body) THEN body ELSE '{}' END AS metadata FROM social_private_outbox
    ), dated AS (SELECT *, CASE WHEN json_type(metadata,'$.envelope.created_at')='integer' THEN json_extract(metadata,'$.envelope.created_at') END AS issued FROM jobs)
    SELECT COUNT(*) AS copies,COUNT(DISTINCT record_uid) AS message_records,
        COUNT(CASE WHEN state='pending' THEN 1 END) AS pending,
        COUNT(CASE WHEN state='stored' THEN 1 END) AS stored,
        COUNT(CASE WHEN state='ready' THEN 1 END) AS ready,
        COUNT(CASE WHEN state='held' THEN 1 END) AS held,
        COUNT(CASE WHEN state='expired' THEN 1 END) AS expired,
        COUNT(CASE WHEN state='cancelled' THEN 1 END) AS cancelled,
        COUNT(CASE WHEN expires_at<=now THEN 1 END) AS elapsed,
        COUNT(error) AS errors,COALESCE(SUM(length(CAST(body AS BLOB))),0) AS payload_bytes,
        COUNT(CASE WHEN issued IS NULL OR issued<=0 THEN 1 END) AS unknown_age,
        COUNT(CASE WHEN issued>now THEN 1 END) AS future_age,
        MIN(CASE WHEN issued>0 AND issued<=now THEN issued END) AS oldest_at FROM dated,clock", now).await?;
    let destinations = aggregate(tx, "WITH clock(now) AS (VALUES (?)), jobs AS (
        SELECT d.*,o.expires_at,o.state AS copy_state,
        CASE WHEN json_valid(d.receipt) THEN json_extract(d.receipt,'$.stage') END AS stage
        FROM social_private_destination d JOIN social_private_outbox o ON o.id=d.envelope
    ), eligible AS (SELECT *,state IN ('pending','stored') AND copy_state IN ('pending','stored') AND expires_at>now AND COALESCE(stage,'') NOT IN ('recipient-durable','recipient-refused') AS retryable FROM jobs,clock)
    SELECT COUNT(*) AS attempts,
        COUNT(CASE WHEN state='pending' THEN 1 END) AS pending,
        COUNT(CASE WHEN state='stored' THEN 1 END) AS stored,
        COUNT(CASE WHEN state='failed' THEN 1 END) AS failed,
        COUNT(CASE WHEN state='cancelled' THEN 1 END) AS cancelled,
        COUNT(CASE WHEN stage='recipient-durable' THEN 1 END) AS recipient_durable,
        COUNT(CASE WHEN stage='recipient-refused' THEN 1 END) AS recipient_refused,
        COUNT(CASE WHEN receipt IS NOT NULL AND COALESCE(stage,'') NOT IN ('stored','recipient-durable','recipient-refused') THEN 1 END) AS unknown_receipts,
        COUNT(CASE WHEN retryable AND next_attempt<=now THEN 1 END) AS due,
        COUNT(CASE WHEN retryable AND next_attempt>now THEN 1 END) AS delayed,
        COUNT(error) AS errors,COALESCE(SUM(attempts),0) AS requests_made,
        MIN(CASE WHEN retryable THEN next_attempt END) AS next_attempt FROM eligible", now).await?;
    let pickup = aggregate(
        tx,
        "WITH clock(now) AS (VALUES (?)) SELECT COUNT(*) AS jobs,
        COUNT(CASE WHEN next_attempt<=now THEN 1 END) AS due,
        COUNT(CASE WHEN next_attempt>now THEN 1 END) AS delayed,
        COUNT(error) AS errors,MIN(next_attempt) AS next_attempt FROM social_pickup_work,clock",
        now,
    )
    .await?;
    let failures = aggregate(tx, "WITH clock(now) AS (VALUES (?)) SELECT COUNT(*) AS retained,
        COUNT(CASE WHEN expires_at>now AND discard=0 THEN 1 END) AS deferred,
        COUNT(CASE WHEN expires_at>now AND discard=1 THEN 1 END) AS discard_pending,
        COUNT(CASE WHEN expires_at<=now THEN 1 END) AS elapsed,
        COUNT(CASE WHEN expires_at>now AND discard=1 AND next_attempt<=now THEN 1 END) AS due,
        COUNT(CASE WHEN expires_at>now AND discard=1 AND next_attempt>now THEN 1 END) AS delayed,
        MIN(CASE WHEN expires_at>now AND discard=1 THEN next_attempt END) AS next_attempt FROM social_receive_failure,clock", now).await?;
    let admissions = aggregate(tx, "WITH clock(now) AS (VALUES (?)) SELECT COUNT(*) AS retained,
        COUNT(CASE WHEN expires_at>now THEN 1 END) AS current,
        COUNT(CASE WHEN expires_at<=now THEN 1 END) AS elapsed,
        COUNT(CASE WHEN state='accepted' AND expires_at>now THEN 1 END) AS accepted,
        COUNT(CASE WHEN state='provisional' AND expires_at>now THEN 1 END) AS provisional,
        COUNT(CASE WHEN state='blocked' THEN 1 END) AS blocked_evidence,
        COUNT(CASE WHEN state='closed' THEN 1 END) AS closed_evidence FROM social_sender_admission,clock", now).await?;
    let retention = aggregate(tx, "WITH clock(now) AS (VALUES (?)) SELECT COUNT(*) AS contexts,
        COUNT(CASE WHEN state='active' THEN 1 END) AS active,
        COUNT(CASE WHEN state='dormant' THEN 1 END) AS dormant,
        COUNT(CASE WHEN state='retired' THEN 1 END) AS retired,
        COUNT(CASE WHEN state='review' THEN 1 END) AS review,
        MIN(CASE WHEN state='dormant' AND retire_after>now THEN retire_after END) AS next_retirement_at,
        (SELECT COUNT(*) FROM record_extension e,json_each(CASE WHEN json_valid(e.fds) THEN e.fds ELSE '{}' END) v WHERE e.namespace='lince.social.retirements') AS signed_decisions,
        (SELECT COUNT(*) FROM record_extension e,json_each(CASE WHEN json_valid(e.fds) THEN e.fds ELSE '{}' END) v WHERE e.namespace='lince.social.retained-blocks') AS deleted_block_overrides FROM social_context_retention,clock", now).await?;
    let blocks_sql = format!("{} SELECT COUNT(*) AS effective_decisions,
        COUNT(CASE WHEN json_extract(body,'$.blocked')=1 THEN 1 END) AS active_blocks,
        COUNT(CASE WHEN json_extract(body,'$.blocked')=0 THEN 1 END) AS unblocks,
        COUNT(CASE WHEN typeof(context)<>'text' OR typeof(peer)<>'text' OR COALESCE(json_type(body,'$.blocked'),'') NOT IN ('true','false') OR COALESCE(json_type(body,'$.window'),'')<>'integer' OR json_extract(body,'$.window')<=0 THEN 1 END) AS unknown_metadata
        FROM ranked,clock WHERE position=1", admission::EFFECTIVE_DECISIONS);
    let blocks = aggregate(tx, &blocks_sql, now).await?;
    Ok(
        json!({"preparation":preparation,"copies":copies,"destinations":destinations,"pickup":pickup,"receive_failures":failures,"host_admissions":admissions,"key_retention":retention,"blocks":blocks}),
    )
}
