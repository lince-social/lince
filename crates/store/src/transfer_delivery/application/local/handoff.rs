//! Applies the stored acceptance to the local or remote handoff.

use nucleus::new_uid;
use sqlx::{Row, Sqlite, Transaction, sqlite::SqliteRow};

use super::LocalTransferApplicationRow;
use crate::{StoreError, transfer_delivery::validation::protocol};

pub(super) async fn accept(
    tx: &mut Transaction<'_, Sqlite>,
    handoff: &SqliteRow,
    application: &LocalTransferApplicationRow,
    local: bool,
) -> Result<(), StoreError> {
    let at = application.created_at.as_str();
    let changed = if local {
        sqlx::query(
            "UPDATE transfer_application_handoff SET state = 'accepted', updated_at = ?
             WHERE uid = ? AND state = 'pending'",
        )
        .bind(at)
        .bind(&application.handoff_uid)
        .execute(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "UPDATE transfer_remote_application_handoff
             SET state = 'applied', local_application_uid = ?, updated_at = ?
             WHERE uid = ? AND state = 'pending'",
        )
        .bind(&application.uid)
        .bind(at)
        .bind(&application.handoff_uid)
        .execute(&mut **tx)
        .await?
    };
    if changed.rows_affected() != 1 {
        return Err(protocol(
            "remote Transfer application handoff state is stale",
        ));
    }
    if local {
        sqlx::query(
            "UPDATE transfer_application_handoff_detail SET origin_acceptance_fact_uid = ?
             WHERE handoff_uid = ?",
        )
        .bind(&application.application_fact_uid)
        .bind(&application.handoff_uid)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "UPDATE promise SET state = 'kept', updated_at = ?
             WHERE uid = ? AND party_uid = ? AND ? = 0",
        )
        .bind(at)
        .bind(handoff.get::<&str, _>("source_promise_uid"))
        .bind(&application.participant_person_uid)
        .bind(handoff.get::<f64, _>("canonical_remaining_after"))
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO transfer_application_handoff_event
             (uid, handoff_uid, kind, from_state, to_state, fact_uid, reason_code, request_id, created_at)
             VALUES (?, ?, 'accepted', 'pending', 'accepted', ?, 'local_application', ?, ?)",
        )
        .bind(new_uid("tahe"))
        .bind(&application.handoff_uid)
        .bind(&application.application_fact_uid)
        .bind(format!("application:{}", application.uid))
        .bind(at)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
