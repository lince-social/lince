use crate::{StoreError, receipt_events::Candidate, transfer_receipts::Saved};
use nucleus::{
    karma::CanonicalHash,
    transfer::{
        receipt::Publication,
        receipt_fragment::{self, Reference},
    },
};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

pub async fn index_on(
    connection: &mut SqliteConnection,
    record: &str,
    version: u64,
    publication: &Publication,
) -> Result<(), StoreError> {
    for row in receipt_fragment::references(publication).map_err(invalid)? {
        sqlx::query("INSERT INTO transfer_receipt_event_reference(record_uid,version,event_id,scope_id,source_key,source_commitment,slot_id,parent_event,need_event,issuer_organ) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(record_uid,version,event_id) DO NOTHING")
            .bind(record).bind(version as i64).bind(row.event.as_str()).bind(row.scope.as_str()).bind(row.source.as_str()).bind(row.commitment.as_str()).bind(row.slot.as_str()).bind(row.parent.as_ref().map(|id|id.as_str())).bind(row.need.as_ref().map(|id|id.as_str())).bind(row.issuer).execute(&mut *connection).await?;
    }
    Ok(())
}

pub async fn checked(pool: &SqlitePool, saved: &Saved) -> Result<Vec<Reference>, StoreError> {
    let bundle = saved
        .document
        .public_bundle()
        .ok_or_else(|| invalid("Choose an original public proof artifact"))?;
    let expected = receipt_fragment::references(&bundle.publication).map_err(invalid)?;
    let rows:Vec<(String,String,String,String,String,Option<String>,Option<String>,String)>=sqlx::query_as("SELECT event_id,scope_id,source_key,source_commitment,slot_id,parent_event,need_event,issuer_organ FROM transfer_receipt_event_reference WHERE record_uid=? AND version=? ORDER BY event_id LIMIT 257").bind(&saved.record).bind(saved.version as i64).fetch_all(pool).await?;
    let actual = rows
        .into_iter()
        .map(
            |(event, scope, source, commitment, slot, parent, need, issuer)| {
                Ok(Reference {
                    event: CanonicalHash::parse(event).map_err(invalid)?,
                    scope: CanonicalHash::parse(scope).map_err(invalid)?,
                    source: CanonicalHash::parse(source).map_err(invalid)?,
                    commitment: CanonicalHash::parse(commitment).map_err(invalid)?,
                    slot: CanonicalHash::parse(slot).map_err(invalid)?,
                    parent: parent
                        .map(CanonicalHash::parse)
                        .transpose()
                        .map_err(invalid)?,
                    need: need
                        .map(CanonicalHash::parse)
                        .transpose()
                        .map_err(invalid)?,
                    issuer,
                })
            },
        )
        .collect::<Result<Vec<_>, StoreError>>()?;
    if actual != expected {
        return Err(invalid(
            "Original proof reference index is incomplete or differs from its signed document",
        ));
    }
    Ok(actual)
}

pub async fn lookup(
    pool: &SqlitePool,
    scope: &CanonicalHash,
    events: &[CanonicalHash],
    before: Option<(&str, &str)>,
) -> Result<Vec<Candidate>, StoreError> {
    if events.is_empty() {
        return Ok(vec![]);
    }
    if events.len() > 128 {
        return Err(invalid(
            "Look up at most 128 original proof references per batch",
        ));
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT DISTINCT r.record_uid AS record,length(CAST(v.document_json AS BLOB)) AS bytes,r.created_at,v.version FROM transfer_receipt_event_reference f JOIN transfer_receipt_record r ON r.record_uid=f.record_uid JOIN transfer_receipt_version v ON v.record_uid=f.record_uid AND v.version=f.version JOIN record d ON d.uid=r.record_uid WHERE d.deleted_at IS NULL AND f.version=(SELECT MAX(x.version) FROM transfer_receipt_version x WHERE x.record_uid=r.record_uid) AND f.scope_id=",
    );
    query.push_bind(scope.as_str()).push(" AND f.event_id IN (");
    let mut separated = query.separated(",");
    for event in events {
        separated.push_bind(event.as_str());
    }
    query.push(")");
    if let Some((date, uid)) = before {
        query
            .push(" AND (r.created_at,r.record_uid)<(")
            .push_bind(date)
            .push(",")
            .push_bind(uid)
            .push(")");
    }
    query.push(" ORDER BY r.created_at DESC,r.record_uid DESC LIMIT 65");
    query.build_query_as().fetch_all(pool).await
}

pub async fn scope_candidates(
    pool: &SqlitePool,
    scope: &CanonicalHash,
    before: Option<(&str, &str)>,
) -> Result<Vec<Candidate>, StoreError> {
    sqlx::query_as("SELECT DISTINCT r.record_uid AS record,length(CAST(v.document_json AS BLOB)) AS bytes,r.created_at,v.version FROM transfer_receipt_event_reference f JOIN transfer_receipt_record r ON r.record_uid=f.record_uid JOIN transfer_receipt_version v ON v.record_uid=f.record_uid AND v.version=f.version JOIN record d ON d.uid=r.record_uid WHERE d.deleted_at IS NULL AND f.version=(SELECT MAX(x.version) FROM transfer_receipt_version x WHERE x.record_uid=r.record_uid) AND f.scope_id=? AND (? IS NULL OR (r.created_at,r.record_uid)<(?,?)) ORDER BY r.created_at DESC,r.record_uid DESC LIMIT 65")
        .bind(scope.as_str()).bind(before.map(|position|position.0)).bind(before.map(|position|position.0)).bind(before.map(|position|position.1)).fetch_all(pool).await
}
