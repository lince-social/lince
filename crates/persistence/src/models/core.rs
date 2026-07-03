use lince_persistence_table_derive::Table;

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "record")]
#[table(index(
    name = "uq_record_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
pub struct RecordRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: f64,
    pub head: Option<String>,
    pub body: Option<String>,
    #[table(
        references = "organ(id)",
        check = "owner_organ_id IS NULL OR owner_organ_id > 0"
    )]
    pub owner_organ_id: Option<i64>,
    #[table(check = "sync_uid IS NULL OR length(trim(sync_uid)) > 0")]
    pub sync_uid: Option<String>,
    #[table(
        references = "organ(id)",
        check = "origin_organ_id IS NULL OR origin_organ_id > 0"
    )]
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
    #[table(default = "1", check = "transfer_known_peer_polling_enabled IN (0, 1)")]
    pub transfer_known_peer_polling_enabled: i64,
    #[table(
        default = "'soft'",
        check = "transfer_reservation_policy IN ('none', 'soft', 'hard_on_proposal', 'hard_on_consume', 'hard_on_lock')"
    )]
    pub transfer_reservation_policy: String,
    #[table(default = "1", check = "transfer_send_received_receipts IN (0, 1)")]
    pub transfer_send_received_receipts: i64,
    #[table(default = "1", check = "transfer_send_seen_receipts IN (0, 1)")]
    pub transfer_send_seen_receipts: i64,
    #[table(default = "0", check = "transfer_anonymous_package_viewing IN (0, 1)")]
    pub transfer_anonymous_package_viewing: i64,
    #[table(default = "0", check = "transfer_share_quantity_projections IN (0, 1)")]
    pub transfer_share_quantity_projections: i64,
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
    pub topic_text: Option<String>,
    pub agreement_type: Option<String>,
    #[table(
        check = "agreement_percentage IS NULL OR (agreement_percentage >= 0 AND agreement_percentage <= 100)"
    )]
    pub agreement_percentage: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_relation")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_relation_transfer_type",
    columns = "transfer_uid, relation_type"
))]
#[table(index(
    name = "idx_transfer_relation_target_type",
    columns = "target_transfer_uid, relation_type"
))]
#[table(index(
    name = "uq_transfer_relation_identity",
    columns = "transfer_uid, relation_type, target_transfer_uid",
    unique
))]
pub struct TransferRelationRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(transfer_uid)) > 0")]
    pub transfer_uid: String,
    #[table(check = "relation_type IN ('parent', 'depends_on')")]
    pub relation_type: String,
    #[table(check = "length(trim(target_transfer_uid)) > 0")]
    pub target_transfer_uid: String,
    pub position: Option<f64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_tree_config")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_tree_config_uid",
    columns = "transfer_uid",
    unique
))]
pub struct TransferTreeConfigRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(transfer_uid)) > 0")]
    pub transfer_uid: String,
    #[table(check = "branch_mode IN ('inherit', 'duplicated', 'greedy')")]
    pub branch_mode: String,
    #[table(check = "record_sync_mode IN ('none', 'copy_once', 'live')")]
    pub record_sync_mode: String,
    #[table(
        check = "reservation_policy IS NULL OR reservation_policy IN ('none', 'soft', 'hard_on_proposal', 'hard_on_consume', 'hard_on_lock')"
    )]
    pub reservation_policy: Option<String>,
    #[table(references = "record(id)")]
    pub source_record_id: Option<i64>,
    #[table(check = "sync_role IS NULL OR sync_role IN ('need', 'contribution')")]
    pub sync_role: Option<String>,
    pub sync_quantity: Option<f64>,
    pub sync_counterparty_label: Option<String>,
    pub sync_target_organ_id: Option<i64>,
    pub last_synced_record_head: Option<String>,
    #[table(default = "0")]
    pub sync_enabled: i64,
    pub last_synced_at: Option<String>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_party")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_party_transfer_role",
    columns = "transfer_id, participation_kind"
))]
#[table(index(name = "idx_transfer_party_public_key", columns = "public_key"))]
#[table(index(
    name = "uq_transfer_party_uid",
    columns = "transfer_id, party_uid",
    unique
))]
pub struct TransferPartyRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    #[table(check = "party_uid IS NULL OR length(trim(party_uid)) > 0")]
    pub party_uid: Option<String>,
    #[table(
        check = "participation_kind IN ('participant', 'coordinator', 'observer', 'placeholder')"
    )]
    pub participation_kind: String,
    #[table(
        check = "role_hint IS NULL OR role_hint IN ('need', 'contribution', 'support', 'task', 'information', 'reservation')"
    )]
    pub role_hint: Option<String>,
    #[table(check = "length(trim(actor_label)) > 0")]
    pub actor_label: String,
    pub public_key: Option<String>,
    pub organ_id: Option<i64>,
    pub user_id: Option<i64>,
    #[table(default = "0")]
    pub placeholder: i64,
    pub replaced_by_party_id: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_structured_item")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_structured_item_transfer_role",
    columns = "transfer_id, role"
))]
#[table(index(
    name = "idx_transfer_structured_item_source_record",
    columns = "source_record_id"
))]
#[table(index(
    name = "uq_transfer_structured_item_uid",
    columns = "transfer_id, item_uid",
    unique
))]
pub struct TransferStructuredItemRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    #[table(check = "item_uid IS NULL OR length(trim(item_uid)) > 0")]
    pub item_uid: Option<String>,
    #[table(
        check = "role IN ('need', 'contribution', 'support', 'task', 'information', 'reservation')"
    )]
    pub role: String,
    #[table(references = "record(id)")]
    pub source_record_id: Option<i64>,
    pub owner_party_id: Option<i64>,
    #[table(check = "length(trim(title)) > 0")]
    pub title: String,
    pub description: Option<String>,
    pub record_head_snapshot: Option<String>,
    pub record_body_snapshot: Option<String>,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub location: Option<String>,
    #[table(default = "'{}'", check = "json_valid(metadata_json)")]
    pub metadata_json: String,
    #[table(default = "1")]
    pub version: i64,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_interaction")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_interaction_transfer_kind",
    columns = "transfer_id, interaction_kind"
))]
#[table(index(name = "idx_transfer_interaction_from_item", columns = "from_item_id"))]
#[table(index(name = "idx_transfer_interaction_to_item", columns = "to_item_id"))]
#[table(index(
    name = "uq_transfer_interaction_uid",
    columns = "transfer_id, interaction_uid",
    unique
))]
pub struct TransferInteractionRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    #[table(check = "interaction_uid IS NULL OR length(trim(interaction_uid)) > 0")]
    pub interaction_uid: Option<String>,
    #[table(
        check = "interaction_kind IN ('contributes_to', 'depends_on', 'unblocks', 'replaces', 'informs')"
    )]
    pub interaction_kind: String,
    #[table(check = "direction IN ('incoming', 'outgoing', 'mutual', 'informational')")]
    pub direction: String,
    pub from_item_id: Option<i64>,
    pub to_item_id: Option<i64>,
    pub from_party_id: Option<i64>,
    pub to_party_id: Option<i64>,
    pub quantity: Option<f64>,
    pub state: String,
    #[table(
        check = "dependency_kind IS NULL OR dependency_kind IN ('must_agree', 'must_activate', 'must_deliver', 'must_receive', 'must_settle')"
    )]
    pub dependency_kind: Option<String>,
    #[table(default = "'{}'", check = "json_valid(metadata_json)")]
    pub metadata_json: String,
    #[table(default = "1")]
    pub version: i64,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_agreement")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_agreement_scope",
    columns = "transfer_id, scope_kind, scope_id"
))]
#[table(index(name = "idx_transfer_agreement_party", columns = "party_id"))]
#[table(index(
    name = "uq_transfer_agreement_party_scope",
    columns = "transfer_id, party_id, scope_kind, scope_id",
    unique
))]
pub struct TransferAgreementRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    pub party_id: Option<i64>,
    #[table(check = "scope_kind IN ('transfer', 'item', 'interaction')")]
    pub scope_kind: String,
    pub scope_id: Option<i64>,
    #[table(default = "0", check = "agreement_level IN (0, 1, 2)")]
    pub agreement_level: i64,
    pub agreed_item_version: Option<i64>,
    pub agreed_interaction_version: Option<i64>,
    pub event_id: Option<i64>,
    pub agreed_at: Option<String>,
    pub invalidated_at: Option<String>,
    pub invalidated_by_event_id: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_confirmation")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_confirmation_scope",
    columns = "transfer_id, scope_kind, scope_id"
))]
#[table(index(
    name = "uq_transfer_confirmation_party_scope_kind",
    columns = "transfer_id, party_id, scope_kind, scope_id, confirmation_kind",
    unique
))]
pub struct TransferConfirmationRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    pub party_id: Option<i64>,
    #[table(check = "scope_kind IN ('transfer', 'item', 'interaction')")]
    pub scope_kind: String,
    pub scope_id: Option<i64>,
    #[table(check = "confirmation_kind IN ('delivery', 'receipt')")]
    pub confirmation_kind: String,
    pub event_id: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub confirmed_at: String,
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
        check = "event_kind IN ('transfer_created', 'transfer_quantity_changed', 'transfer_inactivated', 'item_created', 'item_edited', 'interaction_created', 'interaction_edited', 'visibility_changed', 'agreement_changed', 'message_sent', 'delivery_confirmed', 'receipt_confirmed', 'package_received', 'package_seen', 'settlement_applied', 'settlement_reverted', 'dispute_opened', 'dispute_resolved')"
    )]
    pub event_kind: String,
    #[table(default = "'{}'", check = "json_valid(payload_json)")]
    pub payload_json: String,
    #[table(references = "transfer_event(id)")]
    pub previous_event_id: Option<i64>,
    pub previous_event_uid: Option<String>,
    pub previous_event_hash: Option<String>,
    pub event_hash: Option<String>,
    pub signature: Option<String>,
    #[table(
        default = "'pending'",
        check = "validation_state IN ('pending', 'valid', 'invalid')"
    )]
    pub validation_state: String,
    pub validation_error: Option<String>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_structured_settlement")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_structured_settlement_scope",
    columns = "transfer_id, scope_kind, scope_id"
))]
#[table(index(
    name = "uq_transfer_structured_settlement_record_scope",
    columns = "transfer_id, party_id, local_record_id, scope_kind, scope_id",
    unique
))]
pub struct TransferStructuredSettlementRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    pub party_id: Option<i64>,
    pub item_id: Option<i64>,
    pub interaction_id: Option<i64>,
    #[table(check = "scope_kind IN ('transfer', 'item', 'interaction')")]
    pub scope_kind: String,
    pub scope_id: Option<i64>,
    #[table(references = "record(id)")]
    pub local_record_id: i64,
    pub quantity_delta: f64,
    pub event_id: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub settled_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_quantity_influence")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_quantity_influence_record_state",
    columns = "record_id, influence_state"
))]
#[table(index(
    name = "idx_transfer_quantity_influence_transfer",
    columns = "transfer_id"
))]
pub struct TransferQuantityInfluenceRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    pub item_id: Option<i64>,
    pub interaction_id: Option<i64>,
    #[table(references = "record(id)")]
    pub record_id: i64,
    pub influence: f64,
    #[table(
        default = "'planned'",
        check = "influence_state IN ('planned', 'active', 'consumed', 'released', 'invalidated')"
    )]
    pub influence_state: String,
    #[table(
        default = "'manual'",
        check = "policy IN ('protect_transfer', 'surplus_transfer', 'proportional', 'manual')"
    )]
    pub policy: String,
    pub event_id: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    pub consumed_at: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "record_transfer_availability")]
