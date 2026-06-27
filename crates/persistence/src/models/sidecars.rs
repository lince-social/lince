use lince_persistence_table_derive::Table;

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record_extension")]
#[table(strict)]
#[table(index(
    name = "uq_record_extension_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
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
    #[table(check = "sync_uid IS NULL OR length(trim(sync_uid)) > 0")]
    pub sync_uid: Option<String>,
    #[table(references = "organ(id)", check = "origin_organ_id IS NULL OR origin_organ_id > 0")]
    pub origin_organ_id: Option<i64>,
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
    name = "uq_record_link_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
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
    #[table(check = "sync_uid IS NULL OR length(trim(sync_uid)) > 0")]
    pub sync_uid: Option<String>,
    #[table(references = "organ(id)", check = "origin_organ_id IS NULL OR origin_organ_id > 0")]
    pub origin_organ_id: Option<i64>,
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
    name = "uq_record_comment_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
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
    #[table(check = "sync_uid IS NULL OR length(trim(sync_uid)) > 0")]
    pub sync_uid: Option<String>,
    #[table(references = "organ(id)", check = "origin_organ_id IS NULL OR origin_organ_id > 0")]
    pub origin_organ_id: Option<i64>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record_worklog")]
#[table(strict)]
#[table(index(
    name = "uq_record_worklog_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
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
    #[table(check = "sync_uid IS NULL OR length(trim(sync_uid)) > 0")]
    pub sync_uid: Option<String>,
    #[table(references = "organ(id)", check = "origin_organ_id IS NULL OR origin_organ_id > 0")]
    pub origin_organ_id: Option<i64>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record_resource_ref")]
#[table(strict)]
#[table(index(
    name = "uq_record_resource_ref_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
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
    #[table(check = "sync_uid IS NULL OR length(trim(sync_uid)) > 0")]
    pub sync_uid: Option<String>,
    #[table(references = "organ(id)", check = "origin_organ_id IS NULL OR origin_organ_id > 0")]
    pub origin_organ_id: Option<i64>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "work_metadata")]
#[table(strict)]
#[table(index(
    name = "uq_work_metadata_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
#[table(index(name = "idx_work_metadata_owner", columns = "owner_kind, owner_id"))]
#[table(index(name = "idx_work_metadata_status", columns = "status"))]
#[table(index(
    name = "uq_work_metadata_owner",
    columns = "owner_kind, owner_id",
    unique
))]
pub struct WorkMetadataRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(
        check = "owner_kind IN ('record', 'transfer', 'transfer_structured_item', 'transfer_interaction')"
    )]
    pub owner_kind: String,
    #[table(check = "owner_id > 0")]
    pub owner_id: i64,
    #[table(check = "task_type IS NULL OR task_type IN ('epic', 'feature', 'task', 'other')")]
    pub task_type: Option<String>,
    pub status: Option<String>,
    #[table(check = "start_at IS NULL OR julianday(start_at) IS NOT NULL")]
    pub start_at: Option<String>,
    #[table(check = "end_at IS NULL OR julianday(end_at) IS NOT NULL")]
    pub end_at: Option<String>,
    #[table(check = "estimate_seconds IS NULL OR estimate_seconds >= 0")]
    pub estimate_seconds: Option<i64>,
    pub completion_notes: Option<String>,
    #[table(default = "'{}'", check = "json_valid(metadata_json)")]
    pub metadata_json: String,
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
    #[table(check = "sync_uid IS NULL OR length(trim(sync_uid)) > 0")]
    pub sync_uid: Option<String>,
    #[table(references = "organ(id)", check = "origin_organ_id IS NULL OR origin_organ_id > 0")]
    pub origin_organ_id: Option<i64>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "work_subject")]
#[table(strict)]
#[table(index(
    name = "uq_work_subject_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
#[table(index(
    name = "uq_work_subject_app_user",
    columns = "app_user_id",
    unique,
    where = "subject_kind = 'app_user' AND app_user_id IS NOT NULL"
))]
#[table(index(
    name = "uq_work_subject_organ",
    columns = "organ_id",
    unique,
    where = "subject_kind = 'organ' AND organ_id IS NOT NULL"
))]
#[table(index(
    name = "uq_work_subject_transfer_party",
    columns = "transfer_party_id",
    unique,
    where = "subject_kind = 'transfer_party' AND transfer_party_id IS NOT NULL"
))]
#[table(index(
    name = "uq_work_subject_remote",
    columns = "subject_kind, remote_base_url, remote_subject_uid",
    unique,
    where = "remote_base_url IS NOT NULL AND remote_subject_uid IS NOT NULL"
))]
pub struct WorkSubjectRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(
        check = "subject_kind IN ('app_user', 'organ', 'transfer_party', 'external_actor', 'placeholder')"
    )]
    pub subject_kind: String,
    #[table(references = "app_user(id) ON DELETE CASCADE")]
    pub app_user_id: Option<i64>,
    #[table(references = "organ(id) ON DELETE CASCADE")]
    pub organ_id: Option<i64>,
    #[table(references = "transfer_party(id) ON DELETE CASCADE")]
    pub transfer_party_id: Option<i64>,
    pub remote_base_url: Option<String>,
    pub remote_public_key: Option<String>,
    pub remote_subject_uid: Option<String>,
    pub display_name_snapshot: Option<String>,
    pub organ_name_snapshot: Option<String>,
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
    #[table(check = "sync_uid IS NULL OR length(trim(sync_uid)) > 0")]
    pub sync_uid: Option<String>,
    #[table(references = "organ(id)", check = "origin_organ_id IS NULL OR origin_organ_id > 0")]
    pub origin_organ_id: Option<i64>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "work_assignment")]
#[table(strict)]
#[table(index(
    name = "uq_work_assignment_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
#[table(index(name = "idx_work_assignment_metadata", columns = "work_metadata_id"))]
#[table(index(name = "idx_work_assignment_subject", columns = "work_subject_id"))]
#[table(index(
    name = "uq_work_assignment_identity",
    columns = "work_metadata_id, work_subject_id, assignment_kind",
    unique
))]
pub struct WorkAssignmentRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "work_metadata(id) ON DELETE CASCADE")]
    pub work_metadata_id: i64,
    #[table(references = "work_subject(id) ON DELETE CASCADE")]
    pub work_subject_id: i64,
    #[table(
        default = "'responsible'",
        check = "assignment_kind IN ('responsible', 'observer', 'helper')"
    )]
    pub assignment_kind: String,
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
    #[table(check = "sync_uid IS NULL OR length(trim(sync_uid)) > 0")]
    pub sync_uid: Option<String>,
    #[table(references = "organ(id)", check = "origin_organ_id IS NULL OR origin_organ_id > 0")]
    pub origin_organ_id: Option<i64>,
}
