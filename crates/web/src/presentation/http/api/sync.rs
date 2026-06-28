use {
    crate::{application::state::AppState, presentation::http::api_error::api_error},
    axum::{
        extract::{
            Query,
            State,
            ws::{Message, WebSocket, WebSocketUpgrade},
        },
        http::{HeaderMap, StatusCode, header},
        Json,
        response::IntoResponse,
    },
    futures::{SinkExt, StreamExt},
    reqwest::Method,
    serde::{Deserialize, Deserializer, Serialize},
    serde_json::{Map, Value, json},
    sqlx::Row,
    std::{
        collections::BTreeSet,
        hash::{Hash, Hasher},
    },
    utils::logging::{LogEntry, log},
};

const LOCAL_ORGAN_ID: i64 = ::application::record_sync::LOCAL_ORGAN_ID;
const SYNC_TABLES: &[&str] = &[
    "record",
    "record_extension",
    "record_link",
    "record_comment",
    "record_worklog",
    "record_resource_ref",
    "work_metadata",
    "work_subject",
    "work_assignment",
];

fn sync_log(message: impl Into<String>) {
    log(LogEntry::Info(message.into()));
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum RecordSyncSocketFrame {
    Hello {
        resource: &'static str,
        mode: &'static str,
    },
    Operation {
        operation: RecordSyncOperationFrame,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct RecordSyncOperationFrame {
    operation_uid: String,
    source_organ_id: i64,
    actor_user_id: Option<i64>,
    root_record_sync_uid: String,
    table_name: String,
    row_sync_uid: String,
    action: String,
    field_payload_json: String,
    operation_clock: String,
    source_operation_uid: Option<String>,
    created_at: String,
    applied_at: Option<String>,
    sent_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecordSyncSnapshotQuery {
    #[serde(default, deserialize_with = "deserialize_owner_organ_ids")]
    owner_organ_id: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct RecordSyncOperationsQuery {
    since_clock: Option<String>,
    #[serde(default, deserialize_with = "deserialize_owner_organ_ids")]
    owner_organ_id: Vec<i64>,
}

fn deserialize_owner_organ_ids<'de, D>(deserializer: D) -> Result<Vec<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OwnerIds {
        One(String),
        Many(Vec<String>),
    }

    let Some(value) = Option::<OwnerIds>::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };
    let raw = match value {
        OwnerIds::One(value) => vec![value],
        OwnerIds::Many(values) => values,
    };
    Ok(raw
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .filter_map(|part| part.parse::<i64>().ok())
                .collect::<Vec<_>>()
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSyncRowFrame {
    table_name: String,
    row_sync_uid: String,
    root_record_sync_uid: String,
    row: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSyncSnapshotResponse {
    rows: Vec<RecordSyncRowFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSyncOperationsResponse {
    operations: Vec<RecordSyncOperationFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSyncFingerprintResponse {
    fingerprint: String,
    row_count: i64,
    max_operation_clock: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSyncApplyRequest {
    source_base_url: Option<String>,
    source_name: Option<String>,
    operations: Vec<RecordSyncOperationFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSyncApplyResponse {
    applied: usize,
    skipped: usize,
}

pub async fn connect_record_sync_socket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let claims = match state.backend.authenticate_authorization(auth_header).await {
        Ok(claims) => claims,
        Err(error) => {
            return api_error(StatusCode::UNAUTHORIZED, error.to_string()).into_response();
        }
    };
    if let Err(error) =
        claims.require_permission(::application::auth::PermissionKey::new("record", "read"))
    {
        tracing::warn!(
            user_id = claims.user_id,
            username = %claims.username,
            error = %error,
            "record sync: socket rejected by permission"
        );
        return api_error(StatusCode::FORBIDDEN, error.to_string()).into_response();
    }

    tracing::info!(
        user_id = claims.user_id,
        username = %claims.username,
        "record sync: socket authenticated"
    );
    ws.max_frame_size(2 * 1024 * 1024)
        .max_message_size(2 * 1024 * 1024)
        .on_upgrade(move |socket| run_record_sync_socket(socket, state, claims))
        .into_response()
}

pub async fn record_sync_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RecordSyncSnapshotQuery>,
) -> impl IntoResponse {
    let claims = match authenticate_record_sync(&state, &headers).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if let Err(error) =
        claims.require_permission(::application::auth::PermissionKey::new("record", "read"))
    {
        return api_error(StatusCode::FORBIDDEN, error.to_string()).into_response();
    }
    match build_snapshot(&state, &query.owner_organ_id).await {
        Ok(snapshot) => {
            sync_log(format!(
                "record sync: served snapshot owners={:?} rows={}",
                query.owner_organ_id,
                snapshot.rows.len()
            ));
            Json(snapshot).into_response()
        }
        Err(error) => {
            sync_log(format!(
                "record sync: snapshot request failed owners={:?} error={error}",
                query.owner_organ_id
            ));
            api_error(StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    }
}

pub async fn record_sync_operations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RecordSyncOperationsQuery>,
) -> impl IntoResponse {
    let claims = match authenticate_record_sync(&state, &headers).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if let Err(error) =
        claims.require_permission(::application::auth::PermissionKey::new("record", "read"))
    {
        return api_error(StatusCode::FORBIDDEN, error.to_string()).into_response();
    }
    match load_operations_since(&state, query.since_clock.as_deref(), &query.owner_organ_id).await {
        Ok(operations) => {
            sync_log(format!(
                "record sync: served operations owners={:?} since={:?} operations={}",
                query.owner_organ_id,
                query.since_clock,
                operations.len()
            ));
            Json(RecordSyncOperationsResponse { operations }).into_response()
        }
        Err(error) => {
            sync_log(format!(
                "record sync: operations request failed owners={:?} since={:?} error={error}",
                query.owner_organ_id,
                query.since_clock
            ));
            api_error(StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    }
}

pub async fn record_sync_fingerprint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RecordSyncSnapshotQuery>,
) -> impl IntoResponse {
    let claims = match authenticate_record_sync(&state, &headers).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if let Err(error) =
        claims.require_permission(::application::auth::PermissionKey::new("record", "read"))
    {
        return api_error(StatusCode::FORBIDDEN, error.to_string()).into_response();
    }
    match build_fingerprint(&state, &query.owner_organ_id).await {
        Ok(response) => {
            sync_log(format!(
                "record sync: served fingerprint owners={:?} rows={} fingerprint={}",
                query.owner_organ_id, response.row_count, response.fingerprint
            ));
            Json(response).into_response()
        }
        Err(error) => {
            sync_log(format!(
                "record sync: fingerprint request failed owners={:?} error={error}",
                query.owner_organ_id
            ));
            api_error(StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    }
}

pub async fn apply_record_sync_operations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<RecordSyncApplyRequest>,
) -> impl IntoResponse {
    let claims = match authenticate_record_sync(&state, &headers).await {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    for permission in [
        ::application::auth::PermissionKey::new("record", "create"),
        ::application::auth::PermissionKey::new("record", "update"),
        ::application::auth::PermissionKey::new("record", "delete"),
    ] {
        if let Err(error) = claims.require_permission(permission) {
            return api_error(StatusCode::FORBIDDEN, error.to_string()).into_response();
        }
    }
    match apply_operations_from_peer(&state, payload).await {
        Ok(response) => {
            sync_log(format!(
                "record sync: accepted pushed operations applied={} skipped={}",
                response.applied, response.skipped
            ));
            Json(response).into_response()
        }
        Err(error) => {
            sync_log(format!("record sync: applying pushed operations failed error={error}"));
            api_error(StatusCode::BAD_GATEWAY, error.to_string()).into_response()
        }
    }
}

async fn authenticate_record_sync(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<::application::auth::AuthSubject, axum::response::Response> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    state
        .backend
        .authenticate_authorization(auth_header)
        .await
        .map_err(|error| api_error(StatusCode::UNAUTHORIZED, error.to_string()).into_response())
}

async fn run_record_sync_socket(
    socket: WebSocket,
    state: AppState,
    claims: ::application::auth::AuthSubject,
) {
    let (mut sender, mut receiver) = socket.split();
    if send_socket_frame(
        &mut sender,
        RecordSyncSocketFrame::Hello {
            resource: "record",
            mode: "catch_up_read",
        },
    )
    .await
    .is_err()
    {
        tracing::warn!(
            user_id = claims.user_id,
            username = %claims.username,
            "record sync: failed to send hello frame"
        );
        return;
    }
    tracing::info!(
        user_id = claims.user_id,
        username = %claims.username,
        "record sync: sent hello frame"
    );

    let operations = match load_recent_operations(&state).await {
        Ok(operations) => operations,
        Err(error) => {
            tracing::warn!(
                user_id = claims.user_id,
                username = %claims.username,
                error = %error,
                "record sync: failed to load operations for socket catch-up"
            );
            let _ = send_socket_frame(
                &mut sender,
                RecordSyncSocketFrame::Error {
                    message: error.to_string(),
                },
            )
            .await;
            return;
        }
    };
    tracing::info!(
        user_id = claims.user_id,
        username = %claims.username,
        operations = operations.len(),
        "record sync: loaded operations for socket catch-up"
    );

    for operation in operations {
        let operation_uid = operation.operation_uid.clone();
        let source_organ_id = operation.source_organ_id;
        let table_name = operation.table_name.clone();
        let row_sync_uid = operation.row_sync_uid.clone();
        let action = operation.action.clone();
        if send_socket_frame(
            &mut sender,
            RecordSyncSocketFrame::Operation { operation },
        )
        .await
        .is_err()
        {
            tracing::warn!(
                user_id = claims.user_id,
                username = %claims.username,
                operation_uid = %operation_uid,
                source_organ_id,
                table_name = %table_name,
                row_sync_uid = %row_sync_uid,
                action = %action,
                "record sync: failed to send operation frame"
            );
            return;
        }
        tracing::debug!(
            user_id = claims.user_id,
            username = %claims.username,
            operation_uid = %operation_uid,
            source_organ_id,
            table_name = %table_name,
            row_sync_uid = %row_sync_uid,
            action = %action,
            "record sync: sent operation frame"
        );
    }

    while let Some(message) = receiver.next().await {
        match message {
            Ok(Message::Close(_)) => {
                tracing::info!(
                    user_id = claims.user_id,
                    username = %claims.username,
                    "record sync: socket closed by peer"
                );
                break;
            }
            Err(error) => {
                tracing::warn!(
                    user_id = claims.user_id,
                    username = %claims.username,
                    error = %error,
                    "record sync: socket receive failed"
                );
                break;
            }
            Ok(Message::Ping(bytes)) => {
                if sender.send(Message::Pong(bytes)).await.is_err() {
                    tracing::warn!(
                        user_id = claims.user_id,
                        username = %claims.username,
                        "record sync: failed to send pong"
                    );
                    break;
                }
            }
            Ok(Message::Text(_)) | Ok(Message::Binary(_)) => {
                tracing::warn!(
                    user_id = claims.user_id,
                    username = %claims.username,
                    "record sync: incoming operation frames are not applied yet"
                );
            }
            _ => {}
        }
    }
}

async fn load_recent_operations(
    state: &AppState,
) -> Result<Vec<RecordSyncOperationFrame>, sqlx::Error> {
    tracing::debug!("record sync: loading recent operations");
    sqlx::query_as::<_, RecordSyncOperationFrame>(
        "SELECT
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
            created_at,
            applied_at,
            sent_at
         FROM record_sync_operation
         ORDER BY operation_clock, source_organ_id, operation_uid
         LIMIT 500",
    )
    .fetch_all(&*state.services.db)
    .await
}

async fn build_snapshot(
    state: &AppState,
    owner_organ_ids: &[i64],
) -> Result<RecordSyncSnapshotResponse, sqlx::Error> {
    let owners = normalized_owners(owner_organ_ids);
    ensure_snapshot_identities(state, &owners).await?;

    let mut rows = Vec::new();
    let record_rows = sqlx::query(
        "SELECT id, quantity, head, body, owner_organ_id, sync_uid, origin_organ_id, created_at, updated_at
         FROM record
         WHERE COALESCE(owner_organ_id, 1) IN (SELECT value FROM json_each(?))
         ORDER BY id",
    )
    .bind(json!(owners).to_string())
    .fetch_all(&*state.services.db)
    .await?;
    let mut root_sync_uids = Vec::new();
    for row in record_rows {
        let row_value = record_row_value(&row);
        let sync_uid = row.get::<String, _>("sync_uid");
        root_sync_uids.push(sync_uid.clone());
        rows.push(RecordSyncRowFrame {
            table_name: "record".to_string(),
            row_sync_uid: sync_uid.clone(),
            root_record_sync_uid: sync_uid,
            row: row_value,
        });
    }

    for table in SYNC_TABLES.iter().copied().filter(|table| *table != "record") {
        rows.extend(snapshot_sidecar_rows(state, table, &root_sync_uids).await?);
    }

    Ok(RecordSyncSnapshotResponse { rows })
}

async fn build_fingerprint(
    state: &AppState,
    owner_organ_ids: &[i64],
) -> Result<RecordSyncFingerprintResponse, sqlx::Error> {
    let owners = normalized_owners(owner_organ_ids);
    ensure_snapshot_identities(state, &owners).await?;
    let owners_json = json!(owners).to_string();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut row_count = 0_i64;

    let records = sqlx::query(
        "SELECT sync_uid, updated_at
         FROM record
         WHERE COALESCE(owner_organ_id, 1) IN (SELECT value FROM json_each(?))
         ORDER BY sync_uid",
    )
    .bind(&owners_json)
    .fetch_all(&*state.services.db)
    .await?;
    let root_sync_uids = records
        .iter()
        .map(|row| row.get::<String, _>("sync_uid"))
        .collect::<Vec<_>>();
    for row in records {
        row_count += 1;
        "record".hash(&mut hasher);
        row.get::<String, _>("sync_uid").hash(&mut hasher);
        row.get::<Option<String>, _>("updated_at").hash(&mut hasher);
    }

    for table in SYNC_TABLES.iter().copied().filter(|table| *table != "record") {
        for row in fingerprint_sidecar_rows(state, table, &root_sync_uids).await? {
            row_count += 1;
            table.hash(&mut hasher);
            row.0.hash(&mut hasher);
            row.1.hash(&mut hasher);
        }
    }

    for row in sqlx::query(
        "SELECT table_name, row_sync_uid, delete_clock
         FROM record_sync_tombstone
         ORDER BY table_name, row_sync_uid",
    )
    .fetch_all(&*state.services.db)
    .await?
    {
        "tombstone".hash(&mut hasher);
        row.get::<String, _>("table_name").hash(&mut hasher);
        row.get::<String, _>("row_sync_uid").hash(&mut hasher);
        row.get::<String, _>("delete_clock").hash(&mut hasher);
    }

    let max_operation_clock = sqlx::query_scalar::<_, String>(
        "SELECT MAX(operation_clock) FROM record_sync_operation",
    )
    .fetch_optional(&*state.services.db)
    .await?;

    Ok(RecordSyncFingerprintResponse {
        fingerprint: format!("{:016x}", hasher.finish()),
        row_count,
        max_operation_clock,
    })
}

async fn fingerprint_sidecar_rows(
    state: &AppState,
    table: &str,
    root_sync_uids: &[String],
) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
    if root_sync_uids.is_empty() {
        return Ok(Vec::new());
    }
    let roots_json = json!(root_sync_uids).to_string();
    let sql = match table {
        "record_extension" => "SELECT t.sync_uid, t.updated_at FROM record_extension t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.sync_uid",
        "record_link" => "SELECT t.sync_uid, t.updated_at FROM record_link t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.sync_uid",
        "record_comment" => "SELECT t.sync_uid, t.updated_at FROM record_comment t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.sync_uid",
        "record_worklog" => "SELECT t.sync_uid, t.updated_at FROM record_worklog t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.sync_uid",
        "record_resource_ref" => "SELECT t.sync_uid, t.updated_at FROM record_resource_ref t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.sync_uid",
        "work_metadata" => "SELECT t.sync_uid, t.updated_at FROM work_metadata t JOIN record r ON r.id = t.owner_id AND t.owner_kind = 'record' WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.sync_uid",
        "work_subject" => "SELECT DISTINCT t.sync_uid, t.updated_at FROM work_subject t JOIN work_assignment wa ON wa.work_subject_id = t.id JOIN work_metadata wm ON wm.id = wa.work_metadata_id JOIN record r ON r.id = wm.owner_id AND wm.owner_kind = 'record' WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.sync_uid",
        "work_assignment" => "SELECT t.sync_uid, t.updated_at FROM work_assignment t JOIN work_metadata wm ON wm.id = t.work_metadata_id JOIN record r ON r.id = wm.owner_id AND wm.owner_kind = 'record' WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.sync_uid",
        _ => return Ok(Vec::new()),
    };
    Ok(sqlx::query(sql)
        .bind(roots_json)
        .fetch_all(&*state.services.db)
        .await?
        .into_iter()
        .map(|row| (row.get("sync_uid"), row.get("updated_at")))
        .collect())
}

async fn ensure_snapshot_identities(state: &AppState, owners: &[i64]) -> Result<(), sqlx::Error> {
    let owners_json = json!(owners).to_string();
    state
        .services
        .writer
        .execute_statement(
            "UPDATE record
         SET sync_uid = COALESCE(sync_uid, 'organ:1:record:' || id),
             owner_organ_id = COALESCE(owner_organ_id, 1),
             origin_organ_id = COALESCE(origin_organ_id, COALESCE(owner_organ_id, 1)),
             created_at = COALESCE(created_at, CURRENT_TIMESTAMP),
             updated_at = COALESCE(updated_at, CURRENT_TIMESTAMP)
         WHERE COALESCE(owner_organ_id, 1) IN (SELECT value FROM json_each(?))"
                .to_string(),
            vec![text(owners_json.clone())],
        )
        .await
        .map_err(sqlx::Error::Io)?;

    for table in SYNC_TABLES.iter().copied().filter(|table| *table != "record") {
        let sql = match table {
            "work_metadata" => {
                "UPDATE work_metadata
                 SET sync_uid = COALESCE(sync_uid, 'organ:1:work_metadata:' || id),
                     origin_organ_id = COALESCE(origin_organ_id, 1)
                 WHERE owner_kind = 'record'
                   AND owner_id IN (
                     SELECT id FROM record WHERE COALESCE(owner_organ_id, 1) IN (SELECT value FROM json_each(?))
                   )"
            }
            "work_subject" => {
                "UPDATE work_subject
                 SET sync_uid = COALESCE(sync_uid, 'organ:1:work_subject:' || id),
                     origin_organ_id = COALESCE(origin_organ_id, 1)
                 WHERE id IN (
                   SELECT wa.work_subject_id
                   FROM work_assignment wa
                   JOIN work_metadata wm ON wm.id = wa.work_metadata_id
                   JOIN record r ON r.id = wm.owner_id AND wm.owner_kind = 'record'
                   WHERE COALESCE(r.owner_organ_id, 1) IN (SELECT value FROM json_each(?))
                 )"
            }
            "work_assignment" => {
                "UPDATE work_assignment
                 SET sync_uid = COALESCE(sync_uid, 'organ:1:work_assignment:' || id),
                     origin_organ_id = COALESCE(origin_organ_id, 1)
                 WHERE work_metadata_id IN (
                   SELECT wm.id
                   FROM work_metadata wm
                   JOIN record r ON r.id = wm.owner_id AND wm.owner_kind = 'record'
                   WHERE COALESCE(r.owner_organ_id, 1) IN (SELECT value FROM json_each(?))
                 )"
            }
            _ => "",
        };
        if !sql.is_empty() {
            state
                .services
                .writer
                .execute_statement(sql.to_string(), vec![text(owners_json.clone())])
                .await
                .map_err(sqlx::Error::Io)?;
        } else if let Some(fk) = record_fk_column(table) {
            state
                .services
                .writer
                .execute_statement(
                    format!(
                        "UPDATE {table}
                 SET sync_uid = COALESCE(sync_uid, 'organ:1:{table}:' || id),
                     origin_organ_id = COALESCE(origin_organ_id, 1)
                 WHERE {fk} IN (
                   SELECT id FROM record WHERE COALESCE(owner_organ_id, 1) IN (SELECT value FROM json_each(?))
                 )"
                    ),
                    vec![text(owners_json.clone())],
                )
                .await
                .map_err(sqlx::Error::Io)?;
        }
    }
    Ok(())
}

async fn snapshot_sidecar_rows(
    state: &AppState,
    table: &str,
    root_sync_uids: &[String],
) -> Result<Vec<RecordSyncRowFrame>, sqlx::Error> {
    if root_sync_uids.is_empty() {
        return Ok(Vec::new());
    }
    let roots_json = json!(root_sync_uids).to_string();
    let (sql, root_column) = match table {
        "record_extension" => (
            "SELECT t.*, r.sync_uid AS root_sync_uid FROM record_extension t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.id",
            "record_id",
        ),
        "record_link" => (
            "SELECT t.*, r.sync_uid AS root_sync_uid FROM record_link t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.id",
            "record_id",
        ),
        "record_comment" => (
            "SELECT t.*, r.sync_uid AS root_sync_uid FROM record_comment t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.id",
            "record_id",
        ),
        "record_worklog" => (
            "SELECT t.*, r.sync_uid AS root_sync_uid FROM record_worklog t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.id",
            "record_id",
        ),
        "record_resource_ref" => (
            "SELECT t.*, r.sync_uid AS root_sync_uid FROM record_resource_ref t JOIN record r ON r.id = t.record_id WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.id",
            "record_id",
        ),
        "work_metadata" => (
            "SELECT t.*, r.sync_uid AS root_sync_uid FROM work_metadata t JOIN record r ON r.id = t.owner_id AND t.owner_kind = 'record' WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.id",
            "owner_id",
        ),
        "work_subject" => (
            "SELECT DISTINCT t.*, r.sync_uid AS root_sync_uid FROM work_subject t JOIN work_assignment wa ON wa.work_subject_id = t.id JOIN work_metadata wm ON wm.id = wa.work_metadata_id JOIN record r ON r.id = wm.owner_id AND wm.owner_kind = 'record' WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.id",
            "",
        ),
        "work_assignment" => (
            "SELECT t.*, r.sync_uid AS root_sync_uid FROM work_assignment t JOIN work_metadata wm ON wm.id = t.work_metadata_id JOIN record r ON r.id = wm.owner_id AND wm.owner_kind = 'record' WHERE r.sync_uid IN (SELECT value FROM json_each(?)) ORDER BY t.id",
            "",
        ),
        _ => return Ok(Vec::new()),
    };
    let query_rows = sqlx::query(sql)
        .bind(roots_json)
        .fetch_all(&*state.services.db)
        .await?;
    let mut rows = Vec::with_capacity(query_rows.len());
    for row in query_rows {
        let sync_uid = row.get::<String, _>("sync_uid");
        let root_sync_uid = row.get::<String, _>("root_sync_uid");
        let mut value = row_to_value(table, &row);
        if !root_column.is_empty() {
            value["root_record_sync_uid"] = Value::String(root_sync_uid.clone());
        }
        if table == "work_assignment" {
            if let Some(uid) = related_sync_uid(state, "work_metadata", row.get("work_metadata_id")).await? {
                value["work_metadata_sync_uid"] = Value::String(uid);
            }
            if let Some(uid) = related_sync_uid(state, "work_subject", row.get("work_subject_id")).await? {
                value["work_subject_sync_uid"] = Value::String(uid);
            }
        }
        rows.push(RecordSyncRowFrame {
            table_name: table.to_string(),
            row_sync_uid: sync_uid,
            root_record_sync_uid: root_sync_uid,
            row: value,
        });
    }
    Ok(rows)
}

async fn load_operations_since(
    state: &AppState,
    since_clock: Option<&str>,
    owner_organ_ids: &[i64],
) -> Result<Vec<RecordSyncOperationFrame>, sqlx::Error> {
    let operations = sqlx::query_as::<_, RecordSyncOperationFrame>(
        "SELECT
            operation_uid, source_organ_id, actor_user_id, root_record_sync_uid, table_name,
            row_sync_uid, action, field_payload_json, operation_clock, source_operation_uid,
            created_at, applied_at, sent_at
         FROM record_sync_operation
         WHERE (? IS NULL OR operation_clock > ?)
         ORDER BY operation_clock, source_organ_id, operation_uid
         LIMIT 500",
    )
    .bind(since_clock)
    .bind(since_clock)
    .fetch_all(&*state.services.db)
    .await?;
    if owner_organ_ids.is_empty() {
        return Ok(operations);
    }
    let owners = normalized_owners(owner_organ_ids);
    let owners_set = owners.into_iter().collect::<BTreeSet<_>>();
    let mut filtered = Vec::new();
    for operation in operations {
        if operation_matches_owners(state, &operation, &owners_set).await? {
            filtered.push(operation);
        }
    }
    Ok(filtered)
}

async fn operation_matches_owners(
    state: &AppState,
    operation: &RecordSyncOperationFrame,
    owners: &BTreeSet<i64>,
) -> Result<bool, sqlx::Error> {
    let owner = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT owner_organ_id FROM record WHERE sync_uid = ? LIMIT 1",
    )
    .bind(&operation.root_record_sync_uid)
    .fetch_optional(&*state.services.db)
    .await?
    .flatten()
    .unwrap_or(LOCAL_ORGAN_ID);
    Ok(owners.contains(&owner))
}

async fn owner_for_operation(
    state: &AppState,
    operation: &RecordSyncOperationFrame,
) -> Result<i64, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, Option<i64>>(
        "SELECT owner_organ_id FROM record WHERE sync_uid = ? LIMIT 1",
    )
    .bind(&operation.root_record_sync_uid)
    .fetch_optional(&*state.services.db)
    .await?
    .flatten()
    .unwrap_or(LOCAL_ORGAN_ID))
}

async fn apply_operations_from_peer(
    state: &AppState,
    payload: RecordSyncApplyRequest,
) -> Result<RecordSyncApplyResponse, sqlx::Error> {
    let peer_organ_id = match payload.source_base_url.as_deref() {
        Some(base_url) if !base_url.trim().is_empty() => ensure_peer_organ(state, base_url, payload.source_name.as_deref()).await?,
        _ => LOCAL_ORGAN_ID,
    };
    let mut applied = 0_usize;
    let mut skipped = 0_usize;
    for mut operation in payload.operations {
        remap_operation_for_receiver(&mut operation, peer_organ_id);
        match apply_operation(state, &operation, peer_organ_id).await {
            Ok(true) => applied += 1,
            Ok(false) => skipped += 1,
            Err(error) => {
                tracing::warn!(
                    operation_uid = %operation.operation_uid,
                    error = %error,
                    "record sync: failed to apply incoming operation"
                );
                skipped += 1;
            }
        }
    }
    Ok(RecordSyncApplyResponse { applied, skipped })
}

async fn ensure_peer_organ(
    state: &AppState,
    base_url: &str,
    name: Option<&str>,
) -> Result<i64, sqlx::Error> {
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM organ WHERE base_url = ? LIMIT 1")
        .bind(base_url.trim().trim_end_matches('/'))
        .fetch_optional(&*state.services.db)
        .await?
    {
        return Ok(id);
    }
    let name = name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Synced organ");
    let outcome = state
        .services
        .writer
        .execute_statement_returning_id(
            "INSERT INTO organ(name, base_url, trust_state) VALUES (?, ?, 'known') RETURNING id".to_string(),
            vec![
                persistence::write_coordinator::SqlParameter::Text(name.to_string()),
                persistence::write_coordinator::SqlParameter::Text(base_url.trim().trim_end_matches('/').to_string()),
            ],
        )
        .await
        .map_err(sqlx::Error::Io)?;
    Ok(outcome.last_insert_rowid.unwrap_or(LOCAL_ORGAN_ID))
}

async fn apply_operation(
    state: &AppState,
    operation: &RecordSyncOperationFrame,
    peer_organ_id: i64,
) -> Result<bool, sqlx::Error> {
    if operation.source_operation_uid.as_deref() == Some(&operation.operation_uid) {
        return Ok(false);
    }
    if sqlx::query_scalar::<_, i64>(
        "SELECT id FROM record_sync_operation WHERE operation_uid = ? LIMIT 1",
    )
    .bind(&operation.operation_uid)
    .fetch_optional(&*state.services.db)
    .await?
    .is_some()
    {
        return Ok(false);
    }

    if operation.action == "delete" {
        apply_delete(state, operation).await?;
    } else {
        apply_upsert(state, operation, peer_organ_id).await?;
    }
    store_applied_operation(state, operation).await?;
    Ok(true)
}

async fn apply_delete(state: &AppState, operation: &RecordSyncOperationFrame) -> Result<(), sqlx::Error> {
    let sql = format!("DELETE FROM {} WHERE sync_uid = ?", operation.table_name);
    state
        .services
        .writer
        .execute_statement(sql, vec![persistence::write_coordinator::SqlParameter::Text(operation.row_sync_uid.clone())])
        .await
        .map_err(sqlx::Error::Io)?;
    Ok(())
}

async fn apply_upsert(
    state: &AppState,
    operation: &RecordSyncOperationFrame,
    peer_organ_id: i64,
) -> Result<(), sqlx::Error> {
    let mut row = serde_json::from_str::<Value>(&operation.field_payload_json)
        .unwrap_or_else(|_| Value::Object(Map::new()));
    if !row.is_object() {
        row = Value::Object(Map::new());
    }
    remap_row_for_receiver(&mut row, peer_organ_id);
    resolve_row_references(state, &operation.table_name, &mut row).await?;
    let Some(columns) = writable_columns(&operation.table_name) else {
        return Ok(());
    };
    let existing_id = sqlx::query_scalar::<_, i64>(&format!(
        "SELECT id FROM {} WHERE sync_uid = ? LIMIT 1",
        operation.table_name
    ))
    .bind(&operation.row_sync_uid)
    .fetch_optional(&*state.services.db)
    .await?;

    if let Some(id) = existing_id {
        let assignments = columns
            .iter()
            .filter(|column| row.get(**column).is_some())
            .map(|column| format!("{column} = ?"))
            .collect::<Vec<_>>();
        if assignments.is_empty() {
            return Ok(());
        }
        let mut params = columns
            .iter()
            .filter_map(|column| row.get(*column).map(sql_param_from_json))
            .collect::<Vec<_>>();
        params.push(persistence::write_coordinator::SqlParameter::Integer(id));
        state
            .services
            .writer
            .execute_statement(
                format!(
                    "UPDATE {} SET {} WHERE id = ?",
                    operation.table_name,
                    assignments.join(", ")
                ),
                params,
            )
            .await
            .map_err(sqlx::Error::Io)?;
    } else {
        let insert_columns = columns
            .iter()
            .filter(|column| row.get(**column).is_some())
            .copied()
            .collect::<Vec<_>>();
        if insert_columns.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; insert_columns.len()].join(", ");
        let params = insert_columns
            .iter()
            .filter_map(|column| row.get(*column).map(sql_param_from_json))
            .collect::<Vec<_>>();
        state
            .services
            .writer
            .execute_statement(
                format!(
                    "INSERT INTO {} ({}) VALUES ({placeholders})",
                    operation.table_name,
                    insert_columns.join(", ")
                ),
                params,
            )
            .await
            .map_err(sqlx::Error::Io)?;
    }
    Ok(())
}

async fn store_applied_operation(
    state: &AppState,
    operation: &RecordSyncOperationFrame,
) -> Result<(), sqlx::Error> {
    state
        .services
        .writer
        .execute_statement(
            "INSERT OR IGNORE INTO record_sync_operation(
                operation_uid, source_organ_id, actor_user_id, root_record_sync_uid, table_name,
                row_sync_uid, action, field_payload_json, operation_clock, source_operation_uid, applied_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)"
                .to_string(),
            vec![
                text(operation.operation_uid.clone()),
                int(operation.source_organ_id),
                optional_int(operation.actor_user_id),
                text(operation.root_record_sync_uid.clone()),
                text(operation.table_name.clone()),
                text(operation.row_sync_uid.clone()),
                text(operation.action.clone()),
                text(operation.field_payload_json.clone()),
                text(operation.operation_clock.clone()),
                optional_text(operation.source_operation_uid.clone()),
            ],
        )
        .await
        .map_err(sqlx::Error::Io)?;
    Ok(())
}

pub async fn run_record_sync_for_organ(
    state: AppState,
    organ_id: i64,
    bearer_token: String,
) -> Result<(), String> {
    sync_log(format!(
        "record sync: run requested organ_id={organ_id} token_present={}",
        !bearer_token.trim().is_empty()
    ));
    let organ = state
        .organs
        .get(organ_id)
        .await?
        .ok_or_else(|| format!("Organ {organ_id} not found"))?;
    let (_, mode) = super::servers::load_sync_policy(&state, organ_id)
        .await
        .map_err(|(_, error)| error.error.clone())?;
    let owners = owners_for_mode(&mode, organ_id);
    sync_log(format!(
        "record sync: policy resolved organ_id={organ_id} mode={mode} owners={owners:?} base_url={}",
        organ.base_url
    ));
    if owners.is_empty() {
        tracing::info!(organ_id, mode = %mode, "record sync: skipped disabled policy");
        sync_log(format!(
            "record sync: skipped disabled policy organ_id={organ_id} mode={mode}"
        ));
        return Ok(());
    }

    tracing::info!(organ_id, mode = %mode, "record sync: run started");
    sync_log(format!("record sync: run started organ_id={organ_id} mode={mode}"));
    if should_refresh_from_remote(&state, &organ.base_url, &bearer_token, organ_id, &owners).await?
    {
        pull_snapshot(&state, &organ.base_url, &bearer_token, organ_id, &owners).await?;
        pull_operations(&state, &organ.base_url, &bearer_token, organ_id, &owners).await?;
    } else {
        sync_log(format!(
            "record sync: fingerprint matched; skipped snapshot/pull organ_id={organ_id}"
        ));
    }
    push_operations(&state, &organ.base_url, &bearer_token, organ_id, &owners).await?;
    tracing::info!(organ_id, mode = %mode, "record sync: run finished");
    sync_log(format!("record sync: run finished organ_id={organ_id} mode={mode}"));
    Ok(())
}

pub fn spawn_record_sync_tasks(state: AppState) {
    sync_log("record sync: background task spawned");
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut logged_empty_tokens = false;
        loop {
            interval.tick().await;
            let tokens = state
                .services
                .remote_organ_auth
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            if tokens.is_empty() {
                if !logged_empty_tokens {
                    sync_log("record sync: background tick found no authenticated remote organ tokens");
                    logged_empty_tokens = true;
                }
                continue;
            }
            logged_empty_tokens = false;
            sync_log(format!(
                "record sync: background tick found {} authenticated remote organ token(s)",
                tokens.len()
            ));
            for (organ_id, bearer_token) in tokens {
                if !has_due_sync_work(&state, organ_id).await.unwrap_or(true) {
                    continue;
                }
                if let Err(error) =
                    run_record_sync_for_organ(state.clone(), organ_id, bearer_token).await
                {
                    sync_log(format!(
                        "record sync: background sync failed organ_id={organ_id} error={error}"
                    ));
                    tracing::warn!(
                        organ_id,
                        error = %error,
                        "record sync: background sync failed"
                    );
                }
            }
        }
    });
}

async fn has_due_sync_work(state: &AppState, organ_id: i64) -> Result<bool, sqlx::Error> {
    if has_unsent_operations_for_organ(state, organ_id).await? {
        return Ok(true);
    }
    let interval = sync_check_interval_seconds(state, organ_id).await?;
    if interval == 0 {
        return Ok(false);
    }
    let owner_scope = owner_scope_for_organ(state, organ_id).await?;
    let due = sqlx::query_scalar::<_, i64>(
        "SELECT CASE
            WHEN next_check_at IS NULL THEN 1
            WHEN julianday(next_check_at) <= julianday(CURRENT_TIMESTAMP) THEN 1
            ELSE 0
         END
         FROM record_sync_peer_state
         WHERE organ_id = ? AND owner_scope = ?",
    )
    .bind(organ_id)
    .bind(&owner_scope)
    .fetch_optional(&*state.services.db)
    .await?
    .unwrap_or(1);
    Ok(due != 0)
}

async fn has_unsent_operations_for_organ(
    state: &AppState,
    organ_id: i64,
) -> Result<bool, sqlx::Error> {
    let (_, mode) = super::servers::load_sync_policy(state, organ_id)
        .await
        .map_err(|(_, error)| sqlx::Error::Protocol(error.error.clone()))?;
    let owners = owners_for_mode(&mode, organ_id);
    if owners.is_empty() {
        return Ok(false);
    }
    Ok(!load_operations_since(state, None, &owners)
        .await?
        .into_iter()
        .filter(|operation| operation.sent_at.is_none())
        .filter(|operation| operation.source_operation_uid.is_none())
        .filter(|operation| operation.source_organ_id == LOCAL_ORGAN_ID)
        .collect::<Vec<_>>()
        .is_empty())
}

async fn sync_check_interval_seconds(state: &AppState, organ_id: i64) -> Result<i64, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT sync_check_interval_seconds FROM organ_sync_policy WHERE organ_id = ?",
    )
    .bind(organ_id)
    .fetch_optional(&*state.services.db)
    .await?
    .unwrap_or(300)
    .max(0))
}

