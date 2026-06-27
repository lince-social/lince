use lince_persistence_table_derive::Table;

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "role")]
#[table(strict)]
pub struct RoleRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(unique, check = "length(trim(name)) > 0")]
    pub name: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "permission")]
#[table(strict)]
pub struct PermissionRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(subject)) > 0")]
    pub subject: String,
    #[table(check = "length(trim(action)) > 0")]
    pub action: String,
    #[table(check = "description IS NULL OR length(trim(description)) > 0")]
    pub description: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "role_permission")]
#[table(strict)]
#[table(primary_key(columns = "role_id, permission_id"))]
pub struct RolePermissionRow {
    #[table(references = "role(id) ON DELETE CASCADE", check = "role_id > 0")]
    pub role_id: i64,
    #[table(
        references = "permission(id) ON DELETE CASCADE",
        check = "permission_id > 0"
    )]
    pub permission_id: i64,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "app_user")]
#[table(strict)]
pub struct AppUserRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(name)) > 0")]
    pub name: String,
    #[table(unique, check = "length(trim(username)) > 0")]
    pub username: String,
    #[table(check = "length(trim(password_hash)) > 0")]
    pub password_hash: String,
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
    #[table(references = "role(id)", check = "role_id IS NULL OR role_id > 0")]
    pub role_id: Option<i64>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "organ")]
#[table(strict)]
pub struct OrganRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(check = "length(trim(name)) > 0")]
    pub name: String,
    #[table(check = "length(trim(base_url)) > 0")]
    pub base_url: String,
    #[table(
        default = "'known'",
        check = "trust_state IN ('unknown', 'known', 'blocked')"
    )]
    pub trust_state: String,
    #[table(default = "0", check = "contact_discovery_enabled IN (0, 1)")]
    pub contact_discovery_enabled: i64,
    #[table(check = "last_seen_at IS NULL OR julianday(last_seen_at) IS NOT NULL")]
    pub last_seen_at: Option<String>,
    #[table(
        check = "last_transfer_polled_at IS NULL OR julianday(last_transfer_polled_at) IS NOT NULL"
    )]
    pub last_transfer_polled_at: Option<String>,
    #[table(default = "100", check = "proximity >= 0")]
    pub proximity: i64,
    #[table(default = "1", check = "transfer_send_received_receipts IN (0, 1)")]
    pub transfer_send_received_receipts: i64,
    #[table(default = "1", check = "transfer_send_seen_receipts IN (0, 1)")]
    pub transfer_send_seen_receipts: i64,
    #[table(default = "0", check = "file_sync_enabled IN (0, 1)")]
    pub file_sync_enabled: i64,
    pub file_sync_path: Option<String>,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "view_dependency")]
#[table(strict)]
#[table(primary_key(columns = "view_id, table_name"))]
pub struct ViewDependencyRow {
    #[table(references = "view(id) ON DELETE CASCADE", check = "view_id > 0")]
    pub view_id: i64,
    #[table(check = "length(trim(table_name)) > 0")]
    pub table_name: String,
}
