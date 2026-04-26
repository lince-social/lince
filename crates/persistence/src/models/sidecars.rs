use lince_persistence_table_derive::Table;

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record_extension")]
#[table(strict)]
#[table(index(
    name = "idx_record_extension_namespace_record",
    columns = "namespace, record_id"
))]
#[table(index(
    name = "uq_record_extension_record_namespace",
    columns = "record_id, namespace",
    unique
))]
pub struct RecordExtensionRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "record(id) ON DELETE CASCADE", check = "record_id > 0")]
    pub record_id: i64,
    #[table(check = "length(trim(namespace)) > 0")]
    pub namespace: String,
    #[table(default = "1", check = "version >= 1")]
    pub version: i64,
    #[table(check = "json_valid(freestyle_data_structure)")]
    pub freestyle_data_structure: String,
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
#[table(name = "record_link")]
#[table(strict)]
#[table(index(
    name = "idx_record_link_record_type",
    columns = "record_id, link_type, target_table"
))]
#[table(index(
    name = "idx_record_link_target_type",
    columns = "target_table, target_id, link_type"
))]
#[table(index(
    name = "uq_record_link_identity",
    columns = "record_id, link_type, target_table, target_id",
    unique
))]
pub struct RecordLinkRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "record(id) ON DELETE CASCADE", check = "record_id > 0")]
    pub record_id: i64,
    #[table(check = "length(trim(link_type)) > 0")]
    pub link_type: String,
    #[table(check = "length(trim(target_table)) > 0")]
    pub target_table: String,
    #[table(check = "target_id > 0")]
    pub target_id: i64,
    pub position: Option<f64>,
    #[table(check = "freestyle_data_structure IS NULL OR json_valid(freestyle_data_structure)")]
    pub freestyle_data_structure: Option<String>,
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
#[table(name = "record_comment")]
#[table(strict)]
#[table(index(
    name = "idx_record_comment_record_created",
    columns = "record_id, created_at DESC"
))]
pub struct RecordCommentRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "record(id) ON DELETE CASCADE", check = "record_id > 0")]
    pub record_id: i64,
    #[table(
        references = "app_user(id) ON DELETE SET NULL",
        check = "author_user_id IS NULL OR author_user_id > 0"
    )]
    pub author_user_id: Option<i64>,
    #[table(check = "length(trim(body)) > 0")]
    pub body: String,
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
    #[table(check = "deleted_at IS NULL OR julianday(deleted_at) IS NOT NULL")]
    pub deleted_at: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record_worklog")]
#[table(strict)]
#[table(index(
    name = "idx_record_worklog_record_started",
    columns = "record_id, started_at DESC"
))]
#[table(index(
    name = "idx_record_worklog_author_started",
    columns = "author_user_id, started_at DESC"
))]
#[table(index(
    name = "idx_record_worklog_one_open_interval",
    columns = "record_id, author_user_id",
    unique,
    where = "ended_at IS NULL"
))]
#[table(check = "length(trim(started_at)) > 0 AND julianday(started_at) IS NOT NULL")]
#[table(
    check = "ended_at IS NULL OR (length(trim(ended_at)) > 0 AND julianday(ended_at) IS NOT NULL)"
)]
#[table(
    check = "last_heartbeat_at IS NULL OR (length(trim(last_heartbeat_at)) > 0 AND julianday(last_heartbeat_at) IS NOT NULL)"
)]
#[table(check = "ended_at IS NULL OR julianday(ended_at) >= julianday(started_at)")]
#[table(
    check = "last_heartbeat_at IS NULL OR julianday(last_heartbeat_at) >= julianday(started_at)"
)]
pub struct RecordWorklogRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "record(id) ON DELETE CASCADE", check = "record_id > 0")]
    pub record_id: i64,
    #[table(
        references = "app_user(id) ON DELETE CASCADE",
        check = "author_user_id > 0"
    )]
    pub author_user_id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub last_heartbeat_at: Option<String>,
    #[table(check = "seconds IS NULL OR seconds >= 0")]
    pub seconds: Option<f64>,
    pub note: Option<String>,
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
#[table(name = "record_resource_ref")]
#[table(strict)]
#[table(index(
    name = "idx_record_resource_ref_record_position",
    columns = "record_id, position, id"
))]
#[table(index(
    name = "uq_record_resource_ref_identity",
    columns = "record_id, provider, resource_path",
    unique
))]
pub struct RecordResourceRefRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "record(id) ON DELETE CASCADE", check = "record_id > 0")]
    pub record_id: i64,
    #[table(check = "length(trim(provider)) > 0")]
    pub provider: String,
    #[table(check = "length(trim(resource_kind)) > 0")]
    pub resource_kind: String,
    #[table(check = "length(trim(resource_path)) > 0")]
    pub resource_path: String,
    pub title: Option<String>,
    pub position: Option<f64>,
    #[table(check = "freestyle_data_structure IS NULL OR json_valid(freestyle_data_structure)")]
    pub freestyle_data_structure: Option<String>,
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