async fn owner_scope_for_organ(state: &AppState, organ_id: i64) -> Result<String, sqlx::Error> {
    let (_, mode) = super::servers::load_sync_policy(state, organ_id)
        .await
        .map_err(|(_, error)| sqlx::Error::Protocol(error.error.clone()))?;
    Ok(owners_for_mode(&mode, organ_id)
        .into_iter()
        .map(|owner| owner.to_string())
        .collect::<Vec<_>>()
        .join(","))
}

async fn should_refresh_from_remote(
    state: &AppState,
    base_url: &str,
    bearer_token: &str,
    organ_id: i64,
    owners: &[i64],
) -> Result<bool, String> {
    let remote = fetch_remote_fingerprint(state, base_url, bearer_token, organ_id, owners).await?;
    let local = build_fingerprint(state, owners)
        .await
        .map_err(|error| error.to_string())?;
    let owner_scope = owners
        .iter()
        .map(|owner| owner.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let changed = local.row_count == 0 || local.fingerprint != remote.fingerprint;
    write_peer_state(state, organ_id, &owner_scope, &remote.fingerprint, changed)
        .await
        .map_err(|error| error.to_string())?;
    sync_log(format!(
        "record sync: fingerprint check organ_id={organ_id} local={} remote={} local_rows={} remote_rows={} changed={changed}",
        local.fingerprint, remote.fingerprint, local.row_count, remote.row_count
    ));
    Ok(changed)
}

async fn fetch_remote_fingerprint(
    state: &AppState,
    base_url: &str,
    bearer_token: &str,
    organ_id: i64,
    owners: &[i64],
) -> Result<RecordSyncFingerprintResponse, String> {
    let remote_owners = owners
        .iter()
        .map(|owner| if *owner == organ_id { LOCAL_ORGAN_ID } else { *owner })
        .collect::<Vec<_>>();
    let path = sync_path("/sync/record/fingerprint", &remote_owners);
    let response = state
        .manas
        .send_backend_request(base_url, bearer_token, Method::GET, &path, None)
        .await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Fingerprint sync failed with {status}: {body}"));
    }
    response
        .json::<RecordSyncFingerprintResponse>()
        .await
        .map_err(|error| format!("Invalid fingerprint sync response: {error}"))
}

