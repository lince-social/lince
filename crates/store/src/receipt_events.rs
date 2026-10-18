use crate::{StoreError, transfer_receipts::Saved};
use nucleus::transfer::{
    receipt::Document,
    receipt_history::{self, Projection},
};
use sqlx::{SqliteConnection, SqlitePool};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

pub async fn index_on(
    connection: &mut SqliteConnection,
    record: &str,
    version: u64,
    document: &Document,
) -> Result<(), StoreError> {
    if let Some(bundle) = document.public_bundle() {
        crate::receipt_references::index_on(connection, record, version, &bundle.publication)
            .await?;
    }
    let Document::Publication(bundle) = document else {
        return Ok(());
    };
    for row in receipt_history::project(&bundle.publication).map_err(invalid)? {
        sqlx::query("INSERT INTO transfer_receipt_event(record_uid,version,event_id,source_key,source_binding,slot_id,outcome_id,semantic_hash,parent_outcome,need_outcome,issuer_organ) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(record_uid,version,event_id) DO NOTHING")
            .bind(record).bind(version as i64).bind(row.event.as_str()).bind(row.source.as_str()).bind(row.binding.as_str()).bind(row.slot.as_str()).bind(row.outcome.as_str()).bind(row.semantic.as_str()).bind(row.parent.as_ref().map(|value|value.as_str())).bind(row.need.as_ref().map(|value|value.as_str())).bind(row.issuer).execute(&mut *connection).await?;
    }
    Ok(())
}

pub async fn checked(pool: &SqlitePool, saved: &Saved) -> Result<Vec<Projection>, StoreError> {
    let Document::Publication(bundle) = &saved.document else {
        return Err(invalid("Only public receipt events enter history"));
    };
    let expected = receipt_history::project(&bundle.publication).map_err(invalid)?;
    let rows:Vec<(String,String,String,String,String,String,Option<String>,Option<String>,String)>=sqlx::query_as("SELECT event_id,source_key,source_binding,slot_id,outcome_id,semantic_hash,parent_outcome,need_outcome,issuer_organ FROM transfer_receipt_event WHERE record_uid=? AND version=? ORDER BY event_id LIMIT 257")
        .bind(&saved.record).bind(saved.version as i64).fetch_all(pool).await?;
    let actual = rows
        .into_iter()
        .map(
            |(event, source, binding, slot, outcome, semantic, parent, need, issuer)| {
                Ok(Projection {
                    event: nucleus::karma::CanonicalHash::parse(event).map_err(invalid)?,
                    source: nucleus::karma::CanonicalHash::parse(source).map_err(invalid)?,
                    binding: nucleus::karma::CanonicalHash::parse(binding).map_err(invalid)?,
                    slot: nucleus::karma::CanonicalHash::parse(slot).map_err(invalid)?,
                    outcome: nucleus::karma::CanonicalHash::parse(outcome).map_err(invalid)?,
                    semantic: nucleus::karma::CanonicalHash::parse(semantic).map_err(invalid)?,
                    parent: parent
                        .map(nucleus::karma::CanonicalHash::parse)
                        .transpose()
                        .map_err(invalid)?,
                    need: need
                        .map(nucleus::karma::CanonicalHash::parse)
                        .transpose()
                        .map_err(invalid)?,
                    issuer,
                })
            },
        )
        .collect::<Result<Vec<_>, StoreError>>()?;
    if actual != expected {
        return Err(invalid(
            "Receipt event index is incomplete or differs from the original signed document",
        ));
    }
    Ok(actual)
}

#[derive(sqlx::FromRow)]
pub struct Candidate {
    pub record: String,
    pub bytes: i64,
    pub created_at: String,
    pub version: i64,
}

pub async fn candidates(
    pool: &SqlitePool,
    origin: &str,
    occurrence: &str,
    before: Option<(&str, &str)>,
) -> Result<Vec<Candidate>, StoreError> {
    sqlx::query_as("SELECT r.record_uid AS record,length(CAST(v.document_json AS BLOB)) AS bytes,r.created_at,v.version FROM transfer_receipt_record r JOIN record d ON d.uid=r.record_uid JOIN transfer_receipt_version v ON v.record_uid=r.record_uid AND v.version=(SELECT MAX(x.version) FROM transfer_receipt_version x WHERE x.record_uid=r.record_uid) WHERE r.source_origin=? AND r.source_occurrence=? AND r.kind='publication' AND d.deleted_at IS NULL AND (? IS NULL OR (r.created_at,r.record_uid)<(?,?)) ORDER BY r.created_at DESC,r.record_uid DESC LIMIT 65")
        .bind(origin).bind(occurrence).bind(before.map(|position|position.0)).bind(before.map(|position|position.0)).bind(before.map(|position|position.1)).fetch_all(pool).await
}
