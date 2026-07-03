use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use injection::cross_cutting::InjectedServices;
use persistence::write_coordinator::{SqlParameter, WriteOutcome};
use serde_json::json;
use sqlx::Row;
use std::{
    collections::BTreeSet,
    io::{Error, ErrorKind},
};

pub const LOCAL_ORGAN_ID: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOrigin {
    Local,
    Remote {
        source_organ_id: i64,
        source_operation_uid: Option<String>,
    },
    RemoteCrdt {
        source_organ_id: i64,
        update_uid: Option<String>,
    },
}

impl SyncOrigin {
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordSyncAction {
    Insert,
    Update,
    Delete,
}

impl RecordSyncAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordRowSyncIdentity {
    pub record_id: u32,
    pub sync_uid: String,
}

pub async fn ensure_record_identity(
    services: InjectedServices,
    record_id: u32,
) -> Result<RecordRowSyncIdentity, Error> {
    let id = i64::from(record_id);
    let existing = sqlx::query_as::<_, (Option<String>, Option<i64>)>(
        "SELECT sync_uid, owner_organ_id FROM record WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&*services.db)
    .await
    .map_err(Error::other)?
    .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("Record {record_id} not found")))?;

    let sync_uid = existing
        .0
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| local_sync_uid("record", id));
    let owner_organ_id = existing.1.unwrap_or(LOCAL_ORGAN_ID);

    services
        .writer
        .execute_statement(
            "UPDATE record
             SET sync_uid = COALESCE(sync_uid, ?),
                 origin_organ_id = COALESCE(origin_organ_id, ?),
                 owner_organ_id = COALESCE(owner_organ_id, ?),
                 updated_at = COALESCE(updated_at, CURRENT_TIMESTAMP),
                 created_at = COALESCE(created_at, CURRENT_TIMESTAMP)
             WHERE id = ?"
                .to_string(),
            vec![
                SqlParameter::Text(sync_uid.clone()),
                SqlParameter::Integer(LOCAL_ORGAN_ID),
                SqlParameter::Integer(owner_organ_id),
                SqlParameter::Integer(id),
            ],
        )
        .await?;

    Ok(RecordRowSyncIdentity {
        record_id,
        sync_uid,
    })
}

pub async fn ensure_table_row_identity(
    services: InjectedServices,
    table_name: &str,
    row_id: i64,
) -> Result<String, Error> {
    validate_sync_table(table_name)?;
    if row_id <= 0 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("Invalid {table_name} id: {row_id}"),
        ));
    }

    let sql = format!("SELECT sync_uid FROM {table_name} WHERE id = ?");
    let sync_uid = sqlx::query_scalar::<_, Option<String>>(&sql)
        .bind(row_id)
        .fetch_optional(&*services.db)
        .await
        .map_err(Error::other)?
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| local_sync_uid(table_name, row_id));

    let sql = format!(
        "UPDATE {table_name}
         SET sync_uid = COALESCE(sync_uid, ?),
             origin_organ_id = COALESCE(origin_organ_id, ?)
         WHERE id = ?"
    );
    services
        .writer
        .execute_statement(
            sql,
            vec![
                SqlParameter::Text(sync_uid.clone()),
                SqlParameter::Integer(LOCAL_ORGAN_ID),
                SqlParameter::Integer(row_id),
            ],
        )
        .await?;

    Ok(sync_uid)
}

pub async fn enqueue_record_operations(
    services: InjectedServices,
    record_ids: impl IntoIterator<Item = u32>,
    action: RecordSyncAction,
    origin: SyncOrigin,
) -> Result<(), Error> {
    if !origin.is_local() {
        return Ok(());
    }

    let ids = record_ids.into_iter().collect::<BTreeSet<_>>();
    for record_id in ids {
        let identity = match action {
            RecordSyncAction::Delete => RecordRowSyncIdentity {
                record_id,
                sync_uid: local_sync_uid("record", i64::from(record_id)),
            },
            _ => ensure_record_identity(services.clone(), record_id).await?,
        };
        enqueue_operation(
            services.clone(),
            &identity.sync_uid,
            "record",
            &identity.sync_uid,
            action,
            record_payload(services.clone(), identity.record_id).await?,
            None,
        )
        .await?;
    }

    Ok(())
}