#[table(strict)]
pub struct RecordTransferAvailabilityRow {
    #[table(primary_key)]
    #[table(references = "record(id) ON DELETE CASCADE")]
    pub record_id: i64,
    #[table(default = "0")]
    pub actual_quantity: f64,
    #[table(default = "0")]
    pub proposed_outgoing_quantity: f64,
    #[table(default = "0")]
    pub proposed_incoming_quantity: f64,
    #[table(default = "0")]
    pub reserved_quantity: f64,
    #[table(default = "0")]
    pub reserved_incoming_quantity: f64,
    #[table(default = "0")]
    pub available_quantity: f64,
    #[table(default = "0")]
    pub planned_quantity: f64,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "message")]
#[table(strict)]
#[table(index(
    name = "idx_message_record_created",
    columns = "record_id, created_at DESC",
    where = "record_id IS NOT NULL"
))]
#[table(index(
    name = "idx_message_transfer_created",
    columns = "transfer_id, created_at",
    where = "transfer_id IS NOT NULL"
))]
#[table(index(
    name = "uq_message_sync_uid",
    columns = "sync_uid",
    unique,
    where = "sync_uid IS NOT NULL"
))]
pub struct MessageRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "record(id) ON DELETE CASCADE")]
    pub record_id: Option<i64>,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: Option<i64>,
    #[table(references = "transfer_interaction(id) ON DELETE SET NULL")]
    pub interaction_id: Option<i64>,
    #[table(references = "message(id) ON DELETE CASCADE")]
    pub parent_message_id: Option<i64>,
    pub author_label: Option<String>,
    #[table(references = "transfer_party(id) ON DELETE SET NULL")]
    pub party_id: Option<i64>,
    #[table(references = "transfer_event(id) ON DELETE SET NULL")]
    pub event_id: Option<i64>,
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
    #[table(references = "organ(id)")]
    pub origin_organ_id: Option<i64>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_visibility_subject")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_visibility_subject_kind",
    columns = "subject_kind"
))]
pub struct TransferVisibilitySubjectRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "subject_kind IN ('user', 'organ', 'party', 'public')")]
    pub subject_kind: String,
    pub user_id: Option<i64>,
    pub organ_id: Option<i64>,
    pub party_id: Option<i64>,
    pub display_name_snapshot: Option<String>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_visibility_policy")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_visibility_policy_mode",
    columns = "visibility_mode"
))]
pub struct TransferVisibilityPolicyRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE", unique)]
    pub transfer_id: i64,
    #[table(
        default = "'hidden'",
        check = "visibility_mode IN ('hidden', 'public', 'restricted')"
    )]
    pub visibility_mode: String,
    #[table(check = "max_visible_proximity IS NULL OR max_visible_proximity >= 0")]
    pub max_visible_proximity: Option<i64>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_visibility_wave")]
