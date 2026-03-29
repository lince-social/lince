use {
    crate::{
        application::kanban_identity::is_supported_kanban_package_filename,
        domain::board::{BoardCard, BoardState},
        infrastructure::board_state_store::BoardStateStore,
    },
    serde::{Deserialize, Serialize},
    serde_json::{Map, Number, Value},
};

const VALID_TASK_TYPES: [&str; 4] = ["epic", "feature", "task", "other"];
const VALID_QUANTITIES: [i64; 5] = [-3, -2, -1, 0, 1];

#[derive(Clone)]
pub struct KanbanFilterService {
    board_state: BoardStateStore,
}

impl KanbanFilterService {
    pub fn new(board_state: BoardStateStore) -> Self {
        Self { board_state }
    }

    pub async fn apply_filters(
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

    #[allow(dead_code)]
    pub fn build_filtered_query(
        &self,
        base_query: &str,
        rows: &[RawKanbanFilterRow],
    ) -> Result<DerivedKanbanQuery, KanbanFilterError> {
        let validated_rows = rows
            .iter()
            .cloned()
            .map(validate_filter_row)
            .collect::<Result<Vec<_>, _>>()?;
        let mut sql = format!(
            "SELECT * FROM ({}) base WHERE 1 = 1",
            trim_sql(base_query)
        );

        for row in validated_rows {
            row.push_sql(&mut sql);
        }

        sql.push_str(" ORDER BY base.quantity ASC, lower(COALESCE(base.head, '')) ASC, base.id DESC");
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
            (&"only_with_open_worklog", &"equals", ValidatedFilterValue::TrueOnly) => {
                sql.push_str(" AND COALESCE(base.active_worklog_count, 0) > 0");
            }
            _ => {}
        }
    }
}

fn validate_filter_row(row: RawKanbanFilterRow) -> Result<ValidatedKanbanFilterRow, KanbanFilterError> {
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

    if !is_supported_kanban_package_filename(&card.package_name) {
        return Err(KanbanFilterError::Unsupported(
            "Esse widget nao usa o package oficial do Kanban.".into(),
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