async fn write_peer_state(
    state: &AppState,
    organ_id: i64,
    owner_scope: &str,
    fingerprint: &str,
    full_snapshot_due: bool,
) -> Result<(), sqlx::Error> {
    let interval = sync_check_interval_seconds(state, organ_id).await?;
    state
        .services
        .writer
        .execute_statement(
            "INSERT INTO record_sync_peer_state(
                organ_id, owner_scope, last_fingerprint, last_full_snapshot_at,
                last_checked_at, next_check_at, last_error
             ) VALUES (
                ?, ?, ?,
                CASE WHEN ? THEN CURRENT_TIMESTAMP ELSE NULL END,
                CURRENT_TIMESTAMP,
                CASE WHEN ? <= 0 THEN NULL ELSE datetime(CURRENT_TIMESTAMP, '+' || ? || ' seconds') END,
                NULL
             )
             ON CONFLICT(organ_id, owner_scope) DO UPDATE SET
                last_fingerprint = excluded.last_fingerprint,
                last_full_snapshot_at = CASE
                    WHEN ? THEN CURRENT_TIMESTAMP
                    ELSE record_sync_peer_state.last_full_snapshot_at
                END,
                last_checked_at = CURRENT_TIMESTAMP,
                next_check_at = excluded.next_check_at,
                last_error = NULL,
                updated_at = CURRENT_TIMESTAMP"
                .to_string(),
            vec![
                int(organ_id),
                text(owner_scope.to_string()),
                text(fingerprint.to_string()),
                int(if full_snapshot_due { 1 } else { 0 }),
                int(interval),
                int(interval),
                int(if full_snapshot_due { 1 } else { 0 }),
            ],
        )
        .await
        .map_err(sqlx::Error::Io)?;
    Ok(())
}

