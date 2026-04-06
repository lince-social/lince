use ::application::auth::AuthSubject;
use injection::cross_cutting::InjectedServices;
use persistence::write_coordinator::SqlParameter;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Map, Value};
use sqlx::{FromRow, Pool, Sqlite};
use std::io::{Error, ErrorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiTable {
    View,
    Record,
    RecordExtension,
    RecordLink,
    RecordComment,
    RecordWorklog,
    RecordResourceRef,
    Frequency,
    KarmaCondition,
    KarmaConsequence,
    Karma,
    Configuration,
    AppUser,
    Role,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TableListQuery {
    pub head_contains: Option<String>,
    pub category: Option<String>,
    pub assignee_id: Option<i64>,
}

impl TableListQuery {
    fn normalized_head_contains(&self) -> Option<String> {
        self.head_contains
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase)
    }

    fn normalized_categories(&self) -> Vec<String> {
        self.category
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase)
            .collect()
    }

    fn normalized_assignee_id(&self) -> Option<i64> {
        self.assignee_id.filter(|value| *value > 0)
    }
}

#[derive(Debug, Clone, Copy)]
enum FieldKind {
    Text,
    NullableText,
    Integer,
    NullableInteger,
    Real,
    NullableReal,
    BooleanInteger,
}

#[derive(Debug, Clone, Copy)]
struct FieldSpec {
    name: &'static str,
    kind: FieldKind,
}

#[derive(Debug, Serialize, FromRow)]
struct ViewRow {
    id: i64,
    name: String,
    query: String,
}

#[derive(Debug, Serialize, FromRow)]
struct RecordRow {
    id: i64,
    quantity: f64,
    head: Option<String>,
    body: Option<String>,
    primary_category: Option<String>,
    categories_json: String,
    assignee_ids_json: String,
    assignee_names_json: String,
}

#[derive(Debug, Serialize, FromRow)]
struct RecordExtensionRow {
    id: i64,
    record_id: i64,
    namespace: String,
    version: i64,
    freestyle_data_structure: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, FromRow)]
struct RecordLinkRow {
    id: i64,
    record_id: i64,
    link_type: String,
    target_table: String,
    target_id: i64,
    position: Option<f64>,
    freestyle_data_structure: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, FromRow)]
struct RecordCommentRow {
    id: i64,
    record_id: i64,
    author_user_id: Option<i64>,
    body: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
struct RecordWorklogRow {
    id: i64,
    record_id: i64,
    author_user_id: i64,
    started_at: String,
    ended_at: Option<String>,
    last_heartbeat_at: Option<String>,
    seconds: Option<f64>,
    note: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, FromRow)]
struct RecordResourceRefRow {
    id: i64,
    record_id: i64,
    provider: String,
    resource_kind: String,
    resource_path: String,
    title: Option<String>,
    position: Option<f64>,
    freestyle_data_structure: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, FromRow)]
struct FrequencyRow {
    id: i64,
    quantity: f64,
    name: String,
    day_week: Option<f64>,
    months: f64,
    days: f64,
    seconds: f64,
    next_date: String,
    finish_date: Option<String>,
    catch_up_sum: i64,
}

#[derive(Debug, Serialize, FromRow)]
struct KarmaConditionRow {
    id: i64,
    quantity: i64,
    name: String,
    condition: String,
}

#[derive(Debug, Serialize, FromRow)]
struct KarmaConsequenceRow {
    id: i64,
    quantity: i64,
    name: String,
    consequence: String,
}

#[derive(Debug, Serialize, FromRow)]
struct KarmaRow {
    id: i64,
    quantity: i64,
    name: String,
    condition_id: i64,
    operator: String,
    consequence_id: i64,
}

#[derive(Debug, Serialize, FromRow)]
struct ConfigurationRow {
    id: i64,
    quantity: Option<i64>,
    name: String,
    language: Option<String>,
    timezone: Option<i64>,
    style: Option<String>,
    show_command_notifications: i64,
    command_notification_seconds: f64,
    delete_confirmation: i64,
    error_toast_seconds: f64,
    keybinding_mode: i64,
    bucket_enabled: i64,
    bucket_username: Option<String>,
    bucket_password: Option<String>,
    bucket_uri: Option<String>,
    bucket_name: Option<String>,
    bucket_region: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
struct PublicAppUserRow {
    id: i64,
    name: String,
    username: String,
    role_id: i64,
    role: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, FromRow)]
struct RoleRow {
    id: i64,
    name: String,
}

