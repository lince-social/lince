use {
    crate::{
        application::kanban_identity::is_supported_graph_widget_filename,
        domain::board::{BoardCard, BoardState},
        infrastructure::board_state_store::BoardStateStore,
    },
    serde::{Deserialize, Serialize},
    serde_json::{Map, Number, Value},
    std::convert::TryFrom,
};

const VALID_TASK_TYPES: [&str; 4] = ["epic", "feature", "task", "other"];
const VALID_QUANTITIES: [i64; 5] = [-3, -2, -1, 0, 1];
const KANBAN_SETTINGS_KEY: &str = "kanban_settings";
const GRAPH_RUNTIME_STATE_KEY: &str = "graph_runtime";
const LEGACY_GRAPH_RUNTIME_STATE_KEY: &str = "kanban_runtime";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanWidgetSettings {
    pub show_parent_context: bool,
    pub view_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateKanbanSettingsRequest {
    pub show_parent_context: Option<bool>,
    pub view_name: Option<String>,
}

#[derive(Clone)]
pub struct KanbanFilterService {
    board_state: BoardStateStore,
}

impl KanbanFilterService {
    pub fn new(board_state: BoardStateStore) -> Self {
        Self { board_state }
    }

    pub async fn persist_filters(
        &self,
        instance_id: &str,
        rows: Vec<RawKanbanFilterRow>,
    ) -> Result<KanbanFilterApplyOutcome, KanbanFilterError> {
        let validated_rows = rows
            .into_iter()
            .map(validate_filter_row)
            .collect::<Result<Vec<_>, _>>()?;
        let persisted_rows = validated_rows
            .iter()
            .map(ValidatedKanbanFilterRow::as_json_value)
            .collect::<Vec<_>>();

        let mut board_state = self.board_state.snapshot().await;
        let card = find_board_card_mut(&mut board_state, instance_id).ok_or_else(|| {
            KanbanFilterError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        validate_kanban_card(card)?;

        let widget_state = ensure_object(&mut card.widget_state);
        let next_version = widget_state
            .get("filters_version")
            .and_then(Value::as_u64)
            .or_else(|| widget_state.get("filtersVersion").and_then(Value::as_u64))
            .unwrap_or(0)
            + 1;
        widget_state.insert("filters".into(), Value::Array(persisted_rows));
        widget_state.insert(
            "filters_version".into(),
            Value::Number(Number::from(next_version)),
        );

        self.board_state
            .replace(board_state)
            .await
            .map_err(KanbanFilterError::Internal)?;

        Ok(KanbanFilterApplyOutcome {
            filters_version: next_version,
        })
    }

    pub async fn update_settings(
        &self,
        instance_id: &str,
        update: UpdateKanbanSettingsRequest,
    ) -> Result<KanbanWidgetSettings, KanbanFilterError> {
        let mut board_state = self.board_state.snapshot().await;
        let card = find_board_card_mut(&mut board_state, instance_id).ok_or_else(|| {
            KanbanFilterError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        validate_kanban_card(card)?;

        let mut settings = extract_kanban_settings(&card.widget_state);
        if let Some(show_parent_context) = update.show_parent_context {
            settings.show_parent_context = show_parent_context;
        }
        if let Some(view_name) = update.view_name {
            settings.view_name =
                Some(normalize_optional_name(Some(&view_name)).ok_or_else(|| {
                    KanbanFilterError::Invalid("Kanban precisa de um view_name nao vazio.".into())
                })?);
        }

        let widget_state = ensure_object(&mut card.widget_state);
        let settings_value = widget_state
            .entry(KANBAN_SETTINGS_KEY)
            .or_insert_with(|| Value::Object(Map::new()));
        let settings_object = ensure_object(settings_value);
        settings_object.insert(
            "show_parent_context".into(),
            Value::Bool(settings.show_parent_context),
        );
        match settings.view_name.as_deref() {
            Some(view_name) => {
                settings_object.insert("view_name".into(), Value::String(view_name.to_string()));
                settings_object.remove("viewName");
            }
            None => {
                settings_object.remove("view_name");
                settings_object.remove("viewName");
            }
        }

        self.board_state
            .replace(board_state)
            .await
            .map_err(KanbanFilterError::Internal)?;

        Ok(settings)
    }

    #[allow(dead_code)]
    pub fn build_filtered_query(
        &self,
        base_query: &str,
        rows: &[RawKanbanFilterRow],
        _settings: &KanbanWidgetSettings,
    ) -> Result<DerivedKanbanQuery, KanbanFilterError> {
        let validated_rows = rows
            .iter()
            .cloned()
            .map(validate_filter_row)
            .collect::<Result<Vec<_>, _>>()?;
        let trimmed_query = trim_sql(base_query);
        let mut sql = format!(
            "SELECT base.* \
             FROM ( \
                 SELECT raw.*, CAST(parent_rel.parent_id AS TEXT) AS parent_id, parent_record.head AS parent_head, \
                        COALESCE(parent_rel.parent_ids_json, '[]') AS parent_ids_json, \
                        COALESCE(parent_rel.parent_heads_json, '[]') AS parent_heads_json \
                 FROM ({trimmed_query}) raw \
                 LEFT JOIN ( \
                     SELECT \
                         rl.record_id, \
                         json_group_array(rl.target_id) AS parent_ids_json, \
                         json_group_array(COALESCE(parent_record.head, '')) AS parent_heads_json, \
                         MIN(rl.target_id) AS parent_id \
                     FROM record_link rl \
                     LEFT JOIN record parent_record ON parent_record.id = rl.target_id \
                     WHERE rl.link_type = 'parent' AND rl.target_table = 'record' \
                     GROUP BY rl.record_id \
                 ) parent_rel ON parent_rel.record_id = CAST(raw.id AS INTEGER) \
                 LEFT JOIN record parent_record ON parent_record.id = parent_rel.parent_id \
             ) base \
             WHERE 1 = 1"
        );

        for row in validated_rows {
            row.push_sql(&mut sql);
        }

        sql.push_str(
            " ORDER BY base.quantity ASC, lower(COALESCE(base.head, '')) ASC, base.id DESC",
        );
        Ok(DerivedKanbanQuery {
            sql,
            bindings: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawKanbanFilterRow {
    pub field: String,
    pub operator: String,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanFilterApplyOutcome {
    pub filters_version: u64,
}

pub fn extract_kanban_settings(widget_state: &Value) -> KanbanWidgetSettings {
    let settings = widget_state
        .get(KANBAN_SETTINGS_KEY)
        .and_then(Value::as_object);

    let show_parent_context = settings
        .and_then(|settings| {
            settings
                .get("show_parent_context")
                .or_else(|| settings.get("showParentContext"))
                .and_then(Value::as_bool)
        })
        .or_else(|| {
            widget_state
                .get("show_parent_context")
                .or_else(|| widget_state.get("showParentContext"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(true);

    let view_name = settings
        .and_then(|settings| {
            settings
                .get("view_name")
                .or_else(|| settings.get("viewName"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            widget_state
                .get("view_name")
                .or_else(|| widget_state.get("viewName"))
                .and_then(Value::as_str)
        })
        .and_then(|value| normalize_optional_name(Some(value)));

    KanbanWidgetSettings {
        show_parent_context,
        view_name,
    }
}

pub fn derived_kanban_view_name(instance_id: &str, view_name: Option<&str>) -> String {
    normalize_optional_name(view_name).unwrap_or_else(|| format!("kanban-{}", instance_id.trim()))
}

pub fn effective_kanban_view_id(widget_state: &Value, view_id: Option<u32>) -> Option<u32> {
    view_id.filter(|value| *value > 0).or_else(|| {
        widget_state
            .get(GRAPH_RUNTIME_STATE_KEY)
            .or_else(|| widget_state.get(LEGACY_GRAPH_RUNTIME_STATE_KEY))
            .and_then(Value::as_object)
            .and_then(|runtime| runtime.get("source_view_id"))
            .and_then(Value::as_i64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
    })
}

fn normalize_optional_name(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::extract_kanban_settings;
    use serde_json::json;

    #[test]
    fn extracts_view_name_from_snake_case_settings() {
        let settings = extract_kanban_settings(&json!({
            "kanban_settings": {
                "show_parent_context": false,
                "view_name": "My Kanban"
            }
        }));

        assert!(!settings.show_parent_context);
        assert_eq!(settings.view_name.as_deref(), Some("My Kanban"));
    }

    #[test]
    fn trims_blank_view_name_and_defaults_to_none() {
        let settings = extract_kanban_settings(&json!({
            "kanban_settings": {
                "view_name": "   "
            }
        }));

        assert!(settings.show_parent_context);
        assert_eq!(settings.view_name, None);
    }

    #[test]
    fn extracts_view_name_from_camel_case_settings() {
        let settings = extract_kanban_settings(&json!({
            "kanban_settings": {
                "showParentContext": false,
                "viewName": "My Kanban"
            }
        }));

        assert!(!settings.show_parent_context);
        assert_eq!(settings.view_name.as_deref(), Some("My Kanban"));
    }

    #[test]
    fn extracts_view_name_from_top_level_fallback() {
        let settings = extract_kanban_settings(&json!({
            "viewName": "Fallback Kanban",
            "showParentContext": false
        }));

        assert!(!settings.show_parent_context);
        assert_eq!(settings.view_name.as_deref(), Some("Fallback Kanban"));
    }

    #[test]
    fn derives_fallback_view_name_from_instance_id() {
        assert_eq!(
            super::derived_kanban_view_name("card-123", None),
            "kanban-card-123"
        );
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DerivedKanbanQuery {
    pub sql: String,
    pub bindings: Vec<KanbanFilterBinding>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum KanbanFilterBinding {
    Text(String),
    Integer(i64),
}

#[derive(Debug, Clone)]
pub enum KanbanFilterError {
    NotFound(String),
    Unsupported(String),
    Invalid(String),
    Internal(String),
}

#[derive(Debug, Clone)]
enum ValidatedFilterValue {
    Text(String),
    TextList(Vec<String>),
    IntegerList(Vec<i64>),
    TrueOnly,
}

#[derive(Debug, Clone)]
struct ValidatedKanbanFilterRow {
    field: &'static str,
    operator: &'static str,
    value: ValidatedFilterValue,
}

impl ValidatedKanbanFilterRow {
    fn as_json_value(&self) -> Value {
        let value = match &self.value {
            ValidatedFilterValue::Text(value) => Value::String(value.clone()),
            ValidatedFilterValue::TextList(values) => Value::Array(
                values
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect::<Vec<_>>(),
            ),
            ValidatedFilterValue::IntegerList(values) => Value::Array(
                values
                    .iter()
                    .copied()
                    .map(Number::from)
                    .map(Value::Number)
                    .collect::<Vec<_>>(),
            ),
            ValidatedFilterValue::TrueOnly => Value::Bool(true),
        };

        Value::Object(Map::from_iter([
            ("field".into(), Value::String(self.field.to_string())),
            ("operator".into(), Value::String(self.operator.to_string())),
            ("value".into(), value),
        ]))
    }

    #[allow(dead_code)]
    fn push_sql(&self, sql: &mut String) {
        match (&self.field, &self.operator, &self.value) {
            (&"text_query", &"contains", ValidatedFilterValue::Text(value)) => {
                let like = sql_like_contains_literal(value);
                sql.push_str(" AND (lower(COALESCE(base.head, '')) LIKE ");
                sql.push_str(&like);
                sql.push_str(" ESCAPE '\\' OR lower(COALESCE(base.body, '')) LIKE ");
                sql.push_str(&like);
                sql.push_str(" ESCAPE '\\')");
            }
            (&"categories_any_json", &"any_of", ValidatedFilterValue::TextList(values)) => {
                sql.push_str(
                    " AND EXISTS (SELECT 1 FROM json_each(COALESCE(base.categories_json, '[]')) value WHERE lower(CAST(value.value AS TEXT)) IN (",
                );
                push_text_literals(
                    sql,
                    &values
                        .iter()
                        .map(|value| value.to_lowercase())
                        .collect::<Vec<_>>(),
                );
                sql.push_str("))");
            }
            (&"assignee_ids_any_json", &"any_of", ValidatedFilterValue::IntegerList(values)) => {
                sql.push_str(
                    " AND EXISTS (SELECT 1 FROM json_each(COALESCE(base.assignee_ids_json, '[]')) value WHERE CAST(value.value AS INTEGER) IN (",
                );
                push_integer_literals(sql, values);
                sql.push_str("))");
            }
            (&"quantities_json", &"any_of", ValidatedFilterValue::IntegerList(values)) => {
                sql.push_str(" AND CAST(base.quantity AS INTEGER) IN (");
                push_integer_literals(sql, values);
                sql.push(')');
            }
            (&"task_types_json", &"any_of", ValidatedFilterValue::TextList(values)) => {
                sql.push_str(" AND lower(COALESCE(base.task_type, '')) IN (");
                push_text_literals(
                    sql,
                    &values
                        .iter()
                        .map(|value| value.to_lowercase())
                        .collect::<Vec<_>>(),
                );
                sql.push(')');
            }
            (&"parent_head_query", &"contains", ValidatedFilterValue::Text(value)) => {
                let like = sql_like_contains_literal(value);
                sql.push_str(" AND (lower(COALESCE(base.parent_head, '')) LIKE ");
                sql.push_str(&like);
                sql.push_str(" ESCAPE '\\' OR EXISTS (SELECT 1 FROM json_each(COALESCE(base.parent_heads_json, '[]')) value WHERE lower(CAST(value.value AS TEXT)) LIKE ");
                sql.push_str(&like);
                sql.push_str(" ESCAPE '\\'))");
            }
            (&"only_with_open_worklog", &"equals", ValidatedFilterValue::TrueOnly) => {
                sql.push_str(" AND COALESCE(base.active_worklog_count, 0) > 0");
            }
            _ => {}
        }
    }
}

fn validate_filter_row(
    row: RawKanbanFilterRow,
) -> Result<ValidatedKanbanFilterRow, KanbanFilterError> {
    let field = row.field.trim();
    let operator = row.operator.trim();

    match (field, operator) {
        ("text_query", "contains") => {
            let value = row
                .value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    KanbanFilterError::Invalid(
                        "Filtro text_query precisa de uma string nao vazia.".into(),
                    )
                })?;
            Ok(ValidatedKanbanFilterRow {
                field: "text_query",
                operator: "contains",
                value: ValidatedFilterValue::Text(value),
            })
        }
        ("categories_any_json", "any_of") => Ok(ValidatedKanbanFilterRow {
            field: "categories_any_json",
            operator: "any_of",
            value: ValidatedFilterValue::TextList(validate_text_array(
                &row.value,
                false,
                "Filtro de categorias precisa de pelo menos uma categoria valida.",
            )?),
        }),
        ("assignee_ids_any_json", "any_of") => Ok(ValidatedKanbanFilterRow {
            field: "assignee_ids_any_json",
            operator: "any_of",
            value: ValidatedFilterValue::IntegerList(validate_integer_array(
                &row.value,
                None,
                "Filtro de assignees precisa de pelo menos um app_user.id valido.",
            )?),
        }),
        ("quantities_json", "any_of") => Ok(ValidatedKanbanFilterRow {
            field: "quantities_json",
            operator: "any_of",
            value: ValidatedFilterValue::IntegerList(validate_integer_array(
                &row.value,
                Some(&VALID_QUANTITIES),
                "Filtro de colunas aceita apenas quantities do Kanban.",
            )?),
        }),
        ("task_types_json", "any_of") => Ok(ValidatedKanbanFilterRow {
            field: "task_types_json",
            operator: "any_of",
            value: ValidatedFilterValue::TextList(validate_text_array(
                &row.value,
                true,
                "Filtro de task_type aceita apenas epic, feature, task ou other.",
            )?),
        }),
        ("parent_head_query", "contains") => {
            let value = row
                .value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    KanbanFilterError::Invalid(
                        "Filtro parent_head_query precisa de uma string nao vazia.".into(),
                    )
                })?;
            Ok(ValidatedKanbanFilterRow {
                field: "parent_head_query",
                operator: "contains",
                value: ValidatedFilterValue::Text(value),
            })
        }
        ("only_with_open_worklog", "equals") => {
            if row.value.as_bool() != Some(true) {
                return Err(KanbanFilterError::Invalid(
                    "Filtro only_with_open_worklog so aceita true quando presente.".into(),
                ));
            }
            Ok(ValidatedKanbanFilterRow {
                field: "only_with_open_worklog",
                operator: "equals",
                value: ValidatedFilterValue::TrueOnly,
            })
        }
        _ => Err(KanbanFilterError::Invalid(
            "Linha de filtro invalida para o Kanban.".into(),
        )),
    }
}

fn validate_text_array(
    value: &Value,
    restrict_to_task_types: bool,
    error_message: &str,
) -> Result<Vec<String>, KanbanFilterError> {
    let Some(values) = value.as_array() else {
        return Err(KanbanFilterError::Invalid(error_message.into()));
    };

    let normalized = values
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    if normalized.is_empty() {
        return Err(KanbanFilterError::Invalid(error_message.into()));
    }

    if restrict_to_task_types
        && !normalized
            .iter()
            .all(|value| VALID_TASK_TYPES.contains(&value.to_lowercase().as_str()))
    {
        return Err(KanbanFilterError::Invalid(error_message.into()));
    }

    Ok(normalized)
}

fn validate_integer_array(
    value: &Value,
    allowed_values: Option<&[i64]>,
    error_message: &str,
) -> Result<Vec<i64>, KanbanFilterError> {
    let Some(values) = value.as_array() else {
        return Err(KanbanFilterError::Invalid(error_message.into()));
    };

    let normalized = values.iter().filter_map(Value::as_i64).collect::<Vec<_>>();
    if normalized.is_empty() {
        return Err(KanbanFilterError::Invalid(error_message.into()));
    }

    if let Some(allowed_values) = allowed_values
        && !normalized
            .iter()
            .all(|value| allowed_values.contains(value))
    {
        return Err(KanbanFilterError::Invalid(error_message.into()));
    }

    Ok(normalized)
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }

    value
        .as_object_mut()
        .expect("widget state object should exist")
}

fn find_board_card_mut<'a>(
    board_state: &'a mut BoardState,
    instance_id: &str,
) -> Option<&'a mut BoardCard> {
    board_state
        .workspaces
        .iter_mut()
        .flat_map(|workspace| workspace.cards.iter_mut())
        .find(|card| card.id == instance_id)
}

fn validate_kanban_card(card: &BoardCard) -> Result<(), KanbanFilterError> {
    if card.kind.trim() != "package" {
        return Err(KanbanFilterError::Unsupported(
            "Esse widget nao e um package oficial.".into(),
        ));
    }

    if !is_supported_graph_widget_filename(&card.package_name) {
        return Err(KanbanFilterError::Unsupported(
            "Esse widget nao usa um package oficial suportado.".into(),
        ));
    }

    Ok(())
}

#[allow(dead_code)]
fn trim_sql(query: &str) -> String {
    query.trim().trim_end_matches(';').trim().to_string()
}

#[allow(dead_code)]
fn push_integer_literals(sql: &mut String, values: &[i64]) {
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            sql.push_str(", ");
        }
        sql.push_str(&value.to_string());
    }
}

#[allow(dead_code)]
fn push_text_literals(sql: &mut String, values: &[String]) {
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            sql.push_str(", ");
        }
        sql.push_str(&sql_text_literal(value));
    }
}

#[allow(dead_code)]
fn sql_text_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[allow(dead_code)]
fn sql_like_contains_literal(value: &str) -> String {
    let escaped = value
        .to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
        .replace('\'', "''");
    format!("'%{escaped}%'")
}