async fn pull_snapshot(
    state: &AppState,
    base_url: &str,
    bearer_token: &str,
    organ_id: i64,
    owners: &[i64],
) -> Result<(), String> {
    let remote_owners = owners
        .iter()
        .map(|owner| if *owner == organ_id { LOCAL_ORGAN_ID } else { *owner })
        .collect::<Vec<_>>();
    let path = sync_path("/sync/record/snapshot", &remote_owners);
    sync_log(format!(
        "record sync: requesting snapshot organ_id={organ_id} url={}{} owners={remote_owners:?}",
        base_url.trim().trim_end_matches('/'),
        path
    ));
    let response = state
        .manas
        .send_backend_request(base_url, bearer_token, Method::GET, &path, None)
        .await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        sync_log(format!(
            "record sync: snapshot failed organ_id={organ_id} status={status} body={body}"
        ));
        return Err(format!("Snapshot sync failed with {status}: {body}"));
    }
    let mut snapshot = response
        .json::<RecordSyncSnapshotResponse>()
        .await
        .map_err(|error| format!("Invalid snapshot sync response: {error}"))?;
    sync_log(format!(
        "record sync: snapshot response organ_id={organ_id} rows={}",
        snapshot.rows.len()
    ));
    for row in &mut snapshot.rows {
        remap_snapshot_row_for_local(row, organ_id);
        let operation = RecordSyncOperationFrame {
            operation_uid: format!("snapshot:{}:{}", row.table_name, row.row_sync_uid),
            source_organ_id: organ_id,
            actor_user_id: None,
            root_record_sync_uid: row.root_record_sync_uid.clone(),
            table_name: row.table_name.clone(),
            row_sync_uid: row.row_sync_uid.clone(),
            action: "insert".to_string(),
            field_payload_json: row.row.to_string(),
            operation_clock: "00000000000000000000:snapshot".to_string(),
            source_operation_uid: None,
            created_at: String::new(),
            applied_at: None,
            sent_at: None,
        };
        let _ = apply_upsert(state, &operation, organ_id).await;
    }
    tracing::info!(organ_id, rows = snapshot.rows.len(), "record sync: snapshot applied");
    sync_log(format!(
        "record sync: snapshot applied organ_id={organ_id} rows={}",
        snapshot.rows.len()
    ));
    Ok(())
}

