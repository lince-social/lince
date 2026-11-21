use nucleus::transfer::{
    OpenPromiseReusePolicy, TransferRevisionInvitation, TransferRevisionParty,
    TransferRevisionPromise, TransferRevisionSnapshot, TransferRevisionTerms,
};
use sqlx::{Row, Sqlite, Transaction, sqlite::SqliteRow};

use crate::{
    StoreError,
    transfers::{
        dependencies_of_on, history_terms,
        item_terms::{decode_item, location_snapshot},
    },
};

pub(in super::super) async fn revision_snapshot(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
) -> Result<TransferRevisionSnapshot, StoreError> {
    let row = sqlx::query(
        "SELECT t.*, r.slug, r.head, r.organ_uid
         FROM transfer t JOIN record r ON r.uid = t.record_uid
         WHERE t.record_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_one(&mut **tx)
    .await?;
    let revision = row.get::<i64, _>("revision").cast_unsigned();
    let parties = sqlx::query(
        "SELECT tp.uid, tp.actor_uid, tp.kind, person.organ_uid FROM transfer_party tp
         JOIN record person ON person.uid = tp.actor_uid
         WHERE tp.transfer_uid = ? ORDER BY tp.uid",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| TransferRevisionParty {
        uid: row.get("uid"),
        person_uid: row.get("actor_uid"),
        organ_uid: row.get("organ_uid"),
        kind: row.get("kind"),
    })
    .collect();
    let invitations = sqlx::query(
        "SELECT uid, attempt, addressed_person_uid, invited_by_person_uid, status, expires_at
         FROM transfer_invitation WHERE transfer_uid = ? ORDER BY uid",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| TransferRevisionInvitation {
        uid: row.get("uid"),
        attempt: row.get::<i64, _>("attempt").cast_unsigned(),
        addressed_person_uid: row.get("addressed_person_uid"),
        invited_by_person_uid: row.get("invited_by_person_uid"),
        status: row.get("status"),
        expires_at: row.get("expires_at"),
    })
    .collect();
    let promises = sqlx::query(
        "SELECT uid, source_promise_uid, revision, record_uid, concept_uid, unit_uid, party_uid, delta,
                window_start, window_end, location_lat, location_lon, location_address,
                condition, reserve_from, state, open_reuse_policy, item_json
         FROM promise WHERE transfer_uid = ? ORDER BY uid",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(promise_from_row)
    .collect::<Result<Vec<_>, _>>()?;
    let dependencies = dependencies_of_on(tx, transfer_uid).await?;
    let mut snapshot = TransferRevisionSnapshot {
        revision,
        transfer: TransferRevisionTerms {
            public_history: history_terms::read_on(tx, transfer_uid).await?,
            uid: row.get("record_uid"),
            origin_organ_uid: row.get("organ_uid"),
            slug: row.get("slug"),
            head: row.get("head"),
            agreement_type: row.get("agreement_type"),
            agreement_pct: row.get("agreement_pct"),
            settlement: row.get("settlement"),
            visibility: row.get("visibility"),
            max_proximity: row.get("max_proximity"),
            satiation: row.get("satiation"),
            parent_uid: row.get("parent_uid"),
            source_uid: row.get("source_uid"),
            reserve_default: row.get("reserve_default"),
            require_confirmation: row.get("require_confirmation"),
            default_place: location_snapshot(
                row.get("default_location_lat"),
                row.get("default_location_lon"),
                row.get("default_location_address"),
            ),
        },
        parties,
        invitations,
        promises,
        dependencies,
        cancellations: crate::transfer_cancellations::terms_on(tx, transfer_uid).await?,
        children: crate::transfer_children::terms_on(tx, transfer_uid).await?,
    };
    snapshot.canonicalize();
    Ok(snapshot)
}

pub(in super::super) async fn validated_snapshot(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
) -> Result<TransferRevisionSnapshot, StoreError> {
    let snapshot = revision_snapshot(tx, transfer_uid).await?;
    nucleus::transfer::exchange::validate_exchanges(&snapshot)
        .map_err(|message| StoreError::Protocol(message.into()))?;
    Ok(snapshot)
}

fn promise_from_row(row: SqliteRow) -> Result<TransferRevisionPromise, StoreError> {
    Ok(TransferRevisionPromise {
        uid: row.get("uid"),
        item: decode_item(row.get("item_json"))?,
        source_promise_uid: row.get("source_promise_uid"),
        revision: row.get::<i64, _>("revision").cast_unsigned(),
        record_uid: row.get("record_uid"),
        concept_uid: row.get("concept_uid"),
        unit_uid: row.get("unit_uid"),
        person_uid: row.get("party_uid"),
        delta: row.get("delta"),
        window_start: row.get("window_start"),
        window_end: row.get("window_end"),
        location: location_snapshot(
            row.get("location_lat"),
            row.get("location_lon"),
            row.get("location_address"),
        ),
        condition: row.get("condition"),
        reserve_from: row.get("reserve_from"),
        state: row.get("state"),
        open_reuse_policy: OpenPromiseReusePolicy::parse(row.get("open_reuse_policy"))
            .unwrap_or_default(),
    })
}
