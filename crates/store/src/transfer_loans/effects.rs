//! Read application effects, ledger corrections, and simulated effects.

use nucleus::DecimalValue;
use sqlx::{Row, SqliteConnection};

use crate::{StoreError, exact};

pub(super) struct Effect {
    pub(super) transfer: String,
    pub(super) exchange: String,
    pub(super) person: String,
    pub(super) record: String,
    pub(super) delta: DecimalValue,
    pub(super) fulfilment: bool,
    pub(super) unit: Option<String>,
}

pub(super) async fn effects(
    connection: &mut SqliteConnection,
    simulated: bool,
) -> Result<Vec<Effect>, StoreError> {
    let rows = sqlx::query(
        "SELECT e.transfer_uid, e.exchange_uid, e.person_uid, e.record_uid, e.mode,
                e.unit_uid, f.uid, f.delta_mantissa, f.delta_scale
         FROM transfer_private_effect e JOIN fact f ON f.uid = e.fact_uid
         UNION ALL
         SELECT s.transfer_uid, json_extract(p.item_json, '$.exchange.uid'), s.owner_person_uid,
                f.record_uid, 'quantity', json_extract(f.payload, '$.local_unit_uid'),
                f.uid, f.delta_mantissa, f.delta_scale
         FROM transfer_occurrence_settlement_slice s JOIN promise p ON p.uid = s.promise_uid
         JOIN fact f ON f.uid = s.application_fact_uid
         WHERE NOT EXISTS (SELECT 1 FROM transfer_private_effect e WHERE e.fact_uid = f.uid)
           AND json_extract(p.item_json, '$.exchange.uid') IS NOT NULL
         UNION ALL
         SELECT h.transfer_uid, json_extract(p.value, '$.exchange'), a.participant_person_uid,
                a.local_record_uid, 'quantity', json_extract(f.payload, '$.local_unit_uid'),
                f.uid, f.delta_mantissa, f.delta_scale
         FROM transfer_local_application a
         JOIN transfer_application_effect_handoff h ON h.uid = a.handoff_uid
         JOIN fact f ON f.uid = a.application_fact_uid
         JOIN transfer_remote_reference r ON r.uid = h.reference_uid, json_each(r.projection, '$.promises') p
         WHERE json_extract(p.value, '$.uid') = h.source_promise_uid
           AND json_extract(p.value, '$.exchange') IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM transfer_private_effect e WHERE e.fact_uid = f.uid)",
    )
    .fetch_all(&mut *connection)
    .await?;
    let mut result = Vec::new();
    for row in rows {
        let corrections = sqlx::query(
            "SELECT f.delta_mantissa, f.delta_scale FROM fact f
             WHERE (f.cause_kind = 'compensation' AND f.cause_uid = ?1) OR EXISTS (
                 SELECT 1 FROM fact_origin o
                 JOIN record own ON own.uid = o.organ_uid AND own.slug = 'local-organ'
                 WHERE o.fact_uid = f.uid AND json_extract(o.payload, '$.cause.kind') = 'compensation'
                   AND json_extract(o.payload, '$.cause.uid') = ?1)",
        )
        .bind(row.get::<&str, _>("uid"))
        .fetch_all(&mut *connection)
        .await?;
        let delta = exact::sum_exact(
            std::iter::once(exact::read_decimal(&row, "delta")?).chain(
                corrections
                    .iter()
                    .map(|row| exact::read_decimal(row, "delta"))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        )?;
        result.push(Effect {
            transfer: row.get("transfer_uid"),
            exchange: row.get("exchange_uid"),
            person: row.get("person_uid"),
            record: row.get("record_uid"),
            delta,
            fulfilment: row.get::<&str, _>("mode") == "fulfilment",
            unit: row.get("unit_uid"),
        });
    }
    if simulated {
        for row in sqlx::query("SELECT * FROM simulation_assumed_transfer_effect")
            .fetch_all(&mut *connection)
            .await?
        {
            result.push(Effect {
                transfer: row.get("transfer_uid"),
                exchange: row.get("exchange_uid"),
                person: row.get("person_uid"),
                record: row.get("record_uid"),
                delta: exact::read_decimal(&row, "local")?,
                fulfilment: row.get::<&str, _>("mode") == "fulfilment",
                unit: row.get("unit_uid"),
            });
        }
    }
    Ok(result)
}