async fn pull_operations(
    state: &AppState,
    base_url: &str,
    bearer_token: &str,
    organ_id: i64,
    owners: &[i64],
) -> Result<(), String> {
    let since = ack_clock(state, organ_id)
        .await
        .map_err(|error| error.to_string())?;
    let remote_owners = owners
        .iter()
        .map(|owner| if *owner == organ_id { LOCAL_ORGAN_ID } else { *owner })
        .collect::<Vec<_>>();
    let mut path = sync_path("/sync/record/operations", &remote_owners);
    if let Some(since) = since {
        path.push_str(if path.contains('?') { "&" } else { "?" });
        path.push_str("since_clock=");
        path.push_str(&urlencoding::encode(&since));
    }
    sync_log(format!(
        "record sync: requesting operations organ_id={organ_id} url={}{} owners={remote_owners:?}",
        base_url.trim().trim_end_matches('/'),
        path
    ));
    let response = state
        .manas
        .send_backend_request(base_url, bearer_token, Method::GET, &path, None)
        .await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        sync_log(format!(
            "record sync: operations pull failed organ_id={organ_id} status={status} body={body}"
        ));
        return Err(format!("Operation pull failed with {status}: {body}"));
    }
    let mut payload = response
        .json::<RecordSyncOperationsResponse>()
        .await
        .map_err(|error| format!("Invalid operation sync response: {error}"))?;
    let mut last_clock = None;
    let mut applied = 0_usize;
    for operation in &mut payload.operations {
        remap_operation_for_local(operation, organ_id);
        if apply_operation(state, operation, organ_id)
            .await
            .map_err(|error| error.to_string())?
        {
            applied += 1;
        }
        last_clock = Some(operation.operation_clock.clone());
    }
    if let Some(clock) = last_clock {
        write_ack(state, organ_id, &clock).await.map_err(|error| error.to_string())?;
    }
    tracing::info!(organ_id, operations = payload.operations.len(), applied, "record sync: operations pulled");
    sync_log(format!(
        "record sync: operations pulled organ_id={organ_id} operations={} applied={applied}",
        payload.operations.len()
    ));
    Ok(())
}