const VIEW_FIELD_SPECS: [FieldSpec; 2] = [
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "query",
        kind: FieldKind::Text,
    },
];

const RECORD_FIELD_SPECS: [FieldSpec; 3] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "head",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "body",
        kind: FieldKind::NullableText,
    },
];

const RECORD_EXTENSION_FIELD_SPECS: [FieldSpec; 4] = [
    FieldSpec {
        name: "record_id",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "namespace",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "version",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "freestyle_data_structure",
        kind: FieldKind::Text,
    },
];

const RECORD_LINK_FIELD_SPECS: [FieldSpec; 6] = [
    FieldSpec {
        name: "record_id",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "link_type",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "target_table",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "target_id",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "position",
        kind: FieldKind::NullableReal,
    },
    FieldSpec {
        name: "freestyle_data_structure",
        kind: FieldKind::NullableText,
    },
];

const RECORD_COMMENT_FIELD_SPECS: [FieldSpec; 4] = [
    FieldSpec {
        name: "record_id",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "author_user_id",
        kind: FieldKind::NullableInteger,
    },
    FieldSpec {
        name: "body",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "deleted_at",
        kind: FieldKind::NullableText,
    },
];

const RECORD_WORKLOG_FIELD_SPECS: [FieldSpec; 7] = [
    FieldSpec {
        name: "record_id",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "author_user_id",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "started_at",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "ended_at",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "last_heartbeat_at",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "seconds",
        kind: FieldKind::NullableReal,
    },
    FieldSpec {
        name: "note",
        kind: FieldKind::NullableText,
    },
];

const RECORD_RESOURCE_REF_FIELD_SPECS: [FieldSpec; 7] = [
    FieldSpec {
        name: "record_id",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "provider",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "resource_kind",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "resource_path",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "title",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "position",
        kind: FieldKind::NullableReal,
    },
    FieldSpec {
        name: "freestyle_data_structure",
        kind: FieldKind::NullableText,
    },
];

const FREQUENCY_FIELD_SPECS: [FieldSpec; 9] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "day_week",
        kind: FieldKind::NullableInteger,
    },
    FieldSpec {
        name: "months",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "days",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "seconds",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "next_date",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "finish_date",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "catch_up_sum",
        kind: FieldKind::Integer,
    },
];

const KARMA_CONDITION_FIELD_SPECS: [FieldSpec; 3] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "condition",
        kind: FieldKind::Text,
    },
];

const KARMA_CONSEQUENCE_FIELD_SPECS: [FieldSpec; 3] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "consequence",
        kind: FieldKind::Text,
    },
];

const KARMA_FIELD_SPECS: [FieldSpec; 5] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "condition_id",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "operator",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "consequence_id",
        kind: FieldKind::Integer,
    },
];

const CONFIGURATION_FIELD_SPECS: [FieldSpec; 16] = [
    FieldSpec {
        name: "quantity",
        kind: FieldKind::NullableInteger,
    },
    FieldSpec {
        name: "name",
        kind: FieldKind::Text,
    },
    FieldSpec {
        name: "language",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "timezone",
        kind: FieldKind::NullableInteger,
    },
    FieldSpec {
        name: "style",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "show_command_notifications",
        kind: FieldKind::BooleanInteger,
    },
    FieldSpec {
        name: "command_notification_seconds",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "delete_confirmation",
        kind: FieldKind::BooleanInteger,
    },
    FieldSpec {
        name: "error_toast_seconds",
        kind: FieldKind::Real,
    },
    FieldSpec {
        name: "keybinding_mode",
        kind: FieldKind::Integer,
    },
    FieldSpec {
        name: "bucket_enabled",
        kind: FieldKind::BooleanInteger,
    },
    FieldSpec {
        name: "bucket_username",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "bucket_password",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "bucket_uri",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "bucket_name",
        kind: FieldKind::NullableText,
    },
    FieldSpec {
        name: "bucket_region",
        kind: FieldKind::NullableText,
    },
];

const ROLE_FIELD_SPECS: [FieldSpec; 1] = [FieldSpec {
    name: "name",
    kind: FieldKind::Text,
}];

#[derive(Clone)]
pub struct BackendApiStore {
    services: InjectedServices,
}

impl BackendApiStore {
    pub fn new(services: InjectedServices) -> Self {
        Self { services }
    }

    pub fn parse_table(&self, table_name: &str) -> Result<ApiTable, Error> {
        parse_api_table(table_name)
    }

