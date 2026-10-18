use super::*;

pub(super) async fn load(
    pool: &store::sqlx::SqlitePool,
    uid: &str,
) -> Result<Option<Saved>, EngineError> {
    let mut tx = pool.begin().await?;
    let saved = load_on(&mut tx, uid).await?;
    tx.rollback().await?;
    Ok(saved)
}

pub(super) async fn load_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<Saved>, EngineError> {
    let raw: Option<Option<String>> = store::sqlx::query_scalar("SELECT CASE WHEN length(CAST(e.fds AS BLOB))<=? THEN e.fds END FROM record_extension e JOIN record r ON r.uid=e.record_uid AND r.deleted_at IS NULL WHERE e.record_uid=? AND e.namespace=?")
        .bind(MAX_BYTES as i64).bind(uid).bind(PRIVATE_NAMESPACE).fetch_optional(&mut **tx).await?;
    raw.map(|raw| {
        let saved: Saved = serde_json::from_str(
            &raw.ok_or_else(|| invalid("Audience plan exceeds its byte limit"))?,
        )?;
        if saved.review.definition.uid != uid
            || saved.review.commitment().map_err(invalid)? != saved.commitment
        {
            return Err(invalid("Audience plan does not match its reviewed fields"));
        }
        Ok(saved)
    })
    .transpose()
}

pub(super) async fn save_on(
    tx: &mut Transaction<'_, Sqlite>,
    saved: &mut Saved,
) -> Result<(), EngineError> {
    saved.revision = saved
        .revision
        .checked_add(1)
        .filter(|value| *value <= i64::MAX as u64)
        .ok_or_else(|| invalid("Audience plan revision limit reached"))?;
    store::records::set_audience_plan_on(tx, &saved.review.definition.uid, saved).await?;
    Ok(())
}

pub(super) async fn replies_on(
    tx: &mut Transaction<'_, Sqlite>,
    definition: &Definition,
) -> Result<String, EngineError> {
    let rows: Vec<String> = store::sqlx::query_scalar("SELECT DISTINCT m.uid FROM record m JOIN record_assertion a ON a.subject_uid=m.uid AND a.retracted_at IS NULL JOIN vocabulary_role_concept mc ON mc.concept_uid=a.predicate_uid JOIN vocabulary_feature mf ON mf.role_uid=mc.role_uid AND mf.feature='message' WHERE m.deleted_at IS NULL AND m.kind='message' AND a.object_uid IN (SELECT t.subject_uid FROM record_assertion t JOIN vocabulary_role_concept tc ON tc.concept_uid=t.predicate_uid JOIN vocabulary_feature tf ON tf.role_uid=tc.role_uid AND tf.feature='thread' WHERE t.object_uid IN (?,?) AND t.retracted_at IS NULL) ORDER BY m.uid LIMIT 513")
        .bind(&definition.source_uid).bind(&definition.projection_uid).fetch_all(&mut **tx).await?;
    if rows.len() > 512 {
        return Err(invalid(
            "Reply coverage exceeds its local bound; keep expansion paused",
        ));
    }
    nucleus::karma::canonical_hash("lince.audience-replies.v1", &rows)
        .map(|hash| hash.as_str().to_owned())
        .map_err(invalid)
}

pub(super) async fn ended_on(
    tx: &mut Transaction<'_, Sqlite>,
    source: &str,
) -> Result<bool, EngineError> {
    Ok(store::sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM record WHERE uid=? AND kind='transfer' AND deleted_at IS NULL AND quantity_mantissa!='0') OR EXISTS(SELECT 1 FROM transfer_cancellation WHERE transfer_uid=? AND applied_fact_uid IS NOT NULL) OR (EXISTS(SELECT 1 FROM promise WHERE transfer_uid=?) AND NOT EXISTS(SELECT 1 FROM promise WHERE transfer_uid=? AND state NOT IN ('kept','broken','withdrawn')))")
        .bind(source).bind(source).bind(source).bind(source).fetch_one(&mut **tx).await?)
}

pub(super) async fn projection_matches_on(
    tx: &mut Transaction<'_, Sqlite>,
    definition: &Definition,
) -> Result<bool, EngineError> {
    let row: Option<(String, String)> = store::sqlx::query_as("SELECT head,body FROM record WHERE uid=? AND deleted_at IS NULL AND replica_root IS NULL AND place_uid IS NULL")
        .bind(&definition.projection_uid).fetch_optional(&mut **tx).await?;
    Ok(row.is_some_and(|(head, body)| {
        head == definition.projection.title
            && serde_json::from_str::<nucleus::social::PostDraft>(&body)
                .ok()
                .as_ref()
                == Some(&definition.projection)
    }))
}

pub(super) async fn publish_on(
    tx: &mut Transaction<'_, Sqlite>,
    saved: &mut Saved,
    policy: &Policy,
) -> Result<(), EngineError> {
    let uid = &saved.review.definition.projection_uid;
    let revision: i64 = store::sqlx::query_scalar(
        "SELECT revision FROM data_visibility WHERE record_uid=? AND data='record'",
    )
    .bind(uid)
    .fetch_one(&mut **tx)
    .await?;
    let revision =
        store::data_visibility::save_on(tx, uid, Data::Record, None, policy, revision as u64)
            .await?;
    saved.policy_revision = revision;
    let value = nucleus::visibility::SavedPolicy {
        controller_uid: String::new(),
        revision,
        policy: policy.clone(),
    };
    store::sync_ops::log_local_tx(
        tx,
        "data_visibility",
        uid,
        "record",
        store::sync_ops::OpKind::Set,
        Some(serde_json::to_string(&value)?),
    )
    .await?;
    store::sqlx::query(
        "UPDATE organ_contact SET share_seen_seq=NULL WHERE share_protein IS NOT NULL",
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn policy_matches_on(
    tx: &mut Transaction<'_, Sqlite>,
    saved: &Saved,
) -> Result<bool, EngineError> {
    let revision: Option<i64> = store::sqlx::query_scalar(
        "SELECT revision FROM data_visibility WHERE record_uid=? AND data='record'",
    )
    .bind(&saved.review.definition.projection_uid)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(revision == Some(saved.policy_revision as i64))
}