#[table(strict)]
#[table(index(
    name = "idx_transfer_visibility_wave_transfer",
    columns = "transfer_id"
))]
#[table(index(name = "idx_transfer_visibility_wave_karma", columns = "karma_id"))]
pub struct TransferVisibilityWaveRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    pub karma_id: Option<i64>,
    #[table(check = "max_visible_proximity >= 0")]
    pub max_visible_proximity: i64,
    #[table(default = "0", check = "active IN (0, 1)")]
    pub active: i64,
    #[table(default = "'karma_consequence'")]
    pub reason: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_visibility_rule")]
#[table(strict)]
#[table(index(name = "idx_transfer_visibility_rule_subject", columns = "subject_id"))]
#[table(index(
    name = "idx_transfer_visibility_rule_scope",
    columns = "transfer_id, scope_kind, scope_id"
))]
pub struct TransferVisibilityRuleRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    pub subject_id: i64,
    #[table(
        check = "scope_kind IN ('transfer', 'item', 'interaction', 'event', 'record', 'message')"
    )]
    pub scope_kind: String,
    pub scope_id: Option<i64>,
    #[table(default = "0")]
    pub can_discover: i64,
    #[table(default = "0")]
    pub can_view: i64,
    #[table(default = "0")]
    pub can_edit: i64,
    #[table(default = "0")]
    pub can_agree: i64,
    #[table(default = "0")]
    pub can_confirm_delivery: i64,
    #[table(default = "0")]
    pub can_confirm_receipt: i64,
    #[table(default = "0")]
    pub can_settle: i64,
    #[table(default = "0")]
    pub can_message: i64,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub created_at: String,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub updated_at: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_visibility_field")]
#[table(strict)]
#[table(index(
    name = "uq_transfer_visibility_field_rule_name",
    columns = "visibility_rule_id, field_name",
    unique
))]
pub struct TransferVisibilityFieldRow {
    #[table(primary_key)]
    pub id: i64,
    pub visibility_rule_id: i64,
    #[table(check = "length(trim(field_name)) > 0")]
    pub field_name: String,
    #[table(default = "0")]
    pub visible: i64,
    #[table(default = "0")]
    pub editable: i64,
    pub redaction_label: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[allow(dead_code)]
#[table(name = "transfer_package_receipt")]
#[table(strict)]
#[table(index(
    name = "uq_transfer_package_receipt_transfer_source",
    columns = "transfer_id, source_base_url",
    unique
))]
pub struct TransferPackageReceiptRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(references = "transfer(id) ON DELETE CASCADE")]
    pub transfer_id: i64,
    pub source_base_url: Option<String>,
    #[table(default = "CURRENT_TIMESTAMP")]
    pub received_at: String,
    pub seen_at: Option<String>,
    #[table(default = "0")]
    pub received_receipt_generated: i64,
    #[table(default = "0")]
    pub seen_receipt_generated: i64,
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