async fn push_operations(
    state: &AppState,
    base_url: &str,
    bearer_token: &str,
    organ_id: i64,
    owners: &[i64],
) -> Result<(), String> {
    let operations = load_operations_since(state, None, owners)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|operation| operation.sent_at.is_none())
        .filter(|operation| operation.source_operation_uid.is_none())
        .filter(|operation| operation.source_organ_id == LOCAL_ORGAN_ID)
        .collect::<Vec<_>>();
    sync_log(format!(
        "record sync: local operations selected organ_id={organ_id} owners={owners:?} operations={}",
        operations.len()
    ));
    let mut local_owned = Vec::new();
    let mut remote_owned = Vec::new();
    let mut local_owned_uids = Vec::new();
    let mut remote_owned_uids = Vec::new();
    for operation in operations {
        match owner_for_operation(state, &operation)
            .await
            .map_err(|error| error.to_string())?
        {
            owner if owner == organ_id => {
                remote_owned_uids.push(operation.operation_uid.clone());
                let mut operation = operation;
                remap_operation_for_remote(&mut operation, organ_id);
                remote_owned.push(operation);
            }
            LOCAL_ORGAN_ID => {
                local_owned_uids.push(operation.operation_uid.clone());
                local_owned.push(operation);
            }
            _ => {}
        }
    }
    let mut pushed = 0_usize;
    if !remote_owned.is_empty() {
        pushed += post_operations_to_remote(state, base_url, bearer_token, None, remote_owned).await?;
        mark_operations_sent(state, &remote_owned_uids)
            .await
            .map_err(|error| error.to_string())?;
    }
    if !local_owned.is_empty() {
        let source_base_url = format!("http://127.0.0.1:{}", state.listening_port);
        pushed += post_operations_to_remote(
            state,
            base_url,
            bearer_token,
            Some(source_base_url),
            local_owned,
        )
        .await?;
        mark_operations_sent(state, &local_owned_uids)
            .await
            .map_err(|error| error.to_string())?;
    }
    tracing::info!(organ_id, operations = pushed, "record sync: operations pushed");
    sync_log(format!(
        "record sync: operations pushed organ_id={organ_id} operations={pushed}"
    ));
    Ok(())
}

async fn post_operations_to_remote(
    state: &AppState,
    base_url: &str,
    bearer_token: &str,
    source_base_url: Option<String>,
    operations: Vec<RecordSyncOperationFrame>,
) -> Result<usize, String> {
    sync_log(format!(
        "record sync: posting operations url={}{} operations={} source_base_url={:?}",
        base_url.trim().trim_end_matches('/'),
        "/sync/record/operations",
        operations.len(),
        source_base_url
    ));
    let response = state
        .manas
        .send_backend_request(
            base_url,
            bearer_token,
            Method::POST,
            "/sync/record/operations",
            Some(json!({
                "sourceBaseUrl": source_base_url,
                "sourceName": "Lince",
                "operations": operations,
            })),
        )
        .await?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        sync_log(format!(
            "record sync: operation push failed status={status} body={body}"
        ));
        return Err(format!("Operation push failed with {status}: {body}"));
    }
    let result = response
        .json::<RecordSyncApplyResponse>()
        .await
        .map_err(|error| format!("Invalid operation push response: {error}"))?;
    sync_log(format!(
        "record sync: operation push response applied={} skipped={}",
        result.applied, result.skipped
    ));
    Ok(result.applied + result.skipped)
}

async fn mark_operations_sent(
    state: &AppState,
    operation_uids: &[String],
) -> Result<(), sqlx::Error> {
    if operation_uids.is_empty() {
        return Ok(());
    }
    for uid in operation_uids {
        state
            .services
            .writer
            .execute_statement(
                "UPDATE record_sync_operation SET sent_at = CURRENT_TIMESTAMP WHERE operation_uid = ?"
                    .to_string(),
                vec![text(uid.clone())],
            )
            .await
            .map_err(sqlx::Error::Io)?;
    }
    Ok(())
}

fn normalized_owners(owner_organ_ids: &[i64]) -> Vec<i64> {
    let mut owners = owner_organ_ids
        .iter()
        .copied()
        .filter(|owner| *owner > 0)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if owners.is_empty() {
        owners.push(LOCAL_ORGAN_ID);
    }
    owners
}

fn owners_for_mode(mode: &str, organ_id: i64) -> Vec<i64> {
    match mode {
        "sync_incoming" => vec![organ_id],
        "sync_outgoing" => vec![LOCAL_ORGAN_ID],
        "sync_both" => vec![LOCAL_ORGAN_ID, organ_id],
        _ => Vec::new(),
    }
}

fn sync_path(base: &str, owners: &[i64]) -> String {
    if owners.is_empty() {
        return base.to_string();
    }
    let query = owners
        .iter()
        .map(|owner| format!("owner_organ_id={owner}"))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{query}")
}

