use lince_persistence_table_derive::Table;

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "organ_sync_policy")]
#[table(strict)]
#[table(index(name = "uq_organ_sync_policy_organ", columns = "organ_id", unique))]
pub struct OrganSyncPolicyRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "organ(id) ON DELETE CASCADE", check = "organ_id > 0")]
    pub organ_id: i64,
    #[table(default = "'[]'", check = "json_valid(sync_resources)")]
    pub sync_resources: String,
    #[table(
        default = "'none'",
        check = "record_sync_mode IN ('none', 'sync_outgoing', 'sync_incoming', 'sync_both')"
    )]
    pub record_sync_mode: String,
    #[table(
        default = "CURRENT_TIMESTAMP",
        check = "julianday(created_at) IS NOT NULL"
    )]
    pub created_at: String,
    #[table(
        default = "CURRENT_TIMESTAMP",
        check = "julianday(updated_at) IS NOT NULL"
    )]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record_sync_operation")]
#[table(strict)]
#[table(index(
    name = "uq_record_sync_operation_uid",
    columns = "operation_uid",
    unique
))]
#[table(index(
    name = "idx_record_sync_operation_root_clock",
    columns = "root_record_sync_uid, operation_clock"
))]
#[table(index(
    name = "idx_record_sync_operation_source_clock",
    columns = "source_organ_id, operation_clock"
))]
#[table(index(
    name = "idx_record_sync_operation_row",
    columns = "table_name, row_sync_uid"
))]
pub struct RecordSyncOperationRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(operation_uid)) > 0")]
    pub operation_uid: String,
    #[table(references = "organ(id)", check = "source_organ_id > 0")]
    pub source_organ_id: i64,
    #[table(references = "app_user(id)", check = "actor_user_id IS NULL OR actor_user_id > 0")]
    pub actor_user_id: Option<i64>,
    #[table(check = "length(trim(root_record_sync_uid)) > 0")]
    pub root_record_sync_uid: String,
    #[table(check = "length(trim(table_name)) > 0")]
    pub table_name: String,
    #[table(check = "length(trim(row_sync_uid)) > 0")]
    pub row_sync_uid: String,
    #[table(check = "action IN ('insert', 'update', 'delete')")]
    pub action: String,
    #[table(default = "'{}'", check = "json_valid(field_payload_json)")]
    pub field_payload_json: String,
    #[table(check = "length(trim(operation_clock)) > 0")]
    pub operation_clock: String,
    #[table(check = "source_operation_uid IS NULL OR length(trim(source_operation_uid)) > 0")]
    pub source_operation_uid: Option<String>,
    #[table(
        default = "CURRENT_TIMESTAMP",
        check = "julianday(created_at) IS NOT NULL"
    )]
    pub created_at: String,
    #[table(check = "applied_at IS NULL OR julianday(applied_at) IS NOT NULL")]
    pub applied_at: Option<String>,
    #[table(check = "sent_at IS NULL OR julianday(sent_at) IS NOT NULL")]
    pub sent_at: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record_sync_tombstone")]
#[table(strict)]
#[table(index(
    name = "uq_record_sync_tombstone_row",
    columns = "table_name, row_sync_uid",
    unique
))]
pub struct RecordSyncTombstoneRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(table_name)) > 0")]
    pub table_name: String,
    #[table(check = "length(trim(row_sync_uid)) > 0")]
    pub row_sync_uid: String,
    #[table(check = "length(trim(root_record_sync_uid)) > 0")]
    pub root_record_sync_uid: String,
    #[table(check = "length(trim(delete_clock)) > 0")]
    pub delete_clock: String,
    #[table(references = "organ(id)", check = "source_organ_id > 0")]
    pub source_organ_id: i64,
    #[table(
        default = "CURRENT_TIMESTAMP",
        check = "julianday(created_at) IS NOT NULL"
    )]
    pub created_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record_sync_ack")]
#[table(strict)]
#[table(index(
    name = "uq_record_sync_ack_organ",
    columns = "organ_id",
    unique
))]
pub struct RecordSyncAckRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "organ(id) ON DELETE CASCADE", check = "organ_id > 0")]
    pub organ_id: i64,
    #[table(check = "last_ack_operation_clock IS NULL OR length(trim(last_ack_operation_clock)) > 0")]
    pub last_ack_operation_clock: Option<String>,
    #[table(check = "last_ack_operation_uid IS NULL OR length(trim(last_ack_operation_uid)) > 0")]
    pub last_ack_operation_uid: Option<String>,
    #[table(
        default = "CURRENT_TIMESTAMP",
        check = "julianday(updated_at) IS NOT NULL"
    )]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record_sync_pending_dependency")]
#[table(strict)]
#[table(index(
    name = "idx_record_sync_pending_dependency_missing",
    columns = "missing_table_name, missing_row_sync_uid"
))]
pub struct RecordSyncPendingDependencyRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(operation_uid)) > 0")]
    pub operation_uid: String,
    #[table(check = "length(trim(missing_table_name)) > 0")]
    pub missing_table_name: String,
    #[table(check = "length(trim(missing_row_sync_uid)) > 0")]
    pub missing_row_sync_uid: String,
    #[table(default = "CURRENT_TIMESTAMP", check = "julianday(created_at) IS NOT NULL")]
    pub created_at: String,
    #[table(check = "resolved_at IS NULL OR julianday(resolved_at) IS NOT NULL")]
    pub resolved_at: Option<String>,
}