pub async fn enqueue_record_text_crdt_updates(
    services: InjectedServices,
    record_ids: impl IntoIterator<Item = u32>,
    origin: SyncOrigin,
) -> Result<(), Error> {
    if !origin.is_local() {
        return Ok(());
    }

    let ids = record_ids.into_iter().collect::<BTreeSet<_>>();
    for record_id in ids {
        let identity = ensure_record_identity(services.clone(), record_id).await?;
        let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT head, body FROM record WHERE id = ?",
        )
        .bind(i64::from(record_id))
        .fetch_optional(&*services.db)
        .await
        .map_err(Error::other)?;
        let Some((head, body)) = row else {
            continue;
        };
        enqueue_text_crdt_update(
            services.clone(),
            &identity.sync_uid,
            "head",
            head.unwrap_or_default(),
        )
        .await?;
        enqueue_text_crdt_update(
            services.clone(),
            &identity.sync_uid,
            "body",
            body.unwrap_or_default(),
        )
        .await?;
    }

    Ok(())
}

async fn enqueue_text_crdt_update(
    services: InjectedServices,
    record_sync_uid: &str,
    field_name: &str,
    materialized_text: String,
) -> Result<(), Error> {
    let clock = operation_clock();
    let document_uid = format!("record:{record_sync_uid}:{field_name}");
    let update_uid = format!(
        "{LOCAL_ORGAN_ID}:{clock}:text:{}:{}",
        field_name,
        record_sync_uid.replace(':', "_")
    );
    let payload = json!({
        "type": "plain_text_replace",
        "text": materialized_text,
    });
    let update_bytes_base64 = BASE64.encode(payload.to_string().as_bytes());
    services
        .writer
        .execute_statement(
            "INSERT OR IGNORE INTO record_text_crdt_update(
                update_uid,
                document_uid,
                record_sync_uid,
                field_name,
                source_organ_id,
                actor_user_id,
                update_clock,
                update_kind,
                update_bytes_base64,
                materialized_text
            ) VALUES (?, ?, ?, ?, ?, NULL, ?, 'delta', ?, ?)"
                .to_string(),
            vec![
                SqlParameter::Text(update_uid),
                SqlParameter::Text(document_uid),
                SqlParameter::Text(record_sync_uid.to_string()),
                SqlParameter::Text(field_name.to_string()),
                SqlParameter::Integer(LOCAL_ORGAN_ID),
                SqlParameter::Text(clock),
                SqlParameter::Text(update_bytes_base64),
                SqlParameter::Text(materialized_text),
            ],
        )
        .await
        .map(|_| ())
}

pub async fn enqueue_record_sidecar_operation(
    services: InjectedServices,
    table_name: &str,
    row_id: i64,
    root_record_id: u32,
    action: RecordSyncAction,
    origin: SyncOrigin,
) -> Result<(), Error> {
    if !origin.is_local() {
        return Ok(());
    }

    let root = ensure_record_identity(services.clone(), root_record_id).await?;
    let row_sync_uid = if action == RecordSyncAction::Delete {
        local_sync_uid(table_name, row_id)
    } else {
        ensure_table_row_identity(services.clone(), table_name, row_id).await?
    };
    enqueue_operation(
        services.clone(),
        &root.sync_uid,
        table_name,
        &row_sync_uid,
        action,
        sidecar_payload(services, table_name, row_id, &root.sync_uid).await?,
        None,
    )
    .await
    .map(|_| ())
}

pub async fn capture_record_identity(
    services: InjectedServices,
    record_id: u32,
) -> Result<RecordRowSyncIdentity, Error> {
    let id = i64::from(record_id);
    let sync_uid =
        sqlx::query_scalar::<_, Option<String>>("SELECT sync_uid FROM record WHERE id = ?")
            .bind(id)
            .fetch_optional(&*services.db)
            .await
            .map_err(Error::other)?
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| local_sync_uid("record", id));
    Ok(RecordRowSyncIdentity {
        record_id,
        sync_uid,
    })
}

pub async fn enqueue_captured_record_delete(
    services: InjectedServices,
    identity: RecordRowSyncIdentity,
    origin: SyncOrigin,
) -> Result<(), Error> {
    if !origin.is_local() {
        return Ok(());
    }
    enqueue_operation(
        services.clone(),
        &identity.sync_uid,
        "record",
        &identity.sync_uid,
        RecordSyncAction::Delete,
        "{}".to_string(),
        None,
    )
    .await?;
    upsert_tombstone(
        services,
        "record",
        &identity.sync_uid,
        &identity.sync_uid,
        LOCAL_ORGAN_ID,
    )
    .await
}