    pub async fn list_table_rows_filtered(
        &self,
        table: ApiTable,
        query: &TableListQuery,
    ) -> Result<Value, Error> {
        let db = &*self.services.db;
        match table {
            ApiTable::View => serialize_value(
                sqlx::query_as::<_, ViewRow>("SELECT id, name, query FROM view ORDER BY id")
                    .fetch_all(db)
                    .await
                    .map_err(map_sqlx_error)?,
            ),
            ApiTable::Record => serialize_value(self.list_record_rows(db, query).await?),
            ApiTable::RecordExtension => serialize_value(
                sqlx::query_as::<_, RecordExtensionRow>(
                    "SELECT id, record_id, namespace, version, freestyle_data_structure, created_at, updated_at FROM record_extension ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::RecordLink => serialize_value(
                sqlx::query_as::<_, RecordLinkRow>(
                    "SELECT id, record_id, link_type, target_table, target_id, position, freestyle_data_structure, created_at, updated_at FROM record_link ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::RecordComment => serialize_value(
                sqlx::query_as::<_, RecordCommentRow>(
                    "SELECT id, record_id, author_user_id, body, created_at, updated_at, deleted_at FROM record_comment ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::RecordWorklog => serialize_value(
                sqlx::query_as::<_, RecordWorklogRow>(
                    "SELECT id, record_id, author_user_id, started_at, ended_at, last_heartbeat_at, seconds, note, created_at, updated_at FROM record_worklog ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::RecordResourceRef => serialize_value(
                sqlx::query_as::<_, RecordResourceRefRow>(
                    "SELECT id, record_id, provider, resource_kind, resource_path, title, position, freestyle_data_structure, created_at, updated_at FROM record_resource_ref ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::Frequency => serialize_value(
                sqlx::query_as::<_, FrequencyRow>(
                    "SELECT id, quantity, name, day_week, months, days, seconds, next_date, finish_date, catch_up_sum FROM frequency ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::KarmaCondition => serialize_value(
                sqlx::query_as::<_, KarmaConditionRow>(
                    "SELECT id, quantity, name, condition FROM karma_condition ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::KarmaConsequence => serialize_value(
                sqlx::query_as::<_, KarmaConsequenceRow>(
                    "SELECT id, quantity, name, consequence FROM karma_consequence ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::Karma => serialize_value(
                sqlx::query_as::<_, KarmaRow>(
                    "SELECT id, quantity, name, condition_id, operator, consequence_id FROM karma ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::Configuration => serialize_value(
                sqlx::query_as::<_, ConfigurationRow>(
                    "SELECT id, quantity, name, language, timezone, style, show_command_notifications, command_notification_seconds, delete_confirmation, error_toast_seconds, keybinding_mode, bucket_enabled, bucket_username, bucket_password, bucket_uri, bucket_name, bucket_region FROM configuration ORDER BY id",
                )
                .fetch_all(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::AppUser => serialize_value(fetch_public_users(db).await?),
            ApiTable::Role => serialize_value(
                sqlx::query_as::<_, RoleRow>("SELECT id, name FROM role ORDER BY id")
                    .fetch_all(db)
                    .await
                    .map_err(map_sqlx_error)?,
            ),
        }
    }

    pub async fn get_table_row(&self, table: ApiTable, id: i64) -> Result<Value, Error> {
        let db = &*self.services.db;
        match table {
            ApiTable::View => serialize_value(
                sqlx::query_as::<_, ViewRow>("SELECT id, name, query FROM view WHERE id = ?")
                    .bind(id)
                    .fetch_one(db)
                    .await
                    .map_err(map_sqlx_error)?,
            ),
            ApiTable::Record => serialize_value(self.get_record_row(db, id).await?),
            ApiTable::RecordExtension => serialize_value(
                sqlx::query_as::<_, RecordExtensionRow>(
                    "SELECT id, record_id, namespace, version, freestyle_data_structure, created_at, updated_at FROM record_extension WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::RecordLink => serialize_value(
                sqlx::query_as::<_, RecordLinkRow>(
                    "SELECT id, record_id, link_type, target_table, target_id, position, freestyle_data_structure, created_at, updated_at FROM record_link WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::RecordComment => serialize_value(
                sqlx::query_as::<_, RecordCommentRow>(
                    "SELECT id, record_id, author_user_id, body, created_at, updated_at, deleted_at FROM record_comment WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::RecordWorklog => serialize_value(
                sqlx::query_as::<_, RecordWorklogRow>(
                    "SELECT id, record_id, author_user_id, started_at, ended_at, last_heartbeat_at, seconds, note, created_at, updated_at FROM record_worklog WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::RecordResourceRef => serialize_value(
                sqlx::query_as::<_, RecordResourceRefRow>(
                    "SELECT id, record_id, provider, resource_kind, resource_path, title, position, freestyle_data_structure, created_at, updated_at FROM record_resource_ref WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::Frequency => serialize_value(
                sqlx::query_as::<_, FrequencyRow>(
                    "SELECT id, quantity, name, day_week, months, days, seconds, next_date, finish_date, catch_up_sum FROM frequency WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::KarmaCondition => serialize_value(
                sqlx::query_as::<_, KarmaConditionRow>(
                    "SELECT id, quantity, name, condition FROM karma_condition WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::KarmaConsequence => serialize_value(
                sqlx::query_as::<_, KarmaConsequenceRow>(
                    "SELECT id, quantity, name, consequence FROM karma_consequence WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::Karma => serialize_value(
                sqlx::query_as::<_, KarmaRow>(
                    "SELECT id, quantity, name, condition_id, operator, consequence_id FROM karma WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::Configuration => serialize_value(
                sqlx::query_as::<_, ConfigurationRow>(
                    "SELECT id, quantity, name, language, timezone, style, show_command_notifications, command_notification_seconds, delete_confirmation, error_toast_seconds, keybinding_mode, bucket_enabled, bucket_username, bucket_password, bucket_uri, bucket_name, bucket_region FROM configuration WHERE id = ?",
                )
                .bind(id)
                .fetch_one(db)
                .await
                .map_err(map_sqlx_error)?,
            ),
            ApiTable::AppUser => serialize_value(fetch_public_user_by_id(db, id).await?),
            ApiTable::Role => serialize_value(
                sqlx::query_as::<_, RoleRow>("SELECT id, name FROM role WHERE id = ?")
                    .bind(id)
                    .fetch_one(db)
                    .await
                    .map_err(map_sqlx_error)?,
            ),
        }
    }

    async fn list_record_rows(
        &self,
        db: &Pool<Sqlite>,
        query: &TableListQuery,
    ) -> Result<Vec<RecordRow>, Error> {
        let mut sql = String::from(
            "SELECT \
                r.id, \
                r.quantity, \
                r.head, \
                r.body, \
                json_extract(cat.categories_json, '$[0]') AS primary_category, \
                COALESCE(cat.categories_json, '[]') AS categories_json, \
                COALESCE(assignments.assignee_ids_json, '[]') AS assignee_ids_json, \
                COALESCE(assignments.assignee_names_json, '[]') AS assignee_names_json \
             FROM record r \
             LEFT JOIN ( \
                SELECT \
                    re.record_id, \
                    json_extract(re.freestyle_data_structure, '$.categories') AS categories_json \
                FROM record_extension re \
                WHERE re.namespace = 'task.categories' \
             ) cat ON cat.record_id = r.id \
             LEFT JOIN ( \
                SELECT \
                    rl.record_id, \
                    json_group_array(rl.target_id) AS assignee_ids_json, \
                    json_group_array(au.name) AS assignee_names_json \
                FROM record_link rl \
                JOIN app_user au ON au.id = rl.target_id \
                WHERE rl.link_type = 'assigned_to' AND rl.target_table = 'app_user' \
                GROUP BY rl.record_id \
             ) assignments ON assignments.record_id = r.id \
             WHERE 1 = 1",
        );

        let mut params = Vec::new();

        if let Some(head_contains) = query.normalized_head_contains() {
            sql.push_str(" AND lower(COALESCE(r.head, '')) LIKE ? ESCAPE '\\'");
            params.push(SqlParameter::Text(sql_like_contains(&head_contains)));
        }

        let categories = query.normalized_categories();
        if !categories.is_empty() {
            sql.push_str(
                " AND EXISTS ( \
                    SELECT 1 \
                    FROM json_each(COALESCE(cat.categories_json, '[]')) value \
                    WHERE lower(CAST(value.value AS TEXT)) IN (",
            );
            for (index, category) in categories.iter().enumerate() {
                if index > 0 {
                    sql.push_str(", ");
                }
                sql.push('?');
                params.push(SqlParameter::Text(category.clone()));
            }
            sql.push_str("))");
        }

        if let Some(assignee_id) = query.normalized_assignee_id() {
            sql.push_str(
                " AND EXISTS ( \
                    SELECT 1 \
                    FROM record_link rl \
                    WHERE rl.record_id = r.id \
                      AND rl.link_type = 'assigned_to' \
                      AND rl.target_table = 'app_user' \
                      AND rl.target_id = ? \
                )",
            );
            params.push(SqlParameter::Integer(assignee_id));
        }

        sql.push_str(" ORDER BY lower(COALESCE(r.head, '')) ASC, r.id DESC");
        execute_query_as::<RecordRow>(db, &sql, params).await
    }

    async fn get_record_row(&self, db: &Pool<Sqlite>, id: i64) -> Result<RecordRow, Error> {
        let sql =
            "SELECT \
                r.id, \
                r.quantity, \
                r.head, \
                r.body, \
                json_extract(cat.categories_json, '$[0]') AS primary_category, \
                COALESCE(cat.categories_json, '[]') AS categories_json, \
                COALESCE(assignments.assignee_ids_json, '[]') AS assignee_ids_json, \
                COALESCE(assignments.assignee_names_json, '[]') AS assignee_names_json \
             FROM record r \
             LEFT JOIN ( \
                SELECT \
                    re.record_id, \
                    json_extract(re.freestyle_data_structure, '$.categories') AS categories_json \
                FROM record_extension re \
                WHERE re.namespace = 'task.categories' \
             ) cat ON cat.record_id = r.id \
             LEFT JOIN ( \
                SELECT \
                    rl.record_id, \
                    json_group_array(rl.target_id) AS assignee_ids_json, \
                    json_group_array(au.name) AS assignee_names_json \
                FROM record_link rl \
                JOIN app_user au ON au.id = rl.target_id \
                WHERE rl.link_type = 'assigned_to' AND rl.target_table = 'app_user' \
                GROUP BY rl.record_id \
             ) assignments ON assignments.record_id = r.id \
             WHERE r.id = ?";
        execute_query_one_as::<RecordRow>(db, sql, vec![SqlParameter::Integer(id)]).await
    }

    pub fn build_standard_insert(
        &self,
        table: ApiTable,
        object: &Map<String, Value>,
    ) -> Result<(String, Vec<SqlParameter>), Error> {
        reject_common_forbidden_fields(object)?;
        let specs = table
            .standard_field_specs()
            .ok_or_else(|| Error::other("Missing field specs"))?;
        let fields = parse_fields(object, &specs)?;
        if fields.is_empty() {
            return Ok((
                format!("INSERT INTO {} DEFAULT VALUES", table.as_table_name()),
                vec![],
            ));
        }

        let columns = fields
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = vec!["?"; fields.len()].join(", ");
        let params = fields
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>();

        Ok((
            format!(
                "INSERT INTO {} ({columns}) VALUES ({placeholders})",
                table.as_table_name()
            ),
            params,
        ))
    }

    pub fn build_standard_update(
        &self,
        table: ApiTable,
        id: i64,
        object: &Map<String, Value>,
    ) -> Result<(String, Vec<SqlParameter>), Error> {
        reject_common_forbidden_fields(object)?;
        let specs = table
            .standard_field_specs()
            .ok_or_else(|| Error::other("Missing field specs"))?;
        let fields = parse_fields(object, &specs)?;
        if fields.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "At least one writable field is required",
            ));
        }

        let assignments = fields
            .iter()
            .map(|(name, _)| format!("{name} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut params = fields
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        params.push(SqlParameter::Integer(id));

        Ok((
            format!(
                "UPDATE {} SET {assignments} WHERE id = ?",
                table.as_table_name()
            ),
            params,
        ))
    }

    pub async fn build_app_user_insert(
        &self,
        object: &Map<String, Value>,
        password_hash: String,
    ) -> Result<(String, Vec<SqlParameter>), Error> {
        reject_user_forbidden_fields(object)?;
        let name = required_text_field(object, "name")?;
        let username = required_text_field(object, "username")?;

        let role_id = if let Some(value) = object.get("role_id") {
            let role_id = parse_i64_value("role_id", value)?;
            self.ensure_role_exists(role_id).await?;
            role_id
        } else {
            self.role_id_by_name("lince").await?
        };

        reject_unknown_fields(object, &["name", "username", "password", "role_id"])?;

        Ok((
            "INSERT INTO app_user(name, username, password_hash, role_id) VALUES (?, ?, ?, ?)"
                .to_string(),
            vec![
                SqlParameter::Text(name),
                SqlParameter::Text(username),
                SqlParameter::Text(password_hash),
                SqlParameter::Integer(role_id),
            ],
        ))
    }

    pub async fn build_app_user_update(
        &self,
        claims: &AuthSubject,
        id: i64,
        object: &Map<String, Value>,
        password_hash: Option<String>,
    ) -> Result<(String, Vec<SqlParameter>), Error> {
        reject_user_forbidden_fields(object)?;
        reject_unknown_fields(object, &["name", "username", "password", "role_id"])?;

        let mut assignments = Vec::new();
        let mut params = Vec::new();

        if let Some(value) = object.get("name") {
            assignments.push("name = ?".to_string());
            params.push(parse_text_parameter("name", value)?);
        }
        if let Some(value) = object.get("username") {
            assignments.push("username = ?".to_string());
            params.push(parse_text_parameter("username", value)?);
        }
        if let Some(password_hash) = password_hash {
            assignments.push("password_hash = ?".to_string());
            params.push(SqlParameter::Text(password_hash));
        }
        if let Some(value) = object.get("role_id") {
            if !claims.is_admin() {
                return Err(Error::new(
                    ErrorKind::PermissionDenied,
                    "Admin role required",
                ));
            }
            let role_id = parse_i64_value("role_id", value)?;
            self.ensure_role_exists(role_id).await?;
            assignments.push("role_id = ?".to_string());
            params.push(SqlParameter::Integer(role_id));
        }

        if assignments.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "At least one writable field is required",
            ));
        }

        assignments.push("updated_at = CURRENT_TIMESTAMP".to_string());
        params.push(SqlParameter::Integer(id));

        Ok((
            format!(
                "UPDATE app_user SET {} WHERE id = ?",
                assignments.join(", ")
            ),
            params,
        ))
    }

    pub fn build_role_insert(
        &self,
        object: &Map<String, Value>,
    ) -> Result<(String, Vec<SqlParameter>), Error> {
        reject_common_forbidden_fields(object)?;
        let fields = parse_fields(object, &ROLE_FIELD_SPECS)?;
        if fields.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "At least one writable field is required",
            ));
        }

        let columns = fields
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = vec!["?"; fields.len()].join(", ");
        let params = fields
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>();

        Ok((
            format!("INSERT INTO role ({columns}) VALUES ({placeholders})"),
            params,
        ))
    }

    pub fn build_role_update(
        &self,
        id: i64,
        object: &Map<String, Value>,
    ) -> Result<(String, Vec<SqlParameter>), Error> {
        reject_common_forbidden_fields(object)?;
        let fields = parse_fields(object, &ROLE_FIELD_SPECS)?;
        if fields.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "At least one writable field is required",
            ));
        }

        let assignments = fields
            .iter()
            .map(|(name, _)| format!("{name} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut params = fields
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        params.push(SqlParameter::Integer(id));

        Ok((
            format!("UPDATE role SET {assignments} WHERE id = ?"),
            params,
        ))
    }

    pub async fn role_id_by_name(&self, role_name: &str) -> Result<i64, Error> {
        sqlx::query_scalar::<_, i64>("SELECT id FROM role WHERE name = ? LIMIT 1")
            .bind(role_name)
            .fetch_one(&*self.services.db)
            .await
            .map_err(map_sqlx_error)
    }

    pub async fn ensure_role_exists(&self, role_id: i64) -> Result<(), Error> {
        let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM role WHERE id = ?")
            .bind(role_id)
            .fetch_one(&*self.services.db)
            .await
            .map_err(map_sqlx_error)?;

        if exists == 0 {
            return Err(Error::new(ErrorKind::NotFound, "Role not found"));
        }

        Ok(())
    }
}

impl ApiTable {
    pub fn as_table_name(self) -> &'static str {
        match self {
            ApiTable::View => "view",
            ApiTable::Record => "record",
            ApiTable::RecordExtension => "record_extension",
            ApiTable::RecordLink => "record_link",
            ApiTable::RecordComment => "record_comment",
            ApiTable::RecordWorklog => "record_worklog",
            ApiTable::RecordResourceRef => "record_resource_ref",
            ApiTable::Frequency => "frequency",
            ApiTable::KarmaCondition => "karma_condition",
            ApiTable::KarmaConsequence => "karma_consequence",
            ApiTable::Karma => "karma",
            ApiTable::Configuration => "configuration",
            ApiTable::AppUser => "app_user",
            ApiTable::Role => "role",
        }
    }

    fn standard_field_specs(self) -> Option<Vec<FieldSpec>> {
        match self {
            ApiTable::View => Some(VIEW_FIELD_SPECS.to_vec()),
            ApiTable::Record => Some(RECORD_FIELD_SPECS.to_vec()),
            ApiTable::RecordExtension => Some(RECORD_EXTENSION_FIELD_SPECS.to_vec()),
            ApiTable::RecordLink => Some(RECORD_LINK_FIELD_SPECS.to_vec()),
            ApiTable::RecordComment => Some(RECORD_COMMENT_FIELD_SPECS.to_vec()),
            ApiTable::RecordWorklog => Some(RECORD_WORKLOG_FIELD_SPECS.to_vec()),
            ApiTable::RecordResourceRef => Some(RECORD_RESOURCE_REF_FIELD_SPECS.to_vec()),
            ApiTable::Frequency => Some(FREQUENCY_FIELD_SPECS.to_vec()),
            ApiTable::KarmaCondition => Some(KARMA_CONDITION_FIELD_SPECS.to_vec()),
            ApiTable::KarmaConsequence => Some(KARMA_CONSEQUENCE_FIELD_SPECS.to_vec()),
            ApiTable::Karma => Some(KARMA_FIELD_SPECS.to_vec()),
            ApiTable::Configuration => Some(CONFIGURATION_FIELD_SPECS.to_vec()),
            ApiTable::AppUser | ApiTable::Role => None,
        }
    }
}

fn parse_api_table(table_name: &str) -> Result<ApiTable, Error> {
    match table_name {
        "view" => Ok(ApiTable::View),
        "record" => Ok(ApiTable::Record),
        "record_extension" => Ok(ApiTable::RecordExtension),
        "record_link" => Ok(ApiTable::RecordLink),
        "record_comment" => Ok(ApiTable::RecordComment),
        "record_worklog" => Ok(ApiTable::RecordWorklog),
        "record_resource_ref" => Ok(ApiTable::RecordResourceRef),
        "frequency" => Ok(ApiTable::Frequency),
        "karma_condition" => Ok(ApiTable::KarmaCondition),
        "karma_consequence" => Ok(ApiTable::KarmaConsequence),
        "karma" => Ok(ApiTable::Karma),
        "configuration" => Ok(ApiTable::Configuration),
        "app_user" => Ok(ApiTable::AppUser),
        "role" => Ok(ApiTable::Role),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("Unsupported API table: {table_name}"),
        )),
    }
}

fn serialize_value<T: Serialize>(value: T) -> Result<Value, Error> {
    serde_json::to_value(value).map_err(Error::other)
}

async fn execute_query_as<T>(
    db: &Pool<Sqlite>,
    sql: &str,
    params: Vec<SqlParameter>,
) -> Result<Vec<T>, Error>
where
    for<'r> T: FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin,
{
    let mut query = sqlx::query_as::<_, T>(sql);
    for param in params {
        query = match param {
            SqlParameter::Null => query.bind(None::<String>),
            SqlParameter::Integer(value) => query.bind(value),
            SqlParameter::Real(value) => query.bind(value),
            SqlParameter::Text(value) => query.bind(value),
        };
    }
    query.fetch_all(db).await.map_err(map_sqlx_error)
}

async fn execute_query_one_as<T>(
    db: &Pool<Sqlite>,
    sql: &str,
    params: Vec<SqlParameter>,
) -> Result<T, Error>
where
    for<'r> T: FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin,
{
    let mut query = sqlx::query_as::<_, T>(sql);
    for param in params {
        query = match param {
            SqlParameter::Null => query.bind(None::<String>),
            SqlParameter::Integer(value) => query.bind(value),
            SqlParameter::Real(value) => query.bind(value),
            SqlParameter::Text(value) => query.bind(value),
        };
    }
    query.fetch_one(db).await.map_err(map_sqlx_error)
}

fn sql_like_contains(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

async fn fetch_public_users(db: &Pool<Sqlite>) -> Result<Vec<PublicAppUserRow>, Error> {
    sqlx::query_as::<_, PublicAppUserRow>(
        "
        SELECT
            u.id,
            u.name,
            u.username,
            u.role_id,
            r.name AS role,
            u.created_at,
            u.updated_at
        FROM app_user u
        JOIN role r ON r.id = u.role_id
        ORDER BY u.id
        ",
    )
    .fetch_all(db)
    .await
    .map_err(map_sqlx_error)
}

async fn fetch_public_user_by_id(db: &Pool<Sqlite>, id: i64) -> Result<PublicAppUserRow, Error> {
    sqlx::query_as::<_, PublicAppUserRow>(
        "
        SELECT
            u.id,
            u.name,
            u.username,
            u.role_id,
            r.name AS role,
            u.created_at,
            u.updated_at
        FROM app_user u
        JOIN role r ON r.id = u.role_id
        WHERE u.id = ?
        ",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .map_err(map_sqlx_error)
}

fn map_sqlx_error(error: sqlx::Error) -> Error {
    match error {
        sqlx::Error::RowNotFound => Error::new(ErrorKind::NotFound, "Row not found"),
        other => Error::other(other),
    }
}

fn reject_common_forbidden_fields(object: &Map<String, Value>) -> Result<(), Error> {
    if object.contains_key("id") {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "The id field is not writable",
        ));
    }
    Ok(())
}

fn reject_user_forbidden_fields(object: &Map<String, Value>) -> Result<(), Error> {
    reject_common_forbidden_fields(object)?;
    for forbidden in ["password_hash", "role"] {
        if object.contains_key(forbidden) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("The {forbidden} field is not writable"),
            ));
        }
    }
    Ok(())
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed_fields: &[&str],
) -> Result<(), Error> {
    for key in object.keys() {
        if !allowed_fields.iter().any(|allowed| allowed == key) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Unknown or non-writable field: {key}"),
            ));
        }
    }
    Ok(())
}

fn parse_fields(
    object: &Map<String, Value>,
    specs: &[FieldSpec],
) -> Result<Vec<(String, SqlParameter)>, Error> {
    let mut parsed = Vec::new();
    for (key, value) in object {
        let spec = specs.iter().find(|spec| spec.name == key).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("Unknown or non-writable field: {key}"),
            )
        })?;
        parsed.push((key.clone(), parse_parameter(key, value, spec.kind)?));
    }

