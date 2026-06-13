use lince_persistence_table_derive::Table;

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record")]
pub struct RecordRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: f64,
    pub head: Option<String>,
    pub body: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "view")]
pub struct ViewRow {
    #[table(primary_key)]
    pub id: i64,
    pub name: String,
    #[table(default = "'SELECT * FROM record'")]
    pub query: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "collection")]
pub struct CollectionRow {
    #[table(primary_key)]
    pub id: i64,
    pub quantity: Option<i64>,
    pub name: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "configuration")]
pub struct ConfigurationRow {
    #[table(primary_key)]
    pub id: i64,
    pub quantity: Option<i64>,
    pub name: String,
    pub language: Option<String>,
    pub timezone: Option<i64>,
    pub style: Option<String>,
    #[table(default = "0")]
    pub show_command_notifications: i64,
    #[table(default = "-1")]
    pub command_notification_seconds: f64,
    #[table(default = "1")]
    pub delete_confirmation: i64,
    #[table(default = "5")]
    pub error_toast_seconds: f64,
    #[table(default = "0")]
    pub keybinding_mode: i64,
    #[table(default = "0")]
    pub bucket_enabled: i64,
    pub bucket_username: Option<String>,
    pub bucket_password: Option<String>,
    pub bucket_uri: Option<String>,
    pub bucket_name: Option<String>,
    pub bucket_region: Option<String>,
    #[table(default = "0")]
    pub file_sync_enabled: i64,
    pub file_sync_path: Option<String>,
    #[table(default = "0")]
    pub transfer_public_proposals_enabled: i64,
    pub desktop_start_on_login: Option<i64>,
    pub desktop_start_silent: Option<i64>,
    #[table(default = "'rolling'")]
    pub automatic_update_channel: String,
    #[table(default = "1")]
    pub automatic_update_notify_enabled: i64,
    #[table(default = "1")]
    pub automatic_update_install_enabled: i64,
    pub automatic_update_last_seen_revision: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "collection_view")]
pub struct CollectionViewRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: i64,
    #[table(references = "collection(id)")]
    pub collection_id: Option<i64>,
    #[table(references = "view(id)")]
    pub view_id: Option<i64>,
    #[table(default = "'{}'")]
    pub column_sizes: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "command")]
pub struct CommandRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: f64,
    #[table(default = "'Command'")]
    pub name: String,
    pub command: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "frequency")]
pub struct FrequencyRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: f64,
    #[table(default = "'Frequency'")]
    pub name: String,
    pub day_week: Option<f64>,
    #[table(default = "0")]
    pub months: f64,
    #[table(default = "0")]
    pub days: f64,
    #[table(default = "0")]
    pub seconds: f64,
    #[table(sql_type = "TIMESTAMP", default = "CURRENT_TIMESTAMP")]
    pub next_date: String,
    #[table(sql_type = "DATETIME")]
    pub finish_date: Option<String>,
    #[table(default = "0")]
    pub catch_up_sum: i64,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "transfer")]
pub struct TransferRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: f64,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "transfer_item")]
pub struct TransferItemRow {
    pub transfer_id: i64,

    pub contribution_user_id: i64,
    pub contribution_server_id: i64,
    pub contribution_id: i64,
    pub contribution_head: String,
    pub contribution_quantity: f64,

    pub need_user_id: i64,
    pub need_server_id: i64,
    pub need_id: i64,
    pub need_head: String,
    pub need_quantity: f64,

    #[table(default = "0")]
    pub first_agreement: i64,
    #[table(default = "0")]
    pub second_agreement: i64,

    #[table(sql_type = "TIMESTAMP", default = "CURRENT_TIMESTAMP")]
    pub date: String,
    pub location: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "transfer_node_identity")]
#[table(strict)]
pub struct TransferNodeIdentityRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(label)) > 0")]
    pub label: String,
    #[table(check = "length(trim(public_key)) > 0")]
    pub public_key: String,
    #[table(check = "length(trim(secret_key)) > 0")]
    pub secret_key: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "transfer_identity")]