async fn enqueue_operation(
    services: InjectedServices,
    root_record_sync_uid: &str,
    table_name: &str,
    row_sync_uid: &str,
    action: RecordSyncAction,
    field_payload_json: String,
    source_operation_uid: Option<String>,
) -> Result<WriteOutcome, Error> {
    validate_sync_table(table_name)?;
    let clock = operation_clock();
    let operation_uid = format!("{LOCAL_ORGAN_ID}:{clock}:{table_name}:{row_sync_uid}");
    let outcome = services
        .writer
        .execute_statement(
            "INSERT OR IGNORE INTO record_sync_operation(
                operation_uid,
                source_organ_id,
                actor_user_id,
                root_record_sync_uid,
                table_name,
                row_sync_uid,
                action,
                field_payload_json,
                operation_clock,
                source_operation_uid,
                applied_at
            ) VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)"
                .to_string(),
            vec![
                SqlParameter::Text(operation_uid.clone()),
                SqlParameter::Integer(LOCAL_ORGAN_ID),
                SqlParameter::Text(root_record_sync_uid.to_string()),
                SqlParameter::Text(table_name.to_string()),
                SqlParameter::Text(row_sync_uid.to_string()),
                SqlParameter::Text(action.as_str().to_string()),
                SqlParameter::Text(field_payload_json),
                SqlParameter::Text(clock),
                source_operation_uid
                    .map(SqlParameter::Text)
                    .unwrap_or(SqlParameter::Null),
            ],
        )
        .await?;
    tracing::info!(
        operation_uid = %operation_uid,
        source_organ_id = LOCAL_ORGAN_ID,
        root_record_sync_uid,
        table_name,
        row_sync_uid,
        action = action.as_str(),
        rows_affected = outcome.rows_affected,
        "record sync: queued local operation"
    );
    Ok(outcome)
}

async fn upsert_tombstone(
    services: InjectedServices,
    table_name: &str,
    row_sync_uid: &str,
    root_record_sync_uid: &str,
    source_organ_id: i64,
) -> Result<(), Error> {
    validate_sync_table(table_name)?;
    services
        .writer
        .execute_statement(
            "INSERT INTO record_sync_tombstone(
                table_name,
                row_sync_uid,
                root_record_sync_uid,
                delete_clock,
                source_organ_id
            ) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(table_name, row_sync_uid)
            DO UPDATE SET
                root_record_sync_uid = excluded.root_record_sync_uid,
                delete_clock = excluded.delete_clock,
                source_organ_id = excluded.source_organ_id"
                .to_string(),
            vec![
                SqlParameter::Text(table_name.to_string()),
                SqlParameter::Text(row_sync_uid.to_string()),
                SqlParameter::Text(root_record_sync_uid.to_string()),
                SqlParameter::Text(operation_clock()),
                SqlParameter::Integer(source_organ_id),
            ],
        )
        .await
        .map(|_| ())
}

async fn record_payload(services: InjectedServices, record_id: u32) -> Result<String, Error> {
    let row = sqlx::query_as::<
        _,
        (
            i64,
            f64,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
        ),
    >(
        "SELECT id, quantity, head, body, owner_organ_id, sync_uid FROM record WHERE id = ?",
    )
    .bind(i64::from(record_id))
    .fetch_optional(&*services.db)
    .await
    .map_err(Error::other)?;

    let Some((id, quantity, head, body, owner_organ_id, sync_uid)) = row else {
        return Ok("{}".to_string());
    };

    Ok(json!({
        "local_id": id,
        "quantity": quantity,
        "head": head,
        "body": body,
        "owner_organ_id": owner_organ_id,
        "sync_uid": sync_uid,
    })
    .to_string())
}

async fn sidecar_payload(
    services: InjectedServices,
    table_name: &str,
    row_id: i64,
    root_record_sync_uid: &str,
) -> Result<String, Error> {
    validate_sync_table(table_name)?;
    let sql = format!("SELECT * FROM {table_name} WHERE id = ?");
    let row = sqlx::query(&sql)
        .bind(row_id)
        .fetch_optional(&*services.db)
        .await
        .map_err(Error::other)?;
    let Some(row) = row else {
        return Ok("{}".to_string());
    };
    let mut payload = serde_json::Map::new();
    payload.insert("root_record_sync_uid".into(), json!(root_record_sync_uid));
    for column in sync_payload_columns(table_name) {
        payload.insert(
            (*column).into(),
            sqlite_column_to_json(&row, column).unwrap_or(serde_json::Value::Null),
        );
    }
    if table_name == "work_assignment" {
        if let Some(uid) =
            sync_uid_for_local_id(&services, "work_metadata", row.get("work_metadata_id")).await?
        {
            payload.insert("work_metadata_sync_uid".into(), json!(uid));
        }
        if let Some(uid) =
            sync_uid_for_local_id(&services, "work_subject", row.get("work_subject_id")).await?
        {
            payload.insert("work_subject_sync_uid".into(), json!(uid));
        }
    }
    if table_name == "record_link" {
        let target_table: String = row.get("target_table");
        if target_table == "record" {
            if let Some(uid) =
                sync_uid_for_local_id(&services, "record", row.get("target_id")).await?
            {
                payload.insert("target_sync_uid".into(), json!(uid));
            }
        }
    }
    Ok(serde_json::Value::Object(payload).to_string())
}