    parsed.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(parsed)
}

fn parse_parameter(
    field_name: &str,
    value: &Value,
    kind: FieldKind,
) -> Result<SqlParameter, Error> {
    match kind {
        FieldKind::Text => parse_text_parameter(field_name, value),
        FieldKind::NullableText => {
            if value.is_null() {
                Ok(SqlParameter::Null)
            } else {
                parse_text_parameter(field_name, value)
            }
        }
        FieldKind::Integer => Ok(SqlParameter::Integer(parse_i64_value(field_name, value)?)),
        FieldKind::NullableInteger => {
            if value.is_null() {
                Ok(SqlParameter::Null)
            } else {
                Ok(SqlParameter::Integer(parse_i64_value(field_name, value)?))
            }
        }
        FieldKind::Real => Ok(SqlParameter::Real(parse_f64_value(field_name, value)?)),
        FieldKind::NullableReal => {
            if value.is_null() {
                Ok(SqlParameter::Null)
            } else {
                Ok(SqlParameter::Real(parse_f64_value(field_name, value)?))
            }
        }
        FieldKind::BooleanInteger => Ok(SqlParameter::Integer(parse_bool_i64_value(
            field_name, value,
        )?)),
    }
}

fn required_text_field(object: &Map<String, Value>, field_name: &str) -> Result<String, Error> {
    let value = object.get(field_name).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("Missing required field: {field_name}"),
        )
    })?;
    parse_text_value(field_name, value)
}

