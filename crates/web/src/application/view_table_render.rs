use {
    maud::{Markup, html},
    serde_json::Value,
    std::{
        collections::{BTreeMap, BTreeSet},
        hash::{Hash, Hasher},
    },
};

#[derive(Debug, Clone)]
pub struct ViewTableRenderContext {
    pub server_id: String,
    pub view_id: u64,
}

#[derive(Debug, Clone)]
pub struct ViewTableRenderedSync {
    pub html: ViewTableRenderedHtml,
}

#[derive(Debug, Clone)]
pub struct ViewTableRenderedHtml {
    pub status_pill: String,
    pub details_panel: String,
    pub table_body: String,
}

#[derive(Debug)]
pub enum ViewTableRenderError {
    InvalidPayload(String),
}

impl std::fmt::Display for ViewTableRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPayload(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ViewTableRenderError {}

#[derive(Debug, Clone)]
struct NormalizedTable {
    title: String,
    subtitle: String,
    sql: String,
    kind: String,
    columns: Vec<TableColumn>,
    rows: Vec<TableRow>,
}

#[derive(Debug, Clone)]
struct TableColumn {
    key: String,
    label: String,
}

#[derive(Debug, Clone)]
struct TableRow {
    key: String,
    values: BTreeMap<String, String>,
}

pub fn render_sync_payload(
    context: ViewTableRenderContext,
    payload: &str,
) -> Result<ViewTableRenderedSync, ViewTableRenderError> {
    let snapshot = serde_json::from_str::<Value>(payload)
        .map_err(|error| ViewTableRenderError::InvalidPayload(error.to_string()))?;
    let table = normalize_table_payload(&snapshot);

    Ok(ViewTableRenderedSync {
        html: ViewTableRenderedHtml {
            status_pill: render_status_pill("Live", "live").into_string(),
            details_panel: render_details_panel_inner(&context, &table).into_string(),
            table_body: render_table_body_inner(&table).into_string(),
        },
    })
}

pub fn render_error_payload(context: ViewTableRenderContext, message: &str) -> ViewTableRenderedHtml {
    let summary = NormalizedTable {
        title: "Stream unavailable".into(),
        subtitle: message.to_string(),
        sql: String::new(),
        kind: "error".into(),
        columns: vec![],
        rows: vec![],
    };

    ViewTableRenderedHtml {
        status_pill: render_status_pill("Offline", "error").into_string(),
        details_panel: render_error_details_inner(&context, message, &summary).into_string(),
        table_body: render_error_body_inner(message).into_string(),
    }
}

fn normalize_table_payload(value: &Value) -> NormalizedTable {
    let root = value.as_object();
    let query = root
        .and_then(|object| {
            object
                .get("query")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let title = root
        .and_then(|object| {
            object
                .get("name")
                .or_else(|| object.get("title"))
                .or_else(|| object.get("view_name"))
                .or_else(|| object.get("viewName"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Generic table".into());
    let rows_source = extract_rows_source(value);
    let columns_source = root.and_then(|object| object.get("columns")).and_then(Value::as_array);
    let columns = infer_columns(&rows_source, columns_source);
    let rows = normalize_rows(&rows_source, &columns);
    let kind = determine_kind(value);
    let subtitle = build_subtitle(root, &kind, rows.len(), &query);

    NormalizedTable {
        title,
        subtitle,
        sql: query,
        kind,
        columns,
        rows,
    }
}

fn extract_rows_source(value: &Value) -> Vec<Value> {
    if let Some(rows) = value.as_array() {
        return rows.clone();
    }

    let Some(object) = value.as_object() else {
        return vec![value.clone()];
    };

    for key in ["rows", "items", "data"] {
        if let Some(rows) = object.get(key).and_then(Value::as_array) {
            return rows.clone();
        }
    }

    vec![value.clone()]
}

fn determine_kind(value: &Value) -> String {
    if value.is_array() {
        return "array".into();
    }

    if value
        .as_object()
        .and_then(|object| object.get("rows"))
        .and_then(Value::as_array)
        .is_some()
    {
        return "view-snapshot".into();
    }

    if value.is_object() {
        return "object".into();
    }

    "primitive".into()
}

fn build_subtitle(
    root: Option<&serde_json::Map<String, Value>>,
    kind: &str,
    row_count: usize,
    query: &str,
) -> String {
    match kind {
        "view-snapshot" => {
            let mut parts = Vec::new();
            if let Some(view_id) = root
                .and_then(|object| object.get("view_id").or_else(|| object.get("viewId")))
                .and_then(Value::as_u64)
            {
                parts.push(format!("view {view_id}"));
            }
            if !query.trim().is_empty() {
                parts.push(compact_text(query, 120));
            }
            if parts.is_empty() {
                "Snapshot stream".into()
            } else {
                parts.join(" · ")
            }
        }
        "array" => format!("{row_count} item{}", if row_count == 1 { "" } else { "s" }),
        "object" => {
            let key_count = root.map(|object| object.len()).unwrap_or(0);
            format!(
                "{key_count} top-level key{}",
                if key_count == 1 { "" } else { "s" }
            )
        }
        _ => "Single primitive value normalized as a one-cell table.".into(),
    }
}

fn infer_columns(rows: &[Value], explicit_columns: Option<&Vec<Value>>) -> Vec<TableColumn> {
    let mut columns = Vec::new();
    let mut seen = BTreeSet::new();

    let push_column = |columns: &mut Vec<TableColumn>, seen: &mut BTreeSet<String>, key: String, label: String| {
        let normalized_key = key.trim().to_string();
        if normalized_key.is_empty() || !seen.insert(normalized_key.clone()) {
            return;
        }
        columns.push(TableColumn {
            key: normalized_key,
            label,
        });
    };

    if let Some(raw_columns) = explicit_columns && !raw_columns.is_empty() {
        for (index, raw_column) in raw_columns.iter().enumerate() {
            let spec = column_spec_from_value(raw_column, index);
            push_column(&mut columns, &mut seen, spec.key, spec.label);
        }
        return columns;
    }

    let array_rows = rows.iter().filter(|row| row.is_array()).collect::<Vec<_>>();
    if !array_rows.is_empty() {
        let max_length = array_rows
            .iter()
            .map(|row| row.as_array().map(|value| value.len()).unwrap_or(0))
            .max()
            .unwrap_or(0);
        for index in 0..max_length {
            let key = format!("col_{}", index + 1);
            push_column(
                &mut columns,
                &mut seen,
                key.clone(),
                format!("Column {}", index + 1),
            );
        }
        return columns;
    }

    let object_rows = rows.iter().filter(|row| row.is_object()).collect::<Vec<_>>();
    if !object_rows.is_empty() {
        let mut keys = BTreeSet::new();
        for row in object_rows {
            if let Some(object) = row.as_object() {
                for key in object.keys() {
                    keys.insert(key.to_string());
                }
            }
        }

        for key in keys {
            push_column(&mut columns, &mut seen, key.clone(), prettify_key(&key));
        }

        if !columns.is_empty() {
            return columns;
        }
    }

    push_column(&mut columns, &mut seen, "value".into(), "Value".into());
    columns
}

fn column_spec_from_value(raw_column: &Value, index: usize) -> TableColumn {
    if let Some(label) = raw_column.as_str() {
        let key = label.trim().to_string();
        return TableColumn {
            key: if key.is_empty() {
                format!("col_{}", index + 1)
            } else {
                key.clone()
            },
            label: prettify_key(label),
        };
    }

    if let Some(number) = raw_column.as_i64() {
        return TableColumn {
            key: format!("col_{}", index + 1),
            label: number.to_string(),
        };
    }

    if let Some(object) = raw_column.as_object() {
        let key = object
            .get("key")
            .or_else(|| object.get("name"))
            .or_else(|| object.get("id"))
            .or_else(|| object.get("field"))
            .or_else(|| object.get("label"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("col_{}", index + 1));
        let label = object
            .get("label")
            .or_else(|| object.get("name"))
            .or_else(|| object.get("key"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| prettify_key(&key));

        return TableColumn { key, label };
    }

    let key = format!("col_{}", index + 1);
    TableColumn {
        key: key.clone(),
        label: raw_column.as_str().map(prettify_key).unwrap_or(key),
    }
}

fn normalize_rows(rows: &[Value], columns: &[TableColumn]) -> Vec<TableRow> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let key = pick_row_key(row, index);
            let mut values = BTreeMap::new();

            if let Some(array) = row.as_array() {
                for (column_index, column) in columns.iter().enumerate() {
                    let text = array
                        .get(column_index)
                        .map(format_cell_value)
                        .unwrap_or_default();
                    values.insert(column.key.clone(), text);
                }
            } else if let Some(object) = row.as_object() {
                for column in columns {
                    let text = object
                        .get(&column.key)
                        .map(format_cell_value)
                        .unwrap_or_default();
                    values.insert(column.key.clone(), text);
                }
            } else {
                values.insert("value".into(), format_cell_value(row));
            }

            TableRow { key, values }
        })
        .collect()
}

fn pick_row_key(row: &Value, index: usize) -> String {
    if let Some(object) = row.as_object() {
        for key in ["id", "key", "uuid", "slug", "name"] {
            if let Some(value) = object.get(key).and_then(Value::as_str) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return format!("{key}:{trimmed}");
                }
            } else if let Some(value) = object.get(key) {
                let text = format_cell_value(value);
                if !text.trim().is_empty() {
                    return format!("{key}:{text}");
                }
            }
        }
    }

    format!("row-{index}-{}", fingerprint(row))
}

fn format_cell_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string_pretty(value)
            .unwrap_or_else(|_| serde_json::to_string(value).unwrap_or_default()),
    }
}

fn fingerprint(value: &Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn compact_text(value: &str, limit: usize) -> String {
    let text = value.trim();
    if text.len() <= limit {
        return text.to_string();
    }

    let mut compact = text[..limit].trim_end().to_string();
    compact.push('…');
    compact
}

fn prettify_key(key: &str) -> String {
    key.replace('_', " ")
        .replace('-', " ")
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_status_pill(text: &str, tone: &str) -> Markup {
    html! {
        span id="table-status" class="status" data-tone=(tone) { (text) }
    }
}

fn render_details_panel_inner(
    context: &ViewTableRenderContext,
    table: &NormalizedTable,
) -> Markup {
    html! {
        div class="detailStack" {
            section class="detailCard" {
                div class="eyebrow" { "table" }
                div class="detailTitle" { (&table.title) }
                div class="detailCopy" { (&table.subtitle) }
            }

            section class="detailCard" {
                div class="eyebrow" { "metrics" }
                div class="detailGrid" {
                    span class="pill" { "server: " (&context.server_id) }
                    span class="pill" { "view: " (context.view_id) }
                    span class="pill" { "rows: " (table.rows.len()) }
                    span class="pill" { "columns: " (table.columns.len()) }
                    span class="pill" { "kind: " (&table.kind) }
                }
            }

            section class="detailCard" {
                div class="eyebrow" { "sql" }
                pre class="codeBlock" { (&table.sql) }
            }
        }
    }
}

fn render_error_details_inner(
    context: &ViewTableRenderContext,
    message: &str,
    table: &NormalizedTable,
) -> Markup {
    html! {
        div class="detailStack" {
            section class="detailCard detailCard--error" {
                div class="eyebrow" { "table" }
                div class="detailTitle" { (&table.title) }
                div class="detailCopy" { (message) }
            }

            section class="detailCard" {
                div class="eyebrow" { "metrics" }
                div class="detailGrid" {
                    span class="pill" { "server: " (&context.server_id) }
                    span class="pill" { "view: " (context.view_id) }
                    span class="pill" { "rows: 0" }
                    span class="pill" { "columns: 0" }
                    span class="pill" { "kind: error" }
                }
            }
        }
    }
}

fn render_table_body_inner(table: &NormalizedTable) -> Markup {
    if table.columns.is_empty() && table.rows.is_empty() {
        return render_empty_state_inner("No rows or columns were produced by this snapshot.");
    }

    html! {
        div class="tableFrame" {
            table class="table" {
                thead {
                    tr {
                        @for column in &table.columns {
                            th scope="col" {
                                div class="columnName" { (&column.label) }
                            }
                        }
                    }
                }
                tbody {
                    @for row in &table.rows {
                        tr data-row-key=(row.key.as_str()) {
                            @for column in &table.columns {
                                td class="cell" {
                                    div class="cellValue" {
                                        (row.values
                                            .get(&column.key)
                                            .map(|value| value.as_str())
                                            .unwrap_or(""))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn render_error_body_inner(message: &str) -> Markup {
    html! {
        div class="emptyState errorState" {
            div class="stateTitle" { "Stream unavailable" }
            div class="stateCopy" { (message) }
            button
                class="button button--accent"
                type="button"
                data-on:click="window.TableWidget?.reconnect?.()"
            {
                "Retry"
            }
        }
    }
}

fn render_empty_state_inner(message: &str) -> Markup {
    html! {
        div class="emptyState" {
            div class="stateTitle" { "No table data yet" }
            div class="stateCopy" { (message) }
        }
    }
}
