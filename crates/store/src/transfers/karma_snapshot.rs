use nucleus::transfer::{
    AgreementGuard,
    karma::{Guard, Participant, Snapshot},
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

pub async fn read(pool: &SqlitePool, transfer: &str) -> Result<Snapshot, StoreError> {
    let mut tx = pool.begin().await?;
    let snapshot = read_on(&mut tx, transfer).await?;
    tx.rollback().await?;
    Ok(snapshot)
}

pub async fn read_on(
    tx: &mut Transaction<'_, Sqlite>,
    transfer: &str,
) -> Result<Snapshot, StoreError> {
    let head = sqlx::query("SELECT t.revision, EXISTS(SELECT 1 FROM promise p WHERE p.transfer_uid = t.record_uid AND p.state = 'active') AS active, t.visibility = 'public' AND EXISTS(SELECT 1 FROM visibility_rule v WHERE v.target_uid = t.record_uid AND v.subject_kind = 'public' AND v.grant_level = 'visible') AS published FROM transfer t JOIN record r ON r.uid = t.record_uid AND r.deleted_at IS NULL WHERE t.record_uid = ?")
        .bind(transfer).fetch_one(&mut **tx).await?;
    let revision: i64 = head.get("revision");
    let rows = sqlx::query("SELECT p.actor_uid, COALESCE(a.level, 0) AS level, a.at, a.last_event_uid FROM transfer_party p JOIN record person ON person.uid = p.actor_uid AND person.kind = 'person' AND person.deleted_at IS NULL LEFT JOIN transfer_agreement a ON a.party_uid = p.uid AND a.transfer_uid = p.transfer_uid AND a.revision = ? WHERE p.transfer_uid = ? ORDER BY p.actor_uid")
        .bind(revision).bind(transfer).fetch_all(&mut **tx).await?;
    let mut participants = std::collections::BTreeMap::new();
    for row in rows {
        let at: Option<String> = row.get("at");
        let changed_at_ms = at
            .map(|at| {
                chrono::DateTime::parse_from_rfc3339(&at)
                    .map(|at| at.timestamp_millis())
                    .map_err(|error| StoreError::Protocol(error.to_string()))
            })
            .transpose()?;
        participants.insert(
            row.get("actor_uid"),
            Participant {
                guard: AgreementGuard {
                    level: u8::try_from(row.get::<i64, _>("level"))
                        .map_err(|error| StoreError::Protocol(error.to_string()))?,
                    change_uid: row.get("last_event_uid"),
                },
                changed_at_ms,
            },
        );
    }
    let ready = crate::transfer_agreement::read_on(tx, transfer)
        .await?
        .0
        .ready;
    let snapshot = Snapshot {
        transfer: transfer.into(),
        revision: u64::try_from(revision)
            .map_err(|error| StoreError::Protocol(error.to_string()))?,
        active: head.get("active"),
        published: head.get("published"),
        ready,
        participants,
    };
    snapshot.validate().map_err(StoreError::Protocol)?;
    Ok(snapshot)
}

pub async fn check_on(
    tx: &mut Transaction<'_, Sqlite>,
    transfer: &str,
    expected: Option<&Guard>,
) -> Result<(), StoreError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    expected.validate().map_err(StoreError::Protocol)?;
    let current = read_on(tx, transfer).await?;
    if expected.transfer != transfer || !expected.matches(&current) {
        return Err(StoreError::Protocol("transfer_state_source_changed".into()));
    }
    Ok(())
}

pub async fn check_participant_on(
    tx: &mut Transaction<'_, Sqlite>,
    transfer: &str,
    person: &str,
    expected: Option<&AgreementGuard>,
) -> Result<(), StoreError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let current = sqlx::query("SELECT COALESCE(a.level, 0) AS level, a.last_event_uid FROM transfer t JOIN transfer_party p ON p.transfer_uid = t.record_uid AND p.actor_uid = ? LEFT JOIN transfer_agreement a ON a.transfer_uid = t.record_uid AND a.revision = t.revision AND a.party_uid = p.uid WHERE t.record_uid = ?")
        .bind(person).bind(transfer).fetch_optional(&mut **tx).await?;
    if current.is_none_or(|current| {
        current.get::<i64, _>("level") != i64::from(expected.level)
            || current.get::<Option<String>, _>("last_event_uid") != expected.change_uid
    }) {
        return Err(StoreError::Protocol(
            "transfer_agreement_source_changed".into(),
        ));
    }
    Ok(())
}