fn parse_text_parameter(field_name: &str, value: &Value) -> Result<SqlParameter, Error> {
    Ok(SqlParameter::Text(parse_text_value(field_name, value)?))
}

fn parse_text_value(field_name: &str, value: &Value) -> Result<String, Error> {
    value.as_str().map(str::to_string).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("Expected string for field {field_name}"),
        )
    })
}

fn parse_i64_value(field_name: &str, value: &Value) -> Result<i64, Error> {
    value.as_i64().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("Expected integer for field {field_name}"),
        )
    })
}

fn parse_f64_value(field_name: &str, value: &Value) -> Result<f64, Error> {
    value.as_f64().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("Expected number for field {field_name}"),
        )
    })
}

fn parse_bool_i64_value(field_name: &str, value: &Value) -> Result<i64, Error> {
    if let Some(boolean) = value.as_bool() {
        return Ok(i64::from(boolean));
    }

    let integer = parse_i64_value(field_name, value)?;
    if matches!(integer, 0 | 1) {
        Ok(integer)
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            format!("Expected boolean or 0/1 integer for field {field_name}"),
        ))
    }
}

pub fn validate_file_key(key: &str) -> Result<String, Error> {
    if key.trim().is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "File key cannot be empty",
        ));
    }
    if key.starts_with('/') {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "File key cannot start with '/'",
        ));
    }

    Ok(key.to_string())
}