#[table(strict)]
#[table(index(name = "idx_transfer_identity_uid", columns = "transfer_uid", unique))]
pub struct TransferIdentityRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE", unique)]
    pub transfer_id: i64,
    #[table(check = "length(trim(transfer_uid)) > 0")]
    pub transfer_uid: String,
    pub parent_transfer_uid: Option<String>,
    pub source_transfer_uid: Option<String>,
    #[table(check = "length(trim(state)) > 0")]
    pub state: String,
    #[table(check = "length(trim(title)) > 0")]
    pub title: String,
    #[table(check = "length(trim(coordinator_label)) > 0")]
    pub coordinator_label: String,
    #[table(check = "length(trim(proposer_label)) > 0")]
    pub proposer_label: String,
    #[table(check = "length(trim(counterparty_label)) > 0")]
    pub counterparty_label: String,
    #[table(check = "length(trim(contribution_actor_label)) > 0")]
    pub contribution_actor_label: String,
    pub contribution_public_key: Option<String>,
    #[table(check = "length(trim(need_actor_label)) > 0")]
    pub need_actor_label: String,
    pub need_public_key: Option<String>,
    pub target_organ_id: Option<i64>,
    pub target_organ_name: Option<String>,
    pub target_base_url: Option<String>,
    pub source_base_url: Option<String>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "transfer_event")]
#[table(strict)]
pub struct TransferEventRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    pub transfer_uid: Option<String>,
    pub event_uid: Option<String>,
    #[table(check = "length(trim(actor_label)) > 0")]
    pub actor_label: String,
    pub actor_public_key: Option<String>,
    #[table(
        check = "event_kind IN ('transfer_created', 'item_created', 'agreement_changed', 'delivery_confirmed', 'receipt_confirmed', 'settlement_applied')"
    )]
    pub event_kind: String,
    #[table(default = "'{}'", check = "json_valid(payload_json)")]
    pub payload_json: String,
    #[table(references = "transfer_event(id)")]
    pub previous_event_id: Option<i64>,
    pub previous_event_uid: Option<String>,
    pub signature: Option<String>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "transfer_settlement")]
#[table(strict)]
pub struct TransferSettlementRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE", unique)]
    pub transfer_id: i64,
    #[table(references = "record(id)")]
    pub my_record_id: i64,
    #[table(references = "record(id)")]
    pub server_record_id: i64,
    pub my_quantity_delta: f64,
    pub server_quantity_delta: f64,
    #[table(references = "transfer_event(id)")]
    pub event_id: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub settled_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "transfer_local_settlement")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_local_settlement_transfer_actor",
    columns = "transfer_id, local_actor_label",
    unique
))]
pub struct TransferLocalSettlementRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    #[table(references = "record(id)")]
    pub local_record_id: i64,
    #[table(check = "length(trim(local_actor_label)) > 0")]
    pub local_actor_label: String,
    pub local_quantity_delta: f64,
    #[table(references = "transfer_event(id)")]
    pub event_id: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub settled_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "transfer_sync_cursor")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_sync_cursor_transfer_peer",
    columns = "transfer_id, peer_label",
    unique
))]
pub struct TransferSyncCursorRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    #[table(check = "length(trim(peer_label)) > 0")]
    pub peer_label: String,
    #[table(references = "transfer_event(id)")]
    pub last_event_id: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub last_synced_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_sync_outbox")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_sync_outbox_transfer_target",
    columns = "transfer_id, target_base_url",
    unique
))]
pub struct TransferSyncOutboxRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    #[table(check = "length(trim(target_base_url)) > 0")]
    pub target_base_url: String,
    #[table(default = "0")]
    pub attempts: i64,
    pub last_error: Option<String>,
    pub last_attempt_at: Option<String>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_gossip_package")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_gossip_package_uid",
    columns = "transfer_uid",
    unique
))]
pub struct TransferGossipPackageRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(transfer_uid)) > 0")]
    pub transfer_uid: String,
    #[table(default = "'{}'", check = "json_valid(package_json)")]
    pub package_json: String,
    pub source_base_url: Option<String>,
    pub target_base_url: Option<String>,
    pub observed_from_base_url: Option<String>,
    #[table(default = "0")]
    pub event_count: i64,
    pub latest_event_created_at: Option<String>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub first_seen_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
    pub last_pulsed_at: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "sum")]
pub struct SumRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: f64,
    pub record_id: Option<i64>,
    #[table(sql_type = "BOOLEAN")]
    pub interval_relative: Option<i64>,
    pub interval_length: Option<String>,
    pub sum_mode: Option<i64>,
    pub end_lag: Option<String>,
    #[table(sql_type = "DATETIME")]
    pub end_date: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "history")]
pub struct HistoryRow {
    #[table(primary_key)]
    pub id: i64,
    pub record_id: i64,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub change_time: Option<String>,
    pub old_quantity: f64,
    pub new_quantity: f64,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "query")]
pub struct QueryRow {
    #[table(primary_key)]
    pub id: i64,
    pub name: Option<String>,
    pub query: String,
}
