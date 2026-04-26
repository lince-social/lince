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
