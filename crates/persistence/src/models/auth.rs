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
    #[table(primary_key, check = "length(trim(id)) > 0")]
    pub id: String,
    #[table(check = "length(trim(name)) > 0")]
    pub name: String,
    #[table(check = "length(trim(base_url)) > 0")]
    pub base_url: String,
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