fn record_fk_column(table: &str) -> Option<&'static str> {
    match table {
        "record_extension"
        | "record_link"
        | "record_comment"
        | "record_worklog"
        | "record_resource_ref" => Some("record_id"),
        _ => None,
    }
}

fn writable_columns(table: &str) -> Option<&'static [&'static str]> {
    match table {
        "record" => Some(&[
            "quantity",
            "head",
            "body",
            "owner_organ_id",
            "sync_uid",
            "origin_organ_id",
            "created_at",
            "updated_at",
        ]),
        "record_extension" => Some(&[
            "record_id",
            "namespace",
            "version",
            "freestyle_data_structure",
            "sync_uid",
            "origin_organ_id",
            "created_at",
            "updated_at",
        ]),
        "record_link" => Some(&[
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
        ]),
        "record_comment" => Some(&[
            "record_id",
            "author_user_id",
            "body",
            "created_at",
            "updated_at",
            "deleted_at",
            "sync_uid",
            "origin_organ_id",
        ]),
        "record_worklog" => Some(&[
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
        ]),
        "record_resource_ref" => Some(&[
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
        ]),
        "work_metadata" => Some(&[
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
        ]),
        "work_subject" => Some(&[
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
        ]),
        "work_assignment" => Some(&[
            "work_metadata_id",
            "work_subject_id",
            "assignment_kind",
            "created_at",
            "updated_at",
            "sync_uid",
            "origin_organ_id",
        ]),
        _ => None,
    }
}

fn record_row_value(row: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": row.get::<i64, _>("id"),
        "quantity": row.get::<f64, _>("quantity"),
        "head": row.get::<Option<String>, _>("head"),
        "body": row.get::<Option<String>, _>("body"),
        "owner_organ_id": row.get::<Option<i64>, _>("owner_organ_id"),
        "sync_uid": row.get::<String, _>("sync_uid"),
        "origin_organ_id": row.get::<Option<i64>, _>("origin_organ_id"),
        "created_at": row.get::<Option<String>, _>("created_at"),
        "updated_at": row.get::<Option<String>, _>("updated_at"),
    })
}

fn row_to_value(table: &str, row: &sqlx::sqlite::SqliteRow) -> Value {
    match table {
        "record_extension" => json!({
            "id": row.get::<i64, _>("id"),
            "record_id": row.get::<i64, _>("record_id"),
            "namespace": row.get::<String, _>("namespace"),
            "version": row.get::<i64, _>("version"),
            "freestyle_data_structure": row.get::<String, _>("freestyle_data_structure"),
            "sync_uid": row.get::<String, _>("sync_uid"),
            "origin_organ_id": row.get::<Option<i64>, _>("origin_organ_id"),
            "created_at": row.get::<String, _>("created_at"),
            "updated_at": row.get::<String, _>("updated_at"),
        }),
        "record_link" => json!({
            "id": row.get::<i64, _>("id"),
            "record_id": row.get::<i64, _>("record_id"),
            "link_type": row.get::<String, _>("link_type"),
            "target_table": row.get::<String, _>("target_table"),
            "target_id": row.get::<i64, _>("target_id"),
            "position": row.get::<Option<f64>, _>("position"),
            "freestyle_data_structure": row.get::<Option<String>, _>("freestyle_data_structure"),
            "sync_uid": row.get::<String, _>("sync_uid"),
            "origin_organ_id": row.get::<Option<i64>, _>("origin_organ_id"),
            "created_at": row.get::<String, _>("created_at"),
            "updated_at": row.get::<String, _>("updated_at"),
        }),
        "record_comment" => json!({
            "id": row.get::<i64, _>("id"),
            "record_id": row.get::<i64, _>("record_id"),
            "author_user_id": row.get::<Option<i64>, _>("author_user_id"),
            "body": row.get::<String, _>("body"),
            "created_at": row.get::<String, _>("created_at"),
            "updated_at": row.get::<String, _>("updated_at"),
            "deleted_at": row.get::<Option<String>, _>("deleted_at"),
            "sync_uid": row.get::<String, _>("sync_uid"),
            "origin_organ_id": row.get::<Option<i64>, _>("origin_organ_id"),
        }),
        "record_worklog" => json!({
            "id": row.get::<i64, _>("id"),
            "record_id": row.get::<i64, _>("record_id"),
            "author_user_id": row.get::<i64, _>("author_user_id"),
            "started_at": row.get::<String, _>("started_at"),
            "ended_at": row.get::<Option<String>, _>("ended_at"),
            "last_heartbeat_at": row.get::<Option<String>, _>("last_heartbeat_at"),
            "seconds": row.get::<Option<f64>, _>("seconds"),
            "note": row.get::<Option<String>, _>("note"),
            "created_at": row.get::<String, _>("created_at"),
            "updated_at": row.get::<String, _>("updated_at"),
            "sync_uid": row.get::<String, _>("sync_uid"),
            "origin_organ_id": row.get::<Option<i64>, _>("origin_organ_id"),
        }),
        "record_resource_ref" => json!({
            "id": row.get::<i64, _>("id"),
            "record_id": row.get::<i64, _>("record_id"),
            "provider": row.get::<String, _>("provider"),
            "resource_kind": row.get::<String, _>("resource_kind"),
            "resource_path": row.get::<String, _>("resource_path"),
            "title": row.get::<Option<String>, _>("title"),
            "position": row.get::<Option<f64>, _>("position"),
            "freestyle_data_structure": row.get::<Option<String>, _>("freestyle_data_structure"),
            "created_at": row.get::<String, _>("created_at"),
            "updated_at": row.get::<String, _>("updated_at"),
            "sync_uid": row.get::<String, _>("sync_uid"),
            "origin_organ_id": row.get::<Option<i64>, _>("origin_organ_id"),
        }),
        "work_metadata" => json!({
            "id": row.get::<i64, _>("id"),
            "owner_kind": row.get::<String, _>("owner_kind"),
            "owner_id": row.get::<i64, _>("owner_id"),
            "task_type": row.get::<Option<String>, _>("task_type"),
            "status": row.get::<Option<String>, _>("status"),
            "start_at": row.get::<Option<String>, _>("start_at"),
            "end_at": row.get::<Option<String>, _>("end_at"),
            "estimate_seconds": row.get::<Option<i64>, _>("estimate_seconds"),
            "completion_notes": row.get::<Option<String>, _>("completion_notes"),
            "metadata_json": row.get::<String, _>("metadata_json"),
            "created_at": row.get::<String, _>("created_at"),
            "updated_at": row.get::<String, _>("updated_at"),
            "sync_uid": row.get::<String, _>("sync_uid"),
            "origin_organ_id": row.get::<Option<i64>, _>("origin_organ_id"),
        }),
        "work_subject" => json!({
            "id": row.get::<i64, _>("id"),
            "subject_kind": row.get::<String, _>("subject_kind"),
            "app_user_id": row.get::<Option<i64>, _>("app_user_id"),
            "organ_id": row.get::<Option<i64>, _>("organ_id"),
            "transfer_party_id": row.get::<Option<i64>, _>("transfer_party_id"),
            "remote_base_url": row.get::<Option<String>, _>("remote_base_url"),
            "remote_public_key": row.get::<Option<String>, _>("remote_public_key"),
            "remote_subject_uid": row.get::<Option<String>, _>("remote_subject_uid"),
            "display_name_snapshot": row.get::<Option<String>, _>("display_name_snapshot"),
            "organ_name_snapshot": row.get::<Option<String>, _>("organ_name_snapshot"),
            "created_at": row.get::<String, _>("created_at"),
            "updated_at": row.get::<String, _>("updated_at"),
            "sync_uid": row.get::<String, _>("sync_uid"),
            "origin_organ_id": row.get::<Option<i64>, _>("origin_organ_id"),
        }),
        "work_assignment" => json!({
            "id": row.get::<i64, _>("id"),
            "work_metadata_id": row.get::<i64, _>("work_metadata_id"),
            "work_subject_id": row.get::<i64, _>("work_subject_id"),
            "assignment_kind": row.get::<String, _>("assignment_kind"),
            "created_at": row.get::<String, _>("created_at"),
            "updated_at": row.get::<String, _>("updated_at"),
            "sync_uid": row.get::<String, _>("sync_uid"),
            "origin_organ_id": row.get::<Option<i64>, _>("origin_organ_id"),
        }),
        _ => Value::Object(Map::new()),
    }
}