async fn sync_uid_for_local_id(
    services: &InjectedServices,
    table_name: &str,
    id: i64,
) -> Result<Option<String>, Error> {
    validate_sync_table(table_name)?;
    sqlx::query_scalar::<_, String>(&format!("SELECT sync_uid FROM {table_name} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&*services.db)
        .await
        .map_err(Error::other)
}

fn sync_payload_columns(table_name: &str) -> &'static [&'static str] {
    match table_name {
        "record_extension" => &[
            "record_id",
            "namespace",
            "version",
            "freestyle_data_structure",
            "sync_uid",
            "origin_organ_id",
            "created_at",
            "updated_at",
        ],
        "record_link" => &[
            "record_id",
            "link_type",
            "target_table",
            "target_id",
            "position",
            "freestyle_data_structure",
            "sync_uid",
            "origin_organ_id",
            "created_at",
            "updated_at",
        ],
        "record_comment" => &[
            "record_id",
            "author_user_id",
            "body",
            "created_at",
            "updated_at",
            "deleted_at",
            "sync_uid",
            "origin_organ_id",
        ],
        "record_worklog" => &[
            "record_id",
            "author_user_id",
            "started_at",
            "ended_at",
            "last_heartbeat_at",
            "seconds",
            "note",
            "created_at",
            "updated_at",
            "sync_uid",
            "origin_organ_id",
        ],
        "record_resource_ref" => &[
            "record_id",
            "provider",
            "resource_kind",
            "resource_path",
            "title",
            "position",
            "freestyle_data_structure",
            "created_at",
            "updated_at",
            "sync_uid",
            "origin_organ_id",
        ],
        "work_metadata" => &[
            "owner_kind",
            "owner_id",
            "task_type",
            "status",
            "start_at",
            "end_at",
            "estimate_seconds",
            "completion_notes",
            "metadata_json",
            "created_at",
            "updated_at",
            "sync_uid",
            "origin_organ_id",
        ],
        "work_subject" => &[
            "subject_kind",
            "app_user_id",
            "organ_id",
            "transfer_party_id",
            "remote_base_url",
            "remote_public_key",
            "remote_subject_uid",
            "display_name_snapshot",
            "organ_name_snapshot",
            "created_at",
            "updated_at",
            "sync_uid",
            "origin_organ_id",
        ],
        "work_assignment" => &[
            "work_metadata_id",
            "work_subject_id",
            "assignment_kind",
            "created_at",
            "updated_at",
            "sync_uid",
            "origin_organ_id",
        ],
        _ => &[],
    }
}

fn sqlite_column_to_json(row: &sqlx::sqlite::SqliteRow, column: &str) -> Option<serde_json::Value> {
    use sqlx::Row;
    if let Ok(value) = row.try_get::<Option<String>, _>(column) {
        return Some(json!(value));
    }
    if let Ok(value) = row.try_get::<Option<i64>, _>(column) {
        return Some(json!(value));
    }
    if let Ok(value) = row.try_get::<Option<f64>, _>(column) {
        return Some(json!(value));
    }
    None
}

fn local_sync_uid(table_name: &str, id: i64) -> String {
    format!("organ:{LOCAL_ORGAN_ID}:{table_name}:{id}")
}

fn operation_clock() -> String {
    let now = Utc::now();
    let nanos = now
        .timestamp_nanos_opt()
        .unwrap_or_else(|| now.timestamp_micros() * 1_000);
    format!("{nanos:020}:{LOCAL_ORGAN_ID}")
}

fn validate_sync_table(table_name: &str) -> Result<(), Error> {
    match table_name {
        "record"
        | "record_extension"
        | "record_link"
        | "record_comment"
        | "record_worklog"
        | "record_resource_ref"
        | "work_metadata"
        | "work_subject"
        | "work_assignment" => Ok(()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("Table {table_name} is not part of record sync"),
        )),
    }
}