async fn related_sync_uid(
    state: &AppState,
    table: &str,
    id: i64,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(&format!("SELECT sync_uid FROM {table} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&*state.services.db)
        .await
}

async fn resolve_row_references(
    state: &AppState,
    table: &str,
    row: &mut Value,
) -> Result<(), sqlx::Error> {
    match table {
        "record_extension"
        | "record_link"
        | "record_comment"
        | "record_worklog"
        | "record_resource_ref" => {
            if let Some(uid) = row.get("root_record_sync_uid").and_then(Value::as_str)
                && let Some(id) = local_id_for_sync_uid(state, "record", uid).await?
            {
                row["record_id"] = Value::Number(id.into());
            }
        }
        "work_metadata" => {
            if row.get("owner_kind").and_then(Value::as_str) == Some("record")
                && let Some(uid) = row.get("root_record_sync_uid").and_then(Value::as_str)
                && let Some(id) = local_id_for_sync_uid(state, "record", uid).await?
            {
                row["owner_id"] = Value::Number(id.into());
            }
        }
        "work_assignment" => {
            if let Some(uid) = row.get("work_metadata_sync_uid").and_then(Value::as_str)
                && let Some(id) = local_id_for_sync_uid(state, "work_metadata", uid).await?
            {
                row["work_metadata_id"] = Value::Number(id.into());
            }
            if let Some(uid) = row.get("work_subject_sync_uid").and_then(Value::as_str)
                && let Some(id) = local_id_for_sync_uid(state, "work_subject", uid).await?
            {
                row["work_subject_id"] = Value::Number(id.into());
            }
        }
        _ => {}
    }
    Ok(())
}

async fn local_id_for_sync_uid(
    state: &AppState,
    table: &str,
    sync_uid: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(&format!("SELECT id FROM {table} WHERE sync_uid = ? LIMIT 1"))
        .bind(sync_uid)
        .fetch_optional(&*state.services.db)
        .await
}

async fn ack_clock(state: &AppState, organ_id: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT last_ack_operation_clock FROM record_sync_ack WHERE organ_id = ?",
    )
    .bind(organ_id)
    .fetch_optional(&*state.services.db)
    .await
}

async fn write_ack(state: &AppState, organ_id: i64, clock: &str) -> Result<(), sqlx::Error> {
    state
        .services
        .writer
        .execute_statement(
            "INSERT INTO record_sync_ack(organ_id, last_ack_operation_clock)
             VALUES (?, ?)
             ON CONFLICT(organ_id) DO UPDATE SET
                last_ack_operation_clock = excluded.last_ack_operation_clock,
                updated_at = CURRENT_TIMESTAMP"
                .to_string(),
            vec![int(organ_id), text(clock.to_string())],
        )
        .await
        .map_err(sqlx::Error::Io)?;
    Ok(())
}

fn remap_snapshot_row_for_local(row: &mut RecordSyncRowFrame, organ_id: i64) {
    row.row_sync_uid = remap_uid_prefix(&row.row_sync_uid, LOCAL_ORGAN_ID, organ_id);
    row.root_record_sync_uid = remap_uid_prefix(&row.root_record_sync_uid, LOCAL_ORGAN_ID, organ_id);
    remap_row_uid_fields(&mut row.row, LOCAL_ORGAN_ID, organ_id);
    remap_row_owner(&mut row.row, LOCAL_ORGAN_ID, organ_id);
}

fn remap_operation_for_local(operation: &mut RecordSyncOperationFrame, organ_id: i64) {
    operation.source_organ_id = organ_id;
    operation.operation_uid = remap_uid_prefix(&operation.operation_uid, LOCAL_ORGAN_ID, organ_id);
    operation.root_record_sync_uid =
        remap_uid_prefix(&operation.root_record_sync_uid, LOCAL_ORGAN_ID, organ_id);
    operation.row_sync_uid = remap_uid_prefix(&operation.row_sync_uid, LOCAL_ORGAN_ID, organ_id);
    if let Ok(mut row) = serde_json::from_str::<Value>(&operation.field_payload_json) {
        remap_row_uid_fields(&mut row, LOCAL_ORGAN_ID, organ_id);
        remap_row_owner(&mut row, LOCAL_ORGAN_ID, organ_id);
        operation.field_payload_json = row.to_string();
    }
}

fn remap_operation_for_remote(operation: &mut RecordSyncOperationFrame, organ_id: i64) {
    operation.operation_uid = remap_uid_prefix(&operation.operation_uid, organ_id, LOCAL_ORGAN_ID);
    operation.root_record_sync_uid =
        remap_uid_prefix(&operation.root_record_sync_uid, organ_id, LOCAL_ORGAN_ID);
    operation.row_sync_uid = remap_uid_prefix(&operation.row_sync_uid, organ_id, LOCAL_ORGAN_ID);
    if let Ok(mut row) = serde_json::from_str::<Value>(&operation.field_payload_json) {
        remap_row_uid_fields(&mut row, organ_id, LOCAL_ORGAN_ID);
        remap_row_owner(&mut row, organ_id, LOCAL_ORGAN_ID);
        operation.field_payload_json = row.to_string();
    }
}

fn remap_operation_for_receiver(operation: &mut RecordSyncOperationFrame, peer_organ_id: i64) {
    operation.root_record_sync_uid =
        remap_uid_prefix(&operation.root_record_sync_uid, LOCAL_ORGAN_ID, peer_organ_id);
    operation.row_sync_uid = remap_uid_prefix(&operation.row_sync_uid, LOCAL_ORGAN_ID, peer_organ_id);
    if let Ok(mut row) = serde_json::from_str::<Value>(&operation.field_payload_json) {
        remap_row_uid_fields(&mut row, LOCAL_ORGAN_ID, peer_organ_id);
        remap_row_owner(&mut row, LOCAL_ORGAN_ID, peer_organ_id);
        operation.field_payload_json = row.to_string();
    }
}

fn remap_row_for_receiver(row: &mut Value, peer_organ_id: i64) {
    remap_row_uid_fields(row, LOCAL_ORGAN_ID, peer_organ_id);
    remap_row_owner(row, LOCAL_ORGAN_ID, peer_organ_id);
}

fn remap_uid_prefix(value: &str, from: i64, to: i64) -> String {
    let prefix = format!("organ:{from}:");
    if let Some(rest) = value.strip_prefix(&prefix) {
        format!("organ:{to}:{rest}")
    } else {
        value.to_string()
    }
}

fn remap_row_uid_fields(row: &mut Value, from: i64, to: i64) {
    for key in [
        "sync_uid",
        "root_record_sync_uid",
        "work_metadata_sync_uid",
        "work_subject_sync_uid",
    ] {
        if let Some(value) = row.get_mut(key)
            && let Some(text_value) = value.as_str()
        {
            *value = Value::String(remap_uid_prefix(text_value, from, to));
        }
    }
}

fn remap_row_owner(row: &mut Value, from: i64, to: i64) {
    for key in ["owner_organ_id", "origin_organ_id"] {
        if row.get(key).and_then(Value::as_i64) == Some(from) {
            row[key] = Value::Number(to.into());
        }
    }
}

fn sql_param_from_json(value: &Value) -> persistence::write_coordinator::SqlParameter {
    match value {
        Value::Null => persistence::write_coordinator::SqlParameter::Null,
        Value::Bool(value) => int(if *value { 1 } else { 0 }),
        Value::Number(value) => {
            if let Some(integer) = value.as_i64() {
                int(integer)
            } else if let Some(float) = value.as_f64() {
                persistence::write_coordinator::SqlParameter::Real(float)
            } else {
                persistence::write_coordinator::SqlParameter::Null
            }
        }
        Value::String(value) => text(value.clone()),
        _ => text(value.to_string()),
    }
}

fn text(value: String) -> persistence::write_coordinator::SqlParameter {
    persistence::write_coordinator::SqlParameter::Text(value)
}

fn optional_text(value: Option<String>) -> persistence::write_coordinator::SqlParameter {
    value.map(text).unwrap_or(persistence::write_coordinator::SqlParameter::Null)
}

fn int(value: i64) -> persistence::write_coordinator::SqlParameter {
    persistence::write_coordinator::SqlParameter::Integer(value)
}

fn optional_int(value: Option<i64>) -> persistence::write_coordinator::SqlParameter {
    value.map(int).unwrap_or(persistence::write_coordinator::SqlParameter::Null)
}

async fn send_socket_frame<S>(sender: &mut S, frame: RecordSyncSocketFrame) -> Result<(), ()>
where
    S: futures::Sink<Message> + Unpin,
{
    let payload = serde_json::to_string(&frame).map_err(|_| ())?;
    sender.send(Message::Text(payload.into())).await.map_err(|_| ())
}
